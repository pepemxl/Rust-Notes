# FFI & `unsafe`: puente seguro a C

La Semana 14 baja al nivel más cercano al hardware que ofrece Rust sin salir del
lenguaje: la interfaz con código C. FFI (Foreign Function Interface) es el mecanismo
que permite llamar funciones de bibliotecas escritas en C desde Rust, y exponer
funciones Rust para que las consuma C o cualquier lenguaje que hable ABI de C.

En esta sección aprenderemos:

- Qué significa `unsafe` exactamente: qué invariantes se ceden al compilador y cuáles
  asume el programador.
- Las cinco reglas del Rustonomicon que nunca se pueden violar.
- `extern "C"`, `#[no_mangle]` y `#[repr(C)]`: los tres ingredientes de FFI.
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

> 💡 **Filosofía de la Semana 14:** *`unsafe` no significa "código peligroso sin
> revisar" — significa "código cuya corrección no puede verificar el compilador". Es
> tu responsabilidad documentar y hacer cumplir los invariantes que el compilador ya no
> puede. Escribe el menor bloque `unsafe` posible y envuélvelo en una API que haga
> imposible violarlo.*

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
    *ptr   // unsafe porque desreferenciamos un puntero raw
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
// 1. #[no_mangle]: desactiva name-mangling de Rust
// 2. extern "C": usa la calling convention de C (registros, stack order)
// 3. pub: visible para el linker

#[no_mangle]
pub extern "C" fn sumar(a: i32, b: i32) -> i32 {
    a + b
}

// En C:
// extern int sumar(int a, int b);
// int resultado = sumar(3, 4);  // 7
```

Sin `#[no_mangle]`, el compilador genera nombres como `_ZN7mytool5sumar17h3a1b2c3d4e5f6g7hE`
que C no puede usar. Sin `extern "C"`, la calling convention puede diferir y los
argumentos llegan en el orden o registros equivocados.

### Llamar a C desde Rust

```rust
// Declarar funciones C que queremos usar
extern "C" {
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

// En Rust, representamos un tipo opaco con un enum vacío:
#[repr(C)]
pub enum BufferOpaco {}   // no se puede instanciar, solo usar como *mut BufferOpaco

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
    CStr::from_ptr(ptr)
        .to_str()
        .expect("la string de C no es UTF-8 válido")
}

// Si C puede devolver strings no-UTF-8:
unsafe fn c_a_string_lossy(ptr: *const c_char) -> String {
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
    // Reemplaza bytes inválidos con U+FFFD (el símbolo de reemplazo)
}

extern "C" { fn funcion_c_que_usa_char_ptr(s: *const c_char); }
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

extern "C" { fn funcion_c_que_usa_char_ptr(s: *const c_char); }
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
        drop(Box::from_raw(ptr));   // Drop se ejecuta, memoria liberada
    }
}

// API pública para que C llame:
#[no_mangle]
pub extern "C" fn mi_struct_crear() -> *mut MiStruct {
    Box::into_raw(Box::new(MiStruct::new()))
}

#[no_mangle]
pub unsafe extern "C" fn mi_struct_liberar(ptr: *mut MiStruct) {
    if !ptr.is_null() { drop(Box::from_raw(ptr)); }
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

extern "C" {
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
bindgen = "0.70"
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
#ifndef BUFFER_H
#define BUFFER_H

#include <stddef.h>
#include <stdint.h>

/* Códigos de error */
#define BUF_OK          0
#define BUF_ERR_NULL   -1
#define BUF_ERR_RANGE  -2
#define BUF_ERR_ALLOC  -3

/* Handle opaco — los usuarios nunca ven la estructura interna */
typedef struct BufHandle BufHandle;

/* Crear un buffer de `len` bytes inicializados a cero.
 * Devuelve NULL si falla la asignación de memoria. */
BufHandle* buf_crear(size_t len);

/* Liberar un buffer. Es seguro pasar NULL. */
void buf_liberar(BufHandle* b);

/* Longitud del buffer en bytes. */
size_t buf_len(const BufHandle* b);

/* Escribir `n` bytes de `data` en posición `offset`.
 * Devuelve BUF_OK o código de error. */
int buf_escribir(BufHandle* b, size_t offset, const uint8_t* data, size_t n);

/* Leer `n` bytes desde posición `offset` hacia `dest`.
 * Devuelve BUF_OK o código de error. */
int buf_leer(const BufHandle* b, size_t offset, uint8_t* dest, size_t n);

/* Aplicar XOR con `clave` a todo el buffer (cifrado/descifrado simétrico).
 * Devuelve BUF_OK o código de error. */
int buf_xor(BufHandle* b, uint8_t clave);

/* Borrar el contenido del buffer (poner a cero).
 * Devuelve BUF_OK o BUF_ERR_NULL. */
int buf_limpiar(BufHandle* b);

#endif /* BUFFER_H */
```

`csrc/buffer.c`:

```c
#include "buffer.h"
#include <stdlib.h>
#include <string.h>

struct BufHandle {
    uint8_t* data;
    size_t   len;
};

BufHandle* buf_crear(size_t len) {
    BufHandle* b = (BufHandle*)malloc(sizeof(BufHandle));
    if (!b) return NULL;
    b->data = (uint8_t*)calloc(len, 1);  /* inicializado a cero */
    if (!b->data) { free(b); return NULL; }
    b->len = len;
    return b;
}

void buf_liberar(BufHandle* b) {
    if (!b) return;
    /* Borrar memoria antes de liberar (evitar que secretos queden en heap) */
    memset(b->data, 0, b->len);
    free(b->data);
    free(b);
}

size_t buf_len(const BufHandle* b) {
    if (!b) return 0;
    return b->len;
}

int buf_escribir(BufHandle* b, size_t offset, const uint8_t* data, size_t n) {
    if (!b || !data) return BUF_ERR_NULL;
    if (offset + n > b->len) return BUF_ERR_RANGE;
    memcpy(b->data + offset, data, n);
    return BUF_OK;
}

int buf_leer(const BufHandle* b, size_t offset, uint8_t* dest, size_t n) {
    if (!b || !dest) return BUF_ERR_NULL;
    if (offset + n > b->len) return BUF_ERR_RANGE;
    memcpy(dest, b->data + offset, n);
    return BUF_OK;
}

int buf_xor(BufHandle* b, uint8_t clave) {
    if (!b) return BUF_ERR_NULL;
    for (size_t i = 0; i < b->len; i++) b->data[i] ^= clave;
    return BUF_OK;
}

int buf_limpiar(BufHandle* b) {
    if (!b) return BUF_ERR_NULL;
    memset(b->data, 0, b->len);
    return BUF_OK;
}
```

### `src/raw.rs`: bindings manuales

En proyectos reales estos los genera `bindgen`; aquí los escribimos a mano para ver
exactamente lo que genera:

```rust
//! Bindings crudos de la biblioteca C `buffer`.
//! Código generado (en proyecto real: include!(concat!(env!("OUT_DIR"), "/bindings.rs")))

use std::os::raw::{c_int, c_uchar};

/// Handle opaco — nunca instanciar directamente
#[repr(C)]
pub struct BufHandle {
    _privado: [u8; 0],  // campo de tamaño cero: hace la struct no-instanciable
}

pub const BUF_OK: c_int        =  0;
pub const BUF_ERR_NULL: c_int  = -1;
pub const BUF_ERR_RANGE: c_int = -2;
pub const BUF_ERR_ALLOC: c_int = -3;

extern "C" {
    pub fn buf_crear(len: usize) -> *mut BufHandle;
    pub fn buf_liberar(b: *mut BufHandle);
    pub fn buf_len(b: *const BufHandle) -> usize;
    pub fn buf_escribir(b: *mut BufHandle, offset: usize, data: *const c_uchar, n: usize) -> c_int;
    pub fn buf_leer(b: *const BufHandle, offset: usize, dest: *mut c_uchar, n: usize) -> c_int;
    pub fn buf_xor(b: *mut BufHandle, clave: c_uchar) -> c_int;
    pub fn buf_limpiar(b: *mut BufHandle) -> c_int;
}
```

### `src/error.rs`: tipos de error propios

```rust
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ErrorBuffer {
    #[error("asignación de memoria fallida en C")]
    AsignacionFallida,

    #[error("offset {offset} + longitud {longitud} supera el tamaño del buffer ({tam})")]
    FueraDeRango { offset: usize, longitud: usize, tam: usize },

    #[error("puntero nulo inesperado (error interno)")]
    PunteroNulo,

    #[error("código de error C desconocido: {0}")]
    CodigoDesconocido(i32),
}

pub(crate) fn verificar_codigo(rc: i32, offset: usize, n: usize, tam: usize) -> Result<(), ErrorBuffer> {
    use crate::raw::*;
    match rc {
        r if r == BUF_OK        => Ok(()),
        r if r == BUF_ERR_NULL  => Err(ErrorBuffer::PunteroNulo),
        r if r == BUF_ERR_RANGE => Err(ErrorBuffer::FueraDeRango { offset, longitud: n, tam }),
        r if r == BUF_ERR_ALLOC => Err(ErrorBuffer::AsignacionFallida),
        r => Err(ErrorBuffer::CodigoDesconocido(r)),
    }
}
```

### `src/safe.rs`: el wrapper seguro

Esta es la pieza central de la semana: una API que es imposible de usar mal.

```rust
//! API Rust completamente segura sobre la biblioteca C `buffer`.
//!
//! Invariante de seguridad: `ptr` siempre es no nulo y apunta a un `BufHandle`
//! válido, creado por `buf_crear` y aún no liberado. Se mantiene automáticamente
//! por el constructor (`Buffer::nuevo`) y `impl Drop`.

use std::ptr::NonNull;

use crate::{
    error::{verificar_codigo, ErrorBuffer},
    raw,
};

/// Buffer de bytes con cifrado XOR, gestionado por la biblioteca C.
///
/// # Garantías
/// - La memoria se libera y se borra al hacer `drop`.
/// - Todas las operaciones verifican bounds antes de tocar memoria C.
/// - Imposible crear un `Buffer` con puntero nulo (usa `NonNull`).
pub struct Buffer {
    /// Invariante: siempre válido, no nulo, creado por buf_crear, no liberado.
    ptr: NonNull<raw::BufHandle>,
}

// SAFETY: BufHandle no tiene referencias a datos de hilos; la exclusión
// mutua es responsabilidad del llamador (como con Vec<T>).
unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    /// Crea un nuevo buffer de `longitud` bytes, inicializado a cero.
    pub fn nuevo(longitud: usize) -> Result<Self, ErrorBuffer> {
        // SAFETY: buf_crear devuelve NULL en fallo o un puntero válido.
        let ptr = unsafe { raw::buf_crear(longitud) };

        NonNull::new(ptr)
            .map(|p| Buffer { ptr: p })
            .ok_or(ErrorBuffer::AsignacionFallida)
    }

    /// Longitud del buffer en bytes.
    pub fn len(&self) -> usize {
        // SAFETY: ptr es válido por el invariante del tipo.
        unsafe { raw::buf_len(self.ptr.as_ptr()) }
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Escribe `datos` en posición `offset`.
    ///
    /// # Errors
    /// Devuelve `FueraDeRango` si `offset + datos.len() > self.len()`.
    pub fn escribir(&mut self, offset: usize, datos: &[u8]) -> Result<(), ErrorBuffer> {
        let rc = unsafe {
            raw::buf_escribir(
                self.ptr.as_ptr(),
                offset,
                datos.as_ptr(),
                datos.len(),
            )
        };
        verificar_codigo(rc, offset, datos.len(), self.len())
    }

    /// Lee `n` bytes desde posición `offset`.
    ///
    /// # Errors
    /// Devuelve `FueraDeRango` si `offset + n > self.len()`.
    pub fn leer(&self, offset: usize, n: usize) -> Result<Vec<u8>, ErrorBuffer> {
        let mut dest = vec![0u8; n];
        let rc = unsafe {
            raw::buf_leer(self.ptr.as_ptr(), offset, dest.as_mut_ptr(), n)
        };
        verificar_codigo(rc, offset, n, self.len())?;
        Ok(dest)
    }

    /// Aplica XOR con `clave` a todo el buffer.
    /// Llamar dos veces con la misma clave restaura el original (cifrado simétrico).
    pub fn aplicar_xor(&mut self, clave: u8) -> Result<(), ErrorBuffer> {
        let rc = unsafe { raw::buf_xor(self.ptr.as_ptr(), clave) };
        verificar_codigo(rc, 0, 0, self.len())
    }

    /// Borra todos los bytes del buffer (pone a cero).
    pub fn limpiar(&mut self) -> Result<(), ErrorBuffer> {
        let rc = unsafe { raw::buf_limpiar(self.ptr.as_ptr()) };
        verificar_codigo(rc, 0, 0, self.len())
    }

    /// Devuelve una copia de todos los bytes del buffer.
    pub fn contenido(&self) -> Result<Vec<u8>, ErrorBuffer> {
        self.leer(0, self.len())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: ptr es válido (invariante del tipo) y esta es la única
        // vez que se libera (Drop se llama exactamente una vez por objeto).
        unsafe { raw::buf_liberar(self.ptr.as_ptr()) }
        // buf_liberar internamente borra la memoria con memset antes de free.
    }
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Buffer({} bytes)", self.len())
    }
}
```

### `src/main.rs`: demostración y tests

```rust
mod error;
mod raw;
mod safe;

use safe::Buffer;

fn main() -> Result<(), error::ErrorBuffer> {
    println!("=== Demo: Buffer seguro sobre C ===\n");

    // Crear buffer de 16 bytes
    let mut buf = Buffer::nuevo(16)?;
    println!("Creado: {buf:?}");

    // Escribir un mensaje
    let mensaje = b"Hola desde Rust!";
    buf.escribir(0, mensaje)?;
    println!("Escrito: {:?}", buf.contenido()?);

    // Cifrar con XOR
    buf.aplicar_xor(0x42)?;
    println!("Cifrado (XOR 0x42): {:?}", buf.contenido()?);

    // Descifrar (XOR es simétrico)
    buf.aplicar_xor(0x42)?;
    let descifrado = buf.contenido()?;
    println!("Descifrado: {:?}", String::from_utf8_lossy(&descifrado));

    // Error controlado: fuera de rango
    match buf.escribir(10, b"texto demasiado largo") {
        Ok(_) => panic!("debería haber fallado"),
        Err(e) => println!("\nError esperado: {e}"),
    }

    // buf se libera aquí (Drop → buf_liberar → memset + free)
    println!("\n✓ Buffer liberado automáticamente");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::safe::Buffer;

    #[test]
    fn crear_y_leer_cero() {
        let buf = Buffer::nuevo(8).unwrap();
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.contenido().unwrap(), vec![0u8; 8]);
    }

    #[test]
    fn escribir_y_leer() {
        let mut buf = Buffer::nuevo(16).unwrap();
        buf.escribir(0, b"test").unwrap();
        assert_eq!(&buf.leer(0, 4).unwrap(), b"test");
        assert_eq!(buf.leer(4, 1).unwrap(), vec![0u8]);
    }

    #[test]
    fn escritura_fuera_de_rango() {
        let mut buf = Buffer::nuevo(4).unwrap();
        let err = buf.escribir(2, b"largo").unwrap_err();
        assert!(matches!(err, crate::error::ErrorBuffer::FueraDeRango { .. }));
    }

    #[test]
    fn xor_simetrico() {
        let datos = b"secreto";
        let mut buf = Buffer::nuevo(datos.len()).unwrap();
        buf.escribir(0, datos).unwrap();

        buf.aplicar_xor(0x5A).unwrap();
        let cifrado = buf.contenido().unwrap();
        assert_ne!(cifrado, datos);

        buf.aplicar_xor(0x5A).unwrap();
        assert_eq!(buf.contenido().unwrap(), datos);
    }

    #[test]
    fn limpiar_borra_contenido() {
        let mut buf = Buffer::nuevo(8).unwrap();
        buf.escribir(0, b"datos").unwrap();
        buf.limpiar().unwrap();
        assert_eq!(buf.contenido().unwrap(), vec![0u8; 8]);
    }

    #[test]
    fn drop_no_doble_free() {
        // Simplemente crea y deja que Drop lo limpie.
        // Si hubiera doble-free, el OS o Miri lo detectarían.
        let mut buf = Buffer::nuevo(32).unwrap();
        buf.escribir(0, &[42u8; 32]).unwrap();
        // drop implícito aquí
    }

    #[test]
    fn buffer_vacio_falla() {
        // C permite buf_crear(0) pero es un caso borde
        let buf = Buffer::nuevo(0).unwrap();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }
}
```

### `Cargo.toml` completo del proyecto

```toml
[package]
name    = "safe_ffi"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "2"

[build-dependencies]
cc      = "1"
# bindgen = "0.70"   # descomentar cuando uses headers C externos
```

`build.rs`:

```rust
fn main() {
    println!("cargo:rerun-if-changed=csrc/buffer.h");
    println!("cargo:rerun-if-changed=csrc/buffer.c");

    cc::Build::new()
        .file("csrc/buffer.c")
        .include("csrc")
        .flag_if_supported("-O2")
        .compile("buffer");
}
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

extern "C" {
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
    Buffer { ptr: NonNull::new_unchecked(ptr) }
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

> **Siguiente paso:** Semana 15 — [WebAssembly: Rust en el navegador](section_03.md).
