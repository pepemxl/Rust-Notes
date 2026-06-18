# Curso de Rust 🦀


## 🗓️ 1: FUNDAMENTOS Y EL "BORROW CHECKER" (La curva de aprendizaje)
**Objetivo:** Escribir código que compile sin luchar contra el compilador. Entender *Ownership, Borrowing, Lifetimes*.

| Semana | Temas Clave | Recursos Obligatorios | Práctica / Mini-Proyecto |
| :--- | :--- | :--- | :--- |
| **1** | **Setup & Básicos:** Instalación, Cargo, Variables, Mutabilidad, Tipos escalares/compuestos, Funciones, Comentarios. | 📖 *The Book* Cap 1-3 <br> 🎥 *Rustlings* (Primeros ejercicios) | Configurar entorno. `cargo new hello`. Resolver **Rustlings: `variables`, `functions`, `if`**. |
| **2** | **Ownership & Borrowing (EL CORAZÓN):** Stack vs Heap, Ownership rules, Move vs Clone, References (`&T`, `&mut T`), Reglas del Borrow Checker, Slices. | 📖 *The Book* Cap 4 (¡Léelo 2 veces!) <br> 🧠 *Jon Gjengset - "Crust of Rust: Ownership"* (YouTube) | **Rustlings: `move_semantics`, `references`, `slices`**. <br> **Ejercicio:** Implementar `split_string` manualmente sin `split()`. |
| **3** | **Estructuras de Datos & Enums:** Structs, Tuple Structs, Unit Structs, **Enums (Algebraic Data Types)**, `Option<T>`, `Result<T, E>`, `match`, `if let`, `while let`, Pattern Matching exhaustivo. | 📖 *The Book* Cap 5-6 <br> 📖 *Rust by Example: Enums/Pattern Matching* | **Rustlings: `enums`, `option`, `result`, `match`**. <br> **Mini-Proyecto:** **CLI "Todo List" simple** (solo memoria, usa `Vec<Task>`, `enum Status`, `match` para menú). |
| **4** | **Módulos, Crates, Colecciones & Error Handling:** Sistema de módulos (`mod`, `use`, `pub`), `Vec`, `HashMap`, `String` vs `&str`, Propagación de errores (`?`), `unwrap`/`expect` (cuándo NO usarlos), Traits básicos (`Debug`, `Display`, `Clone`, `Copy`). | 📖 *The Book* Cap 7-9 <br> 📖 *Error Handling in Rust* (Blog de Blog.logrocket o similar) | **Rustlings: `modules`, `hashmap`, `error_handling`**. <br> **Refactor:** Separa el Todo List en módulos (`models`, `storage`, `cli`). Maneja errores de I/O con `Result`. |

> **✅ Hito Mes 1:** Pasar **todos los ejercicios de `rustlings`** (incluyendo `threads` y `macros` básicos) y tener el **Todo List CLI funcionando y modularizado**.


