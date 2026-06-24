# Fase 2 (II): Servidor HTTP, autenticación y tests E2E

La Semana 23 conecta el dominio con el mundo exterior. El `crates/core` con
su lógica pura y el `crates/db` con sus adaptadores SQLx ya están listos.
Esta semana construimos `crates/server`: la capa HTTP con Axum, la pila de
middleware Tower, autenticación JWT con JWKS, observabilidad estructurada y
la suite de tests de integración y E2E que verifica el sistema completo.

En esta sección aprenderemos:

- Cómo diseñar el `AppState` compartido entre handlers de forma que sea
  `Clone` sin copiar datos (`Arc` internamente en los pools).
- El orden correcto de la pila de middleware Tower y por qué importa.
- Cómo implementar un extractor personalizado `AuthUser` que valida JWT
  y devuelve el `UserId` tipado sin código repetido en cada handler.
- Cómo mapear `DomainError` a respuestas HTTP RFC 9457 con un único
  `impl IntoResponse`.
- Cómo generar documentación OpenAPI en tiempo de compilación con `utoipa`
  y servirla como Swagger UI.
- Cómo instrumentar el servidor con `tracing` + OpenTelemetry para logs
  estructurados en JSON y traces distribuidos.
- Cómo escribir tests de integración que levantan el servidor real en un
  puerto libre y tests E2E con contenedores reales.

> *"A system is never the sum of its parts; it's the product of their
> interactions."*
> — Russell Ackoff

---

## Arquitectura del servidor

```text
FLUJO DE UN REQUEST: POST /api/v1/links

  [Cliente]
      │  HTTPS
      ▼
  [TraceLayer]          ← asigna request_id, mide latencia, loguea status
      │
  [CompressionLayer]    ← descomprime request body si viene gzip/br
      │
  [CorsLayer]           ← valida Origin, emite headers Access-Control-*
      │
  [TimeoutLayer]        ← aborta si el handler tarda > 30s (configurable)
      │
  [RateLimitLayer]      ← verifica Redis: ¿esta API key supera RPM?
      │  429 si excede
  [Router de Axum]      ← /api/v1/links → links_router
      │
  [AuthLayer]           ← extractor AuthUser: valida JWT Bearer
      │  401 si inválido
  [Handler]
      │  fn crear_link(State(s), AuthUser(uid), Json(req))
      │      → use case create_link(&mut db_link_repo, uid, code, url)
      │      → 201 Created / DomainError → RFC 9457
      ▼
  [IntoResponse]        ← impl IntoResponse for DomainError
      │
  [CompressionLayer]    ← comprime response body (gzip/zstd por Accept-Encoding)
      │
  [Cliente]

ESTADO COMPARTIDO (AppState):
  ┌─────────────────────────────────────────────┐
  │ pool:     PgPool       (Arc interno)        │
  │ redis:    RedisPool    (Arc interno)        │
  │ config:   Arc<Config>                       │
  │ jwks:     Arc<RwLock<JwksCache>>            │
  └─────────────────────────────────────────────┘
  AppState implementa Clone → cada handler recibe una copia barata
```

---

## `crates/server/Cargo.toml`

```toml
[package]
name    = "linkmetrics-server"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "linkmetrics-server"
path = "src/main.rs"

[dependencies]
linkmetrics-core = { path = "../core" }
linkmetrics-db   = { path = "../db" }

axum       = { workspace = true }
tokio      = { workspace = true }
tower      = { version = "0.4", features = ["full"] }
tower-http = { version = "0.5", features = [
    "trace", "compression-gzip", "compression-zstd",
    "cors", "timeout", "request-id", "sensitive-headers",
] }

# Auth
jsonwebtoken = "9"
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }

# Rate limiting
governor = { version = "0.6", features = ["std"] }
redis    = { version = "0.27", features = ["tokio-comp", "connection-manager"] }

# Observabilidad
tracing            = { workspace = true }
tracing-subscriber = { workspace = true }
opentelemetry      = { version = "0.24", features = ["trace"] }
opentelemetry_otlp = { version = "0.17", features = ["tonic"] }
opentelemetry-semantic-conventions = "0.16"
tracing-opentelemetry = "0.25"
metrics            = "0.23"
metrics-exporter-prometheus = "0.15"

# OpenAPI
utoipa     = { version = "4", features = ["axum_extras", "uuid"] }
utoipa-swagger-ui = { version = "7", features = ["axum"] }

# Config y serialización
figment    = { workspace = true }
serde      = { workspace = true }
serde_json = "1"

# Utilidades
uuid = { workspace = true }
time = { workspace = true }
thiserror = { workspace = true }
anyhow = "1"

[dev-dependencies]
reqwest        = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
testcontainers = { workspace = true }
tokio          = { workspace = true, features = ["rt-multi-thread", "macros"] }
assert_cmd     = "2"
```

---

## Configuración con figment

```rust
// crates/server/src/config.rs

use figment::{providers::{Env, Format, Toml}, Figment};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub host:         String,
    pub port:         u16,
    pub database_url: String,
    pub redis_url:    String,
    pub jwks_url:     String,       // p.ej. "https://auth.ejemplo.com/.well-known/jwks.json"
    pub jwt_audience: String,
    pub log_level:    String,
    pub env:          Env,
    pub cors_origins: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Env {
    Dev,
    Staging,
    Prod,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config: Config = Figment::new()
            .merge(Toml::file("linkmetrics.toml"))
            .merge(Env::prefixed("LM_"))
            .extract()?;
        Ok(config)
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    // En dev/staging: servir Swagger UI. En prod: nunca.
    pub fn serve_openapi(&self) -> bool {
        self.env != Env::Prod
    }
}
```

---

## AppState — estado compartido

```rust
// crates/server/src/state.rs

use linkmetrics_db::{PgClickStore, PgLinkRepo};
use sqlx::PgPool;
use std::sync::Arc;
use crate::config::Config;
use crate::auth::JwksCache;
use tokio::sync::RwLock;

/// Estado compartido entre TODOS los handlers.
/// Clone es O(1): los campos pesados (Pool, Config) son Arc internamente.
#[derive(Clone)]
pub struct AppState {
    pub pool:   PgPool,
    pub config: Arc<Config>,
    pub jwks:   Arc<RwLock<JwksCache>>,
    // El RedisConnectionManager ya es Clone + Arc interno
    pub redis:  redis::aio::ConnectionManager,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let pool = linkmetrics_db::create_pool(&config.database_url).await?;

        let redis_client = redis::Client::open(config.redis_url.as_str())?;
        let redis = redis::aio::ConnectionManager::new(redis_client).await?;

        let jwks = Arc::new(RwLock::new(JwksCache::new()));

        Ok(Self {
            pool,
            config: Arc::new(config),
            jwks,
            redis,
        })
    }

    /// Construir un PgLinkRepo para esta request (barato: clona PgPool = Arc)
    pub fn link_repo(&self) -> PgLinkRepo {
        PgLinkRepo::new(self.pool.clone())
    }

    pub fn click_store(&self) -> PgClickStore {
        PgClickStore::new(self.pool.clone())
    }
}
```

---

## Autenticación JWT con JWKS

### Caché de claves públicas

```rust
// crates/server/src/auth/jwks.rs

use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Clave pública del servidor de autenticación (formato JWK).
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    pub kid: String,     // Key ID — identifica qué clave usó el token
    pub kty: String,     // "RSA" o "EC"
    pub alg: String,     // "RS256" o "ES256"
    pub n:   Option<String>,  // RSA: módulo (base64url)
    pub e:   Option<String>,  // RSA: exponente (base64url)
    // Para EC:
    pub crv: Option<String>,
    pub x:   Option<String>,
    pub y:   Option<String>,
}

#[derive(Debug, Deserialize)]
struct JwksDocument { keys: Vec<Jwk> }

/// Caché de claves públicas con TTL de 1 hora.
/// Se refresca automáticamente si el token usa un `kid` desconocido
/// (permite rotación de claves sin downtime).
pub struct JwksCache {
    keys:         HashMap<String, Jwk>,
    last_refresh: Option<Instant>,
    ttl:          Duration,
}

impl JwksCache {
    pub fn new() -> Self {
        Self {
            keys:         HashMap::new(),
            last_refresh: None,
            ttl:          Duration::from_secs(3600),
        }
    }

    pub fn is_stale(&self) -> bool {
        self.last_refresh
            .map(|t| t.elapsed() > self.ttl)
            .unwrap_or(true)
    }

    pub async fn refresh(&mut self, jwks_url: &str) -> anyhow::Result<()> {
        let doc: JwksDocument = reqwest::get(jwks_url)
            .await?
            .json()
            .await?;
        self.keys = doc.keys.into_iter().map(|k| (k.kid.clone(), k)).collect();
        self.last_refresh = Some(Instant::now());
        Ok(())
    }

    pub fn get(&self, kid: &str) -> Option<&Jwk> {
        self.keys.get(kid)
    }
}
```

### Extractor personalizado `AuthUser`

```rust
// crates/server/src/auth/extractor.rs

use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use linkmetrics_core::domain::UserId;
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

/// Claims que esperamos en el JWT de LinkMetrics.
#[derive(Debug, Deserialize)]
struct Claims {
    sub:  String,   // user UUID como string
    aud:  String,   // audiencia del token
    exp:  u64,      // expiración (epoch seconds)
    iat:  u64,      // emitido en (epoch seconds)
    // Scopes opcionales para RBAC
    #[serde(default)]
    scope: String,
}

/// Extractor: parsea el header `Authorization: Bearer <token>`,
/// valida la firma con JWKS y devuelve el `UserId` tipado.
/// Si falla → 401 automático, el handler nunca se ejecuta.
pub struct AuthUser(pub UserId);

/// Opcional: para rutas que pueden ser autenticadas o anónimas.
pub struct MaybeAuthUser(pub Option<UserId>);

pub struct AuthError(pub &'static str);

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "type":   "about:blank",
            "title":  "Unauthorized",
            "status": 401,
            "detail": self.0,
        });
        (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
    }
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Extraer el header Authorization
        let token = extract_bearer(&parts.headers)
            .ok_or(AuthError("header Authorization: Bearer requerido"))?;

        // 2. Decodificar sin verificar para leer el `kid` del header
        let header = jsonwebtoken::decode_header(token)
            .map_err(|_| AuthError("token JWT malformado"))?;
        let kid = header.kid
            .ok_or(AuthError("JWT sin campo kid en header"))?;

        // 3. Obtener la clave pública (con refresh si es necesario)
        let decoding_key = {
            let mut cache = state.jwks.write().await;
            if cache.is_stale() || cache.get(&kid).is_none() {
                cache.refresh(&state.config.jwks_url).await
                    .map_err(|_| AuthError("no se pudo refrescar JWKS"))?;
            }
            let jwk = cache.get(&kid)
                .ok_or(AuthError("kid desconocido"))?;
            decoding_key_from_jwk(jwk)
                .map_err(|_| AuthError("clave pública inválida"))?
        };

        // 4. Validar firma, expiración y audiencia
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[&state.config.jwt_audience]);

        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature =>
                    AuthError("token expirado"),
                _ => AuthError("token JWT inválido"),
            })?;

        // 5. Parsear el subject como UUID
        let uuid = Uuid::parse_str(&token_data.claims.sub)
            .map_err(|_| AuthError("sub no es un UUID válido"))?;

        Ok(AuthUser(UserId(uuid)))
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let auth = headers.get(axum::http::header::AUTHORIZATION)?.to_str().ok()?;
    auth.strip_prefix("Bearer ")
}

fn decoding_key_from_jwk(jwk: &crate::auth::jwks::Jwk) -> anyhow::Result<DecodingKey> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let n = jwk.n.as_deref().unwrap_or_default();
    let e = jwk.e.as_deref().unwrap_or_default();
    Ok(DecodingKey::from_rsa_components(
        &URL_SAFE_NO_PAD.decode(n)?,
        &URL_SAFE_NO_PAD.decode(e)?,
    )?)
}

// Implementación simplificada para tests (sin JWKS real)
#[cfg(test)]
pub fn auth_user_test(user_id: UserId) -> AuthUser {
    AuthUser(user_id)
}
```

---

## Handlers

### `impl IntoResponse for DomainError` — una sola vez

```rust
// crates/server/src/errors.rs

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use linkmetrics_core::domain::DomainError;
use serde_json::json;

impl IntoResponse for DomainError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        // Nunca filtrar detalles internos al cliente en errores 5xx
        let detail = match &self {
            DomainError::Internal(_) =>
                "error interno del servidor — ver logs".to_string(),
            other => other.to_string(),
        };

        let body = json!({
            "type":   "about:blank",
            "title":  self.http_title(),
            "status": self.http_status(),
            "detail": detail,
        });

        // Loguear errores internos con el detalle real
        if matches!(self, DomainError::Internal(_)) {
            tracing::error!(error = %self, "error interno en handler");
        }

        (status, Json(body)).into_response()
    }
}
```

### Handler: crear link

```rust
// crates/server/src/handlers/links.rs

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use linkmetrics_core::{
    domain::DomainError,
    services::link_service::create_link,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{auth::AuthUser, state::AppState};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateLinkRequest {
    pub target_url: String,
    pub code:       Option<String>,
    pub title:      Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LinkResponse {
    pub id:         Uuid,
    pub code:       String,
    pub target_url: String,
    pub short_url:  String,
    pub title:      Option<String>,
}

/// Crear un nuevo link corto.
#[utoipa::path(
    post,
    path = "/api/v1/links",
    request_body = CreateLinkRequest,
    responses(
        (status = 201, description = "Link creado", body = LinkResponse),
        (status = 409, description = "Código duplicado"),
        (status = 422, description = "Validación fallida"),
        (status = 401, description = "No autenticado"),
        (status = 429, description = "Rate limit excedido"),
    ),
    security(("ApiKeyAuth" = [])),
    tag = "Links"
)]
pub async fn crear_link(
    State(state):  State<AppState>,
    AuthUser(uid): AuthUser,
    Json(req):     Json<CreateLinkRequest>,
) -> Result<impl IntoResponse, DomainError> {
    // Generar código aleatorio si no se especificó
    let code = req.code.unwrap_or_else(|| nanoid_simple(8));

    let mut repo = state.link_repo();
    let link = create_link(&mut repo, uid, code, req.target_url).await?;

    let response = LinkResponse {
        id:         link.id.0,
        short_url:  format!("{}/s/{}", state.config.base_url, link.code),
        code:       link.code,
        target_url: link.target_url,
        title:      link.title,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// Caracteres URL-safe para códigos generados
fn nanoid_simple(len: usize) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut h = DefaultHasher::new();
    SystemTime::now().hash(&mut h);
    std::thread::current().id().hash(&mut h);
    let mut seed = h.finish();

    (0..len).map(|_| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        alphabet[(seed as usize) % alphabet.len()] as char
    }).collect()
}
```

### Handler: redirección pública (hot path)

```rust
// crates/server/src/handlers/redirect.rs

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use linkmetrics_core::{
    domain::DomainError,
    services::link_service::redirect,
};

use crate::state::AppState;

/// Redirección pública — el endpoint más frecuente del sistema.
/// Sin autenticación. La IP se hashea antes de almacenarla (GDPR).
pub async fn redirigir(
    State(state): State<AppState>,
    Path(code):   Path<String>,
    headers:      axum::http::HeaderMap,
) -> Result<Response, DomainError> {
    // Extraer datos del request para analytics
    let ip_raw = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .unwrap_or("unknown");

    let ip_hash = sha256_hex(ip_raw);

    let referrer = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(512).collect());

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(256).collect());

    let mut repo        = state.link_repo();
    let mut click_store = state.click_store();

    let target_url = redirect(
        &repo,
        &mut click_store,
        &code,
        ip_hash,
        None,        // país: resuelto en Semana 24 (GeoIP)
        referrer,
        user_agent,
    ).await?;

    // 301 Permanent para crawlers/SEO; 302 para analytics exacto en Semana 24
    Ok((
        StatusCode::MOVED_PERMANENTLY,
        [(header::LOCATION, target_url)],
    ).into_response())
}

fn sha256_hex(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Simplificado — en producción usar sha2::Sha256 con salt diario
    let mut h = DefaultHasher::new();
    input.hash(&mut h);
    format!("{:016x}", h.finish())
}
```

### Handler: health y readiness

```rust
// crates/server/src/handlers/health.rs

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use std::time::Instant;

use crate::state::AppState;

// Guardado al arranque para calcular uptime
static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

pub fn init_start_time() {
    START.get_or_init(Instant::now);
}

#[derive(Serialize)]
struct HealthBody {
    status:   &'static str,
    version:  &'static str,
    uptime_s: u64,
}

/// GET /health — liveness: el proceso vive. Siempre 200 si responde.
pub async fn liveness() -> impl IntoResponse {
    let uptime_s = START.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);
    Json(HealthBody {
        status:  "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_s,
    })
}

#[derive(Serialize)]
struct ReadyBody {
    status: &'static str,
    checks: std::collections::HashMap<String, CheckStatus>,
}

#[derive(Serialize)]
struct CheckStatus {
    ok:         bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message:    Option<String>,
    latency_ms: u64,
}

/// GET /ready — readiness: todas las dependencias responden.
/// Kubernetes usa esto para el selector de endpoints del Service.
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let mut checks = std::collections::HashMap::new();
    let mut all_ok = true;

    // Verificar PostgreSQL
    let t = Instant::now();
    let db_result = sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await;
    let db_ok = db_result.is_ok();
    all_ok &= db_ok;
    checks.insert("postgres".to_string(), CheckStatus {
        ok:         db_ok,
        message:    db_result.err().map(|e| e.to_string()),
        latency_ms: t.elapsed().as_millis() as u64,
    });

    // Verificar Redis
    let t = Instant::now();
    let redis_result: Result<String, _> = {
        let mut conn = state.redis.clone();
        redis::cmd("PING").query_async(&mut conn).await
    };
    let redis_ok = redis_result.is_ok();
    all_ok &= redis_ok;
    checks.insert("redis".to_string(), CheckStatus {
        ok:         redis_ok,
        message:    redis_result.err().map(|e| e.to_string()),
        latency_ms: t.elapsed().as_millis() as u64,
    });

    let body = ReadyBody {
        status: if all_ok { "ok" } else { "degraded" },
        checks,
    };

    let status = if all_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(body))
}
```

---

## Router y middleware stack

```rust
// crates/server/src/router.rs

use axum::{
    routing::{delete, get, post},
    Router,
};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    sensitive_headers::SetSensitiveHeadersLayer,
    timeout::TimeoutLayer,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use std::time::Duration;

use crate::{
    handlers::{
        health::{liveness, readiness},
        links::crear_link,
        redirect::redirigir,
    },
    openapi::ApiDoc,
    state::AppState,
};

pub fn build_router(state: AppState, serve_openapi: bool) -> Router {
    // ── Router de la API ─────────────────────────────────────────────────
    let api_router = Router::new()
        .route("/links",       post(crear_link))
        .route("/links/{code}", get(obtener_link).delete(desactivar_link))
        .route("/links/{code}/analytics", get(analytics_link))
        .with_state(state.clone());

    // ── Router de operaciones (sin auth) ─────────────────────────────────
    let ops_router = Router::new()
        .route("/health", get(liveness))
        .route("/ready",  get(readiness))
        .route("/metrics", get(metrics_handler))
        .with_state(state.clone());

    // ── Router de redirección pública (sin auth, sin versionado) ─────────
    let redirect_router = Router::new()
        .route("/s/{code}", get(redirigir))
        .with_state(state.clone());

    // ── Composición del router raíz ───────────────────────────────────────
    let mut app = Router::new()
        .nest("/api/v1", api_router)
        .merge(ops_router)
        .merge(redirect_router);

    // Swagger UI solo en dev/staging — NUNCA en prod
    if serve_openapi {
        app = app.merge(
            SwaggerUi::new("/docs")
                .url("/docs/openapi.json", ApiDoc::openapi())
        );
    }

    // ── Pila de middleware Tower (orden importa: de exterior a interior) ──
    app.layer(
        ServiceBuilder::new()
            // 1. Ocultar headers sensibles en los logs (Authorization, Cookie)
            .layer(SetSensitiveHeadersLayer::new([
                axum::http::header::AUTHORIZATION,
                axum::http::header::COOKIE,
            ]))
            // 2. Asignar un UUID único a cada request
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            // 3. Tracing: logs estructurados con latencia y status
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(
                        DefaultMakeSpan::new()
                            .include_headers(true)
                            .level(tracing::Level::INFO),
                    )
                    .on_response(
                        DefaultOnResponse::new()
                            .level(tracing::Level::INFO)
                            .latency_unit(tower_http::LatencyUnit::Milliseconds),
                    ),
            )
            // 4. Timeout global por request
            .layer(TimeoutLayer::new(Duration::from_secs(30)))
            // 5. Compresión de respuestas (gzip / zstd según Accept-Encoding)
            .layer(CompressionLayer::new())
            // 6. CORS (configurar en prod con origins específicos)
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)    // ← restringir en prod
                    .allow_methods(Any)
                    .allow_headers(Any),
            ),
    )
}

// Stubs para compilación — implementados con el repo en el handler real
async fn obtener_link() -> impl axum::response::IntoResponse {
    axum::http::StatusCode::NOT_IMPLEMENTED
}
async fn desactivar_link() -> impl axum::response::IntoResponse {
    axum::http::StatusCode::NOT_IMPLEMENTED
}
async fn analytics_link() -> impl axum::response::IntoResponse {
    axum::http::StatusCode::NOT_IMPLEMENTED
}
async fn metrics_handler() -> impl axum::response::IntoResponse {
    // En producción: metrics_exporter_prometheus::PrometheusHandle::render()
    "# metrics placeholder\n"
}
```

---

## OpenAPI con `utoipa`

```rust
// crates/server/src/openapi.rs

use utoipa::{
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
    Modify, OpenApi,
};

use crate::handlers::links::{CreateLinkRequest, LinkResponse};

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "ApiKeyAuth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "LinkMetrics API",
        version = "1.0.0",
        description = "Plataforma de URL shortening con analytics en tiempo real.",
        contact(name = "LinkMetrics", email = "api@linkmetrics.example"),
    ),
    paths(
        crate::handlers::links::crear_link,
    ),
    components(
        schemas(CreateLinkRequest, LinkResponse),
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Links",     description = "Gestión de URLs cortas"),
        (name = "Analytics", description = "Clicks y métricas"),
        (name = "Ops",       description = "Health, readiness, métricas"),
    ),
)]
pub struct ApiDoc;
```

---

## Observabilidad: tracing + OpenTelemetry

```rust
// crates/server/src/telemetry.rs

use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Inicializar tracing con:
/// - Logs estructurados en JSON (stdout)
/// - Exportación de traces a Jaeger via OTLP (si hay otlp_endpoint)
///
/// El `WorkerGuard` debe mantenerse vivo durante todo el proceso.
/// Dropparlo antes de `main()` = perder los últimos logs.
pub fn init_tracing(
    log_level:     &str,
    otlp_endpoint: Option<&str>,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    // Capa JSON para stdout (producción)
    let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_current_span(true)
        .with_span_list(true);

    if let Some(endpoint) = otlp_endpoint {
        // Capa OpenTelemetry (traces a Jaeger/Tempo/Honeycomb)
        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(endpoint)
            )
            .install_batch(opentelemetry::runtime::Tokio)
            .expect("inicializar tracer OTLP");

        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        tracing_subscriber::registry()
            .with(filter)
            .with(json_layer)
            .with(otel_layer)
            .init();
    } else {
        // Sin OTLP — solo JSON a stdout (dev sin Jaeger)
        tracing_subscriber::registry()
            .with(filter)
            .with(json_layer)
            .init();
    }

    Some(guard)
}
```

---

## `main.rs` — arranque con graceful shutdown

```rust
// crates/server/src/main.rs

mod auth;
mod config;
mod errors;
mod handlers;
mod openapi;
mod router;
mod state;
mod telemetry;

use crate::{
    config::Config,
    handlers::health::init_start_time,
    state::AppState,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Cargar configuración (falla rápido si falta DATABASE_URL, etc.)
    let config = Config::load()?;

    // 2. Inicializar tracing antes de cualquier log
    let _guard = telemetry::init_tracing(
        &config.log_level,
        None, // Some("http://jaeger:4317") en staging/prod
    );

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        env     = ?config.env,
        "LinkMetrics Server iniciando"
    );

    // 3. Inicializar estado (pools de DB y Redis)
    let serve_openapi = config.serve_openapi();
    let state = AppState::new(config).await?;

    // 4. Aplicar migraciones pendientes (en dev/staging; en prod: CI/CD)
    linkmetrics_db::run_migrations(&state.pool).await?;
    tracing::info!("Migraciones aplicadas");

    // 5. Registrar hora de inicio (para /health uptime)
    init_start_time();

    // 6. Construir el router completo
    let addr = state.config.addr();
    let app  = router::build_router(state.clone(), serve_openapi);

    // 7. Vincular al puerto
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(addr = %addr, "Escuchando");

    // 8. Servir con graceful shutdown
    //    Al recibir SIGTERM o Ctrl-C:
    //    - Deja de aceptar nuevas conexiones
    //    - Espera a que las requests en vuelo terminen (hasta 30s)
    //    - Luego sale limpiamente
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // 9. Flush final de traces (crítico para no perder el último span)
    opentelemetry::global::shutdown_tracer_provider();
    tracing::info!("Servidor apagado limpiamente");

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async {
        signal::ctrl_c().await.expect("ctrl-c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c    => tracing::info!("Ctrl-C recibido"),
        _ = terminate => tracing::info!("SIGTERM recibido"),
    }
}
```

---

## Rate limiting con governor + Redis

```rust
// crates/server/src/middleware/rate_limit.rs
//
// Estrategia: Token Bucket por API Key.
// Local (governor): protección por instancia, latencia < 1µs.
// Redis (Lua): protección distribuida entre instancias, latencia ~1ms.
// En dev/single-node: solo governor. En prod multi-instancia: Redis.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use std::{num::NonZeroU32, sync::Arc, time::Duration};

/// Limitador local en memoria. Compartido via Arc entre handlers.
pub type LocalLimiter = Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>;

pub fn build_local_limiter(rpm: u32) -> LocalLimiter {
    let quota = Quota::with_period(Duration::from_secs(60))
        .unwrap()
        .allow_burst(NonZeroU32::new(rpm).unwrap());
    Arc::new(RateLimiter::direct(quota))
}

/// Middleware de rate limiting local.
/// Para rate limiting por API Key (por usuario), usar el Lua script de Redis.
pub async fn rate_limit_middleware(
    State(limiter): State<LocalLimiter>,
    request:        Request,
    next:           Next,
) -> Response {
    if limiter.check().is_err() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", "60")],
            serde_json::json!({
                "type":   "about:blank",
                "title":  "Too Many Requests",
                "status": 429,
                "detail": "Rate limit excedido. Reintentar en 60 segundos.",
            }).to_string(),
        ).into_response();
    }
    next.run(request).await
}

// ── Rate limiting distribuido con Redis Lua ────────────────────────────────
//
// Script Lua atómico: INCR + EXPIRE en una sola operación.
// No hay race condition entre el GET del contador y el SET del TTL.

pub const RATE_LIMIT_LUA: &str = r#"
local key     = KEYS[1]     -- "lm:rl:{api_key_hash}"
local limit   = tonumber(ARGV[1])   -- requests por ventana
local window  = tonumber(ARGV[2])   -- ventana en segundos (60)

local current = redis.call('INCR', key)
if current == 1 then
    redis.call('EXPIRE', key, window)
end

if current > limit then
    local ttl = redis.call('TTL', key)
    return {0, ttl}  -- {permitido=0, retry_after=ttl}
else
    return {1, 0}    -- {permitido=1, retry_after=0}
end
"#;

/// Verificar rate limit de una API key contra Redis.
/// Retorna Ok(()) si está dentro del límite, Err(retry_after_secs) si excede.
pub async fn check_redis_rate_limit(
    redis:       &mut redis::aio::ConnectionManager,
    api_key_hash: &str,
    limit_rpm:    u32,
) -> Result<(), u64> {
    let key = format!("lm:rl:{api_key_hash}");
    let result: (i64, i64) = redis::Script::new(RATE_LIMIT_LUA)
        .key(&key)
        .arg(limit_rpm)
        .arg(60u32)
        .invoke_async(redis)
        .await
        .unwrap_or((1, 0));  // en caso de error Redis: permitir (fail-open)

    if result.0 == 0 {
        Err(result.1 as u64)
    } else {
        Ok(())
    }
}
```

---

## Tests

### Tests de integración — servidor real en puerto libre

```rust
// crates/server/tests/integration/links_test.rs

use axum::http::StatusCode;
use reqwest::Client;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Levantar el servidor en un puerto libre y devolver su dirección.
/// Cada test obtiene su propio servidor aislado.
async fn spawn_test_server() -> (SocketAddr, Client) {
    // Puerto 0 → el OS asigna un puerto libre
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr     = listener.local_addr().unwrap();

    // Nota: en un test real se usaría un DB de test con testcontainers.
    // Aquí lo dejamos como patrón; el servidor con MockState compila igual.
    let state = create_test_state().await;
    let app   = crate::router::build_router(state, false);

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none()) // no seguir redirects en tests
        .build()
        .unwrap();

    (addr, client)
}

async fn create_test_state() -> crate::state::AppState {
    // En CI: usar la DB de test del servicio postgres del workflow.
    // DATABASE_URL se inyecta por el entorno de CI.
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://linkmetrics:linkmetrics_dev@localhost/linkmetrics_test".into());
    let config = crate::config::Config {
        host: "127.0.0.1".into(),
        port: 0,
        database_url,
        redis_url:    "redis://127.0.0.1:6379".into(),
        jwks_url:     "http://localhost/.well-known/jwks.json".into(),
        jwt_audience: "linkmetrics".into(),
        log_level:    "warn".into(),
        env:          crate::config::Env::Dev,
        cors_origins: vec![],
        base_url:     "http://localhost".into(),
    };
    crate::state::AppState::new(config).await.unwrap()
}

#[tokio::test]
async fn health_devuelve_200() {
    let (addr, client) = spawn_test_server().await;

    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn crear_link_sin_auth_da_401() {
    let (addr, client) = spawn_test_server().await;

    let resp = client
        .post(format!("http://{addr}/api/v1/links"))
        .json(&serde_json::json!({ "target_url": "https://ejemplo.com" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], 401);
    assert!(body["detail"].as_str().is_some());
}

#[tokio::test]
async fn redirigir_codigo_inexistente_da_404() {
    let (addr, client) = spawn_test_server().await;

    let resp = client
        .get(format!("http://{addr}/s/codigo-que-no-existe-jamás"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn crear_y_redirigir_link() {
    let (addr, client) = spawn_test_server().await;

    // Usar un token de test — en CI se genera con la clave privada de test
    let token = generar_jwt_test();

    // Crear link
    let create_resp = client
        .post(format!("http://{addr}/api/v1/links"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "target_url": "https://ejemplo.com/destino",
            "code":       "test-link-e2e",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let link: serde_json::Value = create_resp.json().await.unwrap();
    assert_eq!(link["code"], "test-link-e2e");

    // Redirigir
    let redirect_resp = client
        .get(format!("http://{addr}/s/test-link-e2e"))
        .send()
        .await
        .unwrap();

    assert_eq!(redirect_resp.status(), StatusCode::MOVED_PERMANENTLY);
    let location = redirect_resp.headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(location, "https://ejemplo.com/destino");
}

fn generar_jwt_test() -> String {
    // En producción: usar la clave privada RSA de test almacenada en CI secrets.
    // Aquí simplificado como placeholder; el test runner sustituye en CI.
    std::env::var("TEST_JWT").unwrap_or_else(|_| "test-jwt-placeholder".into())
}
```

### Tests E2E con `testcontainers`

```rust
// crates/server/tests/e2e/full_flow_test.rs
//
// Levanta un contenedor real de PostgreSQL y Redis para cada test suite.
// Más lento que integration tests pero verifica el sistema completo.

use testcontainers::{
    clients::Cli,
    core::WaitFor,
    images::postgres::Postgres,
    Image,
};

struct TestEnv {
    db_url:    String,
    redis_url: String,
}

async fn setup_test_env(docker: &Cli) -> TestEnv {
    // PostgreSQL en contenedor efímero
    let pg_container = docker.run(
        Postgres::default()
            .with_env_var("POSTGRES_USER",     "test")
            .with_env_var("POSTGRES_PASSWORD", "test")
            .with_env_var("POSTGRES_DB",       "test_linkmetrics"),
    );
    let pg_port = pg_container.get_host_port_ipv4(5432);
    let db_url  = format!("postgres://test:test@localhost:{pg_port}/test_linkmetrics");

    // Redis en contenedor efímero
    let redis_container = docker.run(
        testcontainers::GenericImage::new("redis", "7-alpine")
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections")),
    );
    let redis_port = redis_container.get_host_port_ipv4(6379);
    let redis_url  = format!("redis://localhost:{redis_port}");

    // Correr migraciones sobre la DB nueva
    let pool = linkmetrics_db::create_pool(&db_url).await.unwrap();
    linkmetrics_db::run_migrations(&pool).await.unwrap();

    TestEnv { db_url, redis_url }
}

#[tokio::test]
async fn flujo_completo_crear_y_redirigir() {
    let docker = Cli::default();
    let env    = setup_test_env(&docker).await;

    // El resto del test igual que integration test pero con DB real
    // que se destruye al salir del scope (docker.run devuelve RAII)
    println!("DB: {}", env.db_url);
    println!("Redis: {}", env.redis_url);
    // ... igual que el integration test pero con infraestructura real
}
```

### Tests del CLI con `assert_cmd`

```rust
// crates/cli/tests/cli_test.rs

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn cli_sin_argumentos_muestra_ayuda() {
    Command::cargo_bin("lm")
        .unwrap()
        .assert()
        .failure()  // sin subcommand → exit 1
        .stderr(contains("Usage: lm <COMMAND>"));
}

#[test]
fn cli_help_funciona() {
    Command::cargo_bin("lm")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("LinkMetrics admin CLI"));
}

#[test]
fn cli_migrate_sin_database_url_falla_con_mensaje_claro() {
    Command::cargo_bin("lm")
        .unwrap()
        .arg("migrate")
        .env_remove("LM_DATABASE_URL")
        .env_remove("DATABASE_URL")
        .assert()
        .failure()
        .stderr(contains("database_url")); // figment menciona el campo faltante
}

#[test]
fn cli_check_sin_db_reporta_error_postgres() {
    Command::cargo_bin("lm")
        .unwrap()
        .arg("check")
        .env("LM_DATABASE_URL", "postgres://no-existe:5432/test")
        .env("LM_REDIS_URL",    "redis://no-existe:6379")
        .assert()
        .success() // check no falla el proceso, solo reporta
        .stdout(contains("✗"));
}
```

---

## Errores de compilación frecuentes esta semana

### Error 1: extractor sin `FromRequestParts` implementado

```text
error[E0277]: the trait bound `AuthUser: FromRequestParts<AppState>` is not satisfied
  --> src/handlers/links.rs:42:17
   |
42 | pub async fn crear_link(
   |                        ^
   |
   = note: add `#[async_trait]` o implementa `FromRequestParts<AppState>` para `AuthUser`

SOLUCIÓN: el impl requiere la anotación correcta:
  #[async_trait]
  impl FromRequestParts<AppState> for AuthUser { ... }
```

### Error 2: `State<T>` y `with_state` de tipo incorrecto

```text
error[E0308]: mismatched types
  --> src/router.rs:18:10
   |
18 |     .route("/links", post(crear_link))
   |             ^^^^^^^ expected `AppState`, found `()`
   |
   = note: el handler extrae State<AppState> pero el router no tiene ese estado

SOLUCIÓN: encadenar .with_state(state) al router, no al app raíz:
  Router::new()
      .route("/links", post(crear_link))
      .with_state(state)   // ← aquí, en el router que tiene handlers que lo usan
```

### Error 3: `IntoResponse` no implementado para el tipo de error

```text
error[E0277]: `DomainError` doesn't implement `IntoResponse`
  --> src/handlers/links.rs:58:5
   |
58 |     let link = create_link(...)?;  // ← el ? propaga DomainError
   |                                ^
   |
SOLUCIÓN: añadir en crates/server/src/errors.rs:
  impl IntoResponse for DomainError { ... }
  
  Y el handler debe retornar Result<impl IntoResponse, DomainError>,
  no Result<impl IntoResponse, anyhow::Error>.
```

### Error 4: lifetime en el extractor de header

```text
error[E0515]: cannot return value referencing local variable `auth`
  --> src/auth/extractor.rs:45:5
   |
44 |     let auth = headers.get(AUTHORIZATION)?.to_str().ok()?;
45 |     auth.strip_prefix("Bearer ")
   |     ^^^^ returns a value referencing data owned by the current function

SOLUCIÓN: el .to_str() devuelve &str atado a la variable local.
  Hay que .to_owned() o retornar Option<String> y luego usar &str del caller.
  
  fn extract_bearer(headers: &HeaderMap) -> Option<String> {
      let auth = headers.get(AUTHORIZATION)?.to_str().ok()?;
      auth.strip_prefix("Bearer ").map(str::to_owned)
  }
```

---

## Deploy a staging

### `docker-compose.staging.yml`

```yaml
# Override para staging (sobre docker-compose.yml base)
# Uso: docker compose -f docker-compose.yml -f docker-compose.staging.yml up -d

services:
  app:
    image: ghcr.io/${GITHUB_REPOSITORY}:${IMAGE_TAG:-latest}
    environment:
      LM_ENV:          staging
      LM_HOST:         0.0.0.0
      LM_PORT:         3000
      LM_DATABASE_URL: ${DATABASE_URL}
      LM_REDIS_URL:    ${REDIS_URL}
      LM_JWKS_URL:     ${JWKS_URL}
      LM_JWT_AUDIENCE: ${JWT_AUDIENCE}
      LM_LOG_LEVEL:    info
      RUST_LOG:        info,tower_http=debug
    ports:
      - "3000:3000"
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
      start_period: 30s
    deploy:
      resources:
        limits:
          cpus: "1"
          memory: 256M
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy
    restart: unless-stopped
```

### Fly.io (`fly.toml`) — alternativa a Docker Compose

```toml
# fly.toml — deploy en Fly.io (staging gratuito en shared-cpu-1x)
app    = "linkmetrics-staging"
region = "mad"   # Madrid; elegir la más cercana

[build]
  dockerfile = "infra/Dockerfile"

[env]
  LM_HOST     = "0.0.0.0"
  LM_PORT     = "3000"
  LM_ENV      = "staging"
  LM_LOG_LEVEL = "info"

[[services]]
  internal_port = 3000
  protocol      = "tcp"

  [[services.ports]]
    handlers = ["tls", "http"]
    port     = 443

  [[services.http_checks]]
    interval = "10s"
    timeout  = "5s"
    grace_period = "30s"
    method   = "GET"
    path     = "/ready"

[mounts]
  # Sin mounts — stateless (DB y Redis son servicios externos)
```

```bash
# Desplegar a staging:
fly secrets set \
  LM_DATABASE_URL="postgres://..." \
  LM_REDIS_URL="redis://..." \
  LM_JWKS_URL="https://auth.ejemplo.com/.well-known/jwks.json" \
  LM_JWT_AUDIENCE="linkmetrics"

fly deploy --image ghcr.io/usuario/linkmetrics:latest
fly status   # verificar que arrancó
fly logs     # ver logs en tiempo real
```

---

## Tabla: métricas RED y USE a instrumentar

| Tipo | Métrica | Label | Qué indica |
|------|---------|-------|-----------|
| **R**ate | `http_requests_total` | method, path, status | Throughput del endpoint |
| **E**rrors | `http_requests_total` (status=5xx) | method, path | Tasa de error |
| **D**uration | `http_request_duration_seconds` | method, path, quantile | Latencia P50/P95/P99 |
| **U**tilization | `db_pool_size` | — | Conexiones activas vs máximo |
| **S**aturation | `db_pool_wait_total` | — | Peticiones esperando conexión |
| **E**rrors (infra) | `db_query_errors_total` | operation | Errores de query por tipo |
| R | `redirect_total` | — | Clicks recibidos |
| D | `redirect_duration_ms` | — | Latencia del hot path |
| R | `rate_limit_rejected_total` | — | Requests rechazados por RPM |

```rust
// Registrar métrica de redirect en el handler (con el crate `metrics`)
metrics::counter!("redirect_total").increment(1);
metrics::histogram!("redirect_duration_ms").record(elapsed_ms);
```

---

## ✅ Checklist de la Semana 23

- [ ] `crates/server` compila: `cargo build -p linkmetrics-server` sin errores
  ni warnings (`-D warnings` en CI).
- [ ] `AppState` implementa `Clone` y es `Send + Sync` (verificación: el
  compilador lo exige al pasarlo a `axum::serve`).
- [ ] El extractor `AuthUser` devuelve 401 con cuerpo RFC 9457 ante token
  ausente, expirado o con firma inválida.
- [ ] `impl IntoResponse for DomainError` es el ÚNICO lugar donde se mapean
  errores a HTTP. Ningún handler tiene `StatusCode::CONFLICT` hardcodeado.
- [ ] `utoipa` genera el spec OpenAPI y Swagger UI está disponible en
  `http://localhost:3000/docs` en modo dev. En modo prod (`LM_ENV=prod`),
  la ruta `/docs` devuelve 404.
- [ ] Todos los handlers están anotados con `#[utoipa::path(...)]` con al
  menos 3 códigos de respuesta documentados.
- [ ] `/health` devuelve 200 siempre que el proceso viva (liveness).
  `/ready` devuelve 503 si PostgreSQL o Redis no responden (readiness).
- [ ] Los logs son JSON estructurado con campos `request_id`, `latency_ms`,
  `status`, `method`, `path`. Verificar con:
  `cargo run | python3 -m json.tool | head -40`
- [ ] El rate limiter rechaza con 429 + header `Retry-After` cuando se
  supera el límite. Test: `for i in {1..200}; do curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3000/api/v1/links; done | sort | uniq -c`
- [ ] `cargo test -p linkmetrics-server --test integration` pasa con el
  servidor y DB reales levantados (`docker compose up -d`).
- [ ] Al menos 5 tests de integración cubren: health 200, sin-auth 401,
  not-found 404, crear link, redirigir link.
- [ ] `assert_cmd` tests del CLI pasan: ayuda, argumentos inválidos, check.
- [ ] La imagen Docker de staging levanta y `/ready` devuelve 200 en menos de
  30 segundos. Verificar con el healthcheck de Docker Compose.
- [ ] `docker image inspect ghcr.io/usuario/linkmetrics:latest | grep Size`
  muestra < 30 MB.
- [ ] Al enviar SIGTERM al proceso (`kill -15 <pid>`), el servidor completa
  las requests en vuelo y sale con código 0 en ≤ 30 segundos. Loguea
  `"Servidor apagado limpiamente"`.

> **Siguiente sección:** [Semana 24 — Especialización y pulido final](section_04.md)
