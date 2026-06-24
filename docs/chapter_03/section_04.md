# Observabilidad, Docker y CI/CD: production-ready

La Semana 12 es la última del Mes 3 y cierra el ciclo del Url Shortener. El código
funciona, las queries están verificadas en compilación, la base de datos persiste — pero
un servicio de producción necesita más: responder "¿está sano?" a Kubernetes, emitir
logs estructurados que un sistema centralizado pueda indexar, exponer métricas para
dashboards de alerta, y todo eso empaquetado en una imagen Docker pequeña y reproducible
que se construye y despliega sin intervención manual.

En esta sección aprenderemos:

- El modelo mental de `tracing`: spans, eventos, suscriptores y capas.
- Cómo configurar logs JSON estructurados con `tracing-subscriber`.
- El macro `#[instrument]` para propagación automática de contexto.
- Métricas Prometheus con los macros `counter!`, `histogram!`, `gauge!`.
- Health checks de Kubernetes: liveness (`/health`) y readiness (`/ready`).
- Dockerfile multi-stage con `cargo-chef` para caches de dependencias reproducibles.
- Imagen final con `distroless`, binario sin símbolos, usuario sin privilegios.
- Pipeline completo de CI/CD con GitHub Actions: lint, test, build multi-arch, push.

> 💡 **Filosofía de la Semana 12:** *Un servicio que no puedes observar es un servicio
> que no puedes operar. Los logs, las métricas y los health checks no son opcionales en
> producción — son la interfaz entre tu código y las personas que lo mantienen a las 3
> de la mañana.*

---

## `tracing`: observabilidad estructurada

### Spans vs eventos

La biblioteca `log` de Rust emite mensajes lineales ("esto pasó"). `tracing` es más
rica: modela la **causalidad** de lo que ocurre en un programa concurrente.

```text
MODELO DE tracing

Span: unidad de trabajo con duración
┌─────────────────────────────────────────────────────────┐
│  span: "acortar_url"  (http.method="POST", code="ab12") │
│                                                         │
│  evento: "validando URL"  ◄── log puntual dentro del span
│  evento: "guardando en BD"                              │
│  span: "almacen::guardar" ◄── span anidado              │
│  │  evento: "INSERT ejecutado" (rows=1, ms=3)           │
│  └────────────────────────────────────────────────────  │
└─────────────────────────────────────────────────────────┘

Cada span tiene: nombre, campos clave-valor, duración, span padre.
Los eventos heredan los campos del span activo.
```

En sistemas distribuidos, el `trace_id` se propaga entre servicios para reconstruir el
flujo completo de una petición de extremo a extremo.

### Macros de eventos

```rust
use tracing::{debug, error, info, trace, warn};

// Niveles: trace < debug < info < warn < error
trace!(detalle = "muy verboso", "solo en diagnóstico");
debug!(query = "SELECT ...", rows = 42, "resultado de BD");
info!(codigo = %codigo, url = %url_orig, "URL acortada");  // %: usa Display
warn!(limite = 0.9, actual = 0.95, "uso de pool elevado");
error!(err = ?e, "error al guardar");                      // ?: usa Debug
```

La diferencia entre `%valor` (Display) y `?valor` (Debug):

```rust
let url = "https://example.com";
info!(url = %url, "petición");   // info!(url = "https://example.com", ...)
info!(url = ?url, "petición");   // info!(url = "\"https://example.com\"", ...)
```

### Spans manuales

```rust
use tracing::{info_span, Instrument};

async fn operacion_compleja() {
    // Span que envuelve todo el bloque async
    let span = info_span!("op_compleja", tarea_id = 42);

    async {
        info!("paso 1");   // este evento tiene tarea_id=42 en su contexto
        info!("paso 2");
    }
    .instrument(span)      // asocia el span al bloque async
    .await;
}
```

### `#[instrument]`: spans automáticos para funciones

El macro más útil de `tracing`. Convierte cada llamada a la función en un span que
captura automáticamente los argumentos como campos:

```rust
use tracing::instrument;

#[instrument(skip(pool), fields(code = %code))]
async fn buscar_en_bd(pool: &sqlx::PgPool, code: &str) -> Option<String> {
    // Dentro de esta función, todos los logs incluyen code="abc123"
    debug!("consultando BD");

    let resultado = sqlx::query_scalar!(
        "SELECT target_url FROM urls WHERE code = $1", code
    )
    .fetch_optional(pool)
    .await
    .ok()?;

    if resultado.is_some() {
        info!("encontrada");
    } else {
        warn!("no encontrada");
    }

    resultado
}
```

Opciones de `#[instrument]`:

| Opción | Efecto |
| :--- | :--- |
| `skip(arg)` | No captura ese argumento (útil para pools, estados grandes) |
| `skip_all` | No captura ningún argumento |
| `fields(clave = expr)` | Añade campos personalizados al span |
| `name = "nombre"` | Sobrescribe el nombre del span (default: nombre de fn) |
| `level = "debug"` | Nivel del span (default: `info`) |
| `err` | Registra el `Err(...)` como evento de error si la fn falla |
| `ret` | Registra el valor de retorno como evento |

---

## Configurar `tracing-subscriber`

El subscriber recibe todos los spans y eventos, y decide qué hacer con ellos (imprimir,
escribir en archivo, enviar a OpenTelemetry, etc.). La arquitectura de capas permite
combinar múltiples destinos.

### Configuración básica (desarrollo)

```rust
use tracing_subscriber::EnvFilter;

tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    // RUST_LOG=info,url_shortener=debug,sqlx=warn
    .init();
```

Salida:

```
2024-09-01T10:30:00Z  INFO url_shortener::handlers: URL acortada codigo="ab12Cd" url="https://..."
2024-09-01T10:30:00Z DEBUG url_shortener::almacen: INSERT ejecutado rows=1 ms=3
```

### Configuración JSON (producción)

Los sistemas de logs (Loki, Datadog, Elasticsearch, CloudWatch) esperan JSON por línea:

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn inicializar_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .json()                         // output JSON
                .with_current_span(true)        // incluye el span activo
                .with_span_list(false)          // sin lista de spans padres (verboso)
                .with_target(true)              // módulo Rust donde ocurrió
                .with_file(false)               // omitir nombre de archivo
                .with_line_number(false)
        )
        .init();
}
```

Salida (una línea por evento, formateada aquí para legibilidad):

```json
{
  "timestamp": "2024-09-01T10:30:00.123456Z",
  "level": "INFO",
  "target": "url_shortener::handlers",
  "span": { "name": "acortar_url", "code": "ab12Cd" },
  "fields": { "message": "URL acortada", "url": "https://rust-lang.org" }
}
```

### Multi-capa: formato legible en dev, JSON en prod

```rust
use std::io;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn inicializar_tracing() {
    let entorno = std::env::var("ENTORNO").unwrap_or_else(|_| "dev".into());
    let filtro  = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));

    let reg = tracing_subscriber::registry().with(filtro);

    if entorno == "prod" {
        reg.with(tracing_subscriber::fmt::layer().json()).init();
    } else {
        reg.with(tracing_subscriber::fmt::layer().pretty()).init();
    }
}
```

### Añadir `trace_id` a cada petición

Para correlacionar logs de una misma petición HTTP (trazabilidad):

```rust
use axum::{extract::Request, middleware::Next, response::Response};
use tracing::Span;
use uuid::Uuid;

pub async fn middleware_trace_id(req: Request, next: Next) -> Response {
    let trace_id = req
        .headers()
        .get("x-trace-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Registrar el trace_id en el span actual para que todos los logs lo hereden
    Span::current().record("trace_id", &trace_id.as_str());

    let mut resp = next.run(req).await;

    // Propagar el trace_id al cliente
    resp.headers_mut().insert(
        "x-trace-id",
        trace_id.parse().unwrap(),
    );
    resp
}
```

---

## Métricas Prometheus

Prometheus es el estándar de facto para métricas en ecosistemas cloud. Funciona con un
modelo pull: tu servicio expone un endpoint `/metrics` y Prometheus scrapeaba
periódicamente.

```text
FLUJO DE MÉTRICAS

Tu código                  Prometheus              Grafana
──────────                 ──────────              ───────
counter!("requests")  →    GET /metrics (cada 15s) → Dashboard
histogram!("latency") →    almacena series temporal → Alertas
gauge!("connections") →    query PromQL            → Visualización
```

### Instalación

```toml
[dependencies]
metrics                     = "0.24"
metrics-exporter-prometheus = "0.16"
```

### Inicialización

```rust
use metrics_exporter_prometheus::PrometheusBuilder;

pub fn inicializar_metricas() -> metrics_exporter_prometheus::PrometheusHandle {
    let (recorder, handle) = PrometheusBuilder::new()
        .build()
        .expect("no se pudo inicializar Prometheus");

    metrics::set_global_recorder(recorder)
        .expect("recorder ya inicializado");

    handle
}
```

### Los tres tipos de métrica

```rust
use metrics::{counter, gauge, histogram};
use std::time::Duration;

// COUNTER: solo sube, nunca baja — para conteos acumulados
counter!("http_requests_total",
    "method" => "POST",
    "route"  => "/shorten",
    "status" => "201"
).increment(1);

// GAUGE: puede subir y bajar — para valores instantáneos
gauge!("pool_connections_active").set(7.0);
gauge!("pool_connections_idle").set(3.0);

// HISTOGRAM: distribución de valores — para latencias y tamaños
histogram!("http_request_duration_seconds",
    "route"  => "/shorten",
    "method" => "POST"
).record(Duration::from_millis(45).as_secs_f64());

histogram!("db_query_duration_seconds",
    "query" => "insert_url"
).record(Duration::from_millis(3).as_secs_f64());
```

### Middleware para métricas HTTP automáticas

En lugar de registrar métricas en cada handler, un middleware las captura para todas
las rutas:

```rust
use axum::{extract::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};
use std::time::Instant;

pub async fn middleware_metricas(req: Request, next: Next) -> Response {
    let inicio  = Instant::now();
    let metodo  = req.method().to_string();
    let ruta    = req.uri().path().to_owned();

    // Anonimizar rutas con parámetros: /ab12Cd → /:codigo
    let ruta_plantilla = anonimizar_ruta(&ruta);

    let resp = next.run(req).await;

    let estado  = resp.status().as_u16().to_string();
    let latencia = inicio.elapsed().as_secs_f64();

    counter!("http_requests_total",
        "method" => metodo.clone(),
        "route"  => ruta_plantilla.clone(),
        "status" => estado
    ).increment(1);

    histogram!("http_request_duration_seconds",
        "method" => metodo,
        "route"  => ruta_plantilla
    ).record(latencia);

    resp
}

fn anonimizar_ruta(ruta: &str) -> String {
    // Convierte /abc123 en /:codigo, /abc123/stats en /:codigo/stats
    let partes: Vec<&str> = ruta.trim_start_matches('/').split('/').collect();
    let anonimizadas: Vec<&str> = partes
        .iter()
        .map(|p| if p.len() == 8 && p.chars().all(|c| c.is_alphanumeric()) {
            ":codigo"
        } else {
            p
        })
        .collect();
    format!("/{}", anonimizadas.join("/"))
}
```

### Handler `/metrics`

```rust
use axum::{extract::State, response::IntoResponse};
use metrics_exporter_prometheus::PrometheusHandle;

pub async fn handler_metricas(
    State(handle): State<PrometheusHandle>,
) -> impl IntoResponse {
    handle.render()
}
```

El output que Prometheus leerá:

```
# HELP http_requests_total Total de peticiones HTTP
# TYPE http_requests_total counter
http_requests_total{method="POST",route="/shorten",status="201"} 42
http_requests_total{method="GET",route="/:codigo",status="301"} 189

# HELP http_request_duration_seconds Latencia de peticiones HTTP
# TYPE http_request_duration_seconds histogram
http_request_duration_seconds_bucket{route="/shorten",le="0.005"} 38
http_request_duration_seconds_bucket{route="/shorten",le="0.01"} 41
http_request_duration_seconds_bucket{route="/shorten",le="+Inf"} 42
http_request_duration_seconds_sum{route="/shorten"} 0.94
http_request_duration_seconds_count{route="/shorten"} 42
```

---

## Health checks para Kubernetes

Kubernetes usa dos probes para gestionar el ciclo de vida de los pods:

```text
PROBES DE KUBERNETES

/health (Liveness)                      /ready (Readiness)
───────────────────                     ──────────────────
¿Está vivo el proceso?                  ¿Puede aceptar tráfico?
                                        
Si falla → reiniciar el pod             Si falla → quitar del Service
(útil para deadlocks, panics)           (útil durante arranque, sobrecarga)
                                        
Siempre devuelve 200 OK                 Verifica: BD conectada,
(si devuelve error, algo está           cache caliente, dependencias
muy mal con el proceso mismo)           externas disponibles
```

```rust
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use sqlx::PgPool;

// Liveness: el proceso está vivo
pub async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

// Readiness: las dependencias están disponibles
pub async fn readiness(State(pool): State<PgPool>) -> impl IntoResponse {
    // Verificar que el pool puede adquirir una conexión
    match pool.acquire().await {
        Ok(_conn) => (
            StatusCode::OK,
            Json(json!({
                "status": "ready",
                "checks": { "database": "ok" }
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not ready",
                "checks": { "database": "error" },
                "detail": e.to_string()
            })),
        )
            .into_response(),
    }
}
```

Manifiesto de Kubernetes (referencia):

```yaml
# k8s/deployment.yaml
livenessProbe:
  httpGet:
    path: /health
    port: 3000
  initialDelaySeconds: 5    # esperar N segundos antes del primer check
  periodSeconds: 10
  failureThreshold: 3       # N fallos consecutivos → reiniciar

readinessProbe:
  httpGet:
    path: /ready
    port: 3000
  initialDelaySeconds: 5
  periodSeconds: 5
  failureThreshold: 2       # N fallos → sacar del Service (no del pool)
```

---

## Apagado graceful

Kubernetes envía `SIGTERM` antes de matar el pod. Un servicio que no lo maneja pierde
peticiones en vuelo. Axum lo soporta con `with_graceful_shutdown`:

```rust
use tokio::signal;

async fn senal_apagado() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("no se pudo instalar el handler de Ctrl+C");
    };

    #[cfg(unix)]
    let sigterm = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("no se pudo instalar SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    // Primera señal que llegue (Ctrl+C en dev, SIGTERM de k8s en prod)
    tokio::select! {
        _ = ctrl_c  => {},
        _ = sigterm => {},
    }

    tracing::info!("señal de apagado recibida, cerrando conexiones...");
}

// En main:
// axum::serve(listener, app)
//     .with_graceful_shutdown(senal_apagado())
//     .await?;
```

---

## Dockerfile multi-stage con `cargo-chef`

El problema del Dockerfile ingenuo es que `cargo build` recompila **todas** las
dependencias si cambia cualquier línea de código fuente. Con proyectos grandes, esto
puede tardar 10-15 minutos en CI. `cargo-chef` resuelve esto separando la compilación
de dependencias del código de la aplicación.

```text
SIN cargo-chef:                     CON cargo-chef:
                                    
Cambio en handlers.rs               Cambio en handlers.rs
         ↓                                   ↓
Invalida TODO el layer Docker        Solo invalida el layer
         ↓                           del código fuente
cargo build tarda 15 min                     ↓
                                    Deps cacheadas → 30s
```

### El `Dockerfile`

```dockerfile
# ── STAGE 1: Chef — imagen base con cargo-chef preinstalado ───────────────
FROM lukemathwalker/cargo-chef:latest-rust-1.82 AS chef
WORKDIR /app

# ── STAGE 2: Planner — calcula qué deps necesita el proyecto ──────────────
FROM chef AS planner
COPY . .
# Genera recipe.json: la "lista de ingredientes" de dependencias
RUN cargo chef prepare --recipe-path recipe.json

# ── STAGE 3: Builder — compila deps (cacheado) y luego el código ──────────
FROM chef AS builder

# Instalar dependencias del sistema necesarias para compilar (ej. OpenSSL)
# Con rustls no necesitamos libssl-dev
# RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copiar recipe.json e instalar SOLO las dependencias
# Este layer se cachea entre builds mientras Cargo.toml no cambie
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Copiar el código fuente y compilar la aplicación
COPY . .

# Offline mode para sqlx (requiere .sqlx/ commitado)
ENV SQLX_OFFLINE=true

RUN cargo build --release --bin url_shortener

# Reducir tamaño del binario quitando símbolos de debug
RUN strip target/release/url_shortener

# ── STAGE 4: Runtime — imagen mínima sin toolchain de Rust ─────────────────
# distroless/cc-debian12: ~20MB, incluye glibc, sin shell, sin apt, sin root
FROM gcr.io/distroless/cc-debian12 AS runtime

# Usuario sin privilegios (distroless nonroot: UID/GID 65532)
USER 65532:65532

WORKDIR /app

# Copiar SOLO el binario compilado desde el builder
COPY --from=builder /app/target/release/url_shortener /app/url_shortener

# Copiar migraciones (se aplican al arrancar)
COPY --from=builder /app/migrations /app/migrations

# Configuración vía variables de entorno
ENV RUST_LOG=info
ENV ENTORNO=prod

EXPOSE 3000

ENTRYPOINT ["/app/url_shortener"]
```

### Comparación de imágenes base

| Base | Tamaño aprox. | Shell | Paquetes | Cuándo usar |
| :--- | :--- | :--- | :--- | :--- |
| `debian:bookworm-slim` | ~30 MB | sí (`bash`) | `apt-get` | Debug en producción, librerías C extra |
| `gcr.io/distroless/cc-debian12` | ~20 MB | no | ninguno | Producción estándar con glibc |
| `gcr.io/distroless/cc-debian12:nonroot` | ~20 MB | no | ninguno | Producción, sin root explícito |
| `alpine:3.20` | ~5 MB | sí (`sh`) | `apk` | Binarios estáticos compilados con musl |
| `scratch` | 0 MB | no | ninguno | Binarios 100% estáticos (musl + no TLS dinámico) |

**Cuidado con Alpine/musl**: la biblioteca estándar de C `musl` tiene sutiles diferencias
con `glibc` (resolución DNS asíncrona, rendimiento de memoria). Para producción seria,
`distroless` es el mejor equilibrio.

### Optimizaciones de Cargo para release

En `Cargo.toml`:

```toml
[profile.release]
strip    = true    # equivale al `strip` manual del Dockerfile
lto      = true    # Link-Time Optimization: reduce tamaño y mejora velocidad
opt-level = 3      # optimización máxima (ya es el default en release)
codegen-units = 1  # más lento de compilar, binario más pequeño y rápido
panic    = "abort" # en vez de unwind — reduce tamaño, sin unwinding tables
```

Con estas opciones, un proyecto típico de Axum + SQLx queda entre 8-15 MB.

### `docker-compose.yml` para desarrollo local

```yaml
services:
  app:
    build:
      context: .
      target: builder     # ← solo hasta el stage builder para dev
    command: cargo watch -x run   # hot-reload (requiere cargo-watch)
    environment:
      DATABASE_URL: postgres://usuario:clave@db:5432/url_shortener
      BASE_URL: http://localhost:3000
      RUST_LOG: debug
      ENTORNO: dev
    ports:
      - "3000:3000"
    volumes:
      - .:/app                    # montar código fuente para hot-reload
      - cargo-cache:/usr/local/cargo/registry
    depends_on:
      db:
        condition: service_healthy

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER:     usuario
      POSTGRES_PASSWORD: clave
      POSTGRES_DB:       url_shortener
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U usuario -d url_shortener"]
      interval: 5s
      timeout: 3s
      retries: 5

  prometheus:
    image: prom/prometheus:v2.54.0
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro

  grafana:
    image: grafana/grafana:11.2.0
    ports:
      - "3001:3000"
    environment:
      GF_SECURITY_ADMIN_PASSWORD: admin

volumes:
  pgdata:
  cargo-cache:
```

`prometheus.yml`:

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: url_shortener
    static_configs:
      - targets: ['app:3000']
    metrics_path: /metrics
```

---

## `main.rs` final: todo integrado

```rust
mod almacen;
mod error;
mod estado;
mod handlers;
mod models;

use almacen::AlmacenPostgres;
use estado::EstadoApp;
use handlers::{acortar_url, chequeo_salud, estadisticas, listar_urls, redirigir};

use axum::{
    extract::{Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sqlx::PgPool;
use std::time::Instant;
use tokio::signal;
use tower_http::cors::CorsLayer;
use tracing::instrument;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// ── Tracing ───────────────────────────────────────────────────────────────

fn inicializar_tracing() {
    let filtro = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=debug"));
    let entorno = std::env::var("ENTORNO").unwrap_or_else(|_| "dev".into());

    let reg = tracing_subscriber::registry().with(filtro);
    if entorno == "prod" {
        reg.with(tracing_subscriber::fmt::layer().json()).init();
    } else {
        reg.with(tracing_subscriber::fmt::layer().pretty()).init();
    }
}

// ── Métricas ──────────────────────────────────────────────────────────────

fn inicializar_metricas() -> PrometheusHandle {
    let (recorder, handle) = PrometheusBuilder::new().build().unwrap();
    metrics::set_global_recorder(recorder).unwrap();
    handle
}

async fn handler_metricas(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

// ── Health Checks ─────────────────────────────────────────────────────────

async fn liveness() -> &'static str { "OK" }

async fn readiness(State(pool): State<PgPool>) -> impl IntoResponse {
    use axum::http::StatusCode;
    use serde_json::json;
    match pool.acquire().await {
        Ok(_)  => (StatusCode::OK, axum::Json(json!({"status":"ready","db":"ok"}))).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE,
                   axum::Json(json!({"status":"not_ready","db":e.to_string()}))).into_response(),
    }
}

// ── Middleware ────────────────────────────────────────────────────────────

async fn middleware_observabilidad(req: Request, next: Next) -> Response {
    let inicio  = Instant::now();
    let metodo  = req.method().to_string();
    let ruta    = req.uri().path().to_owned();

    let resp    = next.run(req).await;

    let estado  = resp.status().as_u16().to_string();
    let latencia = inicio.elapsed().as_secs_f64();

    tracing::info!(
        metodo = %metodo, ruta = %ruta,
        estado = %estado, ms = (latencia * 1000.0) as u64,
        "petición completada"
    );

    counter!("http_requests_total",
        "method" => metodo.clone(), "route" => ruta.clone(), "status" => estado
    ).increment(1);

    histogram!("http_request_duration_seconds",
        "method" => metodo, "route" => ruta
    ).record(latencia);

    resp
}

// ── Apagado graceful ──────────────────────────────────────────────────────

async fn senal_apagado() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("error instalando Ctrl+C");
    };
    #[cfg(unix)]
    let sigterm = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("error instalando SIGTERM")
            .recv().await;
    };
    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c  => tracing::info!("Ctrl+C recibido"),
        _ = sigterm => tracing::info!("SIGTERM recibido"),
    }
    tracing::info!("iniciando apagado graceful...");
}

// ── Entry point ───────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    inicializar_tracing();
    let metricas_handle = inicializar_metricas();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL debe estar definida");
    let base_url = std::env::var("BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3000".into());

    // Pool con tamaño apropiado para el número de workers del runtime
    let pool = sqlx::postgres::PgPoolOptions::new()
        .min_connections(2)
        .max_connections(20)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("migraciones aplicadas");

    let almacen = AlmacenPostgres::nuevo(pool.clone());
    let estado  = EstadoApp::nuevo(almacen, base_url);

    let app = Router::new()
        // Endpoints de negocio
        .route("/shorten",       post(acortar_url::<AlmacenPostgres>))
        .route("/urls",          get(listar_urls::<AlmacenPostgres>))
        .route("/:codigo",       get(redirigir::<AlmacenPostgres>))
        .route("/:codigo/stats", get(estadisticas::<AlmacenPostgres>))
        .with_state(estado)
        // Endpoints de infraestructura (estado propio, no el del negocio)
        .route("/health",   get(liveness))
        .route("/ready",    get(readiness).with_state(pool))
        .route("/metrics",  get(handler_metricas).with_state(metricas_handle))
        // Middleware en orden: primero observabilidad, luego CORS
        .layer(middleware::from_fn(middleware_observabilidad))
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!(addr = %listener.local_addr()?, "servidor arrancado");

    axum::serve(listener, app)
        .with_graceful_shutdown(senal_apagado())
        .await?;

    tracing::info!("servidor apagado limpiamente");
    Ok(())
}
```

---

## Pipeline CI/CD con GitHub Actions

### `.github/workflows/ci.yml`

```yaml
name: CI/CD

on:
  push:
    branches: [main]
  pull_request:
  release:
    types: [published]

permissions:
  contents: read
  packages: write       # necesario para push a GHCR

env:
  REGISTRY:   ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  # ── Job 1: comprobaciones de código ──────────────────────────────────────
  check:
    name: Lint, Test, Audit
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_USER: ci
          POSTGRES_PASSWORD: ci
          POSTGRES_DB: url_shortener_ci
        ports: ["5432:5432"]
        options: >-
          --health-cmd "pg_isready -U ci"
          --health-interval 5s
          --health-timeout 3s
          --health-retries 5

    env:
      DATABASE_URL: postgres://ci:ci@localhost:5432/url_shortener_ci

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2   # cachea target/ entre runs

      - name: Formato
        run: cargo fmt --all -- --check

      - name: Clippy (warnings = errors)
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Aplicar migraciones CI
        run: |
          cargo install sqlx-cli --no-default-features --features postgres,rustls
          sqlx migrate run

      - name: Tests
        run: cargo test --all-features --workspace
        env:
          RUST_LOG: error   # silenciar logs durante tests

      - name: Verificar metadatos SQLx offline
        run: cargo sqlx prepare --check --workspace

      - name: Auditoría de seguridad
        run: |
          cargo install cargo-audit
          cargo audit

      - name: Documentación (sin warnings)
        run: cargo doc --no-deps --all-features
        env:
          RUSTDOCFLAGS: "-D warnings"

  # ── Job 2: construir y publicar imagen Docker ─────────────────────────────
  build-and-push:
    name: Docker Build & Push
    needs: check
    # Solo en push a main o en un release publicado
    if: github.event_name == 'push' || github.event_name == 'release'
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Configurar QEMU (para emulación ARM64)
        uses: docker/setup-qemu-action@v3

      - name: Configurar Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Autenticarse en GHCR
        uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extraer metadatos (tags, labels)
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=ref,event=branch
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=sha,prefix=sha-,format=short
            type=raw,value=latest,enable={{is_default_branch}}

      - name: Build y Push multi-arch
        uses: docker/build-push-action@v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64   # soportar Graviton, Apple Silicon en prod
          push: true
          tags:   ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha          # leer cache de GitHub Actions
          cache-to:   type=gha,mode=max # guardar cache con todas las capas

      - name: Verificar tamaño de imagen
        run: |
          docker pull ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:latest
          SIZE=$(docker image inspect ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:latest \
                   --format='{{.Size}}')
          SIZE_MB=$((SIZE / 1024 / 1024))
          echo "Tamaño imagen: ${SIZE_MB}MB"
          if [ "$SIZE_MB" -gt 30 ]; then
            echo "⚠️  Imagen supera 30MB (${SIZE_MB}MB)"
            exit 1
          fi
```

### Flujo completo de un release

```text
CICLO DE VIDA DE UN CAMBIO

Developer                 GitHub                      Producción
─────────                 ──────                      ──────────
git push main         →   CI: fmt, clippy, test   →   [si pasan]
                          CI: sqlx prepare --check     Docker build amd64+arm64
                          CI: cargo audit              Push ghcr.io/repo:sha-abc123
                                                       Push ghcr.io/repo:main

git tag v1.2.3        →   Release published        →   Docker build amd64+arm64
git push --tags            (event: release)             Push ghcr.io/repo:1.2.3
                                                        Push ghcr.io/repo:1.2
                                                        Push ghcr.io/repo:latest
```

### Recetas `.cargo/config.toml` para builds reproducibles

```toml
[build]
# Usar sccache para cachear compilaciones entre máquinas/CI
# rustc-wrapper = "sccache"

[target.x86_64-unknown-linux-gnu]
# Linkear estáticamente solo algunas librerías del sistema (no musl completo)
# rustflags = ["-C", "target-feature=+crt-static"]

[profile.release]
strip         = true
lto           = true
codegen-units = 1
panic         = "abort"
```

---

## Secuencia de verificación local antes de mergear

```bash
# 1. Lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# 2. Tests
DATABASE_URL=postgres://... cargo test --all-features

# 3. Actualizar metadatos SQLx si cambiaron queries
DATABASE_URL=postgres://... cargo sqlx prepare --workspace
git add .sqlx/

# 4. Build Docker local
docker build -t url-shortener:local .
docker image ls url-shortener:local   # verificar tamaño

# 5. Smoke test del contenedor
docker compose up -d
sleep 3
curl -s http://localhost:3000/health
curl -s http://localhost:3000/ready | jq .
curl -s -X POST http://localhost:3000/shorten \
     -H "Content-Type: application/json" \
     -d '{"url":"https://www.rust-lang.org"}' | jq .
curl -s http://localhost:3000/metrics | grep http_requests_total
docker compose down
```

---

## ✅ Checklist de la Semana 12 (Definition of Done — Production Ready)

### Observabilidad

- [ ] `tracing_subscriber` emite JSON estructurado en producción y formato legible en
  desarrollo (detectado vía variable de entorno `ENTORNO`).
- [ ] Todos los handlers tienen `#[instrument(skip(estado), fields(...))]` con los campos
  relevantes para correlación de logs.
- [ ] El middleware `middleware_observabilidad` registra método, ruta, estado HTTP y
  latencia en cada petición, y emite las métricas correspondientes.
- [ ] `GET /metrics` expone counters e histogramas que Prometheus puede scrapear.
- [ ] `GET /health` devuelve `200 OK` siempre que el proceso esté vivo.
- [ ] `GET /ready` devuelve `503 Service Unavailable` si el pool de BD no puede
  adquirir una conexión.

### Docker

- [ ] El `Dockerfile` tiene cuatro stages: `chef`, `planner`, `builder`, `runtime`.
- [ ] Cambiar `handlers.rs` sin tocar `Cargo.toml` **no** recompila las dependencias
  (verificar en segundo build que el layer de deps es caché `CACHED`).
- [ ] La imagen final usa `distroless/cc-debian12`, corre como usuario `65532` y
  **no tiene shell**.
- [ ] `docker image ls url-shortener:local` muestra menos de **25 MB**.
- [ ] `docker compose up -d` arranca todo el stack y el smoke test pasa.

### CI/CD

- [ ] El job `check` pasa: `fmt`, `clippy -D warnings`, `test`, `sqlx prepare --check`,
  `cargo audit`.
- [ ] El job `build-and-push` construye para `linux/amd64` y `linux/arm64`.
- [ ] El pipeline reutiliza caché de GitHub Actions entre runs (segundo push es
  significativamente más rápido que el primero).
- [ ] Un release publicado en GitHub genera tags `1.2.3`, `1.2` y `latest` en GHCR.

### Proyecto integrador completo

- [ ] `POST /shorten` → `201 Created` + JSON `{ codigo, url_corta }`.
- [ ] `GET /:codigo` → `301 Redirect` + incremento atómico de clics en PG.
- [ ] `GET /:codigo/stats` → JSON con `creada_en` como Unix timestamp (Serde `with`).
- [ ] `GET /health` → `200 OK`.
- [ ] `GET /ready` → `200 OK` si BD disponible, `503` si no.
- [ ] `GET /metrics` → texto Prometheus con `http_requests_total` e histograma.
- [ ] Tests de integración con `testcontainers` pasan (`cargo test --test api_test`).
- [ ] Migraciones se aplican automáticamente al arrancar (`sqlx::migrate!`).
- [ ] `SIGTERM` → apagado graceful (respuestas en vuelo se completan antes de cerrar).

> **Fin del Mes 3.** Has construido un servicio web observable, tipado, seguro y
> contenedorizado. El Mes 4 baja al metal: FFI, CLI profesional y WebAssembly.
>
> **Siguiente paso:** Mes 4 — [Sistemas, CLI avanzado y WASM](../chapter_04/section_00.md).
