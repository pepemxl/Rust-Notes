# Bases de datos con SQLx & Serde avanzado

La Semana 11 eleva el Url Shortener de memoria volátil a persistencia real en
PostgreSQL. El objetivo no es solo "conectarse a una base de datos" — es hacerlo con
las mismas garantías que nos da Rust en el resto del código: errores detectados en
tiempo de compilación, sin cadenas de SQL sueltas, sin mapeos manuales frágiles.

En esta sección aprenderemos:

- Por qué SQLx verifica las queries en tiempo de compilación y cómo funciona.
- Las tres macros de query (`query!`, `query_as!`, `query_scalar!`) y cuándo
  usar cada una.
- Gestión del pool de conexiones con `PgPool`.
- El flujo completo de migraciones con `sqlx-cli`.
- Cómo hacer que el pipeline de CI funcione sin `DATABASE_URL` (modo offline).
- Transacciones y el tipo `Transaction<Postgres>`.
- Serde avanzado: los 9 atributos más importantes con casos de uso reales.
- Serializadores y deserializadores personalizados con `#[serde(with = "...")]`.
- El refactor completo del Url Shortener v2 con PostgreSQL + SQLx + Serde.
- Tests de integración con `testcontainers`.

> 💡 **Filosofía de la Semana 11:** *La diferencia entre un ORM y SQLx es que SQLx te
> deja escribir SQL real y te ayuda a no equivocarte en él. Escribes SQL, el compilador
> lo valida, Rust hace el mapeo. Sin magia, sin rendimiento oculto.*

---

## SQLx: SQL verificado en tiempo de compilación

### El problema con el SQL dinámico

En la mayoría de los lenguajes, una query incorrecta se descubre en producción:

```python
# Python — error en runtime, no en desarrollo
cursor.execute("SELECT nombre, correo FROM usarios WHERE id = %s", (id,))
#                                         ^^^^^^^^ typo en "usuarios"
# ProgrammingError: relation "usarios" does not exist
```

SQLx resuelve esto conectándose a la base de datos durante la compilación y
verificando que cada query es válida:

```text
FLUJO DE COMPILACIÓN CON SQLX

Tu código                  sqlx-cli / macros          Base de datos
───────────                ──────────────────         ─────────────
sqlx::query_as!(       ──►  parsea la SQL         ──►  PREPARE statement
  "SELECT id, url          verifica columnas           verifica tipos
   FROM urls               mapea tipos Rust ↔ PG
   WHERE code = $1", &c)
                           ◄── error de tipos en
                               tiempo de compilación
                               si algo no coincide
```

Si cambias el nombre de una columna en la migración sin actualizar las queries, el
proyecto **no compila**. Este feedback instantáneo elimina una clase entera de bugs.

---

## Instalación y configuración

### `sqlx-cli`

```bash
# Instalar la CLI de SQLx (solo las features de postgres para no compilar todo)
cargo install sqlx-cli --no-default-features --features postgres,rustls

# Verificar
sqlx --version   # sqlx-cli 0.7.x
```

### Dependencias en `Cargo.toml`

```toml
[dependencies]
sqlx = { version = "0.8", features = [
    "runtime-tokio-rustls",   # runtime async + TLS sin OpenSSL
    "postgres",               # driver de PostgreSQL
    "macros",                 # query!, query_as!, query_scalar!
    "migrate",                # sqlx::migrate!
    "uuid",                   # soporte para UUID en columnas PG
    "chrono",                 # DateTime<Utc> ↔ TIMESTAMPTZ
] }
```

### Variable de entorno

SQLx necesita `DATABASE_URL` al compilar para verificar las queries:

```bash
# .env (no subir al repositorio)
DATABASE_URL=postgres://usuario:clave@localhost:5432/url_shortener
```

Con `dotenv` o `direnv` se carga automáticamente. Para CI sin DB usamos el modo
offline (más abajo).

### Levantar PostgreSQL para desarrollo

```yaml
# docker-compose.yml — solo para desarrollo local
services:
  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: usuario
      POSTGRES_PASSWORD: clave
      POSTGRES_DB: url_shortener
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
volumes:
  pgdata:
```

```bash
docker compose up -d db
```

---

## Migraciones con `sqlx-cli`

Las migraciones son archivos SQL versionados que llevan la base de datos de un estado
al siguiente. SQLx los aplica en orden y registra cuáles ya fueron ejecutados en la
tabla `_sqlx_migrations`.

```bash
# Crear la base de datos (si no existe)
sqlx database create

# Crear una nueva migración (genera archivo con timestamp)
sqlx migrate add crear_tabla_urls
# Creates: migrations/20240901120000_crear_tabla_urls.sql

# Aplicar todas las migraciones pendientes
sqlx migrate run

# Ver estado
sqlx migrate info

# Revertir la última (solo si tiene archivo .down.sql)
sqlx migrate revert
```

### La migración de urls

`migrations/20240901120000_crear_tabla_urls.sql`:

```sql
CREATE TABLE urls (
    code        VARCHAR(10)  PRIMARY KEY,
    target_url  TEXT         NOT NULL,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ,
    clicks      BIGINT       NOT NULL DEFAULT 0
);

-- Índice parcial: solo filas con expires_at definido
CREATE INDEX idx_urls_expires ON urls (expires_at)
    WHERE expires_at IS NOT NULL;
```

### Aplicar migraciones desde el código

En lugar de ejecutar `sqlx migrate run` en producción, puedes aplicar migraciones
al arrancar el servidor:

```rust
use sqlx::PgPool;

async fn conectar(url: &str) -> PgPool {
    let pool = PgPool::connect(url).await.expect("no se pudo conectar a la BD");

    // Aplica todas las migraciones en migrations/ al arrancar
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("error al aplicar migraciones");

    pool
}
```

---

## El pool de conexiones

`PgPool` gestiona un conjunto de conexiones reutilizables. Conectarse y desconectarse
de PostgreSQL es costoso; el pool amortiza ese coste:

```text
PgPool (Arc<PoolInner>)
┌─────────────────────────────────────────────┐
│  Conexiones disponibles:  [C1] [C2] [C3]   │
│  Conexiones en uso:       [C4] [C5]         │
│  min_connections: 2                         │
│  max_connections: 10                        │
│  acquire_timeout: 30s                       │
└─────────────────────────────────────────────┘
    │ .acquire().await
    ▼
    PoolConnection<Postgres>  (devuelve al pool al hacer drop)
```

```rust
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

async fn crear_pool(url: &str) -> PgPool {
    PgPoolOptions::new()
        .min_connections(2)           // mantiene mínimo 2 conexiones vivas
        .max_connections(20)          // nunca más de 20 simultáneas
        .acquire_timeout(Duration::from_secs(30))  // error si tarda más
        .connect(url)
        .await
        .unwrap()
}
```

El pool implementa `Clone` de forma barata (solo clona el `Arc` interno), así que se
puede distribuir libremente como estado de Axum.

---

## Las tres macros de query

### `query!` — query sin mapeo a struct

Devuelve un tipo anónimo con campos accesibles directamente. Útil para queries simples
o cuando no quieres crear un struct solo para una query:

```rust
use sqlx::PgPool;

async fn contar_urls(pool: &PgPool) -> i64 {
    let fila = sqlx::query!(
        "SELECT COUNT(*) AS total FROM urls"
    )
    .fetch_one(pool)
    .await
    .unwrap();

    fila.total.unwrap_or(0)   // COUNT devuelve Option<i64> en sqlx
}

async fn insertar_url(pool: &PgPool, code: &str, url: &str) {
    sqlx::query!(
        "INSERT INTO urls (code, target_url) VALUES ($1, $2)",
        code,
        url
    )
    .execute(pool)
    .await
    .unwrap();
}
```

### `query_as!` — mapeo directo a struct

Es la más usada. Mapea cada fila a un struct que **no** necesita ser `FromRow` — la
macro genera el mapeo basándose en los nombres de columnas:

```rust
#[derive(Debug)]
struct FilaUrl {
    code:       String,
    target_url: String,
    clicks:     i64,
}

async fn buscar_url(pool: &PgPool, code: &str) -> Option<FilaUrl> {
    sqlx::query_as!(
        FilaUrl,
        "SELECT code, target_url, clicks FROM urls WHERE code = $1",
        code
    )
    .fetch_optional(pool)   // None si no existe, Err si falla la BD
    .await
    .unwrap()
}

// Método de fetch:
// .fetch_one(pool)         → Err si 0 filas
// .fetch_optional(pool)    → Ok(None) si 0 filas
// .fetch_all(pool)         → Vec<T>
// .fetch(pool)             → Stream<Item=Result<T>>
```

**Los tipos de Rust deben coincidir con los tipos de PostgreSQL.** SQLx valida esto en
compilación:

| PostgreSQL | Rust |
| :--- | :--- |
| `TEXT`, `VARCHAR` | `String` |
| `BIGINT`, `INT8` | `i64` |
| `INTEGER`, `INT4` | `i32` |
| `BOOLEAN` | `bool` |
| `REAL`, `FLOAT4` | `f32` |
| `DOUBLE PRECISION`, `FLOAT8` | `f64` |
| `TIMESTAMPTZ` | `chrono::DateTime<Utc>` (con feature `chrono`) |
| `UUID` | `uuid::Uuid` (con feature `uuid`) |
| `JSONB`, `JSON` | `serde_json::Value` (con feature `json`) |
| Columna nullable (`NULL`) | `Option<T>` |

### `query_scalar!` — una sola columna

Para queries que devuelven exactamente una columna:

```rust
async fn obtener_clicks(pool: &PgPool, code: &str) -> Option<i64> {
    sqlx::query_scalar!(
        "SELECT clicks FROM urls WHERE code = $1",
        code
    )
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn existe_code(pool: &PgPool, code: &str) -> bool {
    sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM urls WHERE code = $1)",
        code
    )
    .fetch_one(pool)
    .await
    .unwrap()
    .unwrap_or(false)
}
```

### `sqlx::FromRow`: el derive para query_as

Cuando necesitas reutilizar el mapeo en múltiples queries, `#[derive(FromRow)]` es más
ergonómico:

```rust
use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, FromRow)]
struct FilaUrl {
    pub code:       String,
    pub target_url: String,
    #[sqlx(default)]           // usa Default si la columna no está en el SELECT
    pub expires_at: Option<DateTime<Utc>>,
    pub clicks:     i64,
}

// Con FromRow se puede usar query_as sin la macro (útil para queries dinámicas):
async fn buscar_con_from_row(pool: &PgPool, code: &str) -> Option<FilaUrl> {
    sqlx::query_as::<_, FilaUrl>(
        "SELECT code, target_url, expires_at, clicks FROM urls WHERE code = $1"
    )
    .bind(code)
    .fetch_optional(pool)
    .await
    .unwrap()
}
```

---

## Transacciones

SQLx expone las transacciones como un tipo que hace rollback automático al hacer
`drop` si no se ha llamado a `.commit()`:

```rust
use sqlx::{PgPool, Postgres, Transaction};

async fn transferir_clicks(
    pool: &PgPool,
    desde: &str,
    hacia: &str,
    cantidad: i64,
) -> Result<(), sqlx::Error> {
    let mut tx: Transaction<'_, Postgres> = pool.begin().await?;

    // Decrementar en origen
    let afectadas = sqlx::query!(
        "UPDATE urls SET clicks = clicks - $1 WHERE code = $2 AND clicks >= $1",
        cantidad, desde
    )
    .execute(&mut *tx)   // ← pasar &mut *tx, no el pool
    .await?
    .rows_affected();

    if afectadas == 0 {
        // tx.rollback() es implícito al hacer drop
        return Err(sqlx::Error::RowNotFound);
    }

    // Incrementar en destino
    sqlx::query!(
        "UPDATE urls SET clicks = clicks + $1 WHERE code = $2",
        cantidad, hacia
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;   // sin esto → rollback automático
    Ok(())
}
```

### Savepoints

```rust
async fn con_savepoint(tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    tx.execute("SAVEPOINT punto1").await?;

    let resultado = operacion_que_puede_fallar(tx).await;

    if resultado.is_err() {
        tx.execute("ROLLBACK TO SAVEPOINT punto1").await?;
    } else {
        tx.execute("RELEASE SAVEPOINT punto1").await?;
    }

    Ok(())
}

async fn operacion_que_puede_fallar(
    _tx: &mut Transaction<'_, Postgres>,
) -> Result<(), sqlx::Error> {
    Ok(())
}
```

---

## Modo offline: CI sin `DATABASE_URL`

El problema de CI: las macros de SQLx se conectan a la BD en compilación, pero los
pipelines de CI generalmente no tienen acceso a una base de datos durante la fase de
compilación.

### Solución: `sqlx prepare`

```bash
# Ejecutar en local (con DATABASE_URL activa):
cargo sqlx prepare --workspace

# Genera el directorio .sqlx/ con los metadatos de todas las queries
# (o sqlx-data.json en versiones antiguas)

# IMPORTANTE: commitear .sqlx/ al repositorio
git add .sqlx/
git commit -m "actualizar metadatos sqlx"
```

En CI, SQLx detecta automáticamente `.sqlx/` y usa los metadatos cached en lugar de
conectarse a la BD:

```yaml
# .github/workflows/ci.yml
- name: Check SQLx offline data
  run: cargo sqlx prepare --check --workspace
  # Falla si las queries en código no coinciden con .sqlx/
  # (detecta que alguien modificó SQL sin actualizar los metadatos)
```

```text
FLUJO OFFLINE

Desarrollo local              CI / Compilación offline
──────────────────            ────────────────────────
DATABASE_URL presente    →    .sqlx/*.json cacheado
query! verifica en BD         query! lee de .sqlx/
cargo sqlx prepare       →    sin conexión a BD
genera .sqlx/                 cargo build --release funciona
git commit .sqlx/
```

---

## Serde avanzado: control total de serialización

`serde` es la biblioteca más usada de Rust, pero la mayoría de los programadores solo
usan `#[derive(Serialize, Deserialize)]`. Los atributos avanzados te dan control
total sobre el formato sin escribir un `Serializer` a mano.

### `#[serde(rename = "nombre")]`: renombrar campos

Cuando el formato de la API difiere de las convenciones de Rust:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct EntradaUrl {
    #[serde(rename = "short_code")]  // JSON: "short_code", Rust: code
    pub code: String,

    #[serde(rename = "originalUrl")] // JSON camelCase, Rust snake_case
    pub target_url: String,
}

// También se puede aplicar a nivel de struct con rename_all:
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]  // todos los campos en camelCase
struct SolicitudApi {
    pub target_url:  String,   // → "targetUrl"
    pub expires_at:  Option<u64>,  // → "expiresAt"
    pub custom_code: Option<String>, // → "customCode"
}
```

### `#[serde(skip_serializing_if = "...")]`: campos opcionales

Para no emitir `null` en JSON cuando un campo es `None` o una colección está vacía:

```rust
#[derive(Serialize, Deserialize)]
struct RespuestaUrl {
    pub code:       String,
    pub short_url:  String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,    // no aparece en JSON si es None

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub etiquetas:  Vec<String>,    // no aparece si está vacío

    #[serde(skip_serializing_if = "es_cero")]
    pub clics: u64,
}

fn es_cero(n: &u64) -> bool { *n == 0 }
```

### `#[serde(default)]` y `#[serde(default = "fn")]`: valores por defecto

Cuando un campo puede estar ausente en el JSON de entrada:

```rust
fn pagina_default() -> u32 { 1 }
fn limite_default() -> u32 { 20 }

#[derive(Deserialize)]
struct Paginacion {
    #[serde(default = "pagina_default")]  // valor 1 si no está en JSON
    pub pagina: u32,

    #[serde(default = "limite_default")] // valor 20 si no está en JSON
    pub limite: u32,

    #[serde(default)]   // usa Default::default() (false para bool)
    pub incluir_expiradas: bool,
}

// JSON: {} → Paginacion { pagina: 1, limite: 20, incluir_expiradas: false }
// JSON: {"pagina": 3} → Paginacion { pagina: 3, limite: 20, incluir_expiradas: false }
```

### `#[serde(flatten)]`: aplanar structs anidados

Mueve los campos de un struct anidado al nivel superior del JSON:

```rust
#[derive(Serialize, Deserialize)]
struct Metadatos {
    pub creada_en:  u64,
    pub creada_por: String,
}

#[derive(Serialize, Deserialize)]
struct EntradaUrl {
    pub code:       String,
    pub target_url: String,

    #[serde(flatten)]
    pub meta: Metadatos,  // sus campos aparecen al mismo nivel
}

// JSON:
// {
//   "code": "abc123",
//   "target_url": "https://...",
//   "creada_en": 1700000000,      ← aplanado desde Metadatos
//   "creada_por": "usuario"       ← aplanado desde Metadatos
// }
```

### `#[serde(skip)]`: campos invisibles para Serde

```rust
#[derive(Serialize, Deserialize)]
struct CacheEntrada {
    pub code:       String,
    pub target_url: String,

    #[serde(skip)]
    pub cache_hit: bool,   // solo para métricas internas, nunca en JSON
}
```

### `#[serde(with = "módulo")]`: serialización personalizada

El atributo más poderoso: delega la serialización a un módulo que expone
`serialize` y `deserialize`. Perfecto para tipos que no implementan Serde:

```rust
use serde::{Deserialize, Serialize};

mod unix_timestamp {
    use serde::{Deserializer, Serializer};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub fn serialize<S: Serializer>(time: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let secs = time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        s.serialize_u64(secs)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(UNIX_EPOCH + Duration::from_secs(secs))
    }
}

#[derive(Serialize, Deserialize)]
struct EntradaUrl {
    pub code: String,

    #[serde(with = "unix_timestamp")]
    pub creada_en: std::time::SystemTime,   // ↔ número entero en JSON
}
```

Muchas bibliotecas ya proveen módulos `with` listos para usar:

```rust
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize)]
struct Evento {
    // chrono provee el módulo serde::ts_seconds
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,          // ↔ Unix timestamp (segundos)

    // O ts_milliseconds para mayor precisión
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp_ms: DateTime<Utc>,       // ↔ Unix timestamp (milisegundos)
}
```

### `#[serde(tag = "tipo")]` y `#[serde(untagged)]`: enums en JSON

Tres formas de serializar un enum:

```rust
// 1. Externamente etiquetado (por defecto)
#[derive(Serialize, Deserialize)]
enum EventoExterno {
    Clic { url_id: String },
    Creacion { url_id: String, usuario: String },
}
// JSON: {"Clic": {"url_id": "abc"}}

// 2. Internamente etiquetado (más idiomático para APIs)
#[derive(Serialize, Deserialize)]
#[serde(tag = "tipo")]
enum EventoInterno {
    Clic { url_id: String },
    Creacion { url_id: String, usuario: String },
}
// JSON: {"tipo": "Clic", "url_id": "abc"}

// 3. Sin etiqueta — infiere por estructura (frágil, usar con cuidado)
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Identificador {
    PorCodigo(String),
    PorId(u64),
}
// JSON: "abc123" o 42
```

### Resumen de atributos de Serde

| Atributo | Ámbito | Efecto |
| :--- | :--- | :--- |
| `#[serde(rename = "x")]` | Campo | Cambia el nombre en JSON |
| `#[serde(rename_all = "camelCase")]` | Struct/Enum | Renombra todos los campos |
| `#[serde(skip_serializing_if = "fn")]` | Campo | Omite si la función devuelve `true` |
| `#[serde(default)]` | Campo | Usa `Default` si falta en deserialización |
| `#[serde(default = "fn")]` | Campo | Llama a `fn()` si falta en deserialización |
| `#[serde(flatten)]` | Campo | Aplana campos del struct anidado |
| `#[serde(skip)]` | Campo | Nunca serializa/deserializa |
| `#[serde(with = "mod")]` | Campo | Serialización totalmente personalizada |
| `#[serde(tag = "campo")]` | Enum | Etiqueta interna para variantes |
| `#[serde(untagged)]` | Enum | Infiere variante por estructura |
| `#[serde(deny_unknown_fields)]` | Struct | Error si JSON tiene campos extra |
| `#[serde(from = "T")]` | Struct | Deserializa convirtiendo desde `T` |

---

## Proyecto: Url Shortener v2 (PostgreSQL + SQLx + Serde)

Refactorizamos el proyecto de la Semana 10 para reemplazar `AlmacenMemoria` con
`AlmacenPostgres`, manteniendo el trait `AlmacenUrls` intacto (Axum ni siquiera nota
el cambio).

### Nuevas dependencias

```toml
[dependencies]
# ... (las de la Semana 10 se mantienen)
sqlx = { version = "0.8", features = [
    "runtime-tokio-rustls",
    "postgres",
    "macros",
    "migrate",
    "uuid",
    "chrono",
] }
chrono = { version = "0.4", features = ["serde"] }
uuid   = { version = "1",   features = ["v4", "serde"] }

[dev-dependencies]
reqwest       = { version = "0.12", features = ["json"] }
testcontainers          = "0.23"
testcontainers-modules  = { version = "0.11", features = ["postgres"] }
```

### `migrations/20240901000001_crear_tabla_urls.sql`

```sql
CREATE TABLE IF NOT EXISTS urls (
    code        VARCHAR(10)  PRIMARY KEY,
    target_url  TEXT         NOT NULL,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ,
    clicks      BIGINT       NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_urls_expires
    ON urls (expires_at)
    WHERE expires_at IS NOT NULL;
```

### `src/models.rs` — con Serde avanzado

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]   // le dice a SQLx que es solo un String
pub struct CodigoCorto(pub String);

impl CodigoCorto {
    pub fn generar() -> Self {
        let id = uuid::Uuid::new_v4();
        let bytes = id.as_bytes();
        let mut s = String::with_capacity(8);
        for &b in &bytes[..6] {
            s.push(match b % 62 {
                n @ 0..=9  => (b'0' + n) as char,
                n @ 10..=35 => (b'a' + n - 10) as char,
                n           => (b'A' + n - 36) as char,
            });
        }
        let nano = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        s.push_str(&format!("{:02}", nano % 62));
        CodigoCorto(s)
    }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for CodigoCorto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { self.0.fmt(f) }
}

/// Fila de la tabla urls — usada con sqlx::FromRow
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FilaUrl {
    pub code:       String,
    pub target_url: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub clicks:     i64,
}

/// Modelo de dominio
#[derive(Debug, Clone)]
pub struct EntradaUrl {
    pub codigo:    CodigoCorto,
    pub url_orig:  String,
    pub creada_en: DateTime<Utc>,
    pub expira_en: Option<DateTime<Utc>>,
    pub clics:     u64,
}

impl From<FilaUrl> for EntradaUrl {
    fn from(f: FilaUrl) -> Self {
        Self {
            codigo:    CodigoCorto(f.code),
            url_orig:  f.target_url,
            creada_en: f.created_at,
            expira_en: f.expires_at,
            clics:     f.clicks as u64,
        }
    }
}

/// Cuerpo de la petición POST /shorten
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]  // acepta "targetUrl" además de "url"
pub struct SolicitudAcortar {
    pub url: String,

    // El cliente puede pedir expiración opcional
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expira_en_segundos: Option<u64>,
}

/// Respuesta de POST /shorten — Serde completo para API pública
#[derive(Debug, Serialize)]
pub struct RespuestaAcortar {
    pub codigo:    CodigoCorto,
    pub url_corta: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expira_en: Option<i64>,  // Unix timestamp, None → no expira
}

/// Estadísticas devueltas por GET /:codigo/stats
#[derive(Debug, Serialize)]
pub struct EstadisticasUrl {
    pub codigo:    CodigoCorto,
    pub url_orig:  String,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub creada_en: DateTime<Utc>,

    #[serde(
        with = "chrono::serde::ts_seconds_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub expira_en: Option<DateTime<Utc>>,

    pub clics: u64,
}

impl From<EntradaUrl> for EstadisticasUrl {
    fn from(e: EntradaUrl) -> Self {
        Self {
            codigo:    e.codigo,
            url_orig:  e.url_orig,
            creada_en: e.creada_en,
            expira_en: e.expira_en,
            clics:     e.clics,
        }
    }
}
```

### `src/almacen.rs` — añadir `AlmacenPostgres`

```rust
use sqlx::PgPool;
use crate::{
    error::ErrorApp,
    models::{CodigoCorto, EntradaUrl, FilaUrl},
};

// (AlmacenUrls trait y AlmacenMemoria de la Semana 10 permanecen sin cambios)
pub trait AlmacenUrls: Send + Sync + 'static {
    fn guardar(&self, entrada: EntradaUrl) -> impl std::future::Future<Output = Result<(), ErrorApp>> + Send;
    fn buscar(&self, codigo: &CodigoCorto) -> impl std::future::Future<Output = Option<EntradaUrl>> + Send;
    fn incrementar_clics(&self, codigo: &CodigoCorto) -> impl std::future::Future<Output = Option<u64>> + Send;
    fn listar_todo(&self) -> impl std::future::Future<Output = Vec<EntradaUrl>> + Send;
}

/// Implementación con PostgreSQL + SQLx
#[derive(Clone)]
pub struct AlmacenPostgres {
    pool: PgPool,
}

impl AlmacenPostgres {
    pub fn nuevo(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl AlmacenUrls for AlmacenPostgres {
    async fn guardar(&self, entrada: EntradaUrl) -> Result<(), ErrorApp> {
        sqlx::query!(
            r#"
            INSERT INTO urls (code, target_url, created_at, expires_at, clicks)
            VALUES ($1, $2, $3, $4, 0)
            ON CONFLICT (code) DO NOTHING
            "#,
            entrada.codigo.as_str(),
            entrada.url_orig,
            entrada.creada_en,
            entrada.expira_en,
        )
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorApp::Almacenamiento)?;
        Ok(())
    }

    async fn buscar(&self, codigo: &CodigoCorto) -> Option<EntradaUrl> {
        sqlx::query_as!(
            FilaUrl,
            "SELECT code, target_url, created_at, expires_at, clicks
             FROM urls WHERE code = $1",
            codigo.as_str()
        )
        .fetch_optional(&self.pool)
        .await
        .ok()?
        .map(EntradaUrl::from)
    }

    async fn incrementar_clics(&self, codigo: &CodigoCorto) -> Option<u64> {
        sqlx::query_scalar!(
            "UPDATE urls SET clicks = clicks + 1 WHERE code = $1 RETURNING clicks",
            codigo.as_str()
        )
        .fetch_optional(&self.pool)
        .await
        .ok()?
        .map(|n| n as u64)
    }

    async fn listar_todo(&self) -> Vec<EntradaUrl> {
        sqlx::query_as!(
            FilaUrl,
            "SELECT code, target_url, created_at, expires_at, clicks
             FROM urls ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(EntradaUrl::from)
        .collect()
    }
}
```

### `src/handlers.rs` — handler con expiración

```rust
use axum::{extract::{Path, State}, http::StatusCode, response::{IntoResponse, Redirect}, Json};
use chrono::{Duration, Utc};
use std::sync::Arc;

use crate::{
    almacen::AlmacenUrls,
    error::ErrorApp,
    estado::EstadoApp,
    models::{CodigoCorto, EntradaUrl, EstadisticasUrl, RespuestaAcortar, SolicitudAcortar},
};

pub async fn chequeo_salud() -> &'static str { "OK" }

pub async fn acortar_url<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Json(cuerpo): Json<SolicitudAcortar>,
) -> Result<(StatusCode, Json<RespuestaAcortar>), ErrorApp> {
    if !cuerpo.url.starts_with("http://") && !cuerpo.url.starts_with("https://") {
        return Err(ErrorApp::UrlInvalida("debe empezar con http:// o https://".into()));
    }

    let expira_en = cuerpo.expira_en_segundos
        .map(|s| Utc::now() + Duration::seconds(s as i64));

    let mut entrada = EntradaUrl::nueva(cuerpo.url);
    entrada.expira_en = expira_en;

    let codigo = entrada.codigo.clone();
    estado.almacen.guardar(entrada).await.map_err(|_| ErrorApp::Almacenamiento)?;

    let url_corta = format!("{}/{}", estado.base_url, codigo);
    let expira_ts = expira_en.map(|dt| dt.timestamp());

    Ok((StatusCode::CREATED, Json(RespuestaAcortar { codigo, url_corta, expira_en: expira_ts })))
}

pub async fn redirigir<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Path(codigo_str): Path<String>,
) -> Result<Redirect, ErrorApp> {
    let codigo = CodigoCorto(codigo_str);
    let entrada = estado.almacen.buscar(&codigo).await.ok_or(ErrorApp::NoEncontrado)?;

    // Verificar expiración
    if let Some(exp) = entrada.expira_en {
        if Utc::now() > exp {
            return Err(ErrorApp::NoEncontrado);
        }
    }

    let almacen = estado.almacen.clone();
    let cod     = codigo.clone();
    tokio::spawn(async move { almacen.incrementar_clics(&cod).await; });

    Ok(Redirect::permanent(&entrada.url_orig))
}

pub async fn estadisticas<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Path(codigo_str): Path<String>,
) -> Result<Json<EstadisticasUrl>, ErrorApp> {
    let codigo  = CodigoCorto(codigo_str);
    let entrada = estado.almacen.buscar(&codigo).await.ok_or(ErrorApp::NoEncontrado)?;
    Ok(Json(entrada.into()))
}

pub async fn listar_urls<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
) -> Json<Vec<EstadisticasUrl>> {
    let lista = estado.almacen.listar_todo().await
        .into_iter().map(EstadisticasUrl::from).collect();
    Json(lista)
}
```

### `src/main.rs` — conectando todo

```rust
mod almacen;
mod error;
mod estado;
mod handlers;
mod models;

use almacen::AlmacenPostgres;
use estado::EstadoApp;
use handlers::*;

use axum::{
    middleware::{self, Next},
    extract::Request,
    response::Response,
    routing::{get, post},
    Router,
};
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

async fn telemetria(req: Request, next: Next) -> Response {
    let inicio = Instant::now();
    let metodo = req.method().clone();
    let uri    = req.uri().path().to_owned();
    let resp   = next.run(req).await;
    tracing::info!(
        metodo = %metodo, ruta = %uri,
        estado = resp.status().as_u16(),
        ms     = inicio.elapsed().as_millis(), "petición"
    );
    resp
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL debe estar definida");
    let base_url = std::env::var("BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3000".into());

    // Crear pool y aplicar migraciones
    let pool = sqlx::PgPool::connect(&database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let almacen = AlmacenPostgres::nuevo(pool.clone());
    let estado  = EstadoApp::nuevo(almacen, base_url);

    let app = Router::new()
        .route("/health",        get(chequeo_salud::<AlmacenPostgres>))
        .route("/shorten",       post(acortar_url::<AlmacenPostgres>))
        .route("/urls",          get(listar_urls::<AlmacenPostgres>))
        .route("/:codigo",       get(redirigir::<AlmacenPostgres>))
        .route("/:codigo/stats", get(estadisticas::<AlmacenPostgres>))
        .layer(middleware::from_fn(telemetria))
        .layer(CorsLayer::permissive())
        .with_state(estado);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("servidor en {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
```

---

## Tests de integración con `testcontainers`

`testcontainers` levanta un contenedor Docker real durante los tests y lo destruye al
terminar. Los tests obtienen una base de datos real sin fixtures compartidos ni estado
entre tests.

### `tests/api_test.rs`

```rust
use testcontainers::{runners::AsyncRunner, ImageExt};
use testcontainers_modules::postgres::Postgres;
use sqlx::PgPool;

// Fixture: crea un PgPool apuntando a un PG efímero
async fn pool_para_test() -> (PgPool, impl Drop) {
    let contenedor = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("Docker disponible");

    let puerto = contenedor.get_host_port_ipv4(5432).await.unwrap();
    let url    = format!(
        "postgres://postgres:postgres@127.0.0.1:{}/postgres",
        puerto
    );

    let pool = PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    (pool, contenedor)   // contenedor vive mientras el test corre
}

#[tokio::test]
async fn ciclo_completo_crear_y_buscar() {
    let (pool, _contenedor) = pool_para_test().await;

    // Importar nuestros tipos de producción
    use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
    use url_shortener::models::EntradaUrl;

    let almacen = AlmacenPostgres::nuevo(pool);

    let entrada  = EntradaUrl::nueva("https://www.rust-lang.org".into());
    let codigo   = entrada.codigo.clone();

    // Guardar
    almacen.guardar(entrada).await.expect("guardar falló");

    // Buscar
    let encontrada = almacen.buscar(&codigo).await.expect("no encontrada");
    assert_eq!(encontrada.url_orig, "https://www.rust-lang.org");
    assert_eq!(encontrada.clics, 0);
}

#[tokio::test]
async fn incrementar_clics_atomico() {
    let (pool, _contenedor) = pool_para_test().await;

    use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
    use url_shortener::models::EntradaUrl;

    let almacen = AlmacenPostgres::nuevo(pool);
    let entrada  = EntradaUrl::nueva("https://example.com".into());
    let codigo   = entrada.codigo.clone();
    almacen.guardar(entrada).await.unwrap();

    // Incrementos concurrentes
    let a = almacen.clone();
    let b = almacen.clone();
    let c = almacen.clone();
    let cod_a = codigo.clone();
    let cod_b = codigo.clone();
    let cod_c = codigo.clone();

    tokio::join!(
        async move { a.incrementar_clics(&cod_a).await },
        async move { b.incrementar_clics(&cod_b).await },
        async move { c.incrementar_clics(&cod_c).await },
    );

    let stats = almacen.buscar(&codigo).await.unwrap();
    assert_eq!(stats.clics, 3);   // UPDATE atómico de PG garantiza 3
}

#[tokio::test]
async fn url_inexistente_devuelve_none() {
    let (pool, _contenedor) = pool_para_test().await;

    use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
    use url_shortener::models::CodigoCorto;

    let almacen = AlmacenPostgres::nuevo(pool);
    let result  = almacen.buscar(&CodigoCorto("NOEXISTE".into())).await;
    assert!(result.is_none());
}
```

Ejecutar los tests:

```bash
# Requiere Docker corriendo
cargo test --test api_test -- --test-threads=4
```

### `sqlx::test`: alternativa sin Docker

Para tests de capa de datos sin levantar contenedores, SQLx provee la macro
`#[sqlx::test]`. Crea una base de datos temporal, aplica migraciones, ejecuta el test
en una transacción y hace rollback automático:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn test_con_sqlx_test(pool: PgPool) {
    use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
    use url_shortener::models::EntradaUrl;

    let almacen = AlmacenPostgres::nuevo(pool);
    let entrada  = EntradaUrl::nueva("https://ferris.rs".into());
    let codigo   = entrada.codigo.clone();

    almacen.guardar(entrada).await.unwrap();

    let encontrada = almacen.buscar(&codigo).await.unwrap();
    assert_eq!(encontrada.url_orig, "https://ferris.rs");
    // Al salir, la transacción hace rollback → BD limpia para el siguiente test
}
```

**Requires**: `DATABASE_URL` en el entorno (o en `.env`). Es más rápido que
testcontainers (no levanta Docker) pero necesita una BD PostgreSQL disponible.

---

## Flujo completo de desarrollo con SQLx

```text
CICLO DE DESARROLLO

1. Diseñar tabla
   ↓
2. sqlx migrate add <nombre>
   Editar el archivo .sql generado
   ↓
3. sqlx migrate run
   (aplica la migración a la BD local)
   ↓
4. Escribir código Rust con query!() / query_as!()
   El compilador verifica las queries contra la BD
   ↓
5. cargo sqlx prepare
   Genera .sqlx/ con metadatos para modo offline
   git add .sqlx/ && git commit
   ↓
6. CI: cargo sqlx prepare --check
   Verifica que .sqlx/ está sincronizado con el código
   (sin necesitar DATABASE_URL en CI)
   ↓
7. Despliegue: sqlx migrate run (o migrate!() al arrancar)
```

---

## ✅ Checklist de la Semana 11

- [ ] Configuro `DATABASE_URL` y levanto PostgreSQL con `docker compose up -d db`.
- [ ] Creo migraciones con `sqlx migrate add`, las edito y las aplico con
  `sqlx migrate run`.
- [ ] Elijo la macro correcta: `query!` (ad-hoc), `query_as!` (struct), `query_scalar!`
  (una columna).
- [ ] Los tipos de Rust en `query_as!` coinciden con los tipos de PostgreSQL
  (incluyendo `Option<T>` para columnas nullable).
- [ ] Uso `PgPoolOptions` para configurar `min_connections` y `max_connections`.
- [ ] El trait `AlmacenUrls` no cambió — solo se añadió `AlmacenPostgres`. Axum no
  sabe qué implementación está detrás.
- [ ] Ejecuto `cargo sqlx prepare` y commiteo `.sqlx/` para que CI compile sin BD.
- [ ] Implemento los 9 atributos de Serde: `rename`, `rename_all`, `skip_serializing_if`,
  `default`, `flatten`, `skip`, `with`, `tag`, `deny_unknown_fields`.
- [ ] Los tests de integración con `testcontainers` pasan y verifican la atomicidad
  de `incrementar_clics`.
- [ ] Opcional: uso `#[sqlx::test]` para tests rápidos sin Docker en macros de BD.

> **Siguiente paso:** Semana 12 — [Observabilidad, Docker y CI/CD: production-ready](section_04.md).
