# 🦀 MES 1: FUNDAMENTOS Y EL "BORROW CHECKER" — Guía Detallada de Estudio

> **Filosofía del Mes:** *"El compilador es tu pair programmer más estricto. Si compila, probablemente funciona."*
> **Meta:** Dejar de luchar contra `borrow checker` y empezar a *pensar* en ownership. Más adelante veremos que significa esto de `ownership`.

---

## 📅 SEMANA 1: SETUP, CARGO Y SINTAXIS BÁSICA
**Objetivo:** Entorno productivo funcionando + sintaxis base (variables, funciones, control de flujo).

### 🎯 Conceptos Clave (The "What")
| Concepto | Detalle Crítico para Rustaceans |
| :--- | :--- |
| **`rustup` / Toolchains** | `stable` (default), `beta`, `nightly`. `rustup update` frecuente. `rustup component add rust-src rust-analyzer clippy rustfmt`. |
| **Cargo** | `cargo new --bin` (ejecutable), `cargo new --lib` (librería). `Cargo.toml` = manifest. `Cargo.lock` = **commitealo** en binarios, **ignóralo** en librerías (generalmente). |
| **Variables** | **Inmutables por defecto** (`let x = 5;`). `mut` es explícito (`let mut x = 5;`). *Shadowing* (`let x = x + 1;`) != Mutabilidad (cambia tipo/valor, nueva dirección memoria). |
| **Tipos Escalares** | Enteros (`i8..i128`, `u8..u128`, `isize`, `usize` - **punteros/tamaños usan `usize`**), Flotantes (`f32`, `f64` default), Bool, Char (`char` = **Unicode Scalar Value**, 4 bytes, comillas simples `'🦀'`). |
| **Tipos Compuestos** | **Tupla** `(i32, f64, u8)` (tamaño fijo, tipos heterogéneos, acceso `.0`, `.1`). **Array** `[T; N]` (tamaño fijo, mismo tipo, en **Stack**). `Vec<T>` (Heap, dinámico, *prefiere este*). |
| **Funciones** | `fn name(param: Type) -> ReturnType { expr }`. **Expresiones vs Sentencias**: Bloques `{}` son expresiones (última línea sin `;` devuelve valor). `return` anticipado solo para *early returns*. |
| **Comentarios** | `//` (línea), `/* */` (bloque, anidables), `///` (Doc comments, **Markdown**, generan `cargo doc`). |

### 🛠️ Setup "Profesional" (Haz esto **AHORA**)
```bash
# 1. Instalación estándar
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 2. Componentes esenciales (IDE/Tooling)
rustup component add rustfmt clippy rust-src rust-analyzer

# 3. Herramientas de cargo imprescindibles
cargo install cargo-edit      # cargo add <crate> / cargo rm / cargo upgrade
cargo install cargo-watch     # cargo watch -x run / -x test / -x clippy
cargo install cargo-tree      # cargo tree -d (duplicados) / -i (invertido)
cargo install cargo-outdated  # Ver updates disponibles
cargo install cargo-audit     # cargo audit (vulnerabilidades CVE)
cargo install cargo-nextest   # Test runner ultra-rápido (cargo nextest run)

# 4. Editor: VS Code + Extensión "rust-analyzer" (Oficial)
#    O Neovim + kickstart.nvim / LazyVim (tienen config rust lista)
#    O IntelliJ IDEA + Rust Plugin
```

### 📝 Ejercicios Obligatorios: **Rustlings** (Curso interactivo oficial)
```bash
git clone https://github.com/rust-lang/rustlings
cd rustlings
rustup override set stable # Asegura toolchain
cargo install --path .
rustlings watch # Modo interactivo: edita -> guarda -> test auto
```
**Completa estos ejercicios (Carpeta `exercises/`):**
1.  `variables/` (mut, shadowing, const, scope)
2.  `functions/` (params, returns, statements vs expressions, divergent `!`)
3.  `if/` (expresiones if, else if, let else - *preview*)
4.  `primitive_types/` (tuples, arrays, slices intro, string literals vs String)

> **💡 Truco:** Si te atascas > 10 min: `rustlings hint exercise_name`. Lee el error del compilador *antes* de la pista.

### 🧪 Mini-Reto: "Hola Cargo Avanzado"
Crea un proyecto `cargo new hello_cargo`.
1.  Añade dependencia: `cargo add rand` (generar número aleatorio).
2.  Añade dev-dependency: `cargo add --dev pretty_assertions`.
3.  Escribe un test en `#[cfg(test)]` que use `pretty_assertions::assert_eq!`.
4.  Ejecuta `cargo test`, `cargo clippy`, `cargo fmt --check`.
5.  Genera docs: `cargo doc --open`.

---

## 📅 SEMANA 2: OWNERSHIP, BORROWING Y SLICES (EL NÚCLEO DURO)
**Objetivo:** Internalizar las 3 reglas. Entender Stack vs Heap. Dejar de poner `clone()` por pánico.

### 🧠 Las 3 Reglas de Oro (Memorízalas)
1.  **Cada valor tiene un *owner* (dueño) único** (variable `let x = ...`).
2.  **Solo puede haber un owner a la vez.**
3.  **Cuando el owner sale de scope, el valor se `drop`ea (libera memoria).**

### 📊 Stack vs Heap: La Intuición Visual
```text
STACK (LIFO, Rápido, Tamaño fijo conocido en compile-time)
┌─────────────────────┐
│  let x: i32 = 5;    │  <-- Valor '5' vive AQUÍ (Copy)
│  let y = x;         │  <-- COPIA bit a bit. x e y independientes.
└─────────────────────┘

HEAP (Puntero + Len + Cap, Tamaño dinámico, Más lento acceso)
┌─────────────────────┐     ┌─────────────────────┐
│  let s1 = String::  │     │   HEAP MEMORY       │
│  from("hello");     │────▶│   [h, e, l, l, o]   │  <-- Datos reales
│  // s1 = (ptr, 5, 5)│     └─────────────────────┘
│  let s2 = s1;       │     💥 MOVE! s1 INVALIDADO
│  // s2 = (ptr, 5, 5)│     s1 ya no apunta a nada válido
└─────────────────────┘     (Double free si usamos s1 y s2)
```

### 🔑 Move vs Clone vs Copy
| Operación | Código | Qué pasa en Memoria | Cuándo usa |
| :--- | :--- | :--- | :--- |
| **Move (Default)** | `let s2 = s1;` | Puntero/len/cap copiados en Stack. **Heap NO copiado**. `s1` **invalidado**. | Tipos **sin** `Copy` trait (`String`, `Vec`, `Box`, structs propios). |
| **Clone (Explícito)** | `let s2 = s1.clone();` | **Deep copy**: Heap nuevo asignado, datos copiados. `s1` y `s2` válidos. | Cuando **necesario** compartir ownership real. Costoso (O(N)). |
| **Copy (Implícito)** | `let y = x;` | **Bitwise copy** en Stack. `x` sigue válido. **Zero cost**. | Tipos **con** `Copy` trait (`i32`, `f64`, `bool`, `char`, tuplas/arrays de `Copy`, `&T`). |

> **Regla de Oro:** ¿Implementa `Copy`? (Primitivos, referencias `&T`). **Sí** -> Copy automático. **No** -> Move automático. ¿Quieres duplicar Heap? `.clone()`.

### 🤝 Referencias & Borrowing (Prestamo)
*   `&T` (Referencia Inmutable / **Shared Reference**): **Muchos** lectores simultáneos. **Nadie** escribe. `&T` **es `Copy`**.
*   `&mut T` (Referencia Mutable / **Exclusive Reference**): **Exactamente UNO** escritor. **Nadie** lee ni escribe a la vez. **No es `Copy`** (es `Move`).
*   **Regla del Borrow Checker:** `&T` XOR `&mut T`. Nunca ambos a la vez en el mismo scope.

### 🔪 Slices (`&[T]`, `&str`)
*   **Vista** (fat pointer: `ptr` + `len`) sobre datos contiguos **sin ownership**.
*   `&str` = Slice de `String` (o string literal `'static`).
*   `&[i32]` = Slice de `Vec<i32>` o Array.
*   **Permiten funciones genéricas sobre secuencias** sin tomar ownership ni requerir `Vec` específico.

### 🎥 Recurso Visual Obligatorio
> **Jon Gjengset - "Crust of Rust: Ownership and Borrowing" (YouTube, ~1.5h)**
> *Verlo a 1.25x, pausar y codificar los ejemplos. Es la mejor explicación visual del borrow checker.*

### 📝 Ejercicios Rustlings (Semana 2)
*   `move_semantics/` (Move, Clone, Copy, funciones tomando ownership)
*   `references/` (`&`, `&mut`, reglas de borrowing, scopes)
*   `slices/` (string slices, array slices, `first_word` exercise)

### 🧪 Ejercicio Práctico: `split_string` Manual
**Objetivo:** Manipular `&str`, indices, slices, loops. **Sin usar** `.split()`, `.split_whitespace()`, `.chars().collect()`.

```rust
// src/main.rs o tests/manual_split.rs
fn split_manual(input: &str, delimiter: char) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    // Pista: Itera sobre input.char_indices()
    // char_indices() -> (byte_index, char)
    // Cuando char == delimiter: push input[start..byte_index], start = byte_index + 1
    // Al final: push input[start..]
    todo!("Implementa aquí")
}

#[test]
fn test_split() {
    assert_eq!(split_manual("a,b,c", ','), vec!["a", "b", "c"]);
    assert_eq!(split_manual("hello world", ' '), vec!["hello", "world"]);
    assert_eq!(split_manual("leading", 'x'), vec!["leading"]);
    assert_eq!(split_manual("", ','), vec![""]);
    // Edge case: delimiter al final "a," -> ["a", ""]
}
```
**Puntos de aprendizaje:** `char_indices` vs `chars` (indices de *bytes* vs *chars*), slicing `&str[start..end]` requiere límites en bordes de char (UTF-8), `Vec<&str>` lifetimes ligados a `input`.

---

## 📅 SEMANA 3: STRUCTS, ENUMS, PATTERN MATCHING Y MANEJO DE ERRORES BÁSICO
**Objetivo:** Modelar dominio del problema con tipos algebraicos (ADTs). Hacer imposibles los estados inválidos.

### 🏗️ Structs (Datos Estructurados)
```rust
// Classic Struct
struct User { username: String, email: String, active: bool, sign_in_count: u64 }

// Tuple Struct (Newtype Pattern - Ver Mes 5)
struct Color(u8, u8, u8); // RGB
struct Point(f64, f64, f64); // 3D

// Unit Struct (Marker types, útil para traits/generics)
struct AlwaysEqual;

// Instanciación
let user = User { email: String::from("a@b.com"), username: String::from("a"), ..Default::default() }; // Requiere Default trait
let black = Color(0, 0, 0);
let origin = Point(0.0, 0.0, 0.0);

// Field Init Shorthand
let username = String::from("me");
let user = User { username, email: String::from("e"), active: true, sign_in_count: 1 };

// Struct Update Syntax (MOVE de campos no especificados!)
let user2 = User { email: String::from("new"), ..user }; // user.username MOVIDO a user2. user inválido parcialmente.
```

### 🎭 Enums (Algebraic Data Types - Sum Types)
**Potencia de Rust:** `enum` != `enum` de C/Java. **Llevan datos**.

```rust
// Enum simple (C-like)
enum Direction { Up, Down, Left, Right }

// Enum con datos (Tagged Union / Sum Type)
enum Message {
    Quit,                       // Unit variant
    Move { x: i32, y: i32 },    // Struct variant (named fields)
    Write(String),              // Tuple variant (1 field)
    ChangeColor(u8, u8, u8),    // Tuple variant (3 fields)
}

// Enum genérico: La base de Rust
enum Option<T> { None, Some(T) }
enum Result<T, E> { Ok(T), Err(E) }
```

### 🔍 Pattern Matching Exhaustivo (`match`)
```rust
fn process(msg: Message) {
    match msg {
        Message::Quit => println!("Bye!"),
        Message::Move { x, y } => println!("Move to ({}, {})", x, y), // Destructuring
        Message::Write(text) => println!("Text: {}", text),
        Message::ChangeColor(r, g, b) => println!("RGB: {}, {}, {}", r, g, b),
    }
    // COMPILADOR AVISA SI FALTA UN VARIANT. Refactor seguro.
}

// if let / while let (Azúcar sintáctico para un solo caso)
if let Message::Write(text) = msg { ... } // Ignora otros variants
while let Some(item) = iterator.next() { ... }
```

### ⚠️ `Option<T>` y `Result<T, E>`: **Nunca uses `null` / Excepciones**
*   `Option<T>`: Valor **puede faltar**. `Some(val)` / `None`.
*   `Result<T, E>`: Operación **puede fallar**. `Ok(val)` / `Err(err)`.
*   **Métodos clave:** `map`, `and_then` (flat_map), `unwrap_or`, `unwrap_or_else`, `expect("msg")` (solo tests/main), **`?,` el operador mágico**.

### 📝 Ejercicios Rustlings (Semana 3)
*   `structs/` (creación, update syntax, métodos asociados `impl`)
*   `enums/` (definición, match, `Option`/`Result` basics)
*   `option/` (`map`, `and_then`, `unwrap_or`)
*   `result/` (`?`, `map_err`, `and_then`)
*   `match/` (exhaustividad, guards `if`, `@` bindings)

### 🛠️ Mini-Proyecto: **CLI Todo List (v1 - Memoria)**
**Requisitos:**
1.  **Modelo (`models.rs`):**
    ```rust
    #[derive(Debug, Clone, PartialEq)]
    pub struct Task { pub id: u64, pub description: String, pub status: Status }
    #[derive(Debug, Clone, Copy, PartialEq)] // Copy para Status simple
    pub enum Status { Todo, Doing, Done }
    ```
2.  **Storage (`storage.rs`):** `struct Store { tasks: Vec<Task>, next_id: u64 }`. Métodos: `add`, `list`, `complete`, `delete`, `find_by_id` (devuelve `Option<&Task>` / `Option<&mut Task>`).
3.  **CLI (`main.rs`):** Loop `loop { print_menu(); read_input(); match input { "add" => ..., "list" => ..., "quit" => break } }`.
4.  **Reglas:** **Cero `unwrap()`/`expect()` en lógica de negocio**. Usa `match` / `if let` / `?` en `main` (main puede devolver `Result<(), Box<dyn Error>>`).

---

## 📅 SEMANA 4: MÓDULOS, COLECCIONES, STRING VS &STR, ERROR HANDLING AVANZADO, TRAITS BÁSICOS
**Objetivo:** Organizar código escalable. Dominar `String`/`&str`. Propagar errores como un pro. Derivar traits estándar.

### 📦 Sistema de Módulos (Rust 2018 Edition - `mod.rs` legacy vs `foo.rs`/`foo/`)
```
src/
├── main.rs         // crate root
├── models.rs       // mod models; -> pub struct Task...
├── storage.rs      // mod storage; -> pub struct Store...
│                   // use crate::models::Task;
├── cli/            // Directorio = módulo
│   ├── mod.rs      // pub mod args; pub mod commands;
│   ├── args.rs
│   └── commands.rs
└── error.rs        // Tipos de error personalizados
```
*   **`mod`**: Declara módulo (privado por defecto).
*   **`pub mod`**: Expone módulo a padres.
*   **`use`**: Trae a scope local. `use crate::models::Task;`, `use std::collections::HashMap;`.
*   **`pub use`**: Re-export (Fachada pública). `pub use crate::storage::Store;`
*   **`crate`** vs **`super`** vs **`self`** paths.

### 📚 Colecciones Estándar (Cheat Sheet Mental)
| Colección | Uso Típico | Clave |
| :--- | :--- | :--- |
| **`Vec<T>`** | Lista ordenada, dinamica. | `push`, `pop`, `get(i)` -> `Option<&T>`, `iter()`, `drain(..)`. |
| **`HashMap<K, V>`** | Clave-Valor, lookup O(1). | `entry(key).or_insert(default)` (patrón *entry API* evita double hash). `get(&key)` -> `Option<&V>`. |
| **`HashSet<T>`** | Unicidad, pertenencia. | `insert`, `contains`, `union`/`intersection`/`difference`. |
| **`BTreeMap`/`BTreeSet`** | Ordenados por clave. | Range queries, `first_key_value()`, `pop_first()`. |

### 🧵 `String` vs `&str` (La confusión eterna)
| Tipo | Owner? | Mutabilidad | Ubicación | Conversión |
| :--- | :--- | :--- | :--- | :--- |
| **`String`** | **Sí** (Owned) | `mut` -> `push_str`, `clear` | Heap | `.as_str()` -> `&str`, `&*s` (Deref coercion) |
| **`&str`** | **No** (Borrowed/View) | **Inmutable** | Stack (fat ptr) / Static (literales) | `.to_string()` / `.to_owned()` -> `String` |
| **`&String`** | No | Inmutable | Stack | Coerciona auto a `&str` via `Deref` |

**Regla de Diseño de APIs:**
*   Argumentos de entrada: **`impl AsRef<str>`** (acepta `&str`, `String`, `&String`, `Box<str>`) o directamente **`&str`**.
*   Propiedad/Retorno: **`String`** (si hay que construir/modificar) o **`&str`** (si es vista a dato existente).
*   **Evita `&String` en firmas públicas** (obliga al caller a tener `String` heap allocated).

### ❌ `unwrap()` / `expect()` vs ✅ `?` / Combinators
```rust
// MAL (Panics en prod)
fn read_config(path: &str) -> Config {
    let content = std::fs::read_to_string(path).unwrap(); // CRASH si no existe
    toml::from_str(&content).unwrap()
}

// BIEN (Propaga error, caller decide)
fn read_config(path: &str) -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string(path)?; // IoError -> ConfigError (via From)
    let config = toml::from_str(&content)?;       // TomlError -> ConfigError
    Ok(config)
}

// EN MAIN / TESTS (Donde panic está OK si es "imposible" o setup)
fn main() -> Result<(), Box<dyn std::error::Error>> { // Box<dyn Error> = type erasure
    let config = read_config("config.toml")?;
    println!("{:?}", config);
    Ok(())
}
```

### 🏷️ Traits Básicos Derivables (`#[derive(...)]`)
| Trait | Para qué sirve | Cuándo implementar manual |
| :--- | :--- | :--- |
| **`Debug`** | `:?` formatting (logs, dev). **Siempre** derivable. | Casi nunca. |
| **`Display`** | Formato usuario final (`{}`). **No derivable**. | Siempre que muestres al user. `fmt::Display`. |
| **`Clone`** | Duplicación explícida (`.clone()`). | Tipos con `String`/`Vec`/Heap. |
| **`Copy`** | Duplicación implícita (bitwise). **Solo si todos los campos son `Copy`**. | `#[derive(Copy, Clone)]` en structs simples (`Color`, `Point`, `Status`). |
| **`PartialEq` / `Eq`** | `==` / `!=`. `Eq` = equivalencia total (req para `HashMap` keys). | Keys de HashMap, comparar dominio. |
| **`PartialOrd` / `Ord`** | `<`, `>`, `sort()`. `Ord` = total order. | Keys de `BTreeMap`, sorting. |
| **`Hash`** | `HashMap`/`HashSet` keys. | Keys de HashMap. |
| **`Default`** | `Default::default()` / `..Default::default()`. | Structs con campos opcionales/valores por defecto. |

### 📝 Ejercicios Rustlings (Semana 4)
*   `modules/` (privacy, `use`, `super`, `crate`, file hierarchy)
*   `hash_map/` (entry API, counting words, `get_or_insert`)
*   `error_handling/` (`?`, `Result` alias, `Box<dyn Error>`, custom error types opcional)
*   `traits/` (derive vs impl, `Debug` vs `Display`, `From`/`Into`)

### 🛠️ Mini-Proyecto: **Refactor Todo List v2 (Modular + Persistencia)**
**Objetivo:** Aplicar todo lo aprendido. Código limpio, testeable, extensible.

#### Estructura Final Obligatoria
```text
todo_cli/
├── Cargo.toml
├── src/
│   ├── main.rs          // Entry point, CLI parsing (clap opcional, o manual), loop principal
│   ├── models.rs        // Task, Status, (derives: Debug, Clone, PartialEq, Serialize, Deserialize)
│   ├── storage.rs       // Trait Storage { fn save... fn load... } + impl FileStorage
│   ├── error.rs         // Enum AppError { Io, Serde, NotFound, ... } + impl From<...>
│   └── commands.rs      // Lógica de cada comando (add, list, done, delete) -> functions
```

#### Requisitos Técnicos Detallados
1.  **Persistencia:** Guarda en `~/.todo/tasks.json` (usa `dirs` crate o `std::env::home_dir()`).
    *   `FileStorage::load() -> Result<Vec<Task>, AppError>` (si no existe -> `Ok(vec![])`).
    *   `FileStorage::save(&[Task]) -> Result<(), AppError>`.
    *   Usa `serde_json` (`cargo add serde --features derive serde_json`).
2.  **Error Handling:**
    *   Define `enum AppError { Io(std::io::Error), Serde(serde_json::Error), TaskNotFound(u64), ... }`.
    *   Implementa `std::error::Error` + `Display` + `Debug` (manual o `thiserror` crate *opcional pero recomendado*).
    *   Implementa `From<std::io::Error> for AppError`, etc. para que `?` funcione automático.
3.  **CLI (Manual o `clap` derive):**
    *   `todo add "comprar leche"`
    *   `todo list` (muestra ID, Status icono [ ]/[x], Desc)
    *   `todo done 1`
    *   `todo del 1`
    *   `todo help`
4.  **Testing (Obligatorio):**
    *   `#[cfg(test)] mod tests { use super::*; ... }` en `storage.rs` y `models.rs`.
    *   Test `Storage`: crea temp dir (`tempfile` crate), save/load, corrupt file handling.
    *   Test `commands`: mockea storage (pasa `&mut dyn Storage` o usa `Vec` en memoria para tests unitarios puros).

#### Entregable "Definition of Done" ✅
*   `cargo run -- add "test"` -> Crea archivo JSON.
*   `cargo run -- list` -> Lee JSON, pretty prints.
*   `cargo run -- done 1` -> Modifica JSON.
*   `cargo test` -> **Todos pasan** (incluyendo edge cases: ID inexistente, archivo corrupto, permisos).
*   `cargo clippy` -> **0 warnings**.
*   `cargo fmt --check` -> **Pasa**.
*   Código organizado en módulos, sin `unwrap()` en lógica (`main` puede `unwrap`/`expect` al final o usar `?` con `Result<(), AppError>`).

---

## 📚 RESUMEN DE RECURSOS SEMANA A SEMANA (Para no perderse)

| Semana | Lectura "The Book" (Oficial) | Video Profundo (Jon Gjengset) | Práctica Activa |
| :--- | :--- | :--- | :--- |
| **1** | Cap 1 (Instalación), 2 (Guessing Game - *léelo por encima*), 3 (Conceptos Comunes) | — | `rustlings` (variables, functions, if, primitive_types) |
| **2** | **Cap 4 (Ownership) - ESTUDIO INTENSIVO** | **"Crust of Rust: Ownership and Borrowing"** | `rustlings` (move_semantics, references, slices) + `split_manual` |
| **3** | Cap 5 (Structs), 6 (Enums/Match) | "Crust of Rust: Enums and Pattern Matching" (Opcional) | `rustlings` (structs, enums, option, result, match) + **Todo List v1** |
| **4** | Cap 7 (Módulos), 8 (Colecciones), 9 (Error Handling) | "Crust of Rust: Error Handling" | `rustlings` (modules, hash_map, error_handling, traits) + **Todo List v2 Refactor** |

---

## ⚠️ PROBLEMAS COMUNES MES 1 (Y CÓMO EVITARLAS)

| Trampa | Síntoma | Solución |
| :--- | :--- | :--- |
| **"String Hell"** | `expected &str, found String` / `cannot move out of borrowed content` | **Firmas:** `fn foo(s: &str)`. **Llamadas:** `foo(&my_string)` (Deref coercion). **Ownership:** `let owned = borrowed.to_owned();`. |
| **`unwrap()` Fever** | Código peta en producción por archivo faltante. | **Prohibido** en librerías/logica. `?` en funciones que devuelven `Result`. `expect("contexto claro")` *solo* en `main`/tests/setup. |
| **Borrow Checker Fight** | `cannot borrow as mutable because also borrowed as immutable` | **Acorta scopes** `{ ... }`. **Clona datos baratos** (`.clone()` en `String` corto, `Copy` types). **Reestructura:** pasa ownership (`fn process(mut v: Vec)`) en lugar de `&mut Vec` si es posible. Usa `RefCell`/`Mutex` *solo* si es.shared mutability real (Mes 2). |
| **Modulos Caos** | `use crate::foo::bar` vs `use super::bar` vs `mod foo;` duplicados. | **Un `mod` por archivo/directorio.** `main.rs` declara `mod models;`. `models.rs` **no** declara `mod models`. `use crate::models::Task;` en hijos. |
| **`Copy` en structs con `String`** | `the trait Copy is not implemented` | `String` **no es `Copy`**. Si tu struct tiene `String`, **no puedes** derivar `Copy`. Usa `Clone`. |
| **Lifetimes en Structs (Prematuro)** | `expected named lifetime parameter` | **Semana 1-2: Evita referencias `&'a T` DENTRO de structs.** Usa `String`, `Vec`, `Box`, `Arc`. Lifetimes en structs = **Mes 2/3**. |

---

## 🧩 MATERIAL COMPLEMENTARIO: Laboratorio de Código Comentado

> Todos los ejemplos de esta sección **compilan con `rustc 1.81` (edición 2021)** salvo los marcados con `// ❌ NO COMPILA`, que son errores *intencionales* para que leas el mensaje del compilador. Copialos en un `cargo new` y juega con ellos.

### 1️⃣ Ownership & Move (la regla del único dueño)

```rust
fn main() {
    let s1 = String::from("hola");
    let s2 = s1;            // MOVE: el dato del heap NO se copia, s1 queda invalidado
    println!("{s2}");       // ✅ OK

    // println!("{s1}");    // ❌ NO COMPILA: "borrow of moved value: `s1`"

    let s3 = s2.clone();    // CLONE: deep copy, nuevo heap. s2 y s3 son válidos
    println!("{s2} y {s3}");

    let x = 5;
    let y = x;              // COPY: i32 implementa Copy -> copia bit a bit
    println!("{x} y {y}");  // ✅ x sigue siendo válido
}
```

**Idea clave:** `String` posee memoria en el *heap*, por eso se **mueve**. `i32` vive entero en el *stack*, por eso se **copia**. El compilador elige automáticamente según si el tipo implementa `Copy`.

### 2️⃣ Borrowing: `&T` vs `&mut T`

```rust
fn longitud(s: &String) -> usize { s.len() }   // préstamo inmutable: solo lee
fn agregar(s: &mut String) { s.push_str(" mundo"); } // préstamo mutable: escribe

fn main() {
    let mut s = String::from("hola");
    let n = longitud(&s);   // presto para leer
    agregar(&mut s);        // presto para escribir
    println!("{s} (longitud previa: {n})"); // "hola mundo (longitud previa: 4)"
}
```

La regla del *borrow checker* — **`&T` XOR `&mut T`** — en acción:

```rust
fn main() {
    let mut v = vec![1, 2, 3];
    let primero = &v[0];     // préstamo inmutable activo
    v.push(4);               // ❌ NO COMPILA: ya hay un &v vivo, push necesita &mut v
    println!("{primero}");
}
// Solución: acorta el scope del préstamo (usa `primero` antes de `push`,
// o clona el valor barato: let primero = v[0];)
```

### 3️⃣ Slices (`&str`, `&[T]`): vistas sin ownership

```rust
/// Devuelve la primera palabra de `s` sin copiar nada (solo una vista).
fn primera_palabra(s: &str) -> &str {
    for (i, c) in s.char_indices() {
        if c == ' ' { return &s[..i]; }
    }
    s
}

fn main() {
    let frase = String::from("hola mundo cruel");
    let palabra = primera_palabra(&frase); // &String coacciona a &str (Deref)
    println!("{palabra}");                 // "hola"

    let nums = [10, 20, 30, 40];
    let medio: &[i32] = &nums[1..3];       // slice de array
    println!("{:?}", medio);               // [20, 30]
}
```

### 4️⃣ Structs + métodos (`impl`)

```rust
#[derive(Debug, Clone, PartialEq)]
struct Rect { ancho: u32, alto: u32 }

impl Rect {
    fn nuevo(ancho: u32, alto: u32) -> Self { Self { ancho, alto } } // constructor asociado
    fn area(&self) -> u32 { self.ancho * self.alto }                 // método (toma &self)
    fn es_cuadrado(&self) -> bool { self.ancho == self.alto }
}

fn main() {
    let r = Rect::nuevo(3, 4);
    println!("area = {}, cuadrado = {}", r.area(), r.es_cuadrado());
    println!("{r:?}");          // Debug: Rect { ancho: 3, alto: 4 }
}
```

### 5️⃣ Enums + `match` + `Option<T>`

```rust
#[derive(Debug)]
enum Forma {
    Circulo(f64),
    Rectangulo(f64, f64),
}

fn area(f: &Forma) -> f64 {
    match f {                                          // match exhaustivo
        Forma::Circulo(r) => std::f64::consts::PI * r * r,
        Forma::Rectangulo(a, b) => a * b,
    }
}

/// Devuelve el índice del objetivo, o `None` si no está.
fn buscar(v: &[i32], objetivo: i32) -> Option<usize> {
    for (i, &x) in v.iter().enumerate() {
        if x == objetivo { return Some(i); }
    }
    None
}

fn main() {
    println!("{:.2}", area(&Forma::Circulo(2.0)));     // 12.57
    match buscar(&[10, 20, 30], 20) {
        Some(i) => println!("encontrado en {i}"),      // encontrado en 1
        None => println!("no está"),
    }
}
```

### 6️⃣ Colecciones: el patrón *entry API* de `HashMap`

```rust
use std::collections::HashMap;

/// Cuenta cuántas veces aparece cada palabra.
fn contar_palabras(texto: &str) -> HashMap<&str, u32> {
    let mut conteo = HashMap::new();
    for palabra in texto.split_whitespace() {
        *conteo.entry(palabra).or_insert(0) += 1; // entry: 1 solo hash, inserta-o-actualiza
    }
    conteo
}

fn main() {
    let c = contar_palabras("rust es rust y rust mola");
    println!("{}", c["rust"]); // 3
}
```

### 7️⃣ Manejo de errores: tipo propio + `From` + operador `?`

```rust
use std::fmt;

#[derive(Debug)]
enum AppError {
    Vacio,
    NoNumerico(std::num::ParseIntError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::Vacio => write!(f, "la entrada está vacía"),
            AppError::NoNumerico(e) => write!(f, "no es un número: {e}"),
        }
    }
}
impl std::error::Error for AppError {}

// Esta impl es lo que permite que `?` convierta el error automáticamente.
impl From<std::num::ParseIntError> for AppError {
    fn from(e: std::num::ParseIntError) -> Self { AppError::NoNumerico(e) }
}

fn parsear(s: &str) -> Result<i32, AppError> {
    if s.is_empty() { return Err(AppError::Vacio); }
    let n: i32 = s.trim().parse()?; // ParseIntError -> AppError gracias a From
    Ok(n)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", parsear("42")?);        // 42
    println!("{:?}", parsear(""));         // Err(Vacio)
    println!("{:?}", parsear("x"));        // Err(NoNumerico(...))
    Ok(())
}
```

> **Conexión con el resto del capítulo:** la sintaxis básica (impresión, números, comentarios) está en
> [Instalación y primeros pasos](section_01.md) y [Aritmética en Rust](section_02.md). Aquí cubrimos el
> **núcleo conceptual** (ownership, borrowing, ADTs, errores) que distingue a Rust de C/C++.

---

## ✅ CHECKLIST FINAL MES 1 (¿Estás listo para Mes 2?)

- [ ] **Entorno:** `rustup`, `cargo-watch`, `clippy`, `rust-analyzer` funcionando. `cargo fmt` en save.
- [ ] **Rustlings:** **100% completado** (hasta `threads` y `macros` básicos si tienes tiempo).
- [ ] **Ownership:** Explicas *Move vs Copy vs Clone* a un compañero sin mirar apuntes. Dibujas Stack/Heap.
- [ ] **Borrowing:** Entiendes por qué `&mut` es exclusivo. Escribes funciones tomando `&str` / `&[T]` naturalmente.
- [ ] **Enums/Pattern Matching:** Modelas estados con `enum`. `match` exhaustivo es tu amigo. `Option`/`Result` son tipos normales.
- [ ] **Error Handling:** Defines `AppError`, implementas `From`, usas `?` operator. `main` devuelve `Result`.
- [ ] **Módulos:** Estructura `src/` limpia. `pub use` para API pública. `crate::` paths claros.
- [ ] **Proyecto Todo List v2:** Compila `clippy` limpio. Tests pasan. Persistencia JSON real. Separación `models`/`storage`/`commands`.
- [ ] **Documentación:** `cargo doc --open` genera docs legibles para tus tipos públicos (`///` comments).

---

### 🚀 PRÓXIMO PASO: MES 2
> **Generics, Traits avanzados, Lifetimes en structs, Smart Pointers (`Box`, `Rc`, `RefCell`), Testing profesional, Publicar Crate.**

*¿Listo? La curva se aplana. Lo prometido es deuda: a partir de Mes 2, Rust se siente como un superpoder.* 🦀
