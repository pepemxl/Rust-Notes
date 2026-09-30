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

!!! quote "Filosofía de la Semana 11"

    *La diferencia entre un ORM y SQLx es que SQLx te
    deja escribir SQL real y te ayuda a no equivocarte en él. Escribes SQL, el compilador
    lo valida, Rust hace el mapeo. Sin magia, sin rendimiento oculto.*

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
use sqlx::PgPool;

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
use sqlx::PgPool;

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
use sqlx::{FromRow, PgPool};
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
use sqlx::{Executor, Postgres, Transaction};

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
use serde::{Deserialize, Serialize};

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
use serde::Deserialize;

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
use serde::{Deserialize, Serialize};

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
use serde::{Deserialize, Serialize};

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
    use serde::{Deserialize, Deserializer, Serializer}; // Deserialize: para u64::deserialize
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
use serde::{Deserialize, Serialize};

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
use serde::{Deserialize, Serialize};

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

Refactorizamos el proyecto de la Semana 10 para agregar `AlmacenPostgres` junto a
`AlmacenMemoria`. El trait `AlmacenUrls` cambia en un punto: sus métodos pasan a ser
**async**, porque hablar con la base de datos es I/O. Los handlers añaden `.await` en
cada llamada, pero siguen siendo genéricos sobre `S: AlmacenUrls`: el router elige la
implementación y los handlers no saben cuál es.

El código completo está en
[`src/chapter_03/url_shortener_v2`](https://github.com/pepemxl/Rust-Notes/tree/master/src/chapter_03/url_shortener_v2);
lo que se muestra aquí se incluye directamente de esos archivos, y el CI compila el
proyecto y ejecuta sus tests contra PostgreSQL real.

### Nuevas dependencias

```toml
[dependencies]
# ... (las de la Semana 10 se mantienen)
sqlx = { version = "0.8", default-features = false, features = [
    "runtime-tokio",
    "tls-rustls",
    "postgres",
    "macros",
    "migrate",
    "uuid",
    "chrono",
] }
chrono = { version = "0.4", features = ["serde"] }
uuid   = { version = "1",   features = ["v4", "serde"] }

[dev-dependencies]
reqwest       = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
testcontainers          = "0.23"
testcontainers-modules  = { version = "0.11", features = ["postgres"] }
```

### `migrations/20240901000001_crear_tabla_urls.sql`

```sql
--8<-- "src/chapter_03/url_shortener_v2/migrations/20240901000001_crear_tabla_urls.sql"
```

### `src/models.rs` — con Serde avanzado

```rust
--8<-- "src/chapter_03/url_shortener_v2/src/models.rs"
```

### `src/almacen.rs` — trait async, `AlmacenMemoria` y `AlmacenPostgres`

```rust
--8<-- "src/chapter_03/url_shortener_v2/src/almacen.rs"
```

### `src/handlers.rs` — handler con expiración

```rust
--8<-- "src/chapter_03/url_shortener_v2/src/handlers.rs"
```

### `src/lib.rs` — los módulos, accesibles para los tests

Los tests de integración de `tests/` son crates aparte: solo pueden usar lo que el
proyecto exporta como **librería**. Por eso los módulos se declaran en `lib.rs` (con
`[lib] name = "url_shortener"` en `Cargo.toml`) y `main.rs` los importa desde ahí:

```rust
--8<-- "src/chapter_03/url_shortener_v2/src/lib.rs"
```

### `src/main.rs` — conectando todo

```rust
--8<-- "src/chapter_03/url_shortener_v2/src/main.rs"
```

---

## Tests de integración con `testcontainers`

`testcontainers` levanta un contenedor Docker real durante los tests y lo destruye al
terminar. Los tests obtienen una base de datos real sin fixtures compartidos ni estado
entre tests.

### `tests/api_test.rs`

```rust
--8<-- "src/chapter_03/url_shortener_v2/tests/api_test.rs"
```

Ejecutar los tests:

```bash
# Requiere Docker corriendo
cargo test --test api_test -- --test-threads=4
```

### `sqlx::test`: alternativa sin Docker

Para tests de capa de datos sin levantar contenedores, SQLx provee la macro
`#[sqlx::test]`. Por cada test **crea una base de datos nueva** en tu PostgreSQL, le
aplica las migraciones y la elimina al terminar, así los tests no comparten estado
(`tests/sqlx_test.rs`):

```rust
--8<-- "src/chapter_03/url_shortener_v2/tests/sqlx_test.rs"
```

**Requiere** `DATABASE_URL` al ejecutar los tests (no al compilar, si usas el caché
`.sqlx/`). Es más rápido que testcontainers porque no levanta un contenedor por test,
pero necesita un PostgreSQL disponible:

```bash
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres \
    cargo test --test sqlx_test -- --ignored
```

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

!!! abstract "Siguiente paso"

    Semana 12 — [Observabilidad, Docker y CI/CD: production-ready](section_04.md).
