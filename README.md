# Curso de Rust 🦀


## MES 🗓️ 1: FUNDAMENTOS Y EL "BORROW CHECKER" (La curva de aprendizaje)
**Objetivo:** Escribir código que compile sin luchar contra el compilador. Entender *Ownership, Borrowing, Lifetimes*.

| Semana | Temas Clave | Recursos Obligatorios | Práctica / Mini-Proyecto |
| :--- | :--- | :--- | :--- |
| **1** | **Setup & Básicos:** Instalación, Cargo, Variables, Mutabilidad, Tipos escalares/compuestos, Funciones, Comentarios. | 📖 *The Book* Cap 1-3 <br> 🎥 *Rustlings* (Primeros ejercicios) | Configurar entorno. `cargo new hello`. Resolver **Rustlings: `variables`, `functions`, `if`**. |
| **2** | **Ownership & Borrowing (EL CORAZÓN):** Stack vs Heap, Ownership rules, Move vs Clone, References (`&T`, `&mut T`), Reglas del Borrow Checker, Slices. | 📖 *The Book* Cap 4 (¡Léelo 2 veces!) <br> 🧠 *Jon Gjengset - "Crust of Rust: Ownership"* (YouTube) | **Rustlings: `move_semantics`, `references`, `slices`**. <br> **Ejercicio:** Implementar `split_string` manualmente sin `split()`. |
| **3** | **Estructuras de Datos & Enums:** Structs, Tuple Structs, Unit Structs, **Enums (Algebraic Data Types)**, `Option<T>`, `Result<T, E>`, `match`, `if let`, `while let`, Pattern Matching exhaustivo. | 📖 *The Book* Cap 5-6 <br> 📖 *Rust by Example: Enums/Pattern Matching* | **Rustlings: `enums`, `option`, `result`, `match`**. <br> **Mini-Proyecto:** **CLI "Todo List" simple** (solo memoria, usa `Vec<Task>`, `enum Status`, `match` para menú). |
| **4** | **Módulos, Crates, Colecciones & Error Handling:** Sistema de módulos (`mod`, `use`, `pub`), `Vec`, `HashMap`, `String` vs `&str`, Propagación de errores (`?`), `unwrap`/`expect` (cuándo NO usarlos), Traits básicos (`Debug`, `Display`, `Clone`, `Copy`). | 📖 *The Book* Cap 7-9 <br> 📖 *Error Handling in Rust* (Blog de Blog.logrocket o similar) | **Rustlings: `modules`, `hashmap`, `error_handling`**. <br> **Refactor:** Separa el Todo List en módulos (`models`, `storage`, `cli`). Maneja errores de I/O con `Result`. |

> **✅ Hito Mes 1:** Pasar **todos los ejercicios de `rustlings`** (incluyendo `threads` y `macros` básicos) y tener el **Todo List CLI funcionando y modularizado**.

## 🗓️ MES 2: PROGRAMACIÓN GENERICA, TRAITS Y TESTING (Escribir Rust "Idiomático")
**Objetivo:** Abstracción sin costo (Zero-cost abstractions), Polimorfismo estático vs dinámico, Testing robusto.

| Semana | Temas Clave | Recursos Obligatorios | Práctica / Mini-Proyecto |
| :--- | :--- | :--- | :--- |
| **5** | **Generics, Traits & Lifetimes:** Sintaxis `<T>`, Trait Bounds (`T: Trait`), `impl Trait` (return/arg), **Lifetimes (`'a`)** en funciones/structs, Elision rules, `static` lifetime. | 📖 *The Book* Cap 10 (Generics/Traits/Lifetimes) <br> 🧠 *Jon Gjengset - "Crust of Rust: Lifetimes"* | **Rustlings: `generics`, `traits`, `lifetimes`**. <br> **Ejercicio:** Implementar un `struct Cache<K, V>` genérico con TTL usando `HashMap` y un trait `TimeProvider` para testear tiempo. |
| **6** | **Traits Avanzados:** Traits como objetos (`dyn Trait`), `Drop`, `From`/`Into`, `TryFrom`/`TryInto`, `Deref`/`DerefMut` (Smart Pointers intro), `Default`, `AsRef`/`AsMut`, Operator Overloading. | 📖 *The Book* Cap 19 (Advanced Traits) <br> 📖 *Rust Design Patterns: Traits* | **Ejercicio:** Crear un `struct Wrapper(Vec<u8>)` que implemente `Deref<Target=[u8]>`, `Write`, `Read`. Entender *Coerción de Deref*. |
| **7** | **Smart Pointers & Interior Mutability:** `Box<T>`, `Rc<T>`/`Arc<T>` (Conteo de referencias), `RefCell<T>`/`Mutex<T>`/`RwLock<T>` (Mutabilidad interior), Patrones `Rc<RefCell<T>>`, `Arc<Mutex<T>>`, Ciclos de referencia (`Weak<T>`). | 📖 *The Book* Cap 15 (Smart Pointers) <br> 🧠 *Jon Gjengset - "Crust of Rust: Interior Mutability"* | **Ejercicio:** Implementar un **Grafo simple** (nodos con hijos/padres) usando `Rc`/`Weak` y `RefCell`. Evitar memory leaks. |
| **8** | **Testing, Documentación & Tooling:** Unit vs Integration tests, `#[cfg(test)]`, `assert!`, `assert_eq!`, Doc tests (`/// ``` `), `cargo test --doc`, Benchmarks (`cargo bench` / `criterion`), `clippy` lints complejos, `rustfmt` config. | 📖 *The Book* Cap 11 (Testing) <br> 📖 *The Book* Cap 14 (Cargo/Crates.io) | **Proyecto Integrador Mes 2:** **Librería `config-loader`**. <br> - Carga config de `File`, `Env`, `CLI` (precedencia). <br> - API genérica `load<T: Deserialize>()`. <br> - **100% cobertura tests + Doc tests + Publicado en crates.io (privado o real).** |

> **✅ Hito Mes 2:** Dominar `dyn Trait` vs `impl Trait`, lifetimes en structs, smart pointers. Publicar una crate librería bien testeada y documentada.

---

## 🗓️ MES 3: ASYNC RUST Y ECOSISTEMA WEB (El "Runtime")
**Objetivo:** Entender *Futures*, *Executors*, *Pin/Unpin*, *Send/Sync*. Backend con **Axum** o **Actix-web**.

| Semana | Temas Clave | Recursos Obligatorios | Práctica / Mini-Proyecto |
| :--- | :--- | :--- | :--- |
| **9** | **Async Fundamentos:** `Future` trait, `poll`, `Pin`, `Unpin`, `async`/`await` desugaring, **Executors (Tokio)**, `spawn`, `join!`, `select!`, `timeout`. **Send + Sync** (Thread safety markers). | 📖 *Async Book* (rust-lang.github.io/async-book) Cap 1-4 <br> 🧠 *Jon Gjengset - "Crust of Rust: Async Basics / Pin"* | **Ejercicio:** Escribir un **mini-runtime** simple que ejecute 3 futures concurrentes sin Tokio (solo `std::future` + `waker` simple) para entender `Waker`/`Context`. |
| **10** | **Tokio Ecosistema & Web Server:** `tokio::spawn` vs `spawn_blocking`, `mpsc`/`oneshot`/`watch` channels, `Mutex`/`RwLock`/`Semaphore` (Tokio versions), **Axum**: Router, Extractors (`Path`, `Query`, `Json`, `Extension`), Handlers, Middleware (Tower), State management (`Arc<AppState>`). | 📖 *Axum Guide* (GitBook) <br> 📖 *Tokio Tutorial* (tokio.rs) | **Proyecto: API REST "Url Shortener"**. <br> - `POST /shorten` -> `short_code`. <br> - `GET /:code` -> Redirect 301. <br> - Storage: `Arc<DashMap>` (memoria) + `tokio::fs` (persistencia JSON Lines). <br> - Middleware: Logging, Rate Limiting simple. |
| **11** | **Bases de Datos & Serialización:** **SQLx** (Compile-time checked SQL, `sqlx::query!`), Migraciones, Pool de conexiones, **Serde** (derive macros, `serialize_with`, `deserialize_with`, `#[serde(flatten)]`, `skip`), JSON/TOML/YAML. | 📖 *SQLx Docs* (Offline mode / Prepare) <br> 📖 *Serde RS* (Attributes) | **Refactor Url Shortener:** <br> - Migrar a **PostgreSQL** (Docker). <br> - Usar `sqlx::query_as!` para seguridad de tipos. <br> - Añadir `created_at`, `expires_at`, `click_count`. <br> - Tests de integración con `testcontainers` o DB separada. |
| **12** | **Observabilidad & Deployment:** `tracing` / `tracing-subscriber` (structured logging), `metrics` / `prometheus`, Health checks (`/health`, `/ready`), Dockerfile **multi-stage** (builder + runtime `distroless`/`gdb`), `cargo-chef` para cache de dependencias, CI/CD (GitHub Actions: test, clippy, fmt, audit, build, docker push). | 📖 *Tracing Docs* <br> 📖 *Docker + Rust Best Practices* | **Entrega Final Mes 3:** **Url Shortener "Production Ready"**. <br> - Logs JSON estructurados. <br> - Métricas Prometheus (`http_requests_total`, `db_latency`). <br> - Dockerfile optimizado (< 20MB imagen final). <br> - Pipeline CI passing. |

> **✅ Hito Mes 3:** API Async robusta, tipada, observable y contenedorizada. Entender *por qué* `Pin` y `Send/Sync` importan en async.

---

## 🗓️ MES 4: SISTEMAS, CLI AVANZADO Y WASM (Rust "Close to Metal")
**Objetivo:** FFI, CLI UX profesional, WebAssembly, Parsing.

| Semana | Temas Clave | Recursos Obligatorios | Práctica / Mini-Proyecto |
| :--- | :--- | :--- | :--- |
| **13** | **CLI Profesional:** **Clap** (Derive API v4+), Subcomandos, Args/Flags/Env vars, Completions (bash/zsh/fish), Coloreo (`anstyle`, `owo-colors`), Progress bars (`indicatif`), TUI intro (`ratatui`). | 📖 *Clap Docs* (Tutorial) <br> 📖 *Ratatui Tutorial* | **Proyecto: `mytool` (CLI Multiusos).** <br> - Subcomandos: `encrypt`, `decrypt`, `hash`, `generate-password`. <br> - Uso de `indicatif` para barras de progreso en hash de archivos grandes. <br> - Generar `man pages` y completions en build. |
| **14** | **FFI (C Interop) & `unsafe`:** `extern "C"`, `#[no_mangle]`, `libc` / `libloading`, Bindgen (generar bindings automáticos), **Reglas de `unsafe`**, `std::ffi::CString`/`CStr`, Layout (`#[repr(C)]`), Invocar C desde Rust y Rust desde C. | 📖 *The Book* Cap 19 (FFI) <br> 📖 *Rust FFI Omnibus* (Blog de Michael Bryan) <br> 🧠 *Crust of Rust: Unsafe* | **Ejercicio:** Wrapper seguro para **`libsodium`** (crypto) o **`sqlite3`** (C API). <br> - Escribir `build.rs` con `bindgen` / `cc`. <br> - API 100% Safe Rust encima de `unsafe` internals. Tests contra librería C real. |
| **15** | **WebAssembly (Wasm):** `wasm-bindgen`, `wasm-pack`, `web-sys` / `js-sys`, Intercambio de tipos (JsValue, arrays, objetos), `console_error_panic_hook`, `wee_alloc`, Paralelismo (`wasm-bindgen-rayon`), Integración con **Vite/Next.js/Leptos/Yew**. | 📖 *Rust Wasm Book* (rustwasm.github.io/book) <br> 📖 *Leptos / Yew Tutorial* (Escoge uno) | **Proyecto: "Mandelbrot Explorer" (Wasm + Frontend).** <br> - Rust: Cálculo paralelo de fractal (`rayon` -> `wasm-bindgen-rayon`). <br> - Frontend (JS/TS/Leptos): Canvas rendering, Zoom/Pan, UI controles. <br> - Medir rendimiento vs JS puro. |
| **16** | **Parsing & Text Processing:** **Nom** (Parser Combinators), **Pest** (PEG Grammar), **Regex** (`regex` crate, `aho-corasick`), `encoding_rs` (non-UTF8), Streaming parsers. | 📖 *Nom Tutorial* (GitHub) <br> 📖 *Pest Book* | **Mini-Proyecto:** **Log Parser CLI**. <br> - Parsear logs `nginx` / `json` / `syslog` streaming (line by line). <br> - Filtrar, agrupar, exportar CSV/JSON. <br> - Benchmark vs `grep`/`jq`/`awk`. |

> **✅ Hito Mes 4:** CLI publicable (`cargo install`), Wrapper FFI seguro, Módulo Wasm funcionando en navegador, Parser robusto.

---

## 🗓️ MES 5: ARQUITECTURA, PATRONES Y RENDIMIENTO (Senior Level)
**Objetivo:** Diseño de sistemas, Concurrency patterns, Profiling, Optimización.

| Semana | Temas Clave | Recursos Obligatorios | Práctica / Mini-Proyecto |
| :--- | :--- | :--- | :--- |
| **17** | **Patrones de Diseño en Rust:** Newtype, Builder, Typestate (PhantomData), Strategy (Traits), Observer (Channels), Actor Model (Actix/tokio actors), Dependency Injection (Traits + Generics / `dyn`), Configuration (Figment / Config). | 📖 *Rust Design Patterns* (rust-unofficial.github.io/patterns) <br> 📖 *Typestate Pattern en Rust* | **Refactor:** Reescribe el **Url Shortener (Mes 3)** usando **Actor Model** (Actix o `tokio` + channels) para el contador de clicks (evitar lock contention en `Arc<Mutex>`). Implementa **Typed State** para `Url` (Draft -> Active -> Expired). |
| **18** | **Concurrencia Avanzada & Lock-Free:** `crossbeam` (channels, epoch GC), `parking_lot` (Mutex más rápido), `dashmap` (Concurrent HashMap), **Atomics** (`Ordering::Relaxed`, `Acquire`, `Release`, `SeqCst`), `Rayon` (Data Parallelism), `tokio::task::spawn_blocking` para CPU-bound. | 📖 *Rust Atomics and Locks* (Mara Bos - **LIBRO CLAVE, leer caps seleccionados**) <br> 📖 *Crossbeam Docs* | **Benchmark:** Implementar **Contador Distribuido** (sharded counter) usando `AtomicUsize` + `Ordering::Relaxed` vs `Mutex` vs `DashMap` vs `Actor`. Graficar throughput vs threads. Entender *False Sharing* (`#[repr(align(64))]`). |
| **19** | **Profiling & Optimización:** `perf` / `flamegraph` (`cargo flamegraph`), `criterion` (microbenchmarks), `dhatu` / `heaptrack` (memoria), **Optimizaciones:** Inlining (`#[inline]`), Monomorphization bloat, `SmallVec` / `ArrayVec` (evitar alloc), `Cow` (Clone on Write), String interning. | 📖 *The Rust Performance Book* (nnethercote.github.io/perf-book) <br> 🧠 *Crust of Rust: Profiling* | **Optimización Real:** Tomar el **Log Parser (Mes 4)** o **Mandelbrot (Mes 4)**. <br> 1. Profilear (`flamegraph`). <br> 2. Identificar *Hot paths*. <br> 3. Aplicar 3 optimizaciones (ej. `SmallVec`, `Cow`, `ahash` hasher, `simd` via `packed_simd` o `std::simd` nightly). <br> 4. Medir ganancia con `criterion`. Documentar en `README`. |
| **20** | **Embedded / No-Std (Opcional pero recomendado):** `no_std`, `alloc`, `panic_handler`, `cortex-m`, `embassy` (async embedded), `probe-run`, RTIC. *Si no te interesa embedded, profundiza en **Compiler Internals** (`rustc_dev_guide`), **Procedural Macros** (`syn`, `quote`, `proc-macro2`), **Cargo Workspace** management.* | 📖 *Discovery Book* (Embedded) <br> 📖 *Rustc Dev Guide* <br> 📖 *Cargo Workspaces* | **Proyecto Final Mes 5 (Elección):** <br> **A) Embedded:** Blinky + Sensor (ej. BMP280) en **Embassy (async)** en QEMU o hardware real (RP2040/STM32). <br> **B) Proc Macro:** Crear derive `#[derive(Builder)]` funcional. <br> **C) Workspace:** Monorepo con `api`, `cli`, `core`, `db`, `macros` crates compartiendo `Cargo.lock`. |

> **✅ Hito Mes 5:** Código rápido, medible, bien arquitecturado. Entender *Zero-cost abstractions* en la práctica. Saber cuándo `unsafe` es necesario y cómo encapsularlo.

---

## 🗓️ MES 6: PROYECTO FINAL CAPSTONE & ESPECIALIZACIÓN
**Objetivo:** Proyecto real de portfolio que integre todo. Profundizar en tu nicho.

### Fase 1: Diseño (Semana 21)
*   **Escribe un `DESIGN.md`**: Arquitectura, Diagramas (Mermaid), Elección de crates (justificación), Modelo de datos, API Contract (OpenAPI), Threat Model (Seguridad básica).
*   **Stack Sugerido:** Axum + SQLx (Postgres) + Redis (Caché/Sessions) + Tokio + Tracing + Clap (CLI Admin) + Wasm (Frontend opcional) + Tests E2E.

### Fase 2: Implementación Core (Semana 22-23)
*   **Core Library (`crate-core`):** Lógica de negocio pura, sin I/O, 100% testable (Unit/Property-based `proptest`).
*   **Server (`crate-server`):** Axum, Extracción de State, Middleware Auth (JWT/JWKS), Rate Limiting, Graceful Shutdown.
*   **CLI Admin (`crate-cli`):** Migraciones DB, Gestión usuarios, Stats, Backup/Restore.
*   **Infra:** `docker-compose.yml` (dev), `Dockerfile` (prod), `sqlx` offline mode, GitHub Actions (Test -> Build -> Deploy Staging).

### Fase 3: Pulido y Especialización (Semana 24)
*   **Elige tu "Mastery Path":**
    1.  **Backend/Cloud:** gRPC (`tonic`), Kubernetes (`kube-rs`), Service Mesh, Distributed Tracing (OpenTelemetry).
    2.  **Systems/CLI:** Completion generators, Plugin system (dynamic loading `libloading`), Self-updater.
    3.  **Wasm/Fullstack:** Leptos/Yew/Dioxus (SSR + Hydration), Optimización bundle size (`wasm-opt`, `twiggy`).
    4.  **Data/ML:** `polars` / `arrow-rs` (DataFrames), `burn` / `candle` (ML), `datafusion` (Query Engine).
    5.  **Embedded:** `embassy` + `defmt` (logging), `probe-rs`, RTOS concepts.
*   **Documentación Final:** `README` profesional, `ARCHITECTURE.md`, `CONTRIBUTING.md`, Changelog, Licencia.
*   **Charla/Demo:** Grava un video de 10 min explicando la arquitectura y decisiones técnicas (simula una tech interview).

---

## 📚 BIBLIOTECA DE REFERENCIA (The "Rustacean" Bookshelf)

1.  **The Rust Programming Language (The Book)** - *Gratis online.* **Base absoluta.**
2.  **Rust by Example** - *Gratis online.* Código > Palabras.
3.  **Rustlings** - *Repo GitHub.* **Imprescindible Mes 1-2.** Ejercicios guiados.
4.  **Effective Rust (TBD / Logrocket/Jon Gjengset posts)** - Patrones intermedios.
5.  **Zero To Production In Rust (Luca Palmieri)** - **Pago/Gratis parcial.** *La biblia del Backend profesional en Rust (Axum/SQLx/Testing/CI).* Cómpralo o síguelo online.
6.  **Rust Atomics and Locks (Mara Bos)** - **Pago.** *Obligatorio Mes 5 para concurrencia real.*
7.  **The Rust Performance Book (Nicholas Nethercote)** - *Gratis online.* Optimización.
8.  **Rust Design Patterns** - *Gratis online.* Patrones idiomáticos.
9.  **Async Rust Book** - *Gratis online.* Fundamentos async.
10. **This Week in Rust (Newsletter)** - Suscríbete. Mantente al día.

---

## 🛠️ HERRAMIENTAS DIARIAS (Instala YA)
```bash
rustup component add rustfmt clippy rust-src rust-analyzer
cargo install cargo-edit cargo-watch cargo-tree cargo-outdated cargo-audit cargo-deny cargo-nextest cargo-criterion cargo-flamegraph cargo-tarpaulin sqlx-cli sea-orm-cli taplo-cli # LSP TOML
# Editores: VS Code (rust-analyzer) / Neovim (rustaceanvim) / IntelliJ (Rust plugin)
```

