# 🦀 MES 5: ARQUITECTURA, PATRONES Y RENDIMIENTO — Guía Detallada (Senior Level)
> **Filosofía del Mes:** *"Rust te obliga a modelar el estado y la concurrencia en el sistema de tipos. Aquí es donde dejas de 'hacer que compile' y empiezas a 'diseñar para que escale, sea mantenible y rápido'. Zero-cost abstractions no son gratis: requieren disciplina arquitectónica."*
> **Meta:** Dominar patrones idiomáticos avanzados, concurrencia *lock-free* real, profiling científico y optimización guiada por datos. Entregar un artefacto que demuestre maestría: **Sistema Actor refactorizado**, **Benchmark de concurrencia riguroso**, **Optimización documentada** y **Proyecto Final de especialización**.

---

## 📅 SEMANA 17: PATRONES DE DISEÑO EN RUST — MODELADO DE ESTADO Y ARQUITECTURA
**Objetivo:** Internalizar patrones que aprovechan el sistema de tipos (Typestate, Newtype, Actor) para hacer imposibles los estados inválidos y gestionar concurrencia sin locks manuales.

### 🎯 Conceptos Clave

#### 1. **Newtype Pattern** (Wrapper de Tupla)
```rust
// Encapsulación, seguridad de tipos, implementación de traits externos (Orphan Rule)
struct UserId(Uuid); // No String, no Uuid directo
struct Email(String); // Validado en constructor

impl UserId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn as_uuid(&self) -> Uuid { self.0 }
}
// Derive: Deref, Display, Serialize, Deserialize, From<Uuid>, Into<Uuid>
```
*   **Uso:** IDs, Validadores (Email, NonEmptyString), Unidades (Meters, Seconds, Bytes).

#### 2. **Builder Pattern** (Con `Default` + `Into` + Generics)
```rust
#[derive(Default)]
struct ConfigBuilder<HasUrl = (), HasTimeout = ()> {
    url: Option<String>,
    timeout: Option<Duration>,
    // PhantomData para marcar estado en tipo
    _state: std::marker::PhantomData<(HasUrl, HasTimeout)>,
}

impl ConfigBuilder<(), ()> {
    pub fn new() -> Self { Default::default() }
}

impl<HasTimeout> ConfigBuilder<(), HasTimeout> {
    pub fn url(mut self, url: impl Into<String>) -> ConfigBuilder<Set, HasTimeout> {
        self.url = Some(url.into());
        // Transición de tipo segura
        ConfigBuilder { url: self.url, timeout: self.timeout, _state: PhantomData }
    }
}

// Terminación: Solo compila si HasUrl=Set Y HasTimeout=Set
impl ConfigBuilder<Set, Set> {
    pub fn build(self) -> Config { Config { url: self.url.unwrap(), timeout: self.timeout.unwrap() } }
}
```
*   **Tipado:** `Set` / `Unset` (unit structs). **Compile-time enforcement** de campos requeridos.

#### 3. **Typestate Pattern** (El "Killer Feature" de Rust)
*   **Problema:** Objeto `Url` puede ser `Draft`, `Active`, `Expired`. Métodos `publish()`, `expire()`, `click()` solo válidos en ciertos estados.
*   **Solución:** Estado en **Tipo**, no en campo `enum`.
```rust
// Estados (Marcadores, Zero-Sized)
struct Draft; struct Active; struct Expired;

struct Url<State> {
    code: String,
    target: String,
    clicks: u64,
    _state: PhantomData<State>, // Clave: usa el parámetro genérico
}

// Constructores
impl Url<Draft> {
    fn new(code: String, target: String) -> Self { Self { code, target, clicks: 0, _state: PhantomData } }
    fn publish(self) -> Url<Active> { // Consume Draft, produce Active
        Url { code: self.code, target: self.target, clicks: 0, _state: PhantomData }
    }
}

// Métodos por Estado
impl Url<Active> {
    fn click(&mut self) { self.clicks += 1; }
    fn expire(self) -> Url<Expired> { Url { ... clicks: self.clicks, _state: PhantomData } }
}

impl Url<Expired> {
    fn redirect_url(&self) -> Option<&str> { None } // Expirados no redirigen
}
```
*   **Ventaja:** **Imposible** llamar `click()` en `Url<Draft>` o `Url<Expired>`. **Cero overhead runtime** (PhantomData = 0 bytes).

#### 4. **Actor Model en Rust** (Concurrencia Estructurada sin Locks)
*   **Principio:** *No compartas memoria para comunicarte; comunica para compartir memoria.*
*   **Implementación:** `tokio::spawn` + `mpsc::channel` (bounded para backpressure).
*   **Actor:** Struct con `receiver` (Mailbox). Método `handle(msg)` muta estado **local** (sin `Mutex`).
*   **Handle:** `Sender<Msg>` clonado barato (`ActorHandle`). `Send` + `Sync`.

```rust
// Mensajes
enum CounterMsg { Increment, Get(oneshot::Sender<u64>), Stop }

// Actor
struct ClickCounter { count: u64, rx: mpsc::Receiver<CounterMsg> }

impl ClickCounter {
    async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                CounterMsg::Increment => self.count += 1,
                CounterMsg::Get(tx) => { let _ = tx.send(self.count); },
                CounterMsg::Stop => break,
            }
        }
    }
}

// Handle (API Pública)
#[derive(Clone)]
struct CounterHandle { tx: mpsc::Sender<CounterMsg> }

impl CounterHandle {
    pub async fn increment(&self) { self.tx.send(CounterMsg::Increment).await.ok(); }
    pub async fn get(&self) -> u64 {
        let (tx, rx) = oneshot::channel();
        self.tx.send(CounterMsg::Get(tx)).await.ok();
        rx.await.unwrap_or(0)
    }
}

// Spawn
fn start_counter() -> CounterHandle {
    let (tx, rx) = mpsc::channel(100); // Bounded!
    let actor = ClickCounter { count: 0, rx };
    tokio::spawn(actor.run());
    CounterHandle { tx }
}
```

#### 5. **Dependency Injection (DI) en Rust**
*   **Generics (Static Dispatch):** `struct Service<DB: Database> { db: DB }`. **Preferido** (rendimiento, inference).
*   **Dyn Trait (Dynamic Dispatch):** `struct Service { db: Box<dyn Database> }`. Para plugins, carga dinámica, reducir code bloat.
*   **Traits como Interfaces:** `trait Database { async fn get(&self, id: Id) -> Result<...>; }`.
*   **Config:** `figment` / `config` crate (Layered: File -> Env -> CLI). `Figment::new().merge(Serialized::default(Config::default())).merge(Env::prefixed("APP_")).extract()`.

### 🛠️ Proyecto: **Refactor Url Shortener v3 — Actor Model + Typestate**

#### Requisitos Arquitectónicos
1.  **Typestate para `UrlEntry`**:
    *   `UrlEntry<Draft>` -> `publish()` -> `UrlEntry<Active>`.
    *   `UrlEntry<Active>` -> `click()` (mut) / `expire()` -> `UrlEntry<Expired>`.
    *   `UrlEntry<Expired>` -> Solo lectura (`redirect_url() -> None`).
    *   Storage guarda `Box<dyn Any>` o Enum `UrlStateVariant` (para persistencia) pero lógica usa Tipos.

2.  **Actor `ClickCounter`**:
    *   Reemplaza `Arc<DashMap<Code, AtomicU64>>` o `Arc<Mutex<HashMap>>`.
    *   Un Actor **por URL** (muchos actores ligeros) **O** Un Actor **Sharded** (HashMap interno `HashMap<Code, u64>`).
    *   *Recomendación:* **Sharded Actor** (ej. 16 shards) para balancear carga y reducir contención en Mailbox único.
    *   `CounterMsg::Increment(code)`, `CounterMsg::GetBatch(vec<Code>, oneshot::Sender<HashMap>)`.

3.  **Actor `PersistenceWriter`** (Opcional):
    *   Recibe `PersistMsg::Upsert(UrlEntry)`, `PersistMsg::IncrementClick(code)`.
    *   Batch writes a DB (Postgres) cada N ms o N ops. Desacopla latencia de request de latencia de disco.

4.  **Integración Axum**:
    *   `AppState` contiene `CounterHandle` (o `Vec<CounterHandle>` shards) y `PersistenceHandle`.
    *   Handler `redirect`: `state.counter.increment(code).await` (fire-and-forget o await para backpressure). `state.persistence.increment(code).await`.

---

## 📅 SEMANA 18: CONCURRENCIA AVANZADA & LOCK-FREE — LA VERDAD BAJO EL CAPÓ
**Objetivo:** Entender *Memory Ordering* (`Acquire`/`Release`/`Relaxed`/`SeqCst`), *False Sharing*, *Epoch-based Reclamation* (`crossbeam`). Escribir estructuras de datos concurrentes correctas y rápidas.

### 🎯 Conceptos Clave (Basado en **"Rust Atomics and Locks" de Mara Bos** — **Lectura Obligatoria**)

#### 1. **Memory Ordering** (El contrato con el CPU/Compiler)
| Ordering | Garantía | Uso Típico |
| :--- | :--- | :--- |
| **`SeqCst` (Sequentially Consistent)** | Orden total global. **Por defecto**. Lento. | Contadores globales simples, flags de inicialización única. |
| **`Acquire` (Load) / `Release` (Store)** | **Sincronización Punto-a-Punto**. Load `Acquire` ve todo lo que Store `Release` previo escribió. | **Patrón estándar:** Flag `ready` (Release store) -> Data write -> Flag `true` (Acquire load) -> Read Data. |
| **`Relaxed`** | Solo atomicidad (no tearing). **Sin ordenamiento** respecto a otras variables. | Contadores estadísticos, generación IDs, estadísticas donde el orden exacto no importa. **Más rápido**. |
| **`AcqRel` (RMW)** | Load `Acquire` + Store `Release` atómico. | `fetch_add`, `compare_exchange` en locks/flags. |

#### 2. **False Sharing** (El asesino silencioso del rendimiento)
*   **Problema:** Dos `AtomicUsize` en hilos distintos pero en **misma Cache Line (64 bytes)**. CPU invalida cache line constantemente ("Ping-pong").
*   **Solución:** `#[repr(align(64))] struct PaddedAtomic(AtomicUsize);` o `cache_padded::CachePadded<AtomicUsize>` (crate `cache-padded`).

#### 3. **Estructuras Lock-Free / Wait-Free**
*   **`crossbeam::channel`**: MPSC/SPSC bounded/unbounded. **Más rápido que `std::sync::mpsc` y `tokio::sync::mpsc`** para non-async.
*   **`crossbeam::epoch`**: **Epoch-based Garbage Collection** para lock-free data structures (Listas, Maps, Queues). Permite `defer_free` seguro sin `Arc` overhead.
*   **`dashmap::DashMap`**: Sharded `RwLock` (std) o `Mutex` (parking_lot). **Read-heavy workloads**.
*   **`parking_lot::Mutex` / `RwLock`**: Implementación en user-space (futex). **Mucho más rápido que `std::sync`** (sin envenenamiento, menos syscalls). Úsalo **siempre** en sync code (no async `.await` en guard).

#### 4. **Rayon (Data Parallelism)**
*   `data.par_iter().map(...).collect()`.
*   **Work Stealing:** Ideal para CPU-bound embarrassingly parallel.
*   **`ThreadPoolBuilder`**: Controlar `num_threads`, `stack_size`, `thread_name`.
*   **Integración Async:** `tokio::task::spawn_blocking(|| rayon::scope(...))` para no bloquear runtime Tokio.

### 🛠️ Benchmark Científico: **Contador Distribuido (Sharded Counter)**

#### Implementaciones a Comparar (`benches/counter_bench.rs`)
```rust
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use dashmap::DashMap;
use crossbeam::channel;

// 1. Atomic Relaxed (Sharded + Cache Padded)
#[repr(align(64))] struct PaddedAtomic(AtomicUsize);
struct RelaxedCounter { shards: Box<[PaddedAtomic]> }
impl RelaxedCounter { 
    fn inc(&self, idx: usize) { self.shards[idx].0.fetch_add(1, Ordering::Relaxed); }
    fn get(&self) -> usize { self.shards.iter().map(|s| s.0.load(Ordering::Relaxed)).sum() }
}

// 2. Mutex (parking_lot) Sharded
struct MutexCounter { shards: Box<[Mutex<usize>]> }

// 3. DashMap (Single Key)
struct DashMapCounter { map: DashMap<usize, usize> } // Key = ThreadID % Shards

// 4. Actor (Crossbeam Channel)
struct ActorCounter { handles: Vec<crossbeam::channel::Sender<Msg>> } // Sharded Actors

// Benchmark Harness
fn bench_counters(c: &mut Criterion) {
    let mut group = c.c.c.benchmark_group("counters");
    for threads in [1, 2, 4, 8, 16, 32, 64] {
        group.throughput(Throughput::Elements(1_000_000));
        
        // Relaxed Atomic
        group.bench_with_input(BenchmarkId::new("AtomicRelaxed", threads), &threads, |b, &t| {
            let c = Arc::new(RelaxedCounter::new(t));
            b.iter(|| { /* spawn t threads, each inc 1M/t times */ });
        });
        // ... repeat for Mutex, DashMap, Actor ...
    }
}
```
**Análisis Requerido en README:**
1.  Gráfico **Throughput (ops/s) vs Threads**.
2.  Explicar por qué `Relaxed` + `Padded` gana en write-heavy.
3.  Explicar colapso de `Mutex`/`DashMap` por contención / cache coherence.
4.  Medir **Latencia (p99/p99.9)** no solo throughput.

---

## 📅 SEMANA 19: PROFILING & OPTIMIZACIÓN — CIENCIA SOBRE INTUICIÓN
**Objetivo:** Metodología: **Medir -> Entender -> Optimizar -> Verificar**. Herramientas: `perf`, `flamegraph`, `criterion`, `heaptrack`, `cargo-llvm-lines`.

### 🎯 Conceptos Clave

#### 1. **Toolchain de Profiling**
| Herramienta | Qué mide | Cómo usar |
| :--- | :--- | :--- |
| **`cargo flamegraph`** | CPU (Flame Graph SVG). Requiere `perf` (Linux) / `dtrace` (macOS) / `DTrace` (Windows). | `cargo flamegraph --bin myapp -- args`. Abrir SVG en navegador. Buscar "platos anchos" (hot paths). |
| **`perf record -g -- call-graph dwarf ./target/release/app`** | Bajo nivel, kernel + user. | `perf report --stdio` / `perf script | inferno-flamegraph >.svg`. |
| **`criterion`** | Microbenchmarks estadísticos (media, mediana, desviación, outliers). | **Único modo confiable** de comparar `fn a()` vs `fn b()`. `black_box` esencial. |
| **`heaptrack` / `dhat` (valgrind)** | Allocaciones (heap), peak memory, leaks, vida objetos. | `heaptrack ./app` -> `heaptrack_gui` / `heaptrack_print`. `dhat` para "leaks temporales" (memoria viva mucho tiempo). |
| **`cargo llvm-lines` / `cargo bloat`** | **Code Bloat** (Monomorphization). ¿Cuántas instancias de `Vec<T>`? ¿Tamaño binario? | `cargo bloat --release --crates`. `cargo llvm-lines --release`. |

#### 2. **Técnicas de Optimización Comunes en Rust**
| Técnica | Qué resuelve | Costo / Trade-off |
| :--- | :--- | :--- |
| **`#[inline]` / `#[inline(always)]`** | Elimina call overhead, permite optimizaciones cross-fn (const prop, SIMD). | **Code Bloat**. Úsalo en funciones *pequeñas, hot, genéricas*. Evita en frías/grandes. |
| **`SmallVec<[T; N]>` / `ArrayVec<[T; N]>`** | Evita allocación Heap para colecciones pequeñas (≤ N). Stack allocation. | `smallvec` crate. **Cuidado:** `push` sobre capacidad -> alloc + copy. `ArrayVec` panic si full. |
| **`Cow<'a, T>` (Clone on Write)** | `Cow::Borrowed(&T)` barato. `to_mut()` clona solo si hay que mutar. | Ideal para args de función que *a veces* modifican, *a veces* no. `fn process(s: Cow<str>)`. |
| **String Interning** (`lasso`, `intaglio`) | Deduplicación strings idénticos (`&'static str` o `u32` ID). | Lookup O(1) global. Útil en parsers, ASTs, logging, símbolos. |
| **`ahash` / `foldhash` (Hasher Rápido)** | `HashMap`/`HashSet` default `SipHash` (seguro DoS). **`ahash`** usa AES-NI (hardware) -> **2x-5x más rápido**. | **NO** para claves expuestas a atacantes (DoS). **SÍ** para internos, caches, índices. `type FastMap<K,V> = HashMap<K,V,BuildHasherDefault<AHasher>>;` |
| **SIMD (`std::simd` nightly / `packed_simd` / `wide`)** | Procesar 4/8/16 elementos por instrucción (AVX2/AVX-512/NEON). | `std::simd` (nightly, estableciendo). `packed_simd` (mantenido). Requiere `target-cpu=native` o `RUSTFLAGS="-C target-cpu=native"`. |
| **Branchless / `likely`/`unlikely`** | Predicción de saltos. `#[cold]` en paths de error. `core::hint::likely()`. | Micro-optimización. Verifica en asm (`cargo asm`). |

### 🛠️ Proyecto: **Optimización Real (Log Parser v2 o Mandelbrot v2)**

#### Metodología Obligatoria (Documentar en `OPTIMIZATION_LOG.md`)
1.  **Baseline:** `criterion` bench actual. `flamegraph` identifica Top 3 hotspots.
2.  **Hipótesis:** "Hotspot X es `HashMap` con `SipHash` -> Cambiar a `ahash`".
3.  **Implementación:** Cambio mínimal (feature flag `fast-hash`).
4.  **Verificación:** `criterion` compara baseline vs new. `cargo bloat` verifica code size.
5.  **Regresión:** `cargo test` + `cargo miri test` (si `unsafe` involucrado).

#### Ejemplos de Optimizaciones Objetivo (Log Parser):
1.  **Parsing:** `nom` -> `memchr`/`memchr3` manual para encontrar delimitadores (newline, space) -> **SIMD `memchr`**.
2.  **Aggregation:** `DashMap<String, Counter>` -> `FastMap<InternedStr, Counter>` (String Interning + `ahash`).
3.  **Output:** `serde_json::to_string` por línea -> `write!` manual a buffer + `stdout.lock()` + `flush` batch.

#### Ejemplos (Mandelbrot):
1.  **Math:** `f64` escalar -> `f32x4` / `f32x8` (`std::simd` / `wide` crate) para 4/8 pixeles a la vez.
2.  **Memory:** `Vec<u8>` interleaved RGBA -> `Vec<[u8;4]>` o planos separados (Structure of Arrays) para vectorización automática.
3.  **Parallelism:** `rayon` -> `rayon` + `ThreadPool` custom (evitar oversubscription si Wasm Workers).

---

## 📅 SEMANA 20: ESPECIALIZACIÓN — ELIGE TU CAMINO MAESTRO
**Objetivo:** Aplicar todo el conocimiento en un dominio profundo. **Elige UNA** ruta y ejecútala a nivel de producción.

### 🛤️ RUTA A: EMBEDDED ASYNC (`no_std` + `embassy`)
> *Rust sin OS. Determinismo. Hardware real.*

**Conceptos Clave:**
*   **`#![no_std]`**: Sin `std`, solo `core` + `alloc` (opcional). `panic_handler` propio (`defmt`/`probe-run`).
*   **`embassy`**: Executor async **embarcado**. Tasks = State machines. **Sin `Pin`/`Unpin` manual** (embassy macros).
*   **HAL (Hardware Abstraction Layer):** `embassy-stm32`, `embassy-rp` (RP2040), `esp-hal`.
*   **Periféricos Async:** `uart.read(&mut buf).await`, `i2c.write(addr, &data).await`, `adc.read().await`.
*   **`defmt`**: Logging **zero-cost** (formato en host, device solo envía índices + args). `defmt-rtt` (RTT probe).
*   **RTIC (Real-Time Interrupt-driven Concurrency):** Alternativa a Embassy. Prioridades HW, análisis de ceiling, shared resources con `Mutex` (ceiling priority protocol).

**Proyecto: "Weather Station RP2040/STM32"**
1.  **Hardware:** RP2040 (Pico) o STM32 (Nucleo) + Sensor BMP280/BME280 (I2C) + Display SSD1306 (I2C/SPI) o LED Matrix.
2.  **Tasks (Embassy):**
    *   `sensor_task`: Lee T/P/H cada 1s -> `Channel<T, SensorData>`.
    *   `display_task`: Recibe `SensorData` -> Render UI (gráficos mini).
    *   `net_task` (si W5500/ESP32 co-proc): Publica MQTT/HTTP.
    *   `blinky_task`: Heartbeat LED.
3.  **CI:** Compila para `thumbv6m-none-eabi` / `thumbv7em-none-eabihf`. Test en **QEMU** (`qemu-system-arm -M raspi2b` o `nucleo-f429zi`).
4.  **Entregable:** Binario `.elf` flasheable. `defmt` logs en terminal. `README` con esquemático Fritzing/KiCad.

### 🛤️ RUTA B: PROCEDURAL MACROS (Compiler Plugin)
> *Extender el lenguaje. `#[derive(Magic)]`.*

**Conceptos Clave:**
*   **`proc-macro` crate type:** `proc_macro` crate (no bin/lib). `proc_macro::TokenStream` in/out.
*   **`syn` (Parsing):** `parse_macro_input!`, `DeriveInput`, `Data`, `Fields`, `Generics`, `WhereClause`, `Attributes`.
*   **`quote` (Codegen):** `quote! { ... }`, `#ident`, `#(#fields)*`, `#(#generics)*`.
*   **`proc-macro2` / `quote` (Span/Higiene):** `Span::call_site()`, `Ident::new(..., span)`.
*   **Error Handling:** `syn::Error::new_spanned(...).into_compile_error()`.
*   **Testing:** `trybuild` (compile-fail tests), `ui` tests.

**Proyecto: `#[derive(Builder)]` Funcional (Estilo `derive_builder` crate)**
```rust
// Input
#[derive(Builder)]
struct Config {
    #[builder(default = "8080")]
    port: u16,
    #[builder(setter(into))]
    host: String,
    #[builder(each = "worker")]
    workers: Vec<WorkerConfig>,
}

// Output Generado (Mental Model)
impl Config {
    pub fn builder() -> ConfigBuilder<ConfigBuilderInit> { ... }
}
struct ConfigBuilder<State> { port: Option<u16>, host: Option<String>, workers: Vec<WorkerConfig>, _state: State }
struct ConfigBuilderInit; struct ConfigBuilderReady;
// Setters mutan self, cambian State tipo (Typestate Builder).
// build() -> Result<Config, BuilderError> (valida required fields).
```
**Requisitos:** Soportar `default`, `setter(into)`, `setter(strip_option)`, `each`, `default_code`, `build_fn(validate = "...")`. **Tests exhaustivos** (compile-fail para campos faltantes, runtime para validación).

### 🛤️ RUTA C: MONOREPO WORKSPACE ARQUITECTURA LIMPIA
> *Escala organizacional. Separación de concerns. Build unificado.*

**Estructura Objetivo:**
```text
my-ecosystem/
├── Cargo.toml              # [workspace], resolver = "2", lints, profiles
├── crates/
│   ├── core/               # Lógica pura, traits, domain models (NO I/O, NO ASYNC idealmente)
│   │   ├── src/lib.rs
│   │   └── Cargo.toml      # deps: serde, thiserror, derive_more
│   ├── db/                 # Implementación SQLx/SeaORM de traits core::repo
│   │   ├── src/lib.rs
│   │   └── Cargo.toml      # deps: sqlx, core, tracing
│   ├── api/                # Handlers Axum, Extractors, OpenAPI (utoipa/aide)
│   │   ├── src/lib.rs
│   │   └── Cargo.toml      # deps: axum, core, db, tower, validator
│   ├── cli/                # CLI Clap, usa api/core como lib
│   │   ├── src/main.rs
│   │   └── Cargo.toml      # deps: clap, core, api (como client reqwest)
│   └── macros/             # Proc macros compartidos (Builder, Typestate, etc)
│       ├── src/lib.rs
│       └── Cargo.toml      # deps: syn, quote, proc-macro2
├── xtask/                  # Build automation (cargo xtask release, cargo xtask codegen)
└── docker/
    ├── Dockerfile.api
    └── Dockerfile.cli
```
**Reglas de Oro Workspace:**
1.  **`resolver = "2"`** en `[workspace]` (Cargo.toml root).
2.  **`[workspace.dependencies]`**: Versiones centralizadas (`serde = { version = "1", features = ["derive"] }`).
3.  **`[workspace.lints]`**: `rust`, `clippy` unificados.
4.  **`[profile.release]` / `[profile.dev]`** unificados (LTO, strip, codegen-units=1).
5.  **`cargo hack`** (CI): `cargo hack check --each-feature --all-targets` (prueba combinaciones features).
6.  **Ciclos de Dependencia:** **PROHIBIDO** `core -> db` y `db -> core`. `core` define Traits. `db` implementa. `api` depende de `core` + `db`.

**Entregable:** `cargo test --workspace` pasa. `cargo build --release --workspace` genera binarios `api`, `cli`. Dockerfiles multi-stage usan `cargo chef` con `Cargo.lock` raíz. `xtask` genera OpenAPI spec desde `api` crate.

---

## 📚 RESUMEN RECURSOS MES 5

| Semana | Lectura Obligatoria (Deep Dive) | Video / Referencia | Práctica Clave |
| :--- | :--- | :--- | :--- |
| **17** | **Rust Design Patterns** (rust-unofficial.github.io/patterns) <br> *Typestate, Builder, Actor, Newtype* | Jon Gjengset: "Crust of Rust: Typestate" | **Refactor Url Shortener**: Typestate URLs + Actor Sharded Counter + Persistence Actor |
| **18** | **"Rust Atomics and Locks" (Mara Bos)** — **Cap 1-6, 9, 11, 12** <br> *Crossbeam Docs (Epoch, Channel)* | *Herb Sutter: "Atomic Weapons" (CppCon, aplica a Rust)* | **Benchmark Científico**: Sharded Counter (Atomic Padded vs Mutex vs DashMap vs Actor) + Gráficos + Análisis False Sharing |
| **19** | **The Rust Performance Book** (nnethercote.github.io/perf-book) <br> *cargo-bloat, cargo-llvm-lines, heaptrack* | **Jon Gjengset: "Crust of Rust: Profiling / Optimization"** | **Optimización Real**: Log Parser o Mandelbrot. 3 Optimizaciones medibles (SIMD, Ahash, SmallVec, Cow, Inlining). Doc en `OPTIMIZATION_LOG.md`. |
| **20** | **A) Discovery Book** (Embedded) <br> **B) Rustc Dev Guide** (Proc Macros) <br> **C) Cargo Workspaces** (Referencia) | *Embassy Book* / *David Tolnay: "Procedural Macros"* | **Proyecto Final Elección**: Embedded (Embassy), Proc Macro (Builder), o Monorepo Workspace Completo. |

---

## ⚠️ PROBLEMAS COMUNES MES 5 (NIVEL SENIOR)

| Área | Trampa | Síntoma | Solución Senior |
| :--- | :--- | :--- | :--- |
| **Typestate** | Explosión de Tipos / Boilerplate | 20 structs `Foo<State1>`, `Foo<State2>`... | **Enum Interno + PhantomData** para almacenamiento. **Generics** solo en API pública. Macros para generar impls repetitivos. |
| **Actor Model** | Mailbox Overflow / Backpressure | `tx.send().await` bloquea indefinidamente / OOM. | **Bounded Channels** (`mpsc::channel(N)`). `try_send` + `yield_now` + retry. **Load Shedding** (drop oldest / reject new). Métricas `mailbox_len`. |
| **Atomics / Ordering** | `Relaxed` usado incorrectamente | Data races lógicos (valores "imposibles"), loops infinitos. | **Default a `SeqCst`**. Solo `Relaxed` si **demostras** (model checking `loom` / `shuttle`) que es seguro. `AcqRel` en RMW (CAS loops). |
| **False Sharing** | Contadores en array `Vec<AtomicUsize>` | Throughput **disminuye** al añadir hilos. | `#[repr(align(64))]` o `cache_padded::CachePadded`. Verificar con `perf stat -e cache-misses,cache-references`. |
| **Profiling** | Optimizar "Hot Path" equivocado | 10% ganancia en función que es 1% del tiempo. | **Flamegraph + `criterion`**. Enfócate en **"Self Time" alto** y **"Callee Count" alto**. Amdahl's Law. |
| **Monomorphization Bloat** | `Vec<T>` instanciado para 50 tipos T | Binario 50MB+. Compile time 10min. | **`dyn Trait` / Type Erasure** en boundaries calientes. `Box<dyn Fn>`. `impl Trait` en args (no return) ayuda. `cargo bloat --release --crates -n 100`. |
| **Proc Macros** | Span/Higiene rota / Errores crípticos | Variable "no encontrada" en código generado / `macro expanded` illegible. | `quote::ToTokens::into_token_stream`. `Span::call_site()` vs `span` del input. `cargo expand` para debug. `trybuild` para tests de error. |
| **no_std / Embedded** | `alloc` + `panic` + `global_allocator` missing | Linker errors: `__rust_alloc`, `eh_personality`. | `extern crate alloc; use alloc::vec::Vec;`. `#[global_allocator] static ALLOC: CortexMHeap = ...`. `panic_halt` / `probe-run` / `defmt`. |

---

## 🧩 MATERIAL COMPLEMENTARIO: Laboratorio de Código Comentado

> Todos los ejemplos **compilan y corren con `rustc 1.81` (edición 2021) usando SOLO `std`** — incluyendo el actor (con `std::sync::mpsc` + `std::thread`) y el contador concurrente. No requieren `tokio`, `crossbeam`, `rayon` ni `criterion`. Los marcados `// ❌ NO COMPILA` demuestran las garantías que el sistema de tipos te regala.

### 1️⃣ Newtype: tipos fuertes con validación

```rust
struct Email(String); // no es un String cualquiera: ya está validado

impl Email {
    fn parse(s: &str) -> Result<Email, String> {
        if s.contains('@') { Ok(Email(s.to_string())) }
        else { Err(format!("email inválido: {s}")) }
    }
    fn as_str(&self) -> &str { &self.0 }
}
// Una vez tienes un `Email`, el resto del código NO vuelve a validar: el tipo es la prueba.
```

### 2️⃣ Typestate: el estado vive en el **tipo** (cero coste en runtime)

```rust
use std::marker::PhantomData;

struct Draft;   struct Active;   struct Expired; // marcadores, 0 bytes

struct Url<S> {
    code: String,
    clicks: u64,
    _estado: PhantomData<S>, // ata el parámetro genérico sin ocupar memoria
}

impl Url<Draft> {
    fn new(code: &str) -> Self { Url { code: code.into(), clicks: 0, _estado: PhantomData } }
    fn publish(self) -> Url<Active> { // consume Draft → produce Active
        Url { code: self.code, clicks: 0, _estado: PhantomData }
    }
}
impl Url<Active> {
    fn click(&mut self) { self.clicks += 1; }
    fn expire(self) -> Url<Expired> {
        Url { code: self.code, clicks: self.clicks, _estado: PhantomData }
    }
}
impl Url<Expired> {
    fn clicks_finales(&self) -> u64 { self.clicks } // expirada: solo lectura
}

fn main() {
    let mut activa = Url::<Draft>::new("abc").publish();
    activa.click();
    let expirada = activa.expire();
    assert_eq!(expirada.clicks_finales(), 1);

    // expirada.click(); // ❌ NO COMPILA: `click` no existe para `Url<Expired>`
}
```

> Este es el *killer feature* de la Semana 17: estados inválidos **no se pueden expresar**. El compilador, no un `if` en runtime, garantiza que nunca llamas `click()` sobre una URL expirada. `PhantomData<S>` ocupa 0 bytes ⇒ abstracción de coste cero.

### 3️⃣ Builder con typestate: campos requeridos verificados en compile-time

```rust
struct Unset;   struct Set;

struct ConfigBuilder<U> { url: Option<String>, _u: PhantomData<U> }

impl ConfigBuilder<Unset> {
    fn new() -> Self { ConfigBuilder { url: None, _u: PhantomData } }
    fn url(self, u: &str) -> ConfigBuilder<Set> { // transición de estado en el tipo
        ConfigBuilder { url: Some(u.into()), _u: PhantomData }
    }
}
impl ConfigBuilder<Set> {
    fn build(self) -> String { self.url.unwrap() } // unwrap SEGURO: el tipo lo garantiza
}

// ConfigBuilder::new().build(); // ❌ NO COMPILA: `build` no existe en <Unset>
let cfg = ConfigBuilder::new().url("http://x").build(); // ✅
```

### 4️⃣ Actor model con `std` (threads + `mpsc`): estado sin `Mutex`

```rust
use std::sync::mpsc::{self, Sender};
use std::thread;

enum Msg {
    Inc,
    Get(Sender<u64>), // canal de respuesta (oneshot emulado con mpsc)
    Stop,
}

#[derive(Clone)]
struct CounterHandle { tx: Sender<Msg> } // handle clonable y barato

impl CounterHandle {
    fn inc(&self) { self.tx.send(Msg::Inc).unwrap(); }
    fn get(&self) -> u64 {
        let (tx, rx) = mpsc::channel();
        self.tx.send(Msg::Get(tx)).unwrap();
        rx.recv().unwrap()
    }
}

fn start_counter() -> (CounterHandle, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<Msg>();
    let join = thread::spawn(move || {
        let mut count = 0u64; // ESTADO PRIVADO del actor: nadie más lo toca
        while let Ok(msg) = rx.recv() {
            match msg {
                Msg::Inc => count += 1,
                Msg::Get(reply) => { reply.send(count).ok(); }
                Msg::Stop => break,
            }
        }
    });
    (CounterHandle { tx }, join)
}

// 4 hilos incrementando vía mensajes; el estado nunca se comparte ⇒ sin data races.
// for _ in 0..4 { let h = handle.clone(); thread::spawn(move || for _ in 0..1000 { h.inc(); }); }
// assert_eq!(handle.get(), 4000);
```

> *"No compartas memoria para comunicarte; comunica para compartir memoria."* El estado (`count`) es **local** al hilo del actor; la concurrencia se serializa por la cola de mensajes. La versión Tokio (Semana 17) es idéntica cambiando `std::thread`/`mpsc` por `tokio::spawn`/`tokio::sync::mpsc`.

### 5️⃣ Contador sharded + *false sharing* evitado con padding

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

#[repr(align(64))] // cada átomo en SU PROPIA línea de cache (64 B) → sin ping-pong
struct Padded(AtomicUsize);

struct Sharded { shards: Vec<Padded> }
impl Sharded {
    fn new(n: usize) -> Self {
        Sharded { shards: (0..n).map(|_| Padded(AtomicUsize::new(0))).collect() }
    }
    fn inc(&self, shard: usize) {
        // Estadística pura: el orden exacto no importa ⇒ `Relaxed` (lo más rápido).
        self.shards[shard].0.fetch_add(1, Ordering::Relaxed);
    }
    fn total(&self) -> usize {
        self.shards.iter().map(|p| p.0.load(Ordering::Relaxed)).sum()
    }
}
// 8 hilos × 10_000 incrementos, cada uno en su shard ⇒ total == 80_000, sin contención.
```

> **`#[repr(align(64))]` es la diferencia entre escalar y colapsar.** Sin él, varios `AtomicUsize` caen en la misma línea de cache y los núcleos se la invalidan mutuamente (false sharing): el throughput *baja* al añadir hilos. Mídelo con `perf stat -e cache-misses`.

### 6️⃣ `Cow`: clona solo cuando hay que mutar

```rust
use std::borrow::Cow;

fn normalizar(s: &str) -> Cow<str> {
    if s.contains(' ') {
        Cow::Owned(s.replace(' ', "_")) // hubo cambio → nueva asignación
    } else {
        Cow::Borrowed(s)                // sin cambios → CERO asignaciones
    }
}

assert!(matches!(normalizar("limpio"),       Cow::Borrowed(_)));
assert!(matches!(normalizar("con espacio"),  Cow::Owned(_)));
```

> El patrón de optimización de la Semana 19: el caso común (sin cambios) no asigna memoria; solo pagas el `clone` cuando realmente modificas. Ideal para funciones que *a veces* transforman su entrada.

---

## ✅ CHECKLIST FINAL MES 5 (Definition of Done — Senior Rustacean)

### 1. Arquitectura & Patrones (Refactor Url Shortener v3)
- [ ] **Typestate:** `Url<Draft>` -> `Url<Active>` -> `Url<Expired>`. **Imposible** compilar lógica inválida (click en expirado).
- [ ] **Actor Model:** `ClickCounter` Sharded (N actors) + `PersistenceWriter` Actor. **Cero `Mutex`/`RwLock`/`DashMap` en hot path**.
- [ ] **DI:** Handlers Axum usan `State<AppState>` con `CounterHandle` (Trait `Counter: Send + Sync`).
- [ ] **Config:** `figment` layered (File -> Env -> CLI). Validación al inicio (`Config::build()?`).
- [ ] **Tests:** Property-based (`proptest`) para transiciones de estado. Integración con Actor mock.

### 2. Concurrencia & Benchmark (Ciencia Rigurosa)
- [ ] **Implementaciones:** 4 variantes (Atomic Padded, Mutex Sharded, DashMap, Actor).
- [ ] **Benchmark:** `criterion` con `Throughput`, múltiples `threads` (1..num_cpus*2).
- [ ] **Análisis:** Gráficos (Throughput vs Threads, Latencia p99 vs Threads).
- [ ] **False Sharing:** Demostrado comparando `AtomicUsize` vs `CachePadded<AtomicUsize>`.
- [ ] **Documentación:** `BENCHMARK_REPORT.md` con conclusiones: "Cuándo usar cada uno".

### 3. Optimización Real (Log Parser o Mandelbrot)
- [ ] **Baseline:** `criterion` + `flamegraph` + `heaptrack` **antes** de tocar código.
- [ ] **3 Optimizaciones Distintas** aplicadas (ej. `ahash`, `SmallVec`, SIMD `std::simd`, `Cow`, String Interning, Branchless).
- [ ] **Verificación:** `criterion` reporta **speedup > 1.5x** (o memory -30%) en *cada* optimización individual.
- [ ] **Regresión Cero:** `cargo test --all-features` + `cargo miri test` (si `unsafe`) pasan.
- [ ] **Artefacto:** `OPTIMIZATION_LOG.md` con: Flamegraph Antes/Después, Criterion Tables, Diff de Código, Lecciones Aprendidas.

### 4. Proyecto Final de Especialización (Uno completado al 100%)
#### **A) Embedded (Embassy)**
- [ ] **Hardware:** RP2040/STM32 + Sensor I2C/SPI + Actuador (LED/Display/Motor).
- [ ] **Architecture:** 3+ Tasks Embassy (`spawner.spawn`), Canales `embassy_sync::channel`.
- [ ] **Observabilidad:** `defmt` + `probe-rs` / `probe-run` logging en host.
- [ ] **CI:** Compila `--target thumbv...`. Corre en **QEMU** (test headless) en GitHub Actions.
- [ ] **Seguridad:** `#[deny(unsafe_op_in_unsafe_fn)]`, `unsafe` solo en PAC/HAL bindings.

#### **B) Proc Macro (`derive(Builder)`)**
- [ ] **Features:** `default`, `setter(into/strip_option)`, `each`, `build_fn(validate)`, `derive(Debug/Clone)`.
- [ ] **Higiene:** Spans correctos (errores apuntan a campo del usuario, no código generado).
- [ ] **Tests:** `trybuild` compile-fail (missing required, type mismatch). Runtime tests (validation logic).
- [ ] **Publicación:** Crate `my-builder` en crates.io con docs.rs construyendo.

#### **C) Monorepo Workspace**
- [ ] **Estructura:** `core`, `db`, `api`, `cli`, `macros` crates. `Cargo.toml` root con `resolver="2"`, `workspace.dependencies`, `workspace.lints`.
- [ ] **Build:** `cargo build --release --workspace` funciona. `cargo test --workspace`.
- [ ] **CI:** `cargo hack check --each-feature --all-targets`. `cargo deny check` (licenses, bans, sources).
- [ ] **Docker:** Multi-stage `cargo-chef` para `api` y `cli` independientes (comparten cache deps).
- [ ] **DX:** `cargo xtask codegen` (genera OpenAPI, DB migrations, Builder macros). `justfile` / `Makefile` para comandos comunes.

---

### 🎓 GRADUACIÓN DEL CURSO: TU PORTFOLIO RUST

Al finalizar el Mes 5, tu GitHub/GitLab debe demostrar:

1.  **`config-loader` (Mes 2):** Librería pura, genérica, 100% tested, publicada.
2.  **`url-shortener` (Mes 3 -> 5):** Servicio **Production-Ready** (Axum, SQLx, Tracing, Prometheus, Docker, CI/CD, **Actor Model, Typestate**).
3.  **`mytool` (Mes 4):** CLI instalable, UX pro, FFI wrapper seguro, Wasm app, Parser streaming.
4.  **`concurrency-benchmarks` (Mes 5):** Ciencia de sistemas aplicada a Rust (Atomics, False Sharing, Actors).
5.  **`optimization-log` (Mes 5):** Metodología de profiling y resultados medibles.
6.  **Proyecto Final (Mes 5):** **Embedded / Proc Macro / Monorepo** — Profundidad en un nicho.

---

### 🚀 MES 6: CAPSTONE & ESPECIALIZACIÓN FINAL
> **El último tramo. Integración total.**
> *Diseñar, implementar, documentar y defender un sistema complejo real (Distribuido, Data Engine, Game Engine, Compiler, Kernel Module, Blockchain, ML Inference Server).*
> **Tu "Master Thesis" en código Rust.**

*Has llegado al nivel donde Rust no es un lenguaje, es una herramienta de ingeniería de sistemas de precisión. Úsala con responsabilidad.* 🦀🏗️⚡