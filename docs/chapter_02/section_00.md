# 🦀 MES 2: PROGRAMACIÓN GENÉRICA, TRAITS Y TESTING — Guía Detallada
> **Filosofía del Mes:** *"Las abstracciones en Rust tienen costo cero en runtime. El costo se paga en compile-time (monomorphization) y en complejidad mental. Aprende a diseñar APIs que el compilador pueda optimizar agresivamente."*
> **Meta:** Dejar de escribir `fn foo(x: String)` y empezar a escribir `fn foo<S: AsRef<str>>(x: S)`. Dominar `dyn Trait` vs `impl Trait`. Publicar tu primera *crate* profesional.

---

## 📅 SEMANA 5: GENERICS, TRAITS BOUNDS Y LIFETIMES (La base de la reutilización)

### 🎯 Conceptos Clave

#### 1. Generics (`<T>`) y Monomorphization
*   **Monomorphization:** El compilador genera una versión concreta de la función/struct por cada tipo usado (`fn foo<i32>`, `foo<String>`). **Zero overhead** vs dynamic dispatch.
*   **Sintaxis:** `fn largest<T: PartialOrd>(list: &[T]) -> &T`.
*   **En Structs/Enums:** `struct Point<T> { x: T, y: T }`. `enum Result<T, E>`.
*   **Múltiples bounds:** `T: Clone + Debug + Send + 'static`.

#### 2. `impl Trait` (Argument Position vs Return Position)
| Posición | Significado | Uso Típico |
| :--- | :--- | :--- |
| **Argumento** `fn foo(x: impl Trait)` | **Azúcar sintáctico** para `fn foo<T: Trait>(x: T)`. *Generics anónimos*. | APIs públicas simples, closures (`FnOnce`), iteradores. |
| **Retorno** `fn foo() -> impl Trait` | **Tipo concreto opaco**. El caller sabe que implementa `Trait`, pero no *cuál*. | Ocultar tipos complejos (iterators, futures), `-> impl Iterator<Item=u32>`. **No se puede devolver tipos distintos en ramas `if/else`** (usa `Box<dyn Trait>` ahí). |

#### 3. Lifetimes (`'a`): El "Borrow Checker" para Referencias en Structs/Generics
*   **Regla Fundamental:** Las referencias **NUNCA** pueden vivir más que el dato al que apuntan (dangling pointer prevention).
*   **Elision Rules (Reglas de elisión - Compiler infiere):**
    1.  Cada parámetro `&` / `&mut` gets su propio lifetime.
    2.  Si hay **exactamente 1** input lifetime -> se asigna a **todos** los output lifetimes.
    3.  Si hay `&self` / `&mut self` -> su lifetime se asigna a todos los outputs.
*   **Cuándo ANOTAR manualmente:**
    *   Structs/Enums con referencias: `struct ImportantExcerpt<'a> { part: &'a str }`.
    *   Múltiples input lifetimes sin relación clara con output.
    *   Métodos donde `self` no es el único input relevante.
*   **`'static`:** Vive toda la ejecución del programa. String literals (`"hello"`), `Box::leak`, globals. **Evita `'static` bounds en generics** a menos que sea necesario (ej. `thread::spawn` requiere `Send + 'static`).

### 🧠 Recurso Visual Obligatorio
> **Jon Gjengset - "Crust of Rust: Lifetimes" (YouTube, ~2h)**
> *Ver secciones: Struct lifetimes, Lifetime elision, `'static`, Variance (covariance/contravariance intro).*

### 📝 Ejercicios Rustlings (Semana 5)
*   `generics/` (structs, enums, methods, bounds).
*   `traits/` (defining, implementing, `impl Trait` syntax, `dyn Trait` preview).
*   `lifetimes/` (functions, structs, `static`, elision).

### 🧪 Ejercicio Práctico: `Cache<K, V>` con TTL y `TimeProvider`
**Objetivo:** Diseño orientado a traits (Dependency Injection), Generics, Lifetimes, Testing de tiempo determinista.

**Especificación (`src/lib.rs`):**
```rust
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

// 1. Trait para abstraer el tiempo (Clave para testing)
pub trait TimeProvider {
    fn now(&self) -> Instant;
}

// 2. Implementación real (Production)
pub struct SystemTime;
impl TimeProvider for SystemTime {
    fn now(&self) -> Instant { Instant::now() }
}

// 3. Entrada de cache con expiración
#[derive(Debug, Clone)]
struct Entry<V> {
    value: V,
    expires_at: Instant,
}

// 4. Cache Genérico
pub struct Cache<K, V, T = SystemTime> 
where 
    K: Eq + Hash + Clone, 
    T: TimeProvider 
{
    map: HashMap<K, Entry<V>>,
    ttl: Duration,
    time: T, // Inyección de dependencia
}

impl<K, V> Cache<K, V, SystemTime> 
where K: Eq + Hash + Clone 
{
    // Constructor conveniente por defecto
    pub fn new(ttl: Duration) -> Self {
        Self { map: HashMap::new(), ttl, time: SystemTime }
    }
}

impl<K, V, T> Cache<K, V, T> 
where 
    K: Eq + Hash + Clone, 
    T: TimeProvider 
{
    // Constructor genérico para tests
    pub fn with_time_provider(ttl: Duration, time: T) -> Self {
        Self { map: HashMap::new(), ttl, time }
    }

    pub fn insert(&mut self, key: K, value: V) {
        let expires_at = self.time.now() + self.ttl;
        self.map.insert(key, Entry { value, expires_at });
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        // Limpieza perezosa (lazy expiration)
        let now = self.time.now();
        if let Some(entry) = self.map.get(key) {
            if entry.expires_at > now {
                return Some(&entry.value);
            }
        }
        // Si expiró o no existe, borrar y devolver None
        self.map.remove(key);
        None
    }
    
    pub fn len(&self) -> usize { self.map.len() }
}
```

**Test Determinista (`src/lib.rs` o `tests/cache_test.rs`):**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use std::cell::RefCell; // Interior mutability para mock time

    // Mock TimeProvider que controlamos manualmente
    struct MockTime {
        current: RefCell<Instant>,
    }
    impl TimeProvider for MockTime {
        fn now(&self) -> Instant { *self.current.borrow() }
    }

    #[test]
    fn test_ttl_expiration() {
        let start = Instant::now();
        let mock = MockTime { current: RefCell::new(start) };
        let mut cache = Cache::with_time_provider(Duration::from_secs(10), mock);

        cache.insert("key", 42);
        assert_eq!(cache.get(&"key"), Some(&42));

        // Avanzar tiempo 5 seg (dentro de TTL)
        *cache.time.current.borrow_mut() += Duration::from_secs(5);
        assert_eq!(cache.get(&"key"), Some(&42));

        // Avanzar tiempo 10 seg más (FUERA de TTL)
        *cache.time.current.borrow_mut() += Duration::from_secs(10);
        assert_eq!(cache.get(&"key"), None); // Expirado y limpiado
        assert_eq!(cache.len(), 0);
    }
}
```
**Puntos de aprendizaje:** `where` clauses, `impl Trait` en retorno no aplica aquí (tipo concreto `Cache<...>`), `RefCell` para mutabilidad interior en test mock, `Instant` vs `SystemTime`.

---

## 📅 SEMANA 6: TRAITS AVANZADOS (La "Standard Library" de tus tipos)

### 🎯 Conceptos Clave

#### 1. `dyn Trait` (Dynamic Dispatch / Trait Objects) vs `impl Trait` / Generics (Static Dispatch)
| Característica | **Generics / `impl Trait` (Static)** | **`dyn Trait` (Dynamic)** |
| :--- | :--- | :--- |
| **Dispatch** | **Monomorphization** (Inline, devirtualization). | **VTable** (Puntero a función, indirect call). |
| **Tamaño** | Conocido en compile-time (`Sized`). | **Unsized** (`?Sized`). Requiere puntero (`Box<dyn Trait>`, `&dyn Trait`, `Arc<dyn Trait>`). |
| **Código Generado** | Múltiples copias (code bloat posible). | Una sola copia. |
| **Heterogeneidad** | **No.** `Vec<impl Trait>` ilegal. `Vec<T>` todos mismo T. | **Sí.** `Vec<Box<dyn Trait>>` mezcla tipos distintos. |
| **Object Safety** | No requerido. | **Requerido:** No `Self: Sized`, no generic methods, no associated constants (mostly), `Self` solo en receiver. |

#### 2. Traits Fundamentales de Conversión (Implementa estos para ergonomía)
```rust
// From / Into (Conversión infalible, lossless)
// Regla: Implementa From. Into viene gratis (blanket impl).
impl From<i32> for MyInt { fn from(v: i32) -> Self { MyInt(v) } }
// Uso: let x = MyInt::from(5); let y: MyInt = 5.into();

// TryFrom / TryInto (Conversión fallible)
// Associated Type: Error
impl TryFrom<i64> for MyInt { 
    type Error = &'static str; 
    fn try_from(v: i64) -> Result<Self, Self::Error> { ... }
}

// AsRef / AsMut (Referencias baratas, genéricas en args)
// ¡Ú
fn read_string(s: impl AsRef<str>) { let s: &str = s.as_ref(); ... }
// Acepta: &str, String, &String, Box<str>, Rc<str>...

// Borrow / BorrowMut (Semántica de "préstamo" para HashMap/Set keys)
// Clave: Hash/Eq deben ser consistentes entre T y &T.
```

#### 3. `Deref` / `DerefMut` (Smart Pointers & Coerción)
*   **`Deref<Target=T>`**: Permite `&SmartPtr` -> `&T` automático (**Deref Coercion**).
*   **Regla de Oro:** `Deref` **NUNCA** debe fallar (no `Result`). Debe ser barato (acceso a campo).
*   **`DerefMut`**: Permite `&mut SmartPtr` -> `&mut T`.
*   **Patrón Newtype:** `struct Wrapper(Vec<u8>); impl Deref for Wrapper { type Target = [u8]; ... }`

#### 4. `Drop` (Destructor)
*   `fn drop(&mut self)`. Llamado automáticamente al salir de scope.
*   **Orden:** Campos en orden de declaración -> `drop(self)`.
*   **Uso:** Liberar recursos no-memoria (file handles, sockets, locks, FFI). **No llames `drop()` manualmente** (usa `std::mem::drop()` si necesitas forzar).

#### 5. Operator Overloading (`std::ops`)
`Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `Not`, `BitAnd`, `BitOr`, `BitXor`, `Shl`, `Shr`, `Index`, `IndexMut`, `Deref`, `DerefMut`, `Fn`, `FnMut`, `FnOnce`.
*   Implementa `Add<&Self>` y `Add<Self>` para ergonomía.

#### 6. `Default` vs `Default::default()`
*   `#[derive(Default)]` requiere que **todos** los campos implementen `Default`.
*   Útil para `..Default::default()` struct update syntax.

### 📝 Ejercicio Práctico: `ByteBuffer` Wrapper
**Objetivo:** `Deref` coercion, `Read`/`Write` traits, `AsRef<[u8]>`, `From<Vec<u8>>`.

```rust
use std::io::{Read, Write, Result as IoResult};
use std::ops::{Deref, DerefMut};

pub struct ByteBuffer(Vec<u8>);

impl ByteBuffer {
    pub fn new() -> Self { Self(Vec::new()) }
    pub fn with_capacity(cap: usize) -> Self { Self(Vec::with_capacity(cap)) }
    pub fn into_inner(self) -> Vec<u8> { self.0 }
}

// 1. Deref a [u8] (Slice) -> Acceso a .len(), .iter(), [..], &[u8]
impl Deref for ByteBuffer {
    type Target = [u8];
    fn deref(&self) -> &[u8] { &self.0 }
}

// 2. DerefMut -> push, extend, write
impl DerefMut for ByteBuffer {
    fn deref_mut(&mut self) -> &mut [u8] { &mut self.0 }
}

// 3. AsRef / AsMut para genéricos
impl AsRef<[u8]> for ByteBuffer { fn as_ref(&self) -> &[u8] { &self.0 } }
impl AsMut<[u8]> for ByteBuffer { fn as_mut(&mut self) -> &mut [u8] { &mut self.0 } }

// 4. Implementar Read / Write (Delega a Vec<u8> / &mut [u8])
// Vec<u8> implementa Write. &[u8] implementa Read.
impl Write for ByteBuffer {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> { self.0.write(buf) }
    fn flush(&mut self) -> IoResult<()> { self.0.flush() }
}

impl Read for ByteBuffer {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> { 
        // Requiere Cursor o leer de un slice. 
        // ByteBuffer es *dueño* de los datos, para leer "consumiéndolos" 
        // necesitaríamos lógica de cursor interno. 
        // Simplificación: Read desde el slice actual (no consumidor).
        (&self.0[..]).read(buf) 
    }
}

// 5. Conversiones
impl From<Vec<u8>> for ByteBuffer { fn from(v: Vec<u8>) -> Self { Self(v) } }
impl From<ByteBuffer> for Vec<u8> { fn from(b: ByteBuffer) -> Vec<u8> { b.0 } }
impl From<&[u8]> for ByteBuffer { fn from(s: &[u8]) -> Self { Self(s.to_vec()) } }
```

**Test de coerción:**
```rust
fn takes_slice(s: &[u8]) { println!("Len: {}", s.len()); }
fn takes_write<W: Write>(mut w: W) { w.write_all(b"hello").unwrap(); }

let mut buf = ByteBuffer::new();
takes_slice(&buf);      // Deref coercion: &ByteBuffer -> &[u8]
takes_write(&mut buf);  // DerefMut coercion: &mut ByteBuffer -> &mut Vec<u8> -> Write
```

---

## 📅 SEMANA 7: SMART POINTERS & INTERIOR MUTABILITY (Compartiendo Estado)

### 🎯 Conceptos Clave

#### 1. `Box<T>` (Owned Heap Pointer)
*   **Un único owner.** Tamaño fijo (puntero). `Deref` a `T`.
*   Usos: Tipos recursivos (`List`, `Tree`), `dyn Trait` objects, Transferir ownership de datos grandes sin copiar, `FnOnce` closures que capturan datos grandes.

#### 2. `Rc<T>` / `Arc<T>` (Reference Counting - Shared Ownership)
| Tipo | Thread-Safe? | Atomic RefCount? | Uso |
| :--- | :--- | :--- | :--- |
| **`Rc<T>`** | ❌ No (`!Send`, `!Sync`) | No (Rápido) | Single-threaded: Graphs, Observers, Caches. |
| **`Arc<T>`** | ✅ Sí (`Send` + `Sync` si `T: Send + Sync`) | Sí (Lento) | Multi-threaded: Shared state across threads. |

*   **`clone()`** incrementa contador (barato). **`drop()`** decrementa. Cuando llega a 0 -> `drop(T)`.
*   **`Rc::new_cyclic` / `Arc::new_cyclic`**: Para crear ciclos *seguros* (ver `Weak`).

#### 3. Interior Mutability: Mutar través de `&T` (Shared Reference)
Rompe la regla " `&T` inmutable, `&mut T` mutable" **de forma segura en runtime**.

| Tipo | Thread-Safe? | Mecanismo | Panic/Block? |
| :--- | :--- | :--- | :--- |
| **`RefCell<T>`** | ❌ | **Runtime Borrow Check** (`borrow()`, `borrow_mut()`). | **Panic** si reglas violadas (already borrowed mutably). |
| **`Mutex<T>`** | ✅ | **Locking** (Bloqueo OS). `lock()` -> `MutexGuard` (RAII). | **Block** (espera). `PoisonError` si panic en holder. |
| **`RwLock<T>`** | ✅ | **Read/Write Lock**. Múltiples lectores O un escritor. | **Block**. `PoisonError`. |

*   **Patrón Canónico Single-Thread:** `Rc<RefCell<T>>`
*   **Patrón Canónico Multi-Thread:** `Arc<Mutex<T>>` o `Arc<RwLock<T>>`
*   **`Cell<T>` / `AtomicU32` / etc:** Para tipos `Copy` (sin referencias internas), `set`/`get`/`replace` sin borrow checking runtime.

#### 4. `Weak<T>` (Romper Ciclos de Referencia / Caches)
*   `Rc::downgrade(&rc)` -> `Weak<T>`.
*   **No cuenta** para el refcount (no impide `drop`).
*   `weak.upgrade()` -> `Option<Rc<T>>` (None si ya dropped).
*   **Usos:** Parent pointers en árboles/grafos, Caches, Observer pattern.

### 🧠 Recurso Visual Obligatorio
> **Jon Gjengset - "Crust of Rust: Interior Mutability" (YouTube)**
> *Entender `UnsafeCell` (la primitiva mágica), `RefCell` vs `Mutex`, `Weak` para ciclos.*

### 🧪 Ejercicio Práctico: **Grafo Dirigido con Ciclos (`Graph<NodeId, NodeData>`)**
**Objetivo:** `Rc`/`Weak` para estructura, `RefCell` para mutabilidad de nodos/aristas, evitar memory leaks.

```rust
use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;

type NodeRef<D> = Rc<RefCell<Node<D>>>;
type WeakNodeRef<D> = Weak<RefCell<Node<D>>>;

#[derive(Debug)]
pub struct Node<D> {
    pub id: usize,
    pub data: D,
    // Hijos: Strong refs (Owner)
    pub children: Vec<NodeRef<D>>,
    // Padre: Weak ref (No owner, evita ciclo)
    pub parent: Option<WeakNodeRef<D>>,
}

impl<D> Node<D> {
    pub fn new(id: usize, data: D) -> NodeRef<D> {
        Rc::new(RefCell::new(Node { id, data, children: Vec::new(), parent: None }))
    }

    // Añadir hijo bidireccional
    pub fn add_child(parent: &NodeRef<D>, child_data: D) -> NodeRef<D> {
        let child = Node::new(
            // Generar ID simple (en real: generador único)
            parent.borrow().children.len(), 
            child_data
        );
        // Link Parent -> Child (Strong)
        parent.borrow_mut().children.push(child.clone());
        // Link Child -> Parent (Weak)
        child.borrow_mut().parent = Some(Rc::downgrade(parent));
        child
    }

    // Obtener padre (Upgrade Weak -> Option<Rc>)
    pub fn get_parent(node: &NodeRef<D>) -> Option<NodeRef<D>> {
        node.borrow().parent.as_ref()?.upgrade()
    }
}

// Grafo contenedor (opcional, para gestión IDs globales)
pub struct Graph<D> {
    nodes: HashMap<usize, NodeRef<D>>,
    next_id: usize,
}

impl<D> Graph<D> {
    pub fn new() -> Self { Self { nodes: HashMap::new(), next_id: 0 } }
    
    pub fn add_root(&mut self, data: D) -> NodeRef<D> {
        let node = Node::new(self.next_id, data);
        self.next_id += 1;
        self.nodes.insert(node.borrow().id, node.clone());
        node
    }
    
    // ... métodos para traversia, búsqueda, etc.
}
```

**Tests Críticos:**
1.  **Crear ciclo:** A -> B -> A (via parent). Verificar que al dropear `Graph` / `root`, memoria se libera (usar `Rc::strong_count` en tests).
2.  **`Weak::upgrade`:** Padre dropeado -> `upgrade()` devuelve `None`.
3.  **Mutabilidad Interior:** Modificar `data` en nodo a través de `&Graph` (shared ref) usando `RefCell`.

---

## 📅 SEMANA 8: TESTING, DOCUMENTACIÓN Y TOOLING (Calidad Profesional)

### 🎯 Conceptos Clave

#### 1. Pirámide de Testing en Rust
| Nivel | Ubicación | Comando | Acceso | Velocidad |
| :--- | :--- | :--- | :--- | :--- |
| **Unit Tests** | `src/lib.rs` / `src/mod.rs` dentro de `#[cfg(test)] mod tests` | `cargo test` | **Privados** (`super::*`) | Muy Rápido |
| **Integration Tests** | `tests/*.rs` (archivos sueltos) | `cargo test` | **Solo API Pública** (`use mycrate::...`) | Rápido |
| **Doc Tests** | Comentarios `/// ```rust ... ``` ` | `cargo test --doc` | **Público** | Medio |
| **Benchmarks** | `benches/*.rs` (requiere `criterion`) | `cargo bench` | Público | Lento (estadístico) |

#### 2. Unit Tests (`#[cfg(test)]`)
```rust
// En src/mimodulo.rs
pub fn add(a: i32, b: i32) -> i32 { a + b }

#[cfg(test)]
mod tests {
    use super::*; // Importa items del modulo padre (incluyendo private)
    
    #[test]
    fn test_add() { assert_eq!(add(2, 2), 4); }
    
    #[test]
    #[should_panic(expected = "divide by zero")] // Test que espera panic
    fn test_div_zero() { div(1, 0); }
    
    #[test]
    fn test_result() -> Result<(), String> { // Test que devuelve Result
        if add(1, 1) == 2 { Ok(()) } else { Err("Math broken".into()) }
    }
}
```

#### 3. Integration Tests (`tests/integration_test.rs`)
```rust
// tests/api_test.rs
use mycrate::public_api::ConfigLoader; // Solo API pública

#[test]
fn test_load_from_file() {
    let loader = ConfigLoader::new();
    // ...
}
```
*   Cada archivo en `tests/` es **crate separada** que linkea tu librería.
*   Usa `tests/common/mod.rs` para helpers compartidos (no se compila como test suelto).

#### 4. Doc Tests (`///`)
```rust
/// Suma dos números.
/// 
/// # Examples
/// 
/// ```
/// let result = mycrate::add(2, 3);
/// assert_eq!(result, 5);
/// ```
/// 
/// # Panics
/// Si overflow (debug mode).
pub fn add(a: i32, b: i32) -> i32 { a + b }
```
*   **Se ejecutan con `cargo test --doc`**. Garantizan que ejemplos en docs compilan y corren.
*   `/// ```rust,no_run` -> Compila pero no ejecuta (ej. requiere red/archivo).
*   `/// ```rust,ignore` -> Ignora completamente.

#### 5. Benchmarks con `criterion` (Estándar de facto)
```toml
# Cargo.toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "my_bench"
harness = false # Criterion usa su propio harness
```
```rust
// benches/my_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mycrate::Cache;

fn bench_cache_insert(c: &mut Criterion) {
    c.bench_function("cache_insert_100", |b| {
        b.iter(|| {
            let mut cache = Cache::new(std::time::Duration::from_secs(60));
            for i in 0..100 {
                cache.insert(black_box(i), black_box(i * 2)); // black_box evita optimización dead code
            }
        })
    });
}

criterion_group!(benches, bench_cache_insert);
criterion_main!(benches);
```

#### 6. Clippy Lints Avanzados (Config en `clippy.toml` o `Cargo.toml`)
```toml
# Cargo.toml [workspace.lints.clippy] o clippy.toml
# Deny = Error de compilación. Warn = Warning. Allow = Silenciar.
clippy::unwrap_used = "deny"           # Prohibir unwrap en prod
clippy::expect_used = "deny"           # Prohibir expect en prod
clippy::panic = "deny"                 # Prohibir panic! explícito
clippy::todo = "warn"                  # Warn en todo!
clippy::unimplemented = "warn"
clippy::print_stdout = "allow"         # Permitir println en CLI
clippy::cognitive_complexity = "warn"  # Funciones muy complejas
clippy::large_enum_variant = "warn"    # Enum variants muy grandes (Boxear)
clippy::type_complexity = "warn"       # Tipos muy anidados (Type Alias)
```
*   Ejecuta: `cargo clippy -- -D warnings` (trata warns como errors).

#### 7. `rustfmt` Config (`rustfmt.toml`)
```toml
# Forzar estilo consistente en equipo
max_width = 100
tab_spaces = 4
hard_tabs = false
newline_style = "Unix"
imports_granularity = "Crate" # Agrupa imports: std, external, crate
group_imports = "StdExternalCrate"
```

---

## 🛠️ PROYECTO INTEGRADOR MES 2: `config-loader` (Librería Publicable)

### 🎯 Objetivo
Crear una crate **library** (`--lib`) robusta, genérica, bien testeada, documentada y publicable en `crates.io`.

### 📋 Especificación Técnica (`config-loader`)

#### 1. API Pública (`src/lib.rs`)
```rust
// Re-exports públicos (Fachada)
pub use error::{ConfigError, Result};
pub use loader::{ConfigLoader, Source, SourcePriority};
pub use traits::TimeProvider; // Si reusas el trait de la semana 5

// Módulos internos
mod error;
mod loader;
mod sources;
mod traits;
mod merging;
```

#### 2. Funcionalidad Core
*   **`ConfigLoader::new()`** -> Builder pattern.
*   **Fuentes (`Source` trait object o Enum):**
    1.  **File:** TOML, JSON, YAML (feature flags: `toml`, `json`, `yaml`). Path + `required: bool`.
    2.  **Env Vars:** Prefijo `APP_`, parsing anidado (`APP_DATABASE__HOST=localhost` -> `database.host`).
    3.  **CLI Args:** Integración opcional con `clap` (feature `clap`), o simple `Vec<String>` parsing `--key=val`.
*   **Precedencia (Merge):** CLI > Env > File > Defaults. **Deep Merge** para structs anidados (no override completo).
*   **API Genérica:**
    ```rust
    impl ConfigLoader {
        pub fn load<T>(self) -> Result<T> 
        where T: for<'de> serde::Deserialize<'de> { ... }
    }
    ```
*   **Validación Opcional:** Trait `Validate` (custom derive o manual) tras carga.

#### 3. Manejo de Errores (`error.rs`)
```rust
use thiserror::Error; // Cargo add thiserror

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error reading {path}: {source}")]
    Io { path: String, #[source] source: std::io::Error },
    
    #[error("Parsing {format} config failed: {source}")]
    Parse { format: String, #[source] source: Box<dyn std::error::Error + Send + Sync> },
    
    #[error("Missing required config source: {0}")]
    MissingSource(String),
    
    #[error("Validation failed for '{field}': {msg}")]
    Validation { field: String, msg: String },
    
    #[error("Merging error: {0}")]
    Merge(String),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
```

#### 4. Testing Strategy (Cobertura 100% `cargo tarpaulin` o `llvm-cov`)
*   **Unit Tests (`src/*`):** Lógica de merging (deep merge maps), parsing env vars anidados, prioridad fuentes, `TimeProvider` mock para `Cache` interno si usas.
*   **Integration Tests (`tests/`):** 
    *   `test_file_toml`, `test_file_json`, `test_env_priority`, `test_cli_override`.
    *   `test_missing_required_file_fails`.
    *   `test_invalid_toml_syntax`.
    *   `test_deserialize_into_struct`.
*   **Doc Tests:** Ejemplos en `ConfigLoader::load`, `Source::File`, `ConfigError`.

#### 5. Features (`Cargo.toml`)
```toml
[features]
default = ["toml", "env"]
toml = ["dep:toml"]
json = ["dep:serde_json"]
yaml = ["dep:serde_yaml"]
env = [] # Std only
clap = ["dep:clap"] # Integración CLI avanzada
```

#### 6. CI/CD (`.github/workflows/ci.yml`)
```yaml
name: CI
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: components: rustfmt, clippy
      - name: Cache
        uses: Swatinem/rust-cache@v2
      - name: Fmt
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
      - name: Test (All features)
        run: cargo test --all-features --workspace
      - name: Doc Test
        run: cargo test --doc --all-features
      - name: Security Audit
        run: cargo audit
      - name: Check Docs Build
        run: cargo doc --all-features --no-deps
```

#### 7. Publicación (Simulada o Real)
1.  `cargo package --list` (Verificar archivos incluidos: `Cargo.toml`, `README.md`, `LICENSE`, `src/`, `tests/`, `benches/`, **NO** `target/`, `.git/`).
2.  `cargo publish --dry-run` (Verifica warnings: description, keywords, categories, license, readme).
3.  `cargo login` (Token crates.io) -> `cargo publish`.
4.  **Versionado:** `0.1.0` -> `0.1.1` (patch), `0.2.0` (minor breaking). **SemVer estricto**.

---

## 📚 RESUMEN RECURSOS MES 2

| Semana | Lectura "The Book" | Video Profundo | Práctica Clave |
| :--- | :--- | :--- | :--- |
| **5** | **Cap 10** (Generics, Traits, Lifetimes) | **Jon Gjengset: "Crust of Rust: Lifetimes"** | `Cache<K,V>` + `TimeProvider` Mock |
| **6** | **Cap 19** (Advanced Traits) <br> *Rust Design Patterns: Traits* | Jon Gjengset: "Crust of Rust: Traits" (Opcional) | `ByteBuffer` (`Deref`, `Read`, `Write`, `From`) |
| **7** | **Cap 15** (Smart Pointers) | **Jon Gjengset: "Crust of Rust: Interior Mutability"** | **Grafo `Rc<RefCell<Node>>` + `Weak`** |
| **8** | **Cap 11** (Testing) <br> **Cap 14** (Cargo/Crates.io) | *The Rust Performance Book* (Benchmarking) | **`config-loader` Crate Completa + CI + Publish** |

---

## ⚠️ PROBLEMAS COMUNES MES 2 (Y SOLUCIONES)

| Trampa | Síntoma | Solución |
| :--- | :--- | :--- |
| **"Trait Object Safety" Error** | `the trait cannot be made into an object` | Revisa: ¿Métodos genéricos? -> Quita generics o usa `Box<dyn Fn...>`. ¿`Self: Sized`? -> Añada `where Self: Sized` al método o quita `Self` de firma. ¿Associated Constants? -> Mueve a método. |
| **Lifetime Hell en Structs** | `'a` por todos lados, `&'a mut self` contagia todo. | **Evita referencias en structs** si es posible (`String`, `Arc`, `Box`). Si necesitas: `'a` en struct + `'a` en impl. Usa `&'a self` methods. Piensa en *Arena Allocation* (Mes 5) para grafos complejos. |
| **`Rc<RefCell<T>>` Spaghetti** | `borrow_mut()` panics en runtime difíciles de depurar. | **Minimiza scope del borrow:** `{ let mut b = cell.borrow_mut(); ... }`. Usa `Ref::map` / `RefMut::map` para devolver referencias a campos internos sin mantener lock del padre. |
| **`Box<dyn Trait>` vs `impl Trait` en Return** | `expected opaque type, found different opaque type` (en `if/else`). | Si ramas devuelven tipos distintos -> **`Box<dyn Trait>`**. Si mismo tipo -> `impl Trait`. |
| **Code Bloat (Monomorphization)** | Binario enorme, compile time alto. | Usa `dyn Trait` / `Box<dyn Trait>` en "hot paths" genéricos llamados con muchos tipos. O `impl Trait` en args (genera una versión por caller, no por callee). |
| **Tests lentos / Flaky** | `cargo test` tarda minutos. Tests fallan aleatoriamente. | **Unit tests puros** (sin I/O, sin tiempo real, sin hilos) = 90% de tests. **Mock traits** (`TimeProvider`, `Storage`, `HttpClient`). Integración solo para "happy path" y contratos. |
| **Doc Tests rotos** | `cargo test --doc` falla por imports privados o `main` function. | Doc tests son **crates aparte**. Usa `use mycrate::*;` o paths absolutos. `/// ```rust,no_run` para ejemplos que requieren setup externo. |

---

## ✅ CHECKLIST FINAL MES 2 (Definition of Done)

- [ ] **Generics & Bounds:** Escribes `fn foo<T: Trait>(x: T)` y `fn bar() -> impl Trait` naturalmente. Entiendes monomorphization.
- [ ] **Lifetimes:** Anotas lifetimes en structs con referencias correctamente. Entiendes elision rules y cuándo el compilador necesita ayuda. `'static` bound entendido.
- [ ] **Traits Std:** Implementas `From`, `TryFrom`, `AsRef`, `Deref`, `Display`, `Debug`, `Default`, `Drop`, `Operator Overloading` correctamente para tus tipos.
- [ ] **Dynamic vs Static:** Sabes cuándo usar `Vec<Box<dyn Trait>>` (heterogéneo) vs `Vec<T: Trait>` (homogéneo, estático). Entiendes VTable vs Monomorphization.
- [ ] **Smart Pointers:** `Box` (recursión/grande), `Rc`/`Arc` (shared ownership), `RefCell`/`Mutex`/`RwLock` (interior mutability). **`Weak` para ciclos**.
- [ ] **Testing:** Unit tests (acceso privado), Integration tests (API pública), Doc tests (ejemplos compilados). **Mocking via Traits** (ej. `TimeProvider`).
- [ ] **Benchmarking:** Usas `criterion` con `black_box`. Entiendes difference entre benchmark y test.
- [ ] **Tooling:** `clippy` `-D warnings` en CI. `rustfmt` configurado. `cargo audit` pasando.
- [ ] **Proyecto `config-loader`:**
    - [ ] Compila `cargo build --release --all-features` sin warnings.
    - [ ] `cargo test --all-features` **100% pass**.
    - [ ] `cargo test --doc --all-features` pass.
    - [ ] `cargo tarpaulin` (o `llvm-cov`) > **90% coverage** (líneas/branches).
    -   [ ] `cargo package --dry-run` limpio.
    -   [ ] `README.md` con badges (build, version, license, docs.rs), ejemplos de uso claros.
    -   [ ] `CHANGELOG.md` (Keep a Changelog format).
    -   [ ] **Publicada en crates.io** (o registry privado) como `config-loader-v1` (nombre único).

---

### 🚀 PRÓXIMO PASO: MES 3
> **ASYNC RUST: Futures, Pin, Tokio, Axum, SQLx, Observabilidad, Docker.**
> *Donde el "Fearless Concurrency" brilla de verdad.*

*Has sobrevivido a la curva de aprendizaje. Ahora construyes sistemas.* 🦀⚙️