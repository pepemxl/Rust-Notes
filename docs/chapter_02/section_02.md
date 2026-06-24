# Traits Avanzados: la librería estándar de tus tipos

La Semana 6 convierte tus tipos en **ciudadanos de primera clase del ecosistema Rust**.
Aprenderás a elegir entre dispatch estático y dinámico, a implementar las conversiones
que hacen ergonómico tu código, a crear smart pointers con `Deref`, a liberar recursos
correctamente con `Drop` y a dar a tus tipos el comportamiento de los operadores de Rust.

En esta sección aprenderemos:

- **`dyn Trait`** (dispatch dinámico con VTable) vs generics/`impl Trait` (dispatch
  estático con monomorphization): cuándo usar cada uno.
- Los **traits de conversión** del estándar: `From`/`Into`, `TryFrom`/`TryInto`,
  `AsRef`/`AsMut`, `Borrow`.
- **`Deref`/`DerefMut`**: cómo funcionan las coerciones automáticas y el patrón newtype.
- **`Drop`**: el destructor de Rust y el patrón RAII.
- **Sobrecarga de operadores** con `std::ops`.
- El ejercicio integrador `ByteBuffer`.

> 💡 **Filosofía de la Semana 6:** *Implementar los traits correctos no es azúcar
> sintáctica — es lo que hace que tu tipo encaje en el ecosistema. Un tipo con `From`,
> `Deref` y `Display` bien implementados se usa igual que los tipos de la std.*

---

## Dispatch estático vs dinámico

Cuando llamas a un método de un trait, Rust necesita saber **qué función ejecutar
exactamente**. Hay dos mecanismos para resolverlo, con compromisos muy distintos.

### Dispatch estático: generics e `impl Trait`

El compilador sabe en compile-time exactamente qué tipo concreto usa la función.
Genera una copia especializada por cada tipo:

```rust
trait Saludar {
    fn saludo(&self) -> String;
}

struct Español;
struct Inglés;

impl Saludar for Español { fn saludo(&self) -> String { "Hola".into() } }
impl Saludar for Inglés  { fn saludo(&self) -> String { "Hello".into() } }

// Dispatch estático: el compilador genera presentar::<Español> y presentar::<Inglés>
fn presentar<T: Saludar>(quien: &T) {
    println!("{}", quien.saludo());
}
// Equivalente con impl Trait:
fn presentar2(quien: &impl Saludar) {
    println!("{}", quien.saludo());
}

fn main() {
    presentar(&Español);   // llama a Español::saludo — inlineado, sin indirección
    presentar(&Inglés);    // llama a Inglés::saludo — inlineado, sin indirección
}
```

El compilador puede **inlinear** la llamada, eliminar ramas muertas y optimizar
agresivamente. Cero coste en runtime. El precio: más tiempo de compilación y binario
más grande.

### Dispatch dinámico: `dyn Trait`

Con `dyn Trait` la decisión de qué función llamar se toma en **tiempo de ejecución**
a través de una *vtable* (tabla de punteros a funciones virtuales):

```rust
fn presentar_dyn(quien: &dyn Saludar) {
    println!("{}", quien.saludo());  // indirección a través de vtable en runtime
}

fn main() {
    let hablante: &dyn Saludar = &Español;  // fat pointer: (puntero al dato, puntero a vtable)
    presentar_dyn(hablante);
    presentar_dyn(&Inglés);
}
```

```text
Fat pointer para &dyn Saludar:
┌──────────────────────┐
│  ptr ──────────────────▶  dato (Español o Inglés)
│  vtable_ptr ───────────▶  vtable de Español (o Inglés)
└──────────────────────┘       ├── drop_fn
                               ├── size / align
                               └── saludo_fn ──▶ Español::saludo
```

El `dyn Trait` es **Unsized**: su tamaño se desconoce en compile-time. Por eso siempre
va detrás de algún puntero: `&dyn Trait`, `Box<dyn Trait>`, `Arc<dyn Trait>`.

### La ventaja clave de `dyn Trait`: colecciones heterogéneas

Con generics solo puedes tener un tipo concreto en cada colección. Con `dyn Trait`
puedes mezclar cualquier tipo que implemente el trait:

```rust
fn main() {
    // Con generics: Vec<T> — todos del mismo tipo T
    let homogeneo: Vec<Box<dyn Saludar>> = vec![
        Box::new(Español),
        Box::new(Inglés),    // ✅ tipos distintos en la misma Vec
        Box::new(Español),
    ];

    for hablante in &homogeneo {
        println!("{}", hablante.saludo());
    }
}
```

Otro caso típico: devolver uno de varios tipos según la lógica en runtime:

```rust
fn crear_saludo(idioma: &str) -> Box<dyn Saludar> {
    match idioma {
        "es" => Box::new(Español),
        _    => Box::new(Inglés),   // tipos distintos en ramas → Box<dyn Trait>
    }
}
```

### Object safety: qué traits pueden ser `dyn`

No todos los traits pueden usarse como `dyn Trait`. El compilador requiere que el trait
sea **object-safe**. Las reglas principales:

| Regla | Motivo |
| :--- | :--- |
| No métodos con tipo de retorno `Self` | El tamaño de `Self` se desconoce |
| No métodos genéricos (`fn foo<T>`) | No se puede construir la vtable para todos los T |
| No constantes asociadas (generalmente) | No va en la vtable |
| `Self` solo puede aparecer en el receptor (`&self`, `&mut self`, `Box<Self>`) | Por lo anterior |

```rust
trait ObjectSafe {
    fn metodo(&self) -> String;          // ✅ ok
    fn consume(self: Box<Self>);         // ✅ ok — Box<Self> en receiver
}

trait NoObjectSafe {
    fn clonar(&self) -> Self;            // ❌ retorna Self (tamaño desconocido)
    fn comparar<T: PartialOrd>(&self, otro: T); // ❌ método genérico
}

// Si necesitas Clone + dyn Trait: patrón clone_box
trait MiTrait: std::fmt::Debug {
    fn clonar_caja(&self) -> Box<dyn MiTrait>;
}
```

### Tabla comparativa definitiva

| Característica | Generics `<T: Trait>` / `impl Trait` | `dyn Trait` |
| :--- | :--- | :--- |
| Dispatch | Estático (compilación) | Dinámico (runtime, VTable) |
| Coste en runtime | **Cero** | Indirección de puntero |
| Tamaño | Conocido en compilación (`Sized`) | Desconocido (`?Sized`) |
| Requiere puntero | No | Sí (`Box`, `&`, `Arc`) |
| Colecciones heterogéneas | No | **Sí** |
| Tipos distintos en ramas `if` | No | **Sí** |
| Object safety | No requerida | **Requerida** |
| Code bloat | Posible (muchas instancias) | No (una sola función) |

**Regla de decisión:**

- ¿Sabes el tipo en compile-time y el rendimiento importa? → **Generics / `impl Trait`**
- ¿Necesitas mezclar tipos distintos en runtime, o la función se llama desde muchos
  tipos distintos y el tamaño del binario importa? → **`dyn Trait`**

---

## Traits de conversión

Estos traits son el vocabulario de las conversiones entre tipos en Rust. Implementarlos
hace que tu tipo funcione con la misma ergonomía que los tipos de la std.

### `From` e `Into`: conversiones infalibles

```rust
// Implementa From<T> para tu tipo.
// Into<U> para T se obtiene gratis por blanket impl de la std:
// impl<T, U> Into<U> for T where U: From<T>

struct Metros(f64);
struct Centimetros(f64);

impl From<Metros> for Centimetros {
    fn from(m: Metros) -> Self {
        Centimetros(m.0 * 100.0)
    }
}

fn main() {
    let m = Metros(1.5);

    // Usando From explícitamente:
    let cm = Centimetros::from(Metros(2.0));
    println!("{} cm", cm.0);           // 200

    // Usando Into (gratuito por el From que implementamos):
    let cm2: Centimetros = m.into();
    println!("{} cm", cm2.0);          // 150

    // Conversiones que ya vienen en std:
    let s: String = String::from("hola");
    let s2: String = "hola".into();    // &str implementa Into<String>
    let n: i64 = 42_i32.into();        // i32 implementa Into<i64>
}
```

**Regla de oro**: implementa siempre `From`, nunca `Into` directamente. El blanket impl
de la std provee `Into` gratis y además el operador `?` usa `From` para convertir errores.

### `TryFrom` y `TryInto`: conversiones fallibles

Cuando la conversión puede fallar (pérdida de precisión, valor fuera de rango, etc.):

```rust
use std::convert::TryFrom;

#[derive(Debug)]
struct Porcentaje(u8);   // solo valores 0–100

#[derive(Debug)]
struct ErrorPorcentaje(i32);

impl std::fmt::Display for ErrorPorcentaje {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} no es un porcentaje válido (0-100)", self.0)
    }
}

impl TryFrom<i32> for Porcentaje {
    type Error = ErrorPorcentaje;

    fn try_from(valor: i32) -> Result<Self, Self::Error> {
        if (0..=100).contains(&valor) {
            Ok(Porcentaje(valor as u8))
        } else {
            Err(ErrorPorcentaje(valor))
        }
    }
}

fn main() {
    let ok = Porcentaje::try_from(75);
    println!("{ok:?}");                     // Ok(Porcentaje(75))

    let err = Porcentaje::try_from(150);
    println!("{err:?}");                    // Err(ErrorPorcentaje(150))

    // TryInto es gratuito por el TryFrom:
    let p: Result<Porcentaje, _> = 50_i32.try_into();
    println!("{p:?}");                      // Ok(Porcentaje(50))

    // Conversiones de la std:
    let grande: i64 = 1_000_000;
    let pequeño: Result<i32, _> = i32::try_from(grande);
    println!("{pequeño:?}");                // Ok(1000000) — cabe en i32

    let demasiado: i64 = i64::MAX;
    let falla: Result<i32, _> = i32::try_from(demasiado);
    println!("{falla:?}");                  // Err(TryFromIntError(()))
}
```

### `AsRef` y `AsMut`: referencias baratas y genéricas

Permiten que una función acepte **cualquier tipo que pueda dar una referencia barata**
al tipo destino:

```rust
// Sin AsRef: solo acepta &str
fn contar_bytes_mal(s: &str) -> usize { s.len() }

// Con AsRef<str>: acepta &str, &String, String, Box<str>, Cow<str>...
fn contar_bytes(s: impl AsRef<str>) -> usize {
    s.as_ref().len()    // as_ref() convierte a &str
}

fn main() {
    let owned = String::from("hola");
    let literal = "mundo";

    println!("{}", contar_bytes(literal));      // 5
    println!("{}", contar_bytes(&owned));       // 4
    println!("{}", contar_bytes(owned));        // 4 — consume la String, ok
    println!("{}", contar_bytes(String::from("rust"))); // 4
}
```

Para rutas del sistema de archivos, el patrón es habitual en la std:

```rust
use std::path::Path;

fn leer_archivo(ruta: impl AsRef<std::path::Path>) -> std::io::Result<String> {
    std::fs::read_to_string(ruta.as_ref())
    // Acepta: &str, &String, String, &Path, PathBuf, &PathBuf...
}
```

`AsMut` es la contraparte mutable:

```rust
fn rellenar_ceros(buf: &mut impl AsMut<[u8]>) {
    for byte in buf.as_mut() {
        *byte = 0;
    }
}
```

**Cuándo implementar `AsRef<T>` para tu tipo**: cuando tu tipo "contiene" o "es"
conceptualmente un `T` y la conversión es gratuita (sin copia, sin fallo).

### `Borrow` y `BorrowMut`

`Borrow<T>` es similar a `AsRef<T>` pero con una garantía adicional: los
traits `Hash`, `Eq` y `Ord` deben comportarse igual para `T` y para el tipo que
implementa `Borrow<T>`. Esto es crítico para las claves de `HashMap` y `HashSet`.

```rust
use std::collections::HashMap;
use std::borrow::Borrow;

fn main() {
    let mut mapa: HashMap<String, i32> = HashMap::new();
    mapa.insert(String::from("clave"), 42);

    // get() acepta cualquier &Q donde String: Borrow<Q> y Q: Hash + Eq
    // String implementa Borrow<str>, así que podemos buscar con &str:
    let valor = mapa.get("clave");      // ✅ no necesita String::from("clave")
    println!("{valor:?}");              // Some(42)
}
```

**Diferencia práctica entre `AsRef` y `Borrow`:**

| | `AsRef<T>` | `Borrow<T>` |
| :--- | :--- | :--- |
| Propósito | Conversión genérica de referencia | Claves de colecciones |
| Garantía extra | Ninguna | `Hash`/`Eq`/`Ord` consistentes |
| Cuándo implementar | API genérica que solo necesita `&T` | Keys de `HashMap`/`HashSet` |

---

## `Deref` y `DerefMut`: coerciones automáticas

`Deref` permite que `&MiTipo` se convierta automáticamente en `&Target` en ciertos
contextos. Es la magia detrás de cómo `String` se usa donde se espera `&str`, o cómo
`Vec<T>` se usa donde se espera `&[T]`.

### Cómo funciona la coerción automática

El compilador aplica `Deref` automáticamente en tres situaciones:

1. Al pasar `&T` donde se espera `&U` y `T: Deref<Target=U>`.
2. Al llamar a un método que no existe en `T` pero sí en `U`.
3. En cadena: `Box<String>` → `String` → `str` (el compilador aplica tantos `Deref`
   como haga falta).

```text
Cadena de Deref automática de la std:
Box<String>  --Deref-->  String  --Deref-->  str
Vec<u8>      --Deref-->  [u8]
Rc<String>   --Deref-->  String  --Deref-->  str
Arc<Vec<u8>> --Deref-->  Vec<u8> --Deref-->  [u8]
```

```rust
fn imprime_str(s: &str) { println!("{s}"); }
fn imprime_slice(b: &[u8]) { println!("{b:?}"); }

fn main() {
    let owned: String = String::from("hola");
    let boxed: Box<String> = Box::new(String::from("mundo"));
    let vec: Vec<u8> = vec![1, 2, 3];

    imprime_str(&owned);      // &String → &str (1 Deref)
    imprime_str(&boxed);      // &Box<String> → &String → &str (2 Deref)
    imprime_slice(&vec);      // &Vec<u8> → &[u8] (1 Deref)
}
```

### Implementar `Deref`: el patrón newtype

El caso de uso más común para implementar `Deref` tú mismo es el **patrón newtype**:
envolver un tipo existente para darle un nombre con semántica más clara, heredando
todos sus métodos:

```rust
use std::ops::{Deref, DerefMut};

/// Newtype sobre Vec<String> que garantiza que todos los elementos son no vacíos.
struct ListaNoVacia(Vec<String>);

impl ListaNoVacia {
    fn nueva() -> Self { Self(Vec::new()) }

    fn agregar(&mut self, s: String) -> Result<(), &'static str> {
        if s.is_empty() {
            Err("no se permiten cadenas vacías")
        } else {
            self.0.push(s);
            Ok(())
        }
    }
}

impl Deref for ListaNoVacia {
    type Target = [String];          // Target = slice, no Vec, para no exponer push()

    fn deref(&self) -> &[String] {
        &self.0
    }
}

fn main() {
    let mut lista = ListaNoVacia::nueva();
    lista.agregar(String::from("uno")).unwrap();
    lista.agregar(String::from("dos")).unwrap();
    println!("{}", lista.len());      // ✅ len() viene de Deref a [String]
    println!("{:?}", lista[0]);       // ✅ indexación viene de Deref

    // lista.push(...)                // ❌ push no disponible: Target es [String], no Vec
    // lista.agregar(String::new())   // Err: cadena vacía
}
```

**Reglas de oro para `Deref`:**

- `Deref` debe ser **infalible** — nunca puede devolver `Result` ni hacer `panic`.
- `Deref` debe ser **barato** — idealmente un simple acceso a campo.
- Implementa `Deref` para smart pointers y newtypes. **No** para conversiones con lógica.
- Expón `Target = [T]` (slice) en lugar de `Vec<T>` cuando quieres dar acceso de
  lectura pero no exponer métodos mutantes.

### `DerefMut`

Igual que `Deref` pero para referencias mutables:

```rust
impl DerefMut for ListaNoVacia {
    fn deref_mut(&mut self) -> &mut [String] {
        &mut self.0
    }
}

fn main() {
    let mut lista = ListaNoVacia::nueva();
    lista.agregar(String::from("hola")).unwrap();
    lista[0] = String::from("mundo");   // ✅ acceso mutable vía DerefMut
    println!("{}", lista[0]);
}
```

---

## `Drop`: el destructor de Rust

`Drop` te permite ejecutar código cuando un valor sale de scope. Es la base del
patrón **RAII** (Resource Acquisition Is Initialization): los recursos se liberan
automáticamente al destruir el objeto que los posee.

```rust
struct ConexionBD {
    host: String,
}

impl ConexionBD {
    fn nueva(host: &str) -> Self {
        println!("Conectando a {host}...");
        Self { host: host.to_string() }
    }

    fn consultar(&self, sql: &str) -> Vec<String> {
        println!("[{}] ejecutando: {sql}", self.host);
        vec![]
    }
}

impl Drop for ConexionBD {
    fn drop(&mut self) {
        println!("Cerrando conexión a {}.", self.host);
        // Aquí enviarías el mensaje de cierre, liberarías el socket, etc.
    }
}

fn main() {
    let conn = ConexionBD::nueva("db.ejemplo.com");
    let _rows = conn.consultar("SELECT 1");
    println!("Fin de main");
}   // conn sale de scope → Drop::drop se llama automáticamente
```

Salida:

```
Conectando a db.ejemplo.com...
[db.ejemplo.com] ejecutando: SELECT 1
Fin de main
Cerrando conexión a db.ejemplo.com.
```

### Orden de drop

Los valores se destruyen en **orden inverso de declaración** (LIFO, como el stack):

```rust
struct Rastreado(&'static str);

impl Drop for Rastreado {
    fn drop(&mut self) { println!("drop: {}", self.0); }
}

fn main() {
    let _a = Rastreado("a");   // se crea primero
    let _b = Rastreado("b");
    let _c = Rastreado("c");   // se crea último
    println!("antes del fin de scope");
}
// Salida: antes del fin de scope / drop: c / drop: b / drop: a
```

Los campos de un struct se destruyen en **orden de declaración** (no inverso).

### `std::mem::drop`: forzar drop temprano

No puedes llamar a `obj.drop()` directamente (el compilador lo prohíbe para evitar
doble free). Si necesitas liberar un recurso antes del fin de scope, usa la función
libre `drop()`:

```rust
fn main() {
    let conn = ConexionBD::nueva("db.ejemplo.com");
    // ... operaciones ...
    drop(conn);    // ✅ libera la conexión aquí, antes del fin de main
    println!("conexión ya cerrada, siguiendo...");
    // usar conn aquí daría error: moved value
}
```

`std::mem::drop` toma `T` por valor, lo que transfiere el ownership y llama a `Drop`
al final de la función `drop`. Es una función de una línea: `fn drop<T>(_: T) {}`.

### `Copy` y `Drop` son mutuamente excluyentes

Un tipo no puede implementar ambos. `Copy` significa "es seguro duplicar bit a bit sin
destructor". Si necesita `Drop`, tiene semántica de ownership y no puede ser `Copy`.

---

## Sobrecarga de operadores con `std::ops`

Puedes dar a tus tipos el comportamiento de los operadores estándar implementando
los traits de `std::ops`.

### `Add`, `Sub`, `Mul`, `Div`

```rust
use std::ops::{Add, Sub, Mul, Neg};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vec2 {
    x: f64,
    y: f64,
}

impl Vec2 {
    fn nuevo(x: f64, y: f64) -> Self { Self { x, y } }
    fn modulo(&self) -> f64 { (self.x * self.x + self.y * self.y).sqrt() }
    fn dot(&self, otro: &Self) -> f64 { self.x * otro.x + self.y * otro.y }
}

// Vec2 + Vec2
impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, otro: Vec2) -> Vec2 {
        Vec2::nuevo(self.x + otro.x, self.y + otro.y)
    }
}

// Vec2 - Vec2
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, otro: Vec2) -> Vec2 {
        Vec2::nuevo(self.x - otro.x, self.y - otro.y)
    }
}

// Vec2 * f64 (escalar)
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, escalar: f64) -> Vec2 {
        Vec2::nuevo(self.x * escalar, self.y * escalar)
    }
}

// -Vec2 (negación unaria)
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::nuevo(-self.x, -self.y)
    }
}

fn main() {
    let a = Vec2::nuevo(1.0, 2.0);
    let b = Vec2::nuevo(3.0, 4.0);

    println!("{:?}", a + b);       // Vec2 { x: 4.0, y: 6.0 }
    println!("{:?}", b - a);       // Vec2 { x: 2.0, y: 2.0 }
    println!("{:?}", a * 3.0);     // Vec2 { x: 3.0, y: 6.0 }
    println!("{:?}", -a);          // Vec2 { x: -1.0, y: -2.0 }
    println!("{:.4}", a.modulo()); // 2.2361
}
```

### `AddAssign` y compañía (`+=`, `-=`, …)

```rust
use std::ops::AddAssign;

impl AddAssign for Vec2 {
    fn add_assign(&mut self, otro: Vec2) {
        self.x += otro.x;
        self.y += otro.y;
    }
}

fn main() {
    let mut v = Vec2::nuevo(1.0, 0.0);
    v += Vec2::nuevo(0.0, 1.0);
    println!("{v:?}");    // Vec2 { x: 1.0, y: 1.0 }
}
```

### `Index` e `IndexMut`: operador `[]`

```rust
use std::ops::{Index, IndexMut};

struct Matriz2x2([f64; 4]);   // almacenada en row-major: [m00, m01, m10, m11]

impl Index<(usize, usize)> for Matriz2x2 {
    type Output = f64;
    fn index(&self, (fila, col): (usize, usize)) -> &f64 {
        &self.0[fila * 2 + col]
    }
}

impl IndexMut<(usize, usize)> for Matriz2x2 {
    fn index_mut(&mut self, (fila, col): (usize, usize)) -> &mut f64 {
        &mut self.0[fila * 2 + col]
    }
}

fn main() {
    let mut m = Matriz2x2([1.0, 2.0, 3.0, 4.0]);
    println!("{}", m[(0, 1)]);    // 2.0
    m[(1, 0)] = 99.0;
    println!("{}", m[(1, 0)]);    // 99.0
}
```

### Tabla de traits de `std::ops`

| Operador | Trait | Método |
| :--- | :--- | :--- |
| `a + b` | `Add<Rhs>` | `fn add(self, rhs: Rhs) -> Output` |
| `a - b` | `Sub<Rhs>` | `fn sub(self, rhs: Rhs) -> Output` |
| `a * b` | `Mul<Rhs>` | `fn mul(self, rhs: Rhs) -> Output` |
| `a / b` | `Div<Rhs>` | `fn div(self, rhs: Rhs) -> Output` |
| `a % b` | `Rem<Rhs>` | `fn rem(self, rhs: Rhs) -> Output` |
| `-a` | `Neg` | `fn neg(self) -> Output` |
| `!a` | `Not` | `fn not(self) -> Output` |
| `a += b` | `AddAssign<Rhs>` | `fn add_assign(&mut self, rhs: Rhs)` |
| `a[i]` | `Index<Idx>` | `fn index(&self, idx: Idx) -> &Output` |
| `a[i] = v` | `IndexMut<Idx>` | `fn index_mut(&mut self, idx: Idx) -> &mut Output` |
| `*a` | `Deref` | `fn deref(&self) -> &Target` |
| `*a = v` | `DerefMut` | `fn deref_mut(&mut self) -> &mut Target` |

**Nota sobre el tipo de retorno `Output`**: todos los traits aritméticos tienen un
tipo asociado `Output` que puede ser distinto del tipo del receptor. Por ejemplo,
`Vec2 * f64 -> Vec2` donde `Rhs = f64` y `Output = Vec2`.

### Convención: implementa para referencias también

Por defecto, los operadores consumen los operandos (toman ownership). Para tipos que
no son `Copy` conviene implementar también la versión para referencias:

```rust
// Además de Add for Vec2 (consume), implementar Add for &Vec2 (presta):
impl Add for &Vec2 {
    type Output = Vec2;
    fn add(self, otro: &Vec2) -> Vec2 {
        Vec2::nuevo(self.x + otro.x, self.y + otro.y)
    }
}

fn main() {
    let a = Vec2::nuevo(1.0, 2.0);
    let b = Vec2::nuevo(3.0, 4.0);
    let c = &a + &b;    // no consume a ni b
    println!("{c:?}");
    println!("{a:?}");  // a sigue válido
}
```

---

## Ejercicio: `ByteBuffer`

Este ejercicio integra todo lo de la semana: `Deref`/`DerefMut`, `From`, `AsRef`/`AsMut`,
`Drop`, y los traits de I/O de la std.

Crea un proyecto con `cargo new byte_buffer --lib` y escribe en `src/lib.rs`:

```rust
use std::io::{self, Cursor, Read, Write};
use std::ops::{Deref, DerefMut};

/// Buffer de bytes con cursor interno para lectura y escritura.
pub struct ByteBuffer {
    datos: Vec<u8>,
    cursor_lectura: usize,
}

impl ByteBuffer {
    pub fn nuevo() -> Self {
        Self { datos: Vec::new(), cursor_lectura: 0 }
    }

    pub fn con_capacidad(cap: usize) -> Self {
        Self { datos: Vec::with_capacity(cap), cursor_lectura: 0 }
    }

    pub fn en_interno(self) -> Vec<u8> { self.datos }

    pub fn longitud(&self) -> usize { self.datos.len() }

    pub fn esta_vacio(&self) -> bool { self.datos.is_empty() }

    /// Bytes disponibles para leer desde la posición del cursor.
    pub fn disponibles(&self) -> usize {
        self.datos.len().saturating_sub(self.cursor_lectura)
    }

    pub fn reiniciar_cursor(&mut self) { self.cursor_lectura = 0; }
}

// ── Deref: permite usar ByteBuffer como &[u8] ─────────────────────────────

impl Deref for ByteBuffer {
    type Target = [u8];
    fn deref(&self) -> &[u8] { &self.datos }
}

impl DerefMut for ByteBuffer {
    fn deref_mut(&mut self) -> &mut [u8] { &mut self.datos }
}

// ── AsRef / AsMut ─────────────────────────────────────────────────────────

impl AsRef<[u8]> for ByteBuffer {
    fn as_ref(&self) -> &[u8] { &self.datos }
}

impl AsMut<[u8]> for ByteBuffer {
    fn as_mut(&mut self) -> &mut [u8] { &mut self.datos }
}

// ── Write: escribe al final del buffer ────────────────────────────────────

impl Write for ByteBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.datos.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

// ── Read: lee desde el cursor interno ─────────────────────────────────────

impl Read for ByteBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let disponibles = &self.datos[self.cursor_lectura..];
        let n = disponibles.len().min(buf.len());
        buf[..n].copy_from_slice(&disponibles[..n]);
        self.cursor_lectura += n;
        Ok(n)
    }
}

// ── From: conversiones ────────────────────────────────────────────────────

impl From<Vec<u8>> for ByteBuffer {
    fn from(v: Vec<u8>) -> Self {
        Self { datos: v, cursor_lectura: 0 }
    }
}

impl From<&[u8]> for ByteBuffer {
    fn from(s: &[u8]) -> Self {
        Self { datos: s.to_vec(), cursor_lectura: 0 }
    }
}

impl From<ByteBuffer> for Vec<u8> {
    fn from(b: ByteBuffer) -> Vec<u8> { b.datos }
}

impl From<&str> for ByteBuffer {
    fn from(s: &str) -> Self { Self::from(s.as_bytes()) }
}

// ── Drop: demostración de RAII ─────────────────────────────────────────────

impl Drop for ByteBuffer {
    fn drop(&mut self) {
        // En un buffer real aquí limpiaríamos memoria sensible, cerraríamos
        // handles externos, etc. Para la demo solo registramos el evento.
        if !self.datos.is_empty() {
            // Borrar datos sensibles de la memoria antes de liberar
            self.datos.iter_mut().for_each(|b| *b = 0);
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn escribir_y_leer() {
        let mut buf = ByteBuffer::nuevo();
        buf.write_all(b"hola").unwrap();
        buf.write_all(b" mundo").unwrap();

        let mut salida = Vec::new();
        buf.reiniciar_cursor();
        buf.read_to_end(&mut salida).unwrap();

        assert_eq!(salida, b"hola mundo");
    }

    #[test]
    fn deref_a_slice() {
        let mut buf = ByteBuffer::nuevo();
        buf.write_all(b"test").unwrap();

        // Deref coercion: &ByteBuffer → &[u8]
        fn toma_slice(s: &[u8]) -> usize { s.len() }
        assert_eq!(toma_slice(&buf), 4);

        // Métodos de [u8] disponibles directamente
        assert_eq!(buf.len(), 4);
        assert!(buf.starts_with(b"te"));
    }

    #[test]
    fn conversiones_from() {
        let desde_vec: ByteBuffer = vec![1u8, 2, 3].into();
        assert_eq!(desde_vec.len(), 3);

        let desde_slice: ByteBuffer = ByteBuffer::from(b"abc" as &[u8]);
        assert_eq!(desde_slice.len(), 3);

        let desde_str: ByteBuffer = ByteBuffer::from("hello");
        assert_eq!(desde_str.len(), 5);

        let de_vuelta: Vec<u8> = desde_str.into();
        assert_eq!(de_vuelta, b"hello");
    }

    #[test]
    fn asref_en_funcion_generica() {
        fn mostrar(b: impl AsRef<[u8]>) -> String {
            b.as_ref().iter().map(|&x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
        }

        let buf: ByteBuffer = ByteBuffer::from("Hi");
        assert_eq!(mostrar(&buf), "48 69");
        // También funciona con Vec<u8> y &[u8] sin cambiar mostrar()
        assert_eq!(mostrar(vec![0x48u8, 0x69]), "48 69");
        assert_eq!(mostrar(b"Hi" as &[u8]), "48 69");
    }

    #[test]
    fn cursor_parcial() {
        let mut buf: ByteBuffer = ByteBuffer::from("abcdef");

        let mut primeros = [0u8; 3];
        buf.read_exact(&mut primeros).unwrap();
        assert_eq!(&primeros, b"abc");
        assert_eq!(buf.disponibles(), 3);

        let mut resto = [0u8; 3];
        buf.read_exact(&mut resto).unwrap();
        assert_eq!(&resto, b"def");
        assert_eq!(buf.disponibles(), 0);
    }
}
```

Ejecuta:

```bash
cargo test
cargo clippy
```

### Puntos de aprendizaje del ejercicio

- `Deref<Target = [u8]>` expone toda la API de slices (`len`, `starts_with`,
  `iter`, indexación) sin que `ByteBuffer` las implemente una a una.
- `AsRef<[u8]>` hace que `ByteBuffer` sea usable en funciones genéricas junto con
  `Vec<u8>`, `&[u8]` y `Box<[u8]>` sin sobrecarga de código.
- `From` en cuatro direcciones (`Vec<u8>`, `&[u8]`, `&str`, `ByteBuffer → Vec<u8>`)
  más el `Into` gratuito, convirtiendo al tipo en un ciudadano de primera clase.
- `Write` delegado a `Vec::extend_from_slice` y `Read` con cursor interno, mostrando
  que implementar traits de la std hace tu tipo compatible con toda la infraestructura
  de I/O existente.
- `Drop` borrando los bytes para ilustrar limpieza de datos sensibles — un patrón real
  en criptografía (buffers de claves, contraseñas).

---

## ✅ Checklist de la Semana 6

- [ ] Explico la diferencia entre dispatch estático (VTable no, monomorphization sí) y
  dinámico (VTable, `dyn Trait`).
- [ ] Sé cuándo usar `Box<dyn Trait>` (colecciones heterogéneas, ramas `if` con tipos
  distintos) y cuándo usar generics (rendimiento, tipo conocido en compilación).
- [ ] Entiendo qué hace un trait "no object-safe" y puedo identificarlo.
- [ ] Implemento `From<T>` (no `Into`) y entiendo por qué `Into` llega gratis.
- [ ] Uso `TryFrom`/`TryInto` para conversiones que pueden fallar.
- [ ] Uso `impl AsRef<T>` en parámetros de función para máxima flexibilidad.
- [ ] Implemento `Deref` para newtypes con la regla: Target = slice, infalible, barato.
- [ ] Entiendo el patrón RAII con `Drop` y sé usar `std::mem::drop` para drop temprano.
- [ ] Implemento al menos `Add`, `Sub`, `Index` para un tipo propio.
- [ ] El ejercicio `ByteBuffer` compila, todos los tests pasan y `clippy` da 0 warnings.
- [ ] Completo la lectura del Cap. 19 de The Book (Advanced Traits).

> **Siguiente paso:** Semana 7 — [Smart Pointers e Interior Mutability: `Box`, `Rc`,
> `Arc`, `RefCell`, `Mutex`](section_03.md).
