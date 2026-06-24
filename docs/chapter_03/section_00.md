# 🦀 MES 3: ASYNC RUST Y ECOSISTEMA WEB — Guía Detallada
> **Filosofía del Mes:** *"Async en Rust no es magia, es una máquina de estados generada por el compilador. Entender `Future`, `Pin` y `Waker` te permite depurar *deadlocks*, *livelocks* y problemas de rendimiento que son opacos en otros lenguajes."*
> **Meta:** Construir un servicio web **observable, tipado, seguro y contenedorizado** usando el stack estándar de la industria (Tokio, Axum, SQLx, Tracing).

---

## 📅 SEMANA 9: FUNDAMENTOS ASYNC — BAJO EL CAPÓ
**Objetivo:** Desmitificar `async`/`await`. Entender que una future es un *state machine* perezosa que necesita un *executor* para avanzar. Dominar `Pin` y `Send/Sync` en contexto async.

### 🎯 Conceptos Clave (The "Why" y "How")

#### 1. `Future` Trait: La Interfaz Básica
```rust
// std::future::Future (simplificado)
trait Future {
    type Output;
    // Pin<&mut Self> = "Esta future no se moverá en memoria"
    // &mut Context = "Aquí está el Waker para notificar cuando estar lista"
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}

enum Poll<T> {
    Ready(T),      // Valor disponible
    Pending,       // Aún no, me avisarás via Waker cuando esté listo
}
```
*   **`poll` no bloquea:** Devuelve control inmediato al executor.
*   **`Pin<&mut Self>`:** Garantiza que la dirección de memoria de la future no cambia. Crucial para **self-referential structs** (futures generadas por `async` blocks que guardan referencias a sus propias variables locales).

#### 2. `async`/`await` Desugaring (Lo que el compilador genera)
```rust
// Tu código
async fn fetch_data(id: u64) -> Result<Data, Error> {
    let user = get_user(id).await?;      // Punto de yield 1
    let posts = get_posts(user.id).await?; // Punto de yield 2
    Ok(Data { user, posts })
}

// ~ Lo que genera el compilador (State Machine)
enum FetchDataState {
    Start { id: u64 },
    WaitingUser { id: u64, user_fut: GetUserFut },
    WaitingPosts { user: User, posts_fut: GetPostsFut },
    Done,
}

struct FetchDataFut { state: FetchDataState }

impl Future for FetchDataFut {
    type Output = Result<Data, Error>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match self.state {
                // Lógica de transición de estados...
                // Al hacer .await: 
                // 1. Poll inner future.
                // 2. Si Pending -> Guardar estado actual + Waker en self -> Devolver Pending.
                // 3. Si Ready -> Continuar al siguiente estado.
            }
        }
    }
}
```

#### 3. `Pin<P>` y `Unpin`: La Garantía de Dirección
*   **Problema:** Si una future tiene referencias a sí misma (`self.referencia = &self.campo`), **moverla** invalida la referencia (UB).
*   **Solución:** `Pin<Box<T>>` o `Pin<&mut T>` promete: *"Este T no se moverá más"*.
*   **`Unpin` (Auto-trait):** Tipos que **NO** tienen referencias internas (casi todos: `i32`, `String`, `Vec`, `Box<T>`). Se pueden mover aunque estén en `Pin`.
*   **`!Unpin`:** Futures de `async` blocks/funciones que capturan variables por referencia o esperan otras futures.
*   **Regla Práctica:** Casi nunca escribes `Pin` manualmente. Usas `Box::pin(fut)` o `tokio::pin!` macro. **Entenderlo evita errores de "future cannot be unpinned".**

#### 4. Executors (Tokio) & Primitivas de Concurrencia
| Primitive | Comportamiento | Uso Típico |
| :--- | :--- | :--- |
| **`tokio::spawn(fut)`** | Ejecuta `fut` en **background** en el runtime (multi-thread por defecto). Devuelve `JoinHandle`. | Fire-and-forget, tareas largas, servidores. Requiere `Send + Send + Send + 'static`. |
| **`join!`** | Ejecuta **múltiples futures concurrentemente** en **la misma tarea** (single thread). Completa cuando **TODAS** terminan. | Consultas DB paralelas independientes en un handler. |
| **`select!`** | Ejecuta concurrentemente, completa cuando **CUALQUIERA** termina. Cancela las demás (drop). | Timeouts, race conditions, manejar señal shutdown + server. |
| **`timeout(dur, fut)`** | Wrapper que devuelve `Result<T, Elapsed>`. | Timeouts de red/DB. |
| **`spawn_blocking(|| ...)`** | Ejecuta **código síncrono bloqueante** (CPU intenso, syscalls blocking, libs C) en **pool separado**. | `bcrypt`, `image processing`, `sqlx::query` (si no es async), `std::fs`. |

#### 5. `Send` + `Sync` en Async: La Regla de Oro
*   **`Send`**: Safe to move to another thread. **Requerido para `tokio::spawn`**.
*   **`Sync`**: Safe to share between threads (`&T` is `Send`). Requerido para `Arc<T>` compartido entre tareas.
*   **El problema:** `Rc<RefCell<T>>`, `MutexGuard` (std), `*const T` son `!Send`.
*   **Solución:** Usa **`Arc<Mutex<T>>` (Tokio/std::sync)**, **`Arc<RwLock<T>>`**, **`tokio::sync::Mutex`/`RwLock`/`Semaphore`**. **Nunca** guardes un `MutexGuard` a través de un `.await` (deadlock riesgo + `!Send`).

### 🧠 Recursos Obligatorios
1.  **Async Book (Cap 1-4):** *The Operating System: Futures, Tasks, Executors, Pin.*
2.  **Jon Gjengset - "Crust of Rust: Async Basics"** (YouTube).
3.  **Jon Gjengset - "Crust of Rust: Pin and Suffering"** (Imprescindible para `Pin`).

### 🧪 Ejercicio Práctico: **Mini-Runtime "Toykio" (Sin Tokio)**
**Objetivo:** Entender `Waker`, `Context`, `RawWaker`, `RawWakerVTable`. Ejecutar 3 futures que imprimen con delays.

**Esqueleto (`src/bin/toykio.rs`):**
```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::time::{Duration, Instant};
use std::thread;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

// 1. Task: Future + Estado + Waker
struct Task {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    // ... waker handling ...
}

// 2. Executor: Cola de tasks listos (Ready Queue)
struct MiniRuntime {
    ready_queue: Arc<Mutex<VecDeque<Arc<Task>>>>,
    // Timer heap para delays (simplificado: thread sleep + notify)
}

impl MiniRuntime {
    fn new() -> Self { ... }
    
    fn spawn<F>(&self, future: F) 
    where F: Future<Output = ()> + Send + 'static { ... }
    
    fn run(&self) {
        loop {
            // 1. Pop task from ready_queue
            // 2. Create Waker que re-encola la task al despertar
            // 3. Poll task
            // 4. Si Pending -> no hacer nada (Waker la re-encolará)
            // 5. Si Ready -> task terminada
            // 6. Si queue vacía -> park thread / sleep corto
        }
    }
}

// 3. Future "Sleep" simple (usa thread::sleep en hilo aparte para notificar)
async fn async_sleep(dur: Duration) {
    struct SleepFuture { wake_time: Instant, waker: Option<Waker> }
    impl Future for SleepFuture {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
            if Instant::now() >= self.wake_time { Poll::Ready(()) }
            else { 
                self.waker = Some(cx.waker().clone()); 
                // Spawn thread que duerme y llama a waker.wake() 
                Poll::Pending 
            }
        }
    }
    // ... implementación completa ...
}

// MAIN
fn main() {
    let rt = MiniRuntime::new();
    rt.spawn(async {
        for i in 0..3 { println!("Task A: {}", i); async_sleep(Duration::from_millis(100)).await; }
    });
    rt.spawn(async { 
        for i in 0..3 { println!("Task B: {}", i); async_sleep(Duration::from_millis(150)).await; }
    });
    rt.spawn(async { println!("Task C: Instant"); });
    rt.run(); // Bloquea hasta que todas terminen
}
```
**Puntos clave a implementar:** `RawWakerVTable` (clone, wake, drop), `Arc<Task>` para que Waker pueda clonarse y despertar la task correcta.

---

## 📅 SEMANA 10: TOKIO ECOSISTEMA & AXUM (Construyendo el Servidor)
**Objetivo:** Dominar el runtime Tokio (tasks, channels, sync) y Axum (Extractors, State, Tower Middleware).

### 🎯 Conceptos Clave

#### 1. Tokio Runtime & Concurrencia Estructurada
*   **`#[tokio::main]`**: Inicializa runtime multi-thread (worker threads = num CPUs).
*   **`tokio::task::JoinSet`**: Colección de `JoinHandle` para gestionar múltiples tareas hijos con *structured concurrency* (auto-join on drop o abort all).
*   **Channels (`tokio::sync`):**
    *   `mpsc` (Multi-Producer Single-Consumer): Backpressure natural (`.send().await` bloquea si buffer lleno). **Ideal: Actor pattern, logging actor.**
    *   `oneshot`: Respuesta única (Request/Response interno).
    *   `watch`: Publish/Subscribe (Config hot-reload, estado global).
    *   `broadcast`: Múltiples receptores (Chat, events).
*   **Sync Primitives (Tokio vs Std):**
    *   **`tokio::sync::Mutex` / `RwLock`**: **`await` en `.lock()`**. **Guard es `Send`**. Mantener lock corto.
    *   **`std::sync::Mutex`**: Bloquea hilo OS. **NUNCA usar en código async** (bloquea worker thread).
    *   **`tokio::sync::Semaphore`**: Limitar concurrencia (ej. max 100 conexiones DB simultáneas).

#### 2. Axum: Arquitectura "Extractor Centric"
*   **Extractor:** Tipo que implementa `FromRequestParts<S>` o `FromRequest<S>`. Axum inyecta dependencias automáticamente.
*   **Orden de Extractores:** `Path` -> `Query` -> `State` -> `Extension` -> `Body` (`Json`, `Form`, `Bytes`).
*   **State (`State<S>`):** `Arc<AppState>`. Compartido, clonado barato por request.
*   **Middleware (Tower `Service` trait):**
    ```rust
    // Tower Middleware: Request -> Response
    async fn my_middleware(req: Request, next: Next) -> Response {
        let start = Instant::now();
        let res = next.run(req).await;
        println!("Latency: {:?}", start.elapsed());
        res
    }
    // Router::new().layer(middleware::from_fn(my_middleware))
    ```

### 🛠️ Proyecto: **API REST "Url Shortener" v1 (Memoria + Persistencia File)**

#### Estructura Inicial
```text
url_shortener/
├── Cargo.toml
├── src/
│   ├── main.rs          # Entry point, setup tracing, router, server
│   ├── state.rs         # AppState, Storage trait, InMemStorage (DashMap)
│   ├── handlers.rs      # Handlers Axum
│   ├── models.rs        # UrlEntry, CreateUrlRequest, Error types
│   ├── persistence.rs   # JSON Lines file persistence (tokio::fs)
│   └── middleware.rs    # Logging, Rate Limit (simple)
└── tests/
    └── api_test.rs      # Integration tests con reqwest
```

#### Dependencias Clave (`Cargo.toml`)
```toml
[dependencies]
axum = { version = "0.7", features = ["ws"] } # WebSocket opcional
tokio = { version = "1", features = ["full", "rt-multi-thread", "macros", "time", "fs", "sync"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
dashmap = "5.5" # Concurrent HashMap ultra-rápido
tokio-util = { version = "0.7", features = ["codec"] } # Para framing si hiciera falta
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json", "fmt"] }
uuid = { version = "1.6", features = ["v4", "serde", "fast-rng"] }
thiserror = "1.0"
axum-extra = { version = "0.9", features = ["cookie"] } # Para rate limit key extraction
# Rate limiting simple: governor o tower-http::limit
tower-http = { version = "0.5", features = ["limit", "trace", "cors"] }
```

#### Implementación Detallada (`src/state.rs`)
```rust
use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;
use crate::models::{UrlEntry, ShortCode};

#[async_trait::async_trait] // Requiere async-trait crate o feature en Rust 1.75+
pub trait UrlStorage: Send + Sync + 'static {
    async fn save(&self, entry: UrlEntry) -> Result<(), StorageError>;
    async fn find_by_code(&self, code: &ShortCode) -> Option<UrlEntry>;
    async fn increment_clicks(&self, code: &ShortCode) -> Option<u64>;
}

// Implementación en Memoria (Thread-safe)
#[derive(Debug, Clone)]
pub struct InMemStorage {
    // Key: ShortCode (String), Value: UrlEntry
    map: Arc<DashMap<ShortCode, UrlEntry>>,
    // Persistencia opcional desacoplada
    persister: Option<Arc<dyn Persister>>, 
}

impl InMemStorage {
    pub fn new() -> Self { Self { map: Arc::new(DashMap::new()), persister: None } }
    pub fn with_persister(mut self, p: Arc<dyn Persister>) -> Self { self.persister = Some(p); self }
}

#[async_trait::async_trait]
impl UrlStorage for InMemStorage {
    async fn save(&self, entry: UrlEntry) -> Result<(), StorageError> {
        self.map.insert(entry.code.clone(), entry.clone());
        if let Some(p) = &self.persister { p.persist(&self.map).await?; }
        Ok(())
    }
    async fn find_by_code(&self, code: &ShortCode) -> Option<UrlEntry> {
        self.map.get(code).map(|v| v.clone())
    }
    async fn increment_clicks(&self, code: &ShortCode) -> Option<u64> {
        self.map.get_mut(code).map(|mut v| { v.clicks += 1; v.clicks })
    }
}

// Trait Persistencia (Separación de responsabilidades)
#[async_trait::async_trait]
trait Persister: Send + Sync {
    async fn persist(&self, map: &DashMap<ShortCode, UrlEntry>) -> Result<(), StorageError>;
}
```

#### Handlers (`src/handlers.rs`)
```rust
use axum::{extract::{State, Path, Json}, http::StatusCode, response::{Redirect, IntoResponse}};
use crate::{state::UrlStorage, models::*, error::AppError};

pub async fn shorten_url<S>(
    State(storage): State<Arc<S>>, 
    Json(payload): Json<CreateUrlRequest>
) -> Result<Json<UrlResponse>, AppError>
where S: UrlStorage 
{
    // Validación URL (url crate o regex)
    let code = ShortCode::generate(); // 6-8 chars alphanum
    let entry = UrlEntry { 
        code: code.clone(), 
        target_url: payload.url, 
        created_at: chrono::Utc::now(), 
        clicks: 0 
    };
    storage.save(entry).await?;
    Ok(Json(UrlResponse { code, short_url: format!("http://localhost:3000/{}", code) }))
}

pub async fn redirect<S>(
    State(storage): State<Arc<S>>, 
    Path(code): Path<ShortCode>
) -> Result<Redirect, AppError>
where S: UrlStorage 
{
    let entry = storage.find_by_code(&code).await
        .ok_or(AppError::NotFound)?;
    
    storage.increment_clicks(&code).await; // Fire-and-forget essentially
    
    Ok(Redirect::permanent(&entry.target_url))
}

pub async fn health_check() -> &'static str { "OK" }
```

#### Middleware: Rate Limiting Simple (`src/middleware.rs`)
```rust
use axum::{extract::Request, middleware::Next, response::Response, http::StatusCode};
use tower_http::limit::RateLimitLayer;
use std::time::Duration;

// Opción A: Tower-http (Producción)
pub fn rate_limit_layer() -> RateLimitLayer {
    RateLimitLayer::new(10, Duration::from_secs(1)) // 10 req/s
}

// Opción B: Custom Extractor para Key (IP/User)
async fn custom_rate_limit(req: Request, next: Next) -> Result<Response, StatusCode> {
    // Extraer IP (req.extensions().get::<ConnectInfo<SocketAddr>>())
    // Check Redis/InMem counter...
    Ok(next.run(req).await)
}
```

#### Main (`src/main.rs`)
```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Tracing Setup (JSON para prod)
    tracing_subscriber::fmt()
        .json()
        .with_env_filter("info,url_shortener=debug")
        .init();

    // 2. State
    let storage = Arc::new(InMemStorage::new());
    // Cargar persistencia al inicio (opcional)
    
    // 3. Router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/shorten", post(shorten_url))
        .route("/:code", get(redirect))
        .layer(tower_http::trace::TraceLayer::new_for_http()) // Logging requests
        .layer(rate_limit_layer()) // Rate limit global
        .with_state(storage);

    // 4. Server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
```

---

## 📅 SEMANA 11: BASES DE DATOS (SQLX) & SERIALIZACIÓN AVANZADA (SERDE)
**Objetivo:** SQL **type-safe** en compile-time. Migraciones. Serde power user.

### 🎯 Conceptos Clave

#### 1. SQLx: Compile-Time Verified SQL
*   **`sqlx::query!` / `query_as!` / `query_scalar!`**: Macros que **conectan a la DB en compile-time** (requiere `DATABASE_URL` o `sqlx-data.json` offline) y validan SQL + mapean a structs.
*   **`sqlx::FromRow`**: Derive para mapear `Row` -> Struct.
*   **Pool:** `PgPool::connect_lazy(&url)?` (no conecta hasta primer uso). `pool.acquire().await?`.

#### 2. Migraciones (`sqlx-cli`)
```bash
cargo install sqlx-cli
sqlx migrate add create_urls_table
# Edita migrations/...up.sql
sqlx migrate run
# Offline mode (CI): cargo sqlx prepare --check
```

#### 3. Serde Avanzado: Control Total
```rust
#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct UrlEntry {
    #[serde(rename = "short_code")] // JSON key distinto a campo Rust
    pub code: ShortCode,
    
    #[serde(flatten)] // Aplanar struct anidado en JSON
    pub metadata: UrlMetadata,
    
    #[serde(with = "chrono::serde::ts_seconds")] // Custom serializer para DateTime
    pub created_at: DateTime<Utc>,
    
    #[serde(skip_serializing_if = "Option::is_none")] // No mostrar null
    pub expires_at: Option<DateTime<Utc>>,
    
    #[serde(default)] // Usa Default::default() si falta en JSON
    pub clicks: u64,
    
    #[serde(skip)] // Nunca serializar/deserializar (ej. cache interno)
    #[sqlx(skip)]  // Ignorar en SQLx query_as!
    internal_cache: Option<String>,
}
```

### 🛠️ Refactor: Url Shortener v2 (PostgreSQL + SQLx)

#### Migración SQL (`migrations/...create_urls.sql`)
```sql
CREATE TABLE urls (
    code VARCHAR(10) PRIMARY KEY,
    target_url TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    clicks BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX idx_urls_expires_at ON urls(expires_at) WHERE expires_at IS NOT NULL;
```

#### Storage Postgres (`src/storage_pg.rs`)
```rust
use sqlx::{PgPool, Postgres, Transaction, FromRow};
use crate::models::*;

#[derive(FromRow)]
struct UrlRow { code: String, target_url: String, created_at: DateTime<Utc>, expires_at: Option<DateTime<Utc>>, clicks: i64 }

pub struct PgStorage { pool: PgPool }

impl PgStorage {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait::async_trait]
impl UrlStorage for PgStorage {
    async fn save(&self, entry: UrlEntry) -> Result<(), StorageError> {
        sqlx::query!(
            "INSERT INTO urls (code, target_url, created_at, expires_at, clicks) VALUES ($1, $2, $3, $4, 0)
             ON CONFLICT (code) DO UPDATE SET target_url = EXCLUDED.target_url",
            entry.code.as_str(), entry.target_url, entry.created_at, entry.expires_at
        ).execute(&self.pool).await?;
        Ok(())
    }

    async fn find_by_code(&self, code: &ShortCode) -> Option<UrlEntry> {
        sqlx::query_as!(UrlRow, "SELECT * FROM urls WHERE code = $1", code.as_str())
            .fetch_optional(&self.pool).await.ok()??
            .map(|r| r.into()) // Into<UrlEntry> para UrlRow
    }

    async fn increment_clicks(&self, code: &ShortCode) -> Option<u64> {
        // Atomic increment + return new value (Postgres specific)
        let row = sqlx::query!("UPDATE urls SET clicks = clicks + 1 WHERE code = $1 RETURNING clicks", code.as_str())
            .fetch_optional(&self.pool).await.ok()??;
        Some(row.clicks as u64)
    }
}
```

#### Tests de Integración (`tests/db_test.rs`)
```rust
use sqlx::PgPool;
use url_shortener::{PgStorage, UrlStorage, models::*};
use testcontainers::{runners::AsyncRunner, GenericImage, ImageExt}; // testcontainers crate

#[tokio::test]
async fn test_pg_storage_full_cycle() {
    // 1. Spin up Postgres en Docker (Testcontainers)
    let container = GenericImage::new("postgres", "16")
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("POSTGRES_DB", "test")
        .with_exposed_port(5432)
        .start().await.unwrap();
    
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@localhost:{}/test", port);
    
    // 2. Pool & Migraciones
    let pool = PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    
    // 3. Test Storage
    let storage = PgStorage::new(pool);
    let entry = UrlEntry::new("https://example.com".parse().unwrap());
    storage.save(entry.clone()).await.unwrap();
    
    let found = storage.find_by_code(&entry.code).await.unwrap();
    assert_eq!(found.target_url, entry.target_url);
    
    let clicks = storage.increment_clicks(&entry.code).await.unwrap();
    assert_eq!(clicks, 1);
}
```
*Nota: Para CI rápido, usa `sqlx::test` macro (feature `test`) que maneja transacciones y rollback automático sin Docker.*

---

## 📅 SEMANA 12: OBSERVABILIDAD, DOCKER & CI/CD (Production Ready)
**Objetivo:** Logs estructurados (JSON), Métricas (Prometheus), Health Checks, Imagen Docker < 20MB, Pipeline CI completo.

### 🎯 Conceptos Clave

#### 1. `tracing` + `tracing-subscriber` (Structured Logging)
*   **`tracing::info!` / `debug!` / `error!`**: Campos clave-valor (`user_id=123`, `latency_ms=45`).
*   **`tracing_subscriber::fmt::format::json()`**: Output JSON para Loki/Datadog/Elastic.
*   **`tracing::instrument`**: Auto-genera span con args de función.
*   **Context Propagation:** `TraceContext` headers (W3C) para distributed tracing.

```rust
// main.rs setup
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

tracing_subscriber::registry()
    .with(EnvFilter::from_default_env()) // RUST_LOG=info,sqlx=warn
    .with(tracing_subscriber::fmt::layer().json()) // JSON output
    .init();

// En handlers
#[tracing::instrument(skip(storage), fields(code = %code))]
async fn redirect<S>(State(storage): State<Arc<S>>, Path(code): Path<ShortCode>) -> ...
```

#### 2. Métricas Prometheus (`metrics` + `metrics-exporter-prometheus`)
```rust
use metrics::{counter, histogram, gauge};
use metrics_exporter_prometheus::PrometheusBuilder;

// En main (una vez)
let recorder = PrometheusBuilder::new().build_recorder();
metrics::set_boxed_recorder(Box::new(recorder)).unwrap();

// En código
counter!("http_requests_total", "method" => "POST", "route" => "/shorten", "status" => "200").increment(1);
histogram!("db_query_duration_seconds", "query" => "insert_url").record(elapsed.as_secs_f64());
gauge!("active_connections").increment(1.0);

// Endpoint /metrics
async fn metrics_endpoint() -> String {
    metrics_exporter_prometheus::encode_to_string().unwrap()
}
// Router: .route("/metrics", get(metrics_endpoint))
```

#### 3. Health Checks (Kubernetes Ready)
| Endpoint | Propósito | Implementación |
| :--- | :--- | :--- |
| **`GET /health` / `/live`** | **Liveness**: Proceso vivo? (No reiniciar). | `200 OK` simple. |
| **`GET /ready`** | **Readiness**: Listo para tráfico? (DB conectada, cache caliente). | Check `pool.acquire().await.is_ok()`. Si falla -> `503 Service Unavailable`. K8s quita del Service. |

```rust
async fn readiness(State(pool): State<PgPool>) -> Result<&'static str, StatusCode> {
    pool.acquire().await.map(|_| "Ready").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}
```

#### 4. Dockerfile Multi-Stage Optimizado (Target: **< 20MB**)
```dockerfile
# ---- STAGE 1: BUILD (cargo-chef para cache deps) ----
FROM lukemathwalker/cargo-chef:latest-rust-1.78 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies (cache layer!)
RUN cargo chef cook --release --recipe-path recipe.json
# Build App
COPY . .
RUN cargo build --release --bin url_shortener

# ---- STAGE 2: RUNTIME (Distroless / Debian Slim / Scratch) ----
# Opción A: gcr.io/distroless/cc-debian12 (Glibc, ~10MB base, no shell)
# Opción B: debian:bookworm-slim (~30MB base, tiene shell/apt)
FROM gcr.io/distroless/cc-debian12 AS runtime

# Non-root user (Distroless nonroot: nonroot:nonroot 65532:65532)
USER 65532:65532

WORKDIR /app
COPY --from=builder /app/target/release/url_shortener /app/url_shortener

# Config via Env Vars
ENV RUST_LOG=info
ENV DATABASE_URL=postgres://...

EXPOSE 3000
ENTRYPOINT ["/app/url_shortener"]
```
*   **`cargo-chef`**: `cargo install cargo-chef`. Cachea dependencias independientemente de cambios en código fuente.
*   **Strip binario:** `cargo build --release` + `strip target/release/url_shortener` (en builder stage) reduce ~30-50%.

#### 5. CI/CD Pipeline (`.github/workflows/ci.yml`)
```yaml
name: CI/CD
on:
  push: branches: [main]
  pull_request:
  release: types: [published] # Trigger en GitHub Release

permissions:
  contents: read
  packages: write # Para ghcr.io

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo test --all-features --workspace
      - run: cargo test --doc --all-features
      - run: cargo sqlx prepare --check --workspace # Verifica SQL offline
      - run: cargo audit # Vulnerabilidades

  build-and-push:
    needs: check
    if: github.event_name == 'push' || github.event_name == 'release'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3
      - name: Log in to GHCR
        uses: docker/login-action@v3
        with: registry: ${{ env.REGISTRY }} username: ${{ github.actor }} password: ${{ secrets.GITHUB_TOKEN }}
      - name: Extract metadata (tags, labels)
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=ref,event=branch
            type=semver,pattern={{version}}
            type=sha
      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          context: .
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

---

## 📚 RESUMEN RECURSOS MES 3

| Semana | Lectura Oficial | Video Profundo | Práctica Clave |
| :--- | :--- | :--- | :--- |
| **9** | **Async Book (Cap 1-4)** | **Jon Gjengset: "Async Basics" + "Pin and Suffering"** | **Mini-Runtime "Toykio"** (Waker, Poll, Pin) |
| **10** | **Axum Guide (GitBook)** <br> **Tokio Tutorial (tokio.rs)** | Jon Gjengset: "Crust of Rust: Tokio" | **Url Shortener v1** (Axum, DashMap, Tower Middleware, JSON Lines) |
| **11** | **SQLx Docs** (Offline Mode, `query!`) <br> **Serde RS** (Attributes) | *Let's Get Rusty: SQLx Tutorial* | **Refactor v2: PostgreSQL + SQLx + Migraciones + Testcontainers** |
| **12** | **Tracing Docs** <br> **Docker Best Practices for Rust** | *Cloud Native Rust: Observability* | **Production Hardening:** Tracing JSON, Prometheus, Health Checks, Docker Distroless + Cargo-Chef, CI/CD GH Actions |

---

## ⚠️ PROBLEMAS COMUNES MES 3 (Y SOLUCIONES)

| Trampa | Síntoma | Solución |
| :--- | :--- | :--- |
| **`!Send` Future en `spawn`** | `future cannot be sent between threads safely` | ¿Capturas `Rc`, `RefCell`, `MutexGuard` (std)? **Mueve la creación DENTRO del `async move {}`** o usa `Arc`/`tokio::sync::Mutex`. Evita `thread_local!` en tareas spawn. |
| **Bloqueo en Runtime (Blocking the Executor)** | Latencia alta, timeouts, health checks fallan, 1 CPU al 100%. | **Nunca** hagas I/O blocking (`std::fs`, `reqwest::blocking`, CPU heavy) en task async. Usa `tokio::task::spawn_blocking(|| { ... })`. |
| **`MutexGuard` a través de `.await`** | Deadlock silencioso / `future not Send` / Panic "lock already held". | **Scope mínimo:** `{ let g = lock.lock().await; ... }` (drop antes de await). O `tokio::sync::Mutex` (guarda `Send`). |
| **SQLx Offline Mode Fallando en CI** | `Prepared query not found in cache` / `DATABASE_URL not set`. | **1.** `cargo sqlx prepare --workspace` localmente. **2.** Commit `sqlx-data.json` (o `.sqlx` dir). **3.** CI: `cargo sqlx prepare --check` **SIN** `DATABASE_URL`. |
| **Tracing no sale en Docker** | No logs en `docker logs` / K8s. | Log a **stdout/stderr** (default tracing). **NO** a archivos. Usa `tracing_subscriber::fmt::layer().json().with_writer(std::io::stdout)`. |
| **Docker Imagen Grande (>100MB)** | `cargo build` en imagen final, `debian` base, debug symbols. | **Multi-stage.** `cargo-chef`. Base `distroless` o `alpine` (cuidado con `musl` vs `glibc` y DNS). `strip` binario. `CARGO_PROFILE_RELEASE_STRIP=true` `CARGO_PROFILE_RELEASE_LTO=true` en `Cargo.toml`. |
| **Rate Limiting / Middleware Order** | Rate limit aplica *después* del handler / no ve IP real. | **Order importa:** `Router::new().layer(RateLimit).layer(Trace).route(...)`. Para IP real detrás de Proxy (Nginx/Cloudflare): `Extension(OriginalIp)` extractor o `tower_http::extract::ConnectInfo`. |

---

## 🧩 MATERIAL COMPLEMENTARIO: Laboratorio de Código Comentado

> Los ejemplos de las secciones **1–5 compilan con `rustc 1.81` (edición 2021) usando SOLO `std`** — sin Tokio ni crates externas. Son la versión mínima y verificable de lo que el runtime hace por dentro, perfectos para conectar con el ejercicio *Toykio* de la Semana 9. Las secciones 6–7 (Tokio/Axum) requieren las dependencias del proyecto y se muestran como referencia idiomática.

### 1️⃣ Una `Future` hecha a mano

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// La future más simple posible: lista en el primer `poll`.
struct Listo(i32);

impl Future for Listo {
    type Output = i32;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        Poll::Ready(self.0)   // nunca devuelve Pending
    }
}
```

> Una `Future` es **perezosa**: no hace nada hasta que alguien la *pollea*. Sin un executor que llame a `poll`, el código `async` jamás avanza.

### 2️⃣ Una future con estado: `Pending` antes de `Ready`

```rust
/// Devuelve `Pending` N veces y luego `Ready`. En cada `Pending` se
/// re-agenda a sí misma despertando su waker (lo que haría un timer real).
struct Cuenta { restante: u32 }

impl Future for Cuenta {
    type Output = &'static str;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        if self.restante == 0 {
            Poll::Ready("listo")
        } else {
            self.restante -= 1;
            cx.waker().wake_by_ref(); // "vuelve a pollearme cuando puedas"
            Poll::Pending
        }
    }
}
```

> **`Pending` + `Waker` es el corazón de async.** La future no bloquea: devuelve `Pending` y guarda (o usa) el `Waker` para avisar al executor cuando vuelva a tener trabajo.

### 3️⃣ `block_on`: un mini-executor de un solo hilo (solo `std`)

```rust
use std::sync::Arc;
use std::task::{Wake, Waker};

struct TareaWaker;
impl Wake for TareaWaker {
    fn wake(self: Arc<Self>) {} // un runtime real re-encolaría la tarea aquí
}

/// Ejecuta una future hasta el final haciendo *busy-poll*.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);              // Pin<Box<F>>: ya no se moverá
    let waker = Waker::from(Arc::new(TareaWaker));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => continue, // runtime real: aquí DUERME el hilo, no gira
        }
    }
}

fn main() {
    assert_eq!(block_on(Listo(42)), 42);
    assert_eq!(block_on(Cuenta { restante: 3 }), "listo");
    println!("OK");
}
```

> Esto es exactamente lo que `#[tokio::main]` hace a gran escala: construye un executor, crea wakers reales (que re-encolan tareas en vez de girar en vacío) y pollea las futures hasta completarlas.

### 4️⃣ `async`/`await`: la *state machine* que genera el compilador

```rust
async fn obtener_usuario(id: u64) -> u64 { id * 10 }
async fn obtener_posts(user: u64) -> u32 { (user % 7) as u32 }

async fn flujo(id: u64) -> (u64, u32) {
    let user = obtener_usuario(id).await;   // ← punto de yield 1
    let posts = obtener_posts(user).await;  // ← punto de yield 2
    (user, posts)
}

// Con el block_on de arriba:
// assert_eq!(block_on(flujo(5)), (50, 1));
```

> Cada `.await` es un **punto de suspensión**: el compilador convierte `flujo` en un `enum` de estados (`Start → EsperandoUsuario → EsperandoPosts → Done`). Por eso una future puede ser *self-referential* y necesita `Pin`.

### 5️⃣ `Send` verificado en compile-time

```rust
use std::sync::{Arc, Mutex};

fn exige_send<T: Send>(_: &T) {} // bound que `tokio::spawn` impone a tu future

fn main() {
    let compartible = Arc::new(Mutex::new(0));
    exige_send(&compartible); // ✅ Arc<Mutex<T>> es Send + Sync

    // let local = std::rc::Rc::new(0);
    // exige_send(&local);     // ❌ NO COMPILA: `Rc<i32>` no es Send
}
```

> Regla de oro async: lo que cruce un `.await` dentro de una tarea `spawn` debe ser `Send`. Por eso usas `Arc<tokio::sync::Mutex<T>>` y **nunca** mantienes un `MutexGuard` (o un `Rc`) vivo a través de un `.await`.

### 6️⃣ Handler de Axum con `AppError` → `IntoResponse` *(requiere las deps del proyecto)*

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

pub enum AppError {
    NotFound,
    Invalid(String),
}

// Esto es lo que convierte tus errores en respuestas HTTP limpias y permite usar `?` en handlers.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "no encontrado".to_string()),
            AppError::Invalid(m) => (StatusCode::BAD_REQUEST, m),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

// async fn handler(...) -> Result<Json<Algo>, AppError> {
//     let x = storage.find(...).await.ok_or(AppError::NotFound)?; // `?` mapea a 404
//     Ok(Json(x))
// }
```

### 7️⃣ `select!` para timeout / shutdown concurrente *(Tokio)*

```rust
// async fn con_timeout() {
//     tokio::select! {
//         _ = trabajo_largo()          => println!("terminó el trabajo"),
//         _ = tokio::time::sleep(d)    => println!("timeout: trabajo cancelado"),
//     }
//     // `select!` pollea ambas ramas; la que NO gana se DROPEA (se cancela).
// }
```

> `join!` espera a **todas** las futures; `select!` corre hasta que **una** termina y cancela el resto. Es el patrón canónico para *timeouts*, *races* y apagado limpio (`select!` entre el server y una señal `ctrl_c`).

---

## ✅ CHECKLIST FINAL MES 3 (Definition of Done - "Production Ready")

### Código & Arquitectura
- [ ] **Async Correcto:** Sin `.block_on()` en código async. `spawn_blocking` para CPU/Blocking I/O. `join!` para paralelismo en handlers.
-   [ ] **Send/Sync:** Todas las tareas `spawn` son `Send + 'static`. State (`AppState`) es `Arc<...>` con `Send + Sync`.
-   [ ] **SQLx:** `sqlx::query_as!` en **100%** de queries. `sqlx-data.json` commiteado. Migraciones versionadas. `sqlx::test` o Testcontainers en tests.
-   [ ] **Serde:** Modelos usan `flatten`, `rename`, `with`, `skip`, `default` apropiadamente. `DateTime<Utc>` serializado ISO8601 / Unix ts.
-   [ ] **Error Handling:** `AppError` enum con `thiserror`. `IntoResponse` impl para mapear a `(StatusCode, Json<ErrorBody>)`. No `unwrap` en handlers.

### Observabilidad
- [ ] **Logs:** `tracing` JSON structured. `trace_id` / `span_id` propagados. Niveles correctos (`info` request, `debug` sql, `error` panic).
- [ ] **Métricas:** `/metrics` endpoint expone: `http_requests_total{method, route, status}`, `http_request_duration_seconds{route}`, `db_query_duration_seconds{query}`, `urls_created_total`, `redirects_total`.
- [ ] **Health:** `/health` (Liveness), `/ready` (Readiness con check DB Pool). Kubernetes probes configurables.

### DevOps & Deployment
- [ ] **Docker:** Multi-stage `Dockerfile`. **Imagen final < 25MB** (Distroless/Scratch). Non-root user. `cargo-chef` cache funcionando (cambio código != rebuild deps).
- [ ] **CI Pipeline (GitHub Actions):**
    - [ ] `fmt` / `clippy -D warnings` / `test` / `sqlx prepare --check` / `audit` **PASS**.
    - [ ] Build Docker **multi-arch** (amd64/arm64) y push a **GHCR**.
    - [ ] Deploy automático a Staging en `push main`. Deploy Prod en `Release published`.
- [ ] **Documentación:** `README` con: Arquitectura, Variables de Entorno (`.env.example`), Cómo correr local (Docker Compose), Cómo testear, Endpoints API (OpenAPI/Swagger opcional via `utoipa`/`aide`).

### Proyecto Integrador: `url-shortener`
- [ ] `POST /shorten` -> `201 Created` + JSON `{ code, short_url }`.
- [ ] `GET /:code` -> `301 Redirect` + Incremento atómico `clicks`.
- [ ] `GET /health` / `/ready` / `/metrics` funcionando.
- [ ] Persistencia **PostgreSQL** (Docker Compose local / Testcontainers CI).
- [ ] Rate Limiting (ej. 10 req/s/IP) funcional.
- [ ] **Ejecuta:** `docker compose up -d` -> `curl -X POST localhost:3000/shorten -d '{"url":"https://rust-lang.org"}'` -> `curl -v localhost:3000/<code>` -> Redirect 301.

---

### 🚀 PRÓXIMO PASO: MES 4
> **SISTEMAS, CLI AVANZADO Y WASM (Rust "Close to Metal")**
> *FFI (`unsafe` seguro), CLI UX Pro (`clap`, `ratatui`), WebAssembly (`wasm-bindgen`, `leptos`/`yew`), Parsing (`nom`/`pest`).*

*Ya tienes un backend profesional. Ahora tocas el metal y el navegador.* 🦀🌐📦