# FFI & `unsafe`: puente seguro a C

La Semana 14 baja al nivel más cercano al hardware que ofrece Rust sin salir del
lenguaje: la interfaz con código C. FFI (Foreign Function Interface) es el mecanismo
que permite llamar funciones de bibliotecas escritas en C desde Rust, y exponer
funciones Rust para que las consuma C o cualquier lenguaje que hable ABI de C.

En esta sección aprenderemos:

- Qué significa `unsafe` exactamente: qué invariantes se ceden al compilador y cuáles
  asume el programador.
- Las cinco reglas del Rustonomicon que nunca se pueden violar.
- `extern "C"`, `#[unsafe(no_mangle)]` y `#[repr(C)]`: los tres ingredientes de FFI (y qué cambió en la edición 2024).
- Tipos de datos en la frontera: `c_int`, `CString`, `CStr`, `*mut T`, `NonNull<T>`.
- Strings entre mundos: el problema del byte NUL y cómo `CString`/`CStr` lo resuelven.
- Transferencia de ownership: `Box::into_raw` y `Box::from_raw`.
- `MaybeUninit<T>` para inicialización de memoria al estilo C.
- El crate `cc` en `build.rs` para compilar C dentro del build de Cargo.
- `bindgen` para generar bindings automáticamente desde headers.
- El patrón "safe wrapper": encapsular `unsafe` en una API imposible de usar mal.
- `impl Drop` para limpieza de recursos C.
- `Send`/`Sync` manuales para tipos FFI.
- `cargo miri` para detectar UB.

!!! quote "Filosofía de la Semana 14"

    *`unsafe` no significa "código peligroso sin
    revisar" — significa "código cuya corrección no puede verificar el compilador". Es
    tu responsabilidad documentar y hacer cumplir los invariantes que el compilador ya no
    puede. Escribe el menor bloque `unsafe` posible y envuélvelo en una API que haga
    imposible violarlo.*

---

## Qué es `unsafe` realmente

En Rust seguro, el compilador garantiza:

- No hay punteros nulos ni colgantes (*dangling*).
- No hay aliasing mutable.
- No hay uso tras liberación (*use-after-free*).
- No hay lectura de memoria no inicializada.

`unsafe` no desactiva el compilador — desactiva **esas cuatro garantías específicas**.
Todo lo demás (tipos, lifetimes fuera de raw pointers, borrow checker para referencias
normales) sigue funcionando. El código `unsafe` es código que necesita razonamiento
humano adicional para ser correcto.

```text
UNSAFE EN RUST

Sin unsafe:                     Con unsafe:
┌─────────────────────┐         ┌─────────────────────────────────────┐
│ Compilador garantiza│         │ Compilador verifica tipos y lifetimes│
│ - no null ptr       │         │ Tú garantizas:                      │
│ - no aliasing mut   │  →      │ - punteros válidos                  │
│ - no use-after-free │         │ - aliasing correcto                 │
│ - no uninit read    │         │ - memoria inicializada              │
└─────────────────────┘         │ - ownership correcto                │
                                └─────────────────────────────────────┘
```

### Las cinco reglas del Rustonomicon

Violar cualquiera de estas produce **Undefined Behavior** (UB): el compilador puede
generar código arbitrariamente incorrecto, incluso en versiones que "parecían funcionar":

1. **No desreferenciar punteros nulos ni inválidos.**
   Un `*const T` puede ser nulo o apuntar a memoria liberada. Antes de desreferenciar,
   verifica que no es nulo y que la memoria sigue válida.

2. **No crear referencias inválidas.**
   `&*ptr` requiere que `ptr` sea: no nulo, correctamente alineado, apuntando a un
   `T` inicializado, y que la referencia sea la única con acceso mutable si es `&mut T`.

3. **No romper el aliasing.**
   En cualquier momento dado, puede haber **una** `&mut T` o **varias** `&T` apuntando
   al mismo dato — nunca ambas a la vez. Violar esto permite al compilador "mover"
   lecturas a través de escrituras, produciendo valores fantasma.

4. **No data races.**
   Acceder al mismo dato desde varios hilos sin sincronización es UB incluso para
   lecturas. `Mutex`, `RwLock`, `Arc` y los tipos atómicos son las soluciones.

5. **No invocar comportamiento definido por implementación que no se garantice.**
   Esto incluye: overflow de enteros en modo release (en Rust seguro hace wrapping por
   configuración, pero en `unsafe` puedes depender de ello sin garantías), transmutes
   de tipos incompatibles, y llamar a C que viole las precondiciones documentadas.

### `unsafe` bloque vs `unsafe fn`

```rust
// unsafe fn: quien llama debe respetar un contrato
// Documenta el contrato con /// # Safety
/// # Safety
/// `ptr` debe ser no nulo, alineado y apuntar a un `i32` inicializado
unsafe fn leer_i32(ptr: *const i32) -> i32 {
    // Edición 2024: dentro de una `unsafe fn` cada operación insegura también
    // necesita su propio bloque `unsafe` (lint `unsafe_op_in_unsafe_fn`).
    // SAFETY: el llamador garantiza el contrato documentado arriba.
    unsafe { *ptr }
}

// Bloque unsafe: aísla la parte insegura dentro de una fn segura
fn sumar_elementos(slice: &[i32]) -> i32 {
    let mut total = 0i32;
    for i in 0..slice.len() {
        // SAFETY: i < slice.len() garantiza que el índice es válido
        total += unsafe { *slice.as_ptr().add(i) };
    }
    total
}
```

La regla: **usa `unsafe fn` cuando la función entera tiene precondiciones que el
llamador debe respetar; usa bloques `unsafe` dentro de fns seguras para aislar la
operación puntual y documentar por qué es segura en ese contexto**.

---

## `extern "C"`: la ABI de la frontera

### Exportar Rust a C

```rust
// Rust que C puede llamar:
// 1. #[unsafe(no_mangle)]: desactiva name-mangling de Rust
//    (en edición 2024 el atributo va envuelto en `unsafe(...)` porque un nombre
//     de símbolo duplicado en el linker es UB)
// 2. extern "C": usa la calling convention de C (registros, stack order)
// 3. pub: visible para el linker

#[unsafe(no_mangle)]
pub extern "C" fn sumar(a: i32, b: i32) -> i32 {
    a + b
}

// En C:
// extern int sumar(int a, int b);
// int resultado = sumar(3, 4);  // 7
```

Sin `#[unsafe(no_mangle)]` (`#[no_mangle]` en ediciones anteriores), el compilador genera nombres como `_ZN7mytool5sumar17h3a1b2c3d4e5f6g7hE`
que C no puede usar. Sin `extern "C"`, la calling convention puede diferir y los
argumentos llegan en el orden o registros equivocados.

### Llamar a C desde Rust

```rust
// Declarar funciones C que queremos usar.
// Edición 2024: el bloque se escribe `unsafe extern`, porque al declarar firmas
// externas *tú* garantizas que coinciden con las reales.
unsafe extern "C" {
    fn abs(n: i32) -> i32;
    fn strlen(s: *const std::os::raw::c_char) -> usize;
    fn malloc(size: usize) -> *mut std::os::raw::c_void;
    fn free(ptr: *mut std::os::raw::c_void);
}

fn valor_absoluto(n: i32) -> i32 {
    // Las llamadas a extern "C" son siempre unsafe:
    // el compilador no puede verificar que la función C es correcta
    unsafe { abs(n) }
}
```

---

## `#[repr(C)]`: layout de memoria garantizado

Sin anotaciones, Rust puede reordenar los campos de una struct para optimizar
alineación. Esto es invisible para código Rust puro, pero rompería cualquier struct
que cruce la frontera FFI:

```rust
// Sin repr(C): Rust puede reordenar los campos
struct PuntoRust { x: f64, y: f64, activo: bool }
// Layout real: podría ser [activo, padding×7, x, y] o cualquier otro

// Con repr(C): campos en el orden declarado, padding como C lo haría
#[repr(C)]
pub struct Punto { pub x: f64, pub y: f64 }

#[repr(C)]
pub struct VectorC { pub ptr: *mut f64, pub len: usize, pub cap: usize }

// repr(C) en enums: la discriminante es un entero C
#[repr(C, i32)]
pub enum EstadoC {
    Ok      = 0,
    Error   = -1,
    Timeout = -2,
}

// Verificar que el tamaño coincide con el esperado:
const _: () = assert!(std::mem::size_of::<Punto>() == 16);
```

### Handles opacos: el puntero vacío tipado

Muchas APIs de C usan handles opacos — el usuario nunca ve la estructura interna:

```rust
// En el header C: typedef struct Buffer Buffer;
// La definición real es privada (opaque)

// En Rust, representamos un tipo opaco con un struct de tamaño cero y campos
// privados (patrón recomendado por el Rustonomicon). `#[repr(C)]` no admite enums
// vacíos, y un enum vacío es un tipo *inhabitado*: crear una referencia a él es UB.
#[repr(C)]
pub struct BufferOpaco {
    _datos: [u8; 0],
    // Sin Send/Sync/Unpin automáticos: no sabemos qué hace C con el objeto.
    _marker: std::marker::PhantomData<(*mut u8, std::marker::PhantomPinned)>,
}   // no se puede instanciar fuera del módulo, solo usar como *mut BufferOpaco

// Esto es mejor que *mut c_void porque es type-safe:
// no puedes pasar accidentalmente un *mut OtroOpaco donde se espera *mut BufferOpaco
```

---

## Tipos en la frontera FFI

### Tabla de equivalencias

| C | Rust (`std::os::raw`) | Rust (`libc`) | Notas |
| :--- | :--- | :--- | :--- |
| `int` | `c_int` | `libc::c_int` | Tamaño variable (≥16 bits, casi siempre 32) |
| `long` | `c_long` | `libc::c_long` | 32 bits en Windows, 64 en Linux/macOS |
| `size_t` | `usize` | `libc::size_t` | Tamaño del puntero |
| `char` | `c_char` | `libc::c_char` | Puede ser `i8` o `u8` según plataforma |
| `unsigned char` | `c_uchar` | `u8` | Seguro como u8 |
| `float` | `f32` | `f32` | Mismo layout |
| `double` | `f64` | `f64` | Mismo layout |
| `void*` | `*mut c_void` | `*mut libc::c_void` | Puntero genérico |
| `const void*` | `*const c_void` | — | Puntero genérico de solo lectura |
| `bool` (C99) | `bool` | — | Solo si `#include <stdbool.h>` en C |

### Strings: `CString` y `CStr`

Las strings en C terminan en `\0`. Las de Rust no. Este desajuste es la fuente más
común de bugs en FFI:

```rust
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

// Rust String → *const c_char (para pasar a C)
fn str_a_c(s: &str) -> CString {
    CString::new(s).expect("la string no puede contener bytes NUL")
    // CString hace la copia y añade el \0 al final
}

// Uso:
fn llamar_funcion_c(nombre: &str) {
    let c_nombre = str_a_c(nombre);
    unsafe {
        // c_nombre.as_ptr() es válido mientras c_nombre vive en este scope
        funcion_c_que_usa_char_ptr(c_nombre.as_ptr());
    }
    // c_nombre se libera aquí
}

// *const c_char recibida de C → &str en Rust
unsafe fn c_a_str<'a>(ptr: *const c_char) -> &'a str {
    // SAFETY: ptr debe ser no nulo, NUL-terminado, y válido por 'a
    assert!(!ptr.is_null(), "puntero nulo recibido de C");
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .expect("la string de C no es UTF-8 válido")
}

// Si C puede devolver strings no-UTF-8:
unsafe fn c_a_string_lossy(ptr: *const c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    // Reemplaza bytes inválidos con U+FFFD (el símbolo de reemplazo)
}

unsafe extern "C" { fn funcion_c_que_usa_char_ptr(s: *const c_char); }
```

**Error clásico: pasar `String::as_ptr()` a C**:

```rust
// ❌ MUY MAL: String no tiene \0 al final
fn bug_clasico(nombre: &str) {
    let ptr = nombre.as_ptr() as *const c_char;
    // C leerá más allá del final de la string → UB
    unsafe { funcion_c_que_usa_char_ptr(ptr); }
}

// ✅ CORRECTO: CString añade \0
fn correcto(nombre: &str) {
    let c = CString::new(nombre).unwrap();
    unsafe { funcion_c_que_usa_char_ptr(c.as_ptr()); }
}

unsafe extern "C" { fn funcion_c_que_usa_char_ptr(s: *const c_char); }
```

---

## Punteros raw y transferencia de ownership

### `NonNull<T>`: el puntero no nulo

`*mut T` puede ser nulo. `NonNull<T>` garantiza no-nulidad en tiempo de compilación y
habilita la null-pointer optimization para `Option<NonNull<T>>`:

```rust
use std::ptr::NonNull;

// Option<NonNull<T>> tiene el mismo tamaño que *mut T:
// None == null, Some(p) == p
const _: () = assert!(
    std::mem::size_of::<Option<NonNull<i32>>>()
    == std::mem::size_of::<*mut i32>()
);

fn crear_no_nulo(n: i32) -> NonNull<i32> {
    let b = Box::new(n);
    // SAFETY: Box::into_raw nunca devuelve null
    unsafe { NonNull::new_unchecked(Box::into_raw(b)) }
}
```

### `Box::into_raw` y `Box::from_raw`: la transferencia de ownership a C

El patrón canónico para pasar un objeto Rust a C y recuperarlo después:

```rust
// Pasar a C: Rust cede la propiedad
fn crear_objeto_para_c() -> *mut MiStruct {
    let objeto = Box::new(MiStruct::new());
    Box::into_raw(objeto)   // Rust ya no gestiona esta memoria
    // C recibe el puntero y debe devolverlo para liberarlo
}

// C devuelve el puntero: Rust recupera la propiedad
unsafe fn liberar_objeto(ptr: *mut MiStruct) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });   // Drop se ejecuta, memoria liberada
    }
}

// API pública para que C llame:
#[unsafe(no_mangle)]
pub extern "C" fn mi_struct_crear() -> *mut MiStruct {
    Box::into_raw(Box::new(MiStruct::new()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mi_struct_liberar(ptr: *mut MiStruct) {
    if !ptr.is_null() { drop(unsafe { Box::from_raw(ptr) }); }
}

struct MiStruct { valor: i32 }
impl MiStruct { fn new() -> Self { Self { valor: 42 } } }
```

**Regla**: cada `Box::into_raw` debe tener exactamente un `Box::from_raw`
correspondiente. Ni más (doble-free) ni menos (leak).

### `MaybeUninit<T>`: memoria sin inicializar

C inicializa estructuras con `memset` o `struct_init()`. Para interoperar:

```rust
use std::mem::MaybeUninit;

unsafe extern "C" {
    fn inicializar_config(cfg: *mut ConfigC) -> i32;
}

#[repr(C)]
struct ConfigC { valor: i32, flags: u32, nombre: [u8; 64] }

fn crear_config() -> Result<ConfigC, i32> {
    // MaybeUninit evita leer memoria no inicializada (UB) antes de que C la llene
    let mut cfg = MaybeUninit::<ConfigC>::uninit();

    let rc = unsafe { inicializar_config(cfg.as_mut_ptr()) };

    if rc == 0 {
        // SAFETY: inicializar_config devolvió 0, por lo que la struct está inicializada
        Ok(unsafe { cfg.assume_init() })
    } else {
        Err(rc)
    }
}
```

---

## Compilar C con `build.rs` y el crate `cc`

`build.rs` es un script que Cargo ejecuta antes de compilar el crate. Se usa para:
- Compilar código C con el crate `cc`.
- Generar bindings con `bindgen`.
- Enlazar con bibliotecas del sistema.
- Generar código Rust (macros procedurales, completions).

```toml
# Cargo.toml
[build-dependencies]
cc      = "1"
bindgen = "0.72"
```

```rust
// build.rs
use std::{env, path::PathBuf};

fn main() {
    // Recompilar si el código C cambia
    println!("cargo:rerun-if-changed=csrc/buffer.h");
    println!("cargo:rerun-if-changed=csrc/buffer.c");

    // Compilar biblioteca C estática y enlazarla
    cc::Build::new()
        .file("csrc/buffer.c")
        .include("csrc")
        .flag_if_supported("-O2")
        .flag_if_supported("-Wall")
        .compile("buffer");  // genera libbuffer.a

    // Generar bindings automáticos con bindgen
    let bindings = bindgen::Builder::default()
        .header("csrc/buffer.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // Edición 2024: genera `unsafe extern "C" { ... }` (requiere bindgen 0.71+)
        .rust_edition(bindgen::RustEdition::Edition2024)
        // Solo incluir lo que necesitamos
        .allowlist_function("buf_.*")
        .allowlist_type("Buf.*")
        .allowlist_var("BUF_.*")
        // Derivar Debug y Default cuando sea posible
        .derive_debug(true)
        .derive_default(true)
        .generate()
        .expect("bindgen falló");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("no se pudieron escribir los bindings");
}
```

---

## Ejercicio: safe wrapper para una biblioteca C propia

Construimos una biblioteca C que gestiona buffers cifrados, y sobre ella una API Rust
completamente segura.

### La biblioteca C: `csrc/buffer.h` y `csrc/buffer.c`

`csrc/buffer.h`:

```c
--8<-- "src/chapter_04/safe_ffi/csrc/buffer.h"
```

`csrc/buffer.c`:

```c
--8<-- "src/chapter_04/safe_ffi/csrc/buffer.c"
```

### `src/raw.rs`: bindings manuales

En proyectos reales estos los genera `bindgen`; aquí los escribimos a mano para ver
exactamente lo que genera:

```rust
--8<-- "src/chapter_04/safe_ffi/src/raw.rs"
```

### `src/error.rs`: tipos de error propios

```rust
--8<-- "src/chapter_04/safe_ffi/src/error.rs"
```

### `src/safe.rs`: el wrapper seguro

Esta es la pieza central de la semana: una API que es imposible de usar mal.

```rust
--8<-- "src/chapter_04/safe_ffi/src/safe.rs"
```

### `src/lib.rs` — la API pública

El wrapper es una **librería**: `raw` queda privado, así que quien use la crate solo
puede llamar a la API segura. El binario de demostración la usa como cualquier otro
cliente.

```rust
--8<-- "src/chapter_04/safe_ffi/src/lib.rs"
```

### `src/main.rs`: demostración y tests

```rust
--8<-- "src/chapter_04/safe_ffi/src/main.rs"
```

### `Cargo.toml` completo del proyecto

```toml
--8<-- "src/chapter_04/safe_ffi/Cargo.toml"
```

`build.rs`:

```rust
--8<-- "src/chapter_04/safe_ffi/build.rs"
```

Ejecutar:

```bash
cargo build
cargo test
cargo run
```

---

## `bindgen` en detalle

En proyectos reales con headers C existentes, `bindgen` genera los bindings
automáticamente. El flujo completo:

```bash
# Instalar bindgen-cli (opcional, para probar desde terminal)
cargo install bindgen-cli

# Generar bindings manualmente para inspección
bindgen csrc/buffer.h \
    --rust-edition 2024 \
    --allowlist-function "buf_.*" \
    --allowlist-type "Buf.*" \
    --allowlist-var "BUF_.*" \
    -o src/bindings_generados.rs
```

Resultado típico de `bindgen` para nuestro header:

```rust
// GENERADO AUTOMÁTICAMENTE POR BINDGEN — NO EDITAR
pub const BUF_OK: i32 = 0;
pub const BUF_ERR_NULL: i32 = -1;
pub const BUF_ERR_RANGE: i32 = -2;
pub const BUF_ERR_ALLOC: i32 = -3;

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct BufHandle {
    pub _address: u8,
}

unsafe extern "C" {
    pub fn buf_crear(len: usize) -> *mut BufHandle;
    pub fn buf_liberar(b: *mut BufHandle);
    pub fn buf_len(b: *const BufHandle) -> usize;
    pub fn buf_escribir(b: *mut BufHandle, offset: usize, data: *const u8, n: usize) -> i32;
    pub fn buf_leer(b: *const BufHandle, offset: usize, dest: *mut u8, n: usize) -> i32;
    pub fn buf_xor(b: *mut BufHandle, clave: u8) -> i32;
    pub fn buf_limpiar(b: *mut BufHandle) -> i32;
}
```

En `build.rs` se incluye con:

```rust
// src/raw.rs
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
```

---

## `Send` y `Sync` manuales para tipos FFI

Rust no puede inferir `Send`/`Sync` para tipos con punteros raw. Debes implementarlos
manualmente si el análisis indica que son seguros:

```rust
pub struct Buffer { ptr: NonNull<raw::BufHandle> }

// SAFETY: BufHandle es un buffer de bytes sin referencias internas a hilos.
// La exclusión mutua al acceder desde múltiples hilos es responsabilidad del llamador
// (igual que con Vec<u8>). Si C usara TLS o estado global, esto no sería seguro.
unsafe impl Send for Buffer {}

// SAFETY: &Buffer solo permite lecturas (buf_leer, buf_len), que son seguras
// desde múltiples hilos simultáneamente si C lo garantiza. buf_leer no modifica
// el buffer.
unsafe impl Sync for Buffer {}
```

Si el análisis indica que **no** es `Send` o `Sync`, simplemente no implementes el
trait. El compilador rechazará cualquier intento de mover el tipo a otro hilo.

---

## Detectar UB con `cargo miri`

`miri` es un intérprete de MIR (la representación intermedia de Rust) que detecta UB
en tiempo de ejecución durante los tests:

```bash
# Instalar la toolchain nightly con miri
rustup component add miri --toolchain nightly

# Ejecutar tests bajo miri
cargo +nightly miri test
```

`miri` detecta:

- Desreferenciación de punteros nulos o inválidos.
- Use-after-free.
- Aliasing mutable ilegal.
- Lectura de memoria no inicializada.
- Acceso a memoria fuera de bounds.

```
# Ejemplo de salida de miri con un bug:
error: Undefined Behavior: dereferencing pointer which is dangling
  --> src/safe.rs:82:14
   |
82 |         unsafe { raw::buf_leer(self.ptr.as_ptr(), ...) }
   |
   = note: use-after-free
```

**Limitación**: `miri` no puede ejecutar código C real. Para tests que crucen FFI
necesitas herramientas a nivel de OS: `valgrind` (Linux) o `AddressSanitizer`:

```bash
# Compilar con AddressSanitizer para detectar UB en C+Rust
RUSTFLAGS="-Z sanitizer=address" \
  cargo +nightly test --target x86_64-unknown-linux-gnu
```

---

## Patrones de documentación para código `unsafe`

Siempre documenta el contrato de cada bloque `unsafe`:

```rust
/// Crea un nuevo buffer de `longitud` bytes.
///
/// # Errors
/// Devuelve `AsignacionFallida` si la asignación de memoria del sistema falla.
///
/// # Panics
/// Nunca.
pub fn nuevo(longitud: usize) -> Result<Self, ErrorBuffer> {
    // SAFETY: buf_crear devuelve NULL si falla (documentado en buffer.h).
    // NonNull::new convierte NULL en None, que mapeamos a Err.
    // Si no es NULL, el puntero es válido y está inicializado (calloc + setup).
    let ptr = unsafe { raw::buf_crear(longitud) };
    NonNull::new(ptr)
        .map(|p| Buffer { ptr: p })
        .ok_or(ErrorBuffer::AsignacionFallida)
}

// Para unsafe fn: usa /// # Safety
/// # Safety
/// `ptr` debe ser un puntero válido a un `BufHandle` creado por `buf_crear`
/// y que no haya sido liberado todavía.
pub unsafe fn desde_ptr(ptr: *mut raw::BufHandle) -> Self {
    // SAFETY: el llamador garantiza que `ptr` es válido (y por tanto no nulo).
    Buffer { ptr: unsafe { NonNull::new_unchecked(ptr) } }
}
```

---

## Resumen: el ciclo de vida de un wrapper FFI

```text
FLUJO COMPLETO DE UN WRAPPER SEGURO

1. Header C (buffer.h)
        ↓ bindgen / manual
2. Bindings raw (src/raw.rs)
   - extern "C" { fn buf_crear(...) }
   - #[repr(C)] struct BufHandle { ... }
        ↓ encapsulación
3. Safe wrapper (src/safe.rs)
   - struct Buffer { ptr: NonNull<raw::BufHandle> }
   - impl Buffer: API sin unsafe en superficie
   - impl Drop: limpieza automática
   - unsafe impl Send + Sync: si aplica
        ↓ usuario final
4. API de negocio (src/main.rs)
   - Buffer::nuevo(16)?
   - buf.escribir(0, datos)?   ← cero unsafe visible
   - buf.aplicar_xor(0x42)?
   - // drop automático → buf_liberar → memset + free
```

---

## ✅ Checklist de la Semana 14

- [ ] Explico las cinco reglas del Rustonomicon y doy un ejemplo de cada UB que
  producirían si se violan.
- [ ] Distingo `unsafe fn` (quien llama asume el contrato) de bloque `unsafe` dentro
  de una fn segura (encapsula operación puntual).
- [ ] `#[repr(C)]` está en todas las structs que cruzan la frontera FFI. Verifico con
  `std::mem::size_of::<T>() == size_of_C` en un `const _: ()` assertion.
- [ ] Nunca paso `String::as_ptr()` a C. Siempre uso `CString::new(s).unwrap().as_ptr()`
  y mantengo la `CString` viva mientras C usa el puntero.
- [ ] Cada `Box::into_raw` tiene exactamente un `Box::from_raw` correspondiente.
- [ ] `build.rs` compila el código C con el crate `cc` y los `rerun-if-changed` están
  configurados para no recompilar innecesariamente.
- [ ] El struct `Buffer` usa `NonNull<BufHandle>` en lugar de `*mut BufHandle`, lo que
  habilita la null-pointer optimization y expresa el invariante de no-nulidad.
- [ ] `impl Drop for Buffer` llama a `buf_liberar` exactamente una vez.
- [ ] `unsafe impl Send for Buffer` y `unsafe impl Sync for Buffer` están documentados
  con comentarios `// SAFETY:` que justifican por qué son correctos.
- [ ] Todos los bloques `unsafe` tienen comentario `// SAFETY:` explicando por qué son
  seguros en ese contexto.
- [ ] `cargo test` pasa los 7 tests del wrapper.
- [ ] Opcional: `cargo +nightly miri test` pasa (para la parte Rust; no puede ejecutar
  el C real).

!!! abstract "Siguiente paso"

    Semana 15 — [WebAssembly: Rust en el navegador](section_03.md).
