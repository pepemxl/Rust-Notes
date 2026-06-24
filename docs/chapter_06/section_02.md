# Fase 2 (I): Implementación core — make it work

La Semana 22 arranca la implementación real del capstone. El `DESIGN.md`
existe, los ADRs están escritos, el contrato OpenAPI está acordado. Ahora
construimos las capas que no dependen del protocolo HTTP: el dominio puro
(`crates/core`) y los adaptadores de base de datos (`crates/db`). Cuando
termina la semana, el sistema puede persistir y leer datos, migrar el esquema
y ejecutar la lógica de negocio — todo sin que el servidor HTTP exista todavía.

En esta sección aprenderemos:

- Cómo inicializar un workspace Cargo con dependencias centralizadas y lints
  globales (`workspace.dependencies`, `workspace.lints`).
- Por qué `crates/core` nunca importa `sqlx`, `axum` ni `tokio` — y cómo
  mantener esa disciplina con el compilador.
- Cómo modelar el dominio con typestates que hacen imposibles los estados
  inválidos en tiempo de compilación.
- Cómo usar `proptest` para probar invariantes de dominio con cientos de
  entradas generadas automáticamente.
- Cómo escribir migraciones idempotentes y queries verificadas en compilación
  con `sqlx::query_as!` en offline mode.
- Cómo construir una imagen Docker multi-stage con `cargo-chef` que respeta
  el caché de capas incluso cuando el código cambia.

> *"Make it work, make it right, make it fast — in that order."*
> — Kent Beck

---

## La regla de dependencias del workspace

```text
GRAFO DE DEPENDENCIAS (acíclico, estricto):

  crates/core    ←── (nadie dentro del workspace)
       ↑
  crates/db      ←── impl de los ports de core con sqlx
       ↑
  crates/cli     ←── combina core + db para comandos admin
       ↑
  crates/server  ←── combina core + db + HTTP (Semana 23)
       ↑
  xtask          ←── automatización (codegen, release, lint)

REGLAS:
  ✅ core puede depender de: serde, thiserror, time, uuid, derive_more
  ❌ core NO puede depender de: sqlx, axum, tokio, redis, reqwest
  ✅ db puede depender de: core, sqlx, tokio
  ❌ db NO puede depender de: server, cli, axum
  ✅ server puede depender de: core, db
  ✅ cli puede depender de: core, db

SI VIOLA LA REGLA:
  error[E0432]: unresolved import `sqlx`
  En crates/core/src/lib.rs — señal de que rompiste la separación.
```

---

## Estructura del workspace

```text
linkmetrics/                    ← raíz del workspace
├── Cargo.toml                  ← workspace root (no [package])
├── Cargo.lock                  ← commiteado (es una aplicación)
├── .sqlx/                      ← queries SQLx offline-verificadas
│   └── *.json
├── crates/
│   ├── core/                   ← dominio puro, cero I/O
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── domain/         ← tipos (Link, Click, User, ApiKey)
│   │       ├── ports/          ← traits (LinkRepo, ClickStore…)
│   │       └── services/       ← use cases (puras, testeables sin DB)
│   ├── db/                     ← adaptadores SQLx
│   │   ├── Cargo.toml
│   │   ├── migrations/
│   │   │   ├── 20240601000000_initial.sql
│   │   │   └── 20240601000001_indexes.sql
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pg_link_repo.rs
│   │       └── pg_click_store.rs
│   ├── server/                 ← HTTP (Semana 23)
│   │   └── ...
│   └── cli/                    ← CLI admin
│       ├── Cargo.toml
│       └── src/main.rs
├── xtask/                      ← cargo xtask codegen/release/lint
│   └── src/main.rs
└── infra/
    ├── docker-compose.yml
    ├── Dockerfile
    └── Dockerfile.dev
```

### `Cargo.toml` raíz del workspace

```toml
[workspace]
resolver = "2"
members = [
    "crates/core",
    "crates/db",
    "crates/server",
    "crates/cli",
    "xtask",
]
exclude = ["target"]

# ── Versiones centralizadas ───────────────────────────────────────────────
[workspace.dependencies]
# Dominio
serde       = { version = "1",     features = ["derive"] }
thiserror   = "1"
derive_more = { version = "1",     features = ["display", "from", "error"] }
uuid        = { version = "1",     features = ["v7", "serde"] }
time        = { version = "0.3",   features = ["serde", "formatting"] }

# DB (solo crates/db y crates/server los activan)
sqlx = { version = "0.8", default-features = false, features = [
    "runtime-tokio", "postgres", "chrono", "uuid", "json", "offline",
] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# HTTP (solo crates/server)
axum = { version = "0.7", features = ["ws"] }

# Observabilidad
tracing             = "0.1"
tracing-subscriber  = { version = "0.3", features = ["env-filter", "json"] }

# Config
figment = { version = "0.10", features = ["env", "toml"] }

# CLI
clap = { version = "4", features = ["derive", "env"] }

# Testing
proptest       = "1"
testcontainers = "0.21"

# ── Lints globales ────────────────────────────────────────────────────────
[workspace.lints.rust]
unused_crate_dependencies = "warn"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
all      = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
# Excepciones razonadas
module_name_repetitions = "allow"  # LinkRepo en mod link está bien
missing_errors_doc      = "allow"  # docs en progress durante desarrollo

# ── Perfiles ──────────────────────────────────────────────────────────────
[profile.release]
lto            = true
strip          = "symbols"
codegen-units  = 1
panic          = "abort"

[profile.dev.package."*"]
opt-level = 1   # deps compiladas con O1 en dev: 3x más rápido sin perder debug info
```

---

## `crates/core` — dominio puro

Este crate es la pieza más importante del sistema. Nunca tendrá un `sqlx`,
un `axum` ni un `tokio` en su `[dependencies]`. Cuando sus tests pasan, el
sistema es correcto — independientemente de qué base de datos uses o qué
framework HTTP elijas.

### `crates/core/Cargo.toml`

```toml
[package]
name    = "linkmetrics-core"
version = "0.1.0"
edition = "2021"

[dependencies]
serde      = { workspace = true }
thiserror  = { workspace = true }
uuid       = { workspace = true }
time       = { workspace = true }

[dev-dependencies]
proptest = { workspace = true }
```

### Tipos del dominio

```rust
// crates/core/src/domain/link.rs

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Identificadores de nueva-línea (Newtype) ──────────────────────────────
// Evitan confundir LinkId con UserId aunque ambos sean Uuid internamente.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl LinkId {
    pub fn new() -> Self { Self(Uuid::now_v7()) }
}
impl UserId {
    pub fn new() -> Self { Self(Uuid::now_v7()) }
}

// ── Typestate: los estados posibles de un Link ────────────────────────────

/// Recién creado, no ha sido revisado por el sistema.
pub struct Draft;
/// Activo y sirve redirecciones.
pub struct Active;
/// Caducado (por fecha o por desactivación manual).
pub struct Expired;

/// Un link tipado con su estado en el tipo.
/// `Link<Draft>` no puede redirigir — el compilador lo impide.
#[derive(Debug, Clone)]
pub struct Link<S = Active> {
    pub id:         LinkId,
    pub user_id:    UserId,
    pub code:       String,
    pub target_url: String,
    pub title:      Option<String>,
    _state: std::marker::PhantomData<S>,
}

impl Link<Draft> {
    /// El único constructor. Produce un Draft que debe ser activado.
    pub fn new(user_id: UserId, code: String, target_url: String) -> Self {
        Self {
            id: LinkId::new(),
            user_id,
            code,
            target_url,
            title: None,
            _state: std::marker::PhantomData,
        }
    }

    /// Valida y transiciona a Active. Consume el Draft.
    pub fn activate(self) -> Result<Link<Active>, DomainError> {
        validate_code(&self.code)?;
        validate_url(&self.target_url)?;
        Ok(Link {
            id:         self.id,
            user_id:    self.user_id,
            code:       self.code,
            target_url: self.target_url,
            title:      self.title,
            _state:     std::marker::PhantomData,
        })
    }
}

impl Link<Active> {
    /// Desactiva el link. Consume el Active.
    pub fn expire(self) -> Link<Expired> {
        Link {
            id:         self.id,
            user_id:    self.user_id,
            code:       self.code,
            target_url: self.target_url,
            title:      self.title,
            _state:     std::marker::PhantomData,
        }
    }

    /// Solo los links activos pueden redirigir.
    pub fn target_url(&self) -> &str {
        &self.target_url
    }
}

// Un Link<Expired> no tiene método target_url() — no puede redirigir.
// Esto es correcto por construcción.

// ── Registro de click ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Click {
    pub link_id:    LinkId,
    pub ip_hash:    String,  // SHA-256 + salt; nunca IP en texto plano
    pub country:    Option<String>,
    pub referrer:   Option<String>,
    pub user_agent: Option<String>,
}
```

### Errores del dominio

```rust
// crates/core/src/domain/errors.rs

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("validación fallida: {0}")]
    Validation(String),

    #[error("código '{0}' ya existe")]
    Conflict(String),

    #[error("recurso no encontrado")]
    NotFound,

    #[error("sin autorización")]
    Unauthorized,

    #[error("error interno: {0}")]
    Internal(String),
}

// Mapeo a códigos HTTP — un único lugar, nunca duplicado
impl DomainError {
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Validation(_)  => 422,
            Self::Conflict(_)    => 409,
            Self::NotFound       => 404,
            Self::Unauthorized   => 403,
            Self::Internal(_)    => 500,
        }
    }

    pub fn http_title(&self) -> &'static str {
        match self {
            Self::Validation(_)  => "Unprocessable Entity",
            Self::Conflict(_)    => "Conflict",
            Self::NotFound       => "Not Found",
            Self::Unauthorized   => "Forbidden",
            Self::Internal(_)    => "Internal Server Error",
        }
    }
}
```

### Ports — los contratos que implementará `crates/db`

```rust
// crates/core/src/ports/link_repo.rs

use crate::domain::{DomainError, Link, LinkId, UserId};

/// Puerto de persistencia de links.
/// `crates/core` solo conoce esta interfaz — nunca PgPool ni SQLx.
pub trait LinkRepo {
    fn save(&mut self, link: &Link<crate::domain::link::Active>) -> Result<(), DomainError>;
    fn find_by_code(&self, code: &str) -> Result<Option<Link>, DomainError>;
    fn find_by_id(&self, id: LinkId) -> Result<Option<Link>, DomainError>;
    fn find_by_user(&self, user_id: UserId, page: u32, per_page: u32)
        -> Result<Vec<Link>, DomainError>;
    fn deactivate(&mut self, id: LinkId, requester: UserId) -> Result<(), DomainError>;
}

// crates/core/src/ports/click_store.rs

use crate::domain::{Click, DomainError, LinkId};

pub trait ClickStore {
    fn record(&mut self, click: Click) -> Result<(), DomainError>;
    fn count_by_link(&self, link_id: LinkId) -> Result<u64, DomainError>;
}

// crates/core/src/ports/clock.rs

/// Abstracción del tiempo — fundamental para tests deterministas.
/// En producción: SystemClock. En tests: FixedClock.
pub trait Clock: Send + Sync {
    fn now_unix_ms(&self) -> u64;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_unix_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}
```

### Validaciones del dominio

```rust
// crates/core/src/domain/validation.rs

use crate::domain::DomainError;

const MAX_CODE_LEN: usize = 32;
const MIN_CODE_LEN: usize = 3;

pub fn validate_code(code: &str) -> Result<(), DomainError> {
    if code.len() < MIN_CODE_LEN || code.len() > MAX_CODE_LEN {
        return Err(DomainError::Validation(format!(
            "código debe tener entre {MIN_CODE_LEN} y {MAX_CODE_LEN} caracteres, \
             tiene {}", code.len()
        )));
    }
    if !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(DomainError::Validation(
            "código solo puede contener letras ASCII, números, '-' y '_'".into()
        ));
    }
    if code.starts_with('-') || code.ends_with('-') {
        return Err(DomainError::Validation(
            "código no puede empezar ni terminar con '-'".into()
        ));
    }
    Ok(())
}

pub fn validate_url(url: &str) -> Result<(), DomainError> {
    if url.is_empty() {
        return Err(DomainError::Validation("URL no puede estar vacía".into()));
    }
    if url.len() > 2048 {
        return Err(DomainError::Validation(
            format!("URL demasiado larga: {} caracteres (máx 2048)", url.len())
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(DomainError::Validation(
            "URL debe comenzar con http:// o https://".into()
        ));
    }
    Ok(())
}
```

### Use cases — lógica de negocio pura

```rust
// crates/core/src/services/link_service.rs

use crate::{
    domain::{link::{Active, Draft, Link}, DomainError, UserId},
    ports::{ClickStore, LinkRepo},
};

/// Crear un nuevo link. Toda la validación ocurre aquí.
/// Sin I/O — el repo es un trait, puede ser en memoria.
pub fn create_link(
    repo:       &mut impl LinkRepo,
    user_id:    UserId,
    code:       String,
    target_url: String,
) -> Result<Link<Active>, DomainError> {
    // Verificar que el código no exista (por eso necesitamos el repo)
    if repo.find_by_code(&code)?.is_some() {
        return Err(DomainError::Conflict(code));
    }
    // El typestate garantiza que activar() valida antes de guardar
    let link = Link::<Draft>::new(user_id, code, target_url)
        .activate()?;
    repo.save(&link)?;
    Ok(link)
}

/// Resolver redirección y registrar click.
pub fn redirect(
    link_repo:   &impl LinkRepo,
    click_store: &mut impl ClickStore,
    code:        &str,
    ip_hash:     String,
    country:     Option<String>,
    referrer:    Option<String>,
    user_agent:  Option<String>,
) -> Result<String, DomainError> {
    let link = link_repo
        .find_by_code(code)?
        .ok_or(DomainError::NotFound)?;

    let click = crate::domain::Click {
        link_id: link.id,
        ip_hash,
        country,
        referrer,
        user_agent,
    };
    click_store.record(click)?;

    Ok(link.target_url.clone())
}
```

### Tests con `proptest`

```rust
// crates/core/src/services/link_service_tests.rs
// (o en tests/ del crate)

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{link::Active, DomainError, Link, LinkId, UserId},
        ports::LinkRepo,
    };
    use proptest::prelude::*;

    // ── Adapter en memoria para tests (implementa el port) ────────────────

    #[derive(Default)]
    struct MemLinkRepo {
        store: Vec<Link<Active>>,
    }

    impl LinkRepo for MemLinkRepo {
        fn save(&mut self, link: &Link<Active>) -> Result<(), DomainError> {
            if self.store.iter().any(|l| l.code == link.code) {
                return Err(DomainError::Conflict(link.code.clone()));
            }
            self.store.push(link.clone());
            Ok(())
        }

        fn find_by_code(&self, code: &str) -> Result<Option<Link<Active>>, DomainError> {
            Ok(self.store.iter().find(|l| l.code == code).cloned())
        }

        fn find_by_id(&self, id: LinkId) -> Result<Option<Link<Active>>, DomainError> {
            Ok(self.store.iter().find(|l| l.id == id).cloned())
        }

        fn find_by_user(&self, user_id: UserId, page: u32, per_page: u32)
            -> Result<Vec<Link<Active>>, DomainError>
        {
            let all: Vec<_> = self.store.iter()
                .filter(|l| l.user_id == user_id)
                .cloned()
                .collect();
            let start = ((page - 1) * per_page) as usize;
            Ok(all.into_iter().skip(start).take(per_page as usize).collect())
        }

        fn deactivate(&mut self, id: LinkId, requester: UserId) -> Result<(), DomainError> {
            let pos = self.store.iter().position(|l| l.id == id)
                .ok_or(DomainError::NotFound)?;
            if self.store[pos].user_id != requester {
                return Err(DomainError::Unauthorized);
            }
            self.store.remove(pos);
            Ok(())
        }
    }

    fn usuario() -> UserId { UserId::new() }

    // ── Tests unitarios deterministas ──────────────────────────────────────

    #[test]
    fn crear_link_exitoso() {
        let mut repo = MemLinkRepo::default();
        let link = create_link(&mut repo, usuario(), "mi-link".into(), "https://ejemplo.com".into());
        assert!(link.is_ok());
        assert_eq!(link.unwrap().code, "mi-link");
    }

    #[test]
    fn codigo_duplicado_es_conflict() {
        let mut repo = MemLinkRepo::default();
        let uid = usuario();
        create_link(&mut repo, uid, "dup".into(), "https://a.com".into()).unwrap();
        let r = create_link(&mut repo, uid, "dup".into(), "https://b.com".into());
        assert_eq!(r, Err(DomainError::Conflict("dup".into())));
    }

    #[test]
    fn codigo_corto_es_validation() {
        let mut repo = MemLinkRepo::default();
        let r = create_link(&mut repo, usuario(), "ab".into(), "https://a.com".into());
        assert!(matches!(r, Err(DomainError::Validation(_))));
    }

    #[test]
    fn url_sin_esquema_es_validation() {
        let mut repo = MemLinkRepo::default();
        let r = create_link(&mut repo, usuario(), "ok-code".into(), "ejemplo.com".into());
        assert!(matches!(r, Err(DomainError::Validation(_))));
    }

    #[test]
    fn link_expirado_no_tiene_metodo_target_url() {
        // Este test COMPILARÍA si Link<Expired> tuviera target_url().
        // Como no lo tiene, el código siguiente NO compila:
        //
        // let expired = link.expire();
        // let _ = expired.target_url(); // ← error[E0599]: no method named `target_url`
        //
        // Por eso los tests de typestate son en tiempo de compilación, no runtime.
        // Este comentario DOCUMENTA la garantía, la prueba es que el código de arriba
        // no compila aunque lo intentes.
    }

    // ── Tests con proptest: cientos de entradas automáticas ───────────────

    proptest! {
        // Estrategia: generar códigos válidos (longitud 3-32, charset correcto)
        #[test]
        fn links_validos_siempre_se_crean(
            code in "[a-zA-Z0-9][a-zA-Z0-9_-]{1,30}[a-zA-Z0-9]"
        ) {
            let mut repo = MemLinkRepo::default();
            let r = create_link(
                &mut repo,
                usuario(),
                code.clone(),
                "https://ejemplo.com".into(),
            );
            prop_assert!(r.is_ok(), "código '{code}' debería ser válido pero falló: {:?}", r);
        }

        #[test]
        fn urls_sin_esquema_siempre_fallan(
            url in "[a-zA-Z0-9]{3,20}\\.[a-zA-Z]{2,4}"
        ) {
            let mut repo = MemLinkRepo::default();
            let r = create_link(&mut repo, usuario(), "cod-val".into(), url);
            prop_assert!(matches!(r, Err(DomainError::Validation(_))));
        }

        #[test]
        fn dos_links_distintos_no_colisionan(
            code1 in "[a-zA-Z0-9]{3,15}",
            code2 in "[a-zA-Z0-9]{3,15}",
        ) {
            // Si los códigos son distintos, ambas creaciones deben funcionar
            prop_assume!(code1 != code2);
            let mut repo = MemLinkRepo::default();
            let uid = usuario();
            let r1 = create_link(&mut repo, uid, code1.clone(), "https://a.com".into());
            let r2 = create_link(&mut repo, uid, code2.clone(), "https://b.com".into());
            // Ambos pueden fallar si el código no cumple el charset — lo aceptamos
            if r1.is_ok() && r2.is_ok() {
                // Pero no deben interferir entre sí
                prop_assert_eq!(repo.find_by_code(&code1).unwrap().unwrap().code, code1);
                prop_assert_eq!(repo.find_by_code(&code2).unwrap().unwrap().code, code2);
            }
        }
    }
}
```

---

## `crates/db` — adaptadores SQLx

### `crates/db/Cargo.toml`

```toml
[package]
name    = "linkmetrics-db"
version = "0.1.0"
edition = "2021"

[dependencies]
linkmetrics-core = { path = "../core" }
sqlx   = { workspace = true }
tokio  = { workspace = true }
uuid   = { workspace = true }

[dev-dependencies]
testcontainers = { workspace = true }
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
```

### Migraciones SQL

```sql
-- crates/db/migrations/20240601000000_initial.sql

-- Extensiones primero
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ── Usuarios ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS users (
    id             UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    email          TEXT         NOT NULL UNIQUE,
    name           TEXT         NOT NULL,
    password_hash  TEXT         NOT NULL,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- ── API Keys ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS api_keys (
    id               UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_hash         TEXT         NOT NULL UNIQUE,  -- SHA-256 del key real
    name             TEXT         NOT NULL,
    rate_limit_rpm   INT          NOT NULL DEFAULT 100,
    is_active        BOOLEAN      NOT NULL DEFAULT true,
    expires_at       TIMESTAMPTZ,
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- ── Links ───────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS links (
    id          UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID         NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code        TEXT         NOT NULL UNIQUE,
    target_url  TEXT         NOT NULL,
    title       TEXT,
    is_active   BOOLEAN      NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ
);

-- ── Clicks ──────────────────────────────────────────────────────────────────
-- Tabla append-only: NUNCA se actualiza ni borra (auditabilidad + GDPR).
-- ip_hash: SHA-256 + salt diario. NUNCA la IP en texto plano.
CREATE TABLE IF NOT EXISTS clicks (
    id          UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    link_id     UUID         NOT NULL REFERENCES links(id) ON DELETE CASCADE,
    ip_hash     TEXT         NOT NULL,
    country     CHAR(2),
    referrer    TEXT,
    user_agent  TEXT,
    clicked_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);
```

```sql
-- crates/db/migrations/20240601000001_indexes.sql

-- Índice principal de lookup de redirección (la query más frecuente)
CREATE INDEX IF NOT EXISTS idx_links_code_active
    ON links (code)
    WHERE is_active = true;

-- Índice para listar links de un usuario
CREATE INDEX IF NOT EXISTS idx_links_user_created
    ON links (user_id, created_at DESC);

-- Índice para analytics por link
CREATE INDEX IF NOT EXISTS idx_clicks_link_at
    ON clicks (link_id, clicked_at DESC);

-- Índice para analytics por país
CREATE INDEX IF NOT EXISTS idx_clicks_country
    ON clicks (country, clicked_at DESC)
    WHERE country IS NOT NULL;

-- Índice para lookup de API key (la query de autenticación)
CREATE INDEX IF NOT EXISTS idx_api_keys_hash_active
    ON api_keys (key_hash)
    WHERE is_active = true;
```

### `PgLinkRepo` — el adaptador PostgreSQL

```rust
// crates/db/src/pg_link_repo.rs

use linkmetrics_core::{
    domain::{link::Active, DomainError, Link, LinkId, UserId},
    ports::LinkRepo,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgLinkRepo {
    pool: PgPool,
}

impl PgLinkRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// Tipo de fila que SQLx mapea directamente desde PostgreSQL
#[derive(sqlx::FromRow)]
struct LinkRow {
    id:         Uuid,
    user_id:    Uuid,
    code:       String,
    target_url: String,
    title:      Option<String>,
    is_active:  bool,
}

impl From<LinkRow> for Link<Active> {
    fn from(row: LinkRow) -> Self {
        Link {
            id:         linkmetrics_core::domain::LinkId(row.id),
            user_id:    linkmetrics_core::domain::UserId(row.user_id),
            code:       row.code,
            target_url: row.target_url,
            title:      row.title,
            _state:     std::marker::PhantomData,
        }
    }
}

// Nota: los métodos son async porque SQLx lo requiere.
// Los traits de core son síncronos — en el server usaremos
// spawn_blocking o traits async-trait separados.
// Esta semana construimos la versión async directamente:
impl PgLinkRepo {
    pub async fn save_async(&self, link: &Link<Active>) -> Result<(), DomainError> {
        sqlx::query!(
            r#"
            INSERT INTO links (id, user_id, code, target_url, title, is_active)
            VALUES ($1, $2, $3, $4, $5, true)
            "#,
            link.id.0,
            link.user_id.0,
            link.code,
            link.target_url,
            link.title.as_deref(),
        )
        .execute(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db) if db.constraint() == Some("links_code_key") =>
                DomainError::Conflict(link.code.clone()),
            e => DomainError::Internal(e.to_string()),
        })?;
        Ok(())
    }

    pub async fn find_by_code_async(&self, code: &str) -> Result<Option<Link<Active>>, DomainError> {
        // query_as! verifica en compilación que los tipos Rust coinciden con
        // las columnas de PostgreSQL. Si renombras una columna en SQL,
        // el compilador lo detecta — no en runtime.
        let row = sqlx::query_as!(
            LinkRow,
            r#"
            SELECT id, user_id, code, target_url, title, is_active
            FROM   links
            WHERE  code = $1 AND is_active = true
            "#,
            code
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(row.map(Link::from))
    }

    pub async fn find_by_user_async(
        &self,
        user_id: UserId,
        page:     u32,
        per_page: u32,
    ) -> Result<Vec<Link<Active>>, DomainError> {
        let offset = (page.saturating_sub(1) * per_page) as i64;
        let limit  = per_page as i64;

        let rows = sqlx::query_as!(
            LinkRow,
            r#"
            SELECT id, user_id, code, target_url, title, is_active
            FROM   links
            WHERE  user_id = $1 AND is_active = true
            ORDER  BY created_at DESC
            LIMIT  $2 OFFSET $3
            "#,
            user_id.0,
            limit,
            offset,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(rows.into_iter().map(Link::from).collect())
    }

    pub async fn deactivate_async(&self, id: LinkId, requester: UserId) -> Result<(), DomainError> {
        let result = sqlx::query!(
            r#"
            UPDATE links
            SET    is_active = false
            WHERE  id = $1 AND user_id = $2
            "#,
            id.0,
            requester.0,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        if result.rows_affected() == 0 {
            // El link no existe O no pertenece al usuario — ambos son NotFound
            // por seguridad (no revelar si existe pero de otro usuario)
            Err(DomainError::NotFound)
        } else {
            Ok(())
        }
    }
}
```

### `PgClickStore` — grabación de clicks

```rust
// crates/db/src/pg_click_store.rs

use linkmetrics_core::{
    domain::{Click, DomainError, LinkId},
};
use sqlx::PgPool;

pub struct PgClickStore {
    pool: PgPool,
}

impl PgClickStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn record_async(&self, click: &Click) -> Result<(), DomainError> {
        sqlx::query!(
            r#"
            INSERT INTO clicks (link_id, ip_hash, country, referrer, user_agent)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            click.link_id.0,
            click.ip_hash,
            click.country.as_deref(),
            click.referrer.as_deref(),
            click.user_agent.as_deref(),
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    pub async fn count_by_link_async(&self, link_id: LinkId) -> Result<u64, DomainError> {
        // query_scalar! para una sola columna de un tipo escalar
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM clicks WHERE link_id = $1",
            link_id.0,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .unwrap_or(0);

        Ok(count as u64)
    }
}
```

### Connection pool y configuración

```rust
// crates/db/src/lib.rs

pub mod pg_link_repo;
pub mod pg_click_store;

pub use pg_link_repo::PgLinkRepo;
pub use pg_click_store::PgClickStore;

use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

/// Crear pool con timeouts y tamaño configurables.
/// Llámalo una vez al arrancar y comparte el pool clonado (Arc interno).
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect(database_url)
        .await
}

/// Ejecutar migraciones pendientes. Llamado por `cli migrate` y al arrancar
/// en dev. En prod: el pipeline CI/CD llama `sqlx migrate run` explícitamente.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
```

---

## SQLx offline mode — queries verificadas sin DB

Uno de los superpoderes de SQLx es verificar las queries contra el schema real
en tiempo de compilación. Para que CI pueda compilar sin tener PostgreSQL,
se genera un cache local:

```text
FLUJO DE DESARROLLO:

  1. Tener PostgreSQL corriendo (dev):
     docker compose up -d postgres

  2. Generar el cache offline (ejecutar en la raíz del workspace):
     DATABASE_URL="postgres://..." cargo sqlx prepare --workspace

     Esto genera archivos .sqlx/*.json con la firma de cada query.

  3. Hacer commit de .sqlx/:
     git add .sqlx/ && git commit -m "chore: update sqlx query cache"

  4. En CI (sin PostgreSQL):
     SQLX_OFFLINE=true cargo build --workspace

     El compilador usa el cache en lugar de conectarse a la DB.

  5. En CI: verificar que el cache está actualizado:
     cargo sqlx prepare --workspace --check
     (falla si hay queries sin cache o cache desactualizado)
```

```text
ERROR TÍPICO CUANDO EL CACHE ESTÁ DESACTUALIZADO:

error: no offline data for query, looked in `.sqlx/query-abc123.json`
  --> crates/db/src/pg_link_repo.rs:55:9
   |
55 |         sqlx::query_as!(
   |         ^^^^^^^^^^^^^^^^
   |
   = note: ensure you're connected to a database and run `cargo sqlx prepare`

SOLUCIÓN:
  DATABASE_URL="postgres://..." cargo sqlx prepare --workspace
  git add .sqlx && git commit -m "chore: update sqlx query cache"
```

### Tabla comparativa: macros SQLx

| Macro | Retorna | Cuándo usarla |
|-------|---------|---------------|
| `query!()` | `Record` sin tipo nombrado | Queries ad-hoc, un campo |
| `query_as!(Tipo, ...)` | `Tipo` (FromRow) | Cuando mapeas a un struct tuyo |
| `query_scalar!()` | Tipo escalar (`i64`, `bool`…) | `COUNT`, `EXISTS`, `MAX` |
| `query_file!()` | como `query!` | SQL largo en archivo `.sql` separado |
| `query_as_unchecked!()` | como `query_as!` | ❌ evitar — no verifica tipos |

---

## `crates/cli` — CLI básico

Esta semana construimos los comandos de migración y estadísticas básicas.
Los comandos de gestión de usuarios vendrán en la Semana 23.

### `crates/cli/Cargo.toml`

```toml
[package]
name    = "linkmetrics-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "lm"
path = "src/main.rs"

[dependencies]
linkmetrics-core = { path = "../core" }
linkmetrics-db   = { path = "../db" }
clap   = { workspace = true }
tokio  = { workspace = true }
figment = { workspace = true }
```

```rust
// crates/cli/src/main.rs

use clap::{Parser, Subcommand};
use figment::{providers::{Env, Format, Toml}, Figment};
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "lm", version, about = "LinkMetrics admin CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Aplicar migraciones pendientes a la base de datos
    Migrate,
    /// Mostrar estadísticas del sistema
    Stats {
        /// Mostrar solo últimas N horas
        #[arg(short, long, default_value = "24")]
        hours: u32,
    },
    /// Verificar conectividad con dependencias
    Check,
}

#[derive(Debug, Deserialize)]
struct Config {
    database_url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let config: Config = Figment::new()
        .merge(Toml::file("linkmetrics.toml"))
        .merge(Env::prefixed("LM_"))
        .extract()?;

    let pool = linkmetrics_db::create_pool(&config.database_url).await?;

    match cli.command {
        Commands::Migrate => {
            println!("Aplicando migraciones...");
            linkmetrics_db::run_migrations(&pool).await?;
            println!("✓ Migraciones aplicadas.");
        }
        Commands::Stats { hours } => {
            let total_links: i64 = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM links WHERE is_active = true"
            )
            .fetch_one(&pool)
            .await?
            .unwrap_or(0);

            let clicks_period: i64 = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM clicks WHERE clicked_at > now() - make_interval(hours => $1)",
                hours as i32
            )
            .fetch_one(&pool)
            .await?
            .unwrap_or(0);

            println!("LinkMetrics Stats");
            println!("  Links activos : {total_links}");
            println!("  Clicks ({hours}h)  : {clicks_period}");
        }
        Commands::Check => {
            // Verificar conexión DB
            let db_ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
            println!("  postgres: {}", if db_ok { "✓ OK" } else { "✗ ERROR" });
        }
    }

    Ok(())
}
```

---

## `infra/` — Infraestructura local

### `infra/docker-compose.yml`

```yaml
# docker-compose.yml — entorno de desarrollo completo
# Uso: docker compose up -d
# El servidor Rust corre fuera de Docker para desarrollo (cargo watch)

services:
  # ── Base de datos ────────────────────────────────────────────────────────
  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER:     linkmetrics
      POSTGRES_PASSWORD: linkmetrics_dev
      POSTGRES_DB:       linkmetrics
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U linkmetrics"]
      interval: 5s
      timeout: 5s
      retries: 10
    restart: unless-stopped

  # ── Cache y rate limiting ─────────────────────────────────────────────────
  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes --save 60 1
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 3s
      retries: 5
    restart: unless-stopped

  # ── Object storage (S3-compatible) ───────────────────────────────────────
  minio:
    image: minio/minio:latest
    command: server /data --console-address ":9001"
    environment:
      MINIO_ROOT_USER:     minioadmin
      MINIO_ROOT_PASSWORD: minioadmin123
    ports:
      - "9000:9000"
      - "9001:9001"
    volumes:
      - minio_data:/data
    healthcheck:
      test: ["CMD", "mc", "ready", "local"]
      interval: 10s
      timeout: 5s
      retries: 5

  # ── SMTP testing (inspección de emails sin enviarlos) ────────────────────
  mailpit:
    image: axllent/mailpit:latest
    ports:
      - "1025:1025"  # SMTP
      - "8025:8025"  # Web UI: http://localhost:8025

  # ── Observabilidad local (traces) ─────────────────────────────────────────
  jaeger:
    image: jaegertracing/all-in-one:latest
    environment:
      COLLECTOR_OTLP_ENABLED: "true"
    ports:
      - "16686:16686"  # Jaeger UI
      - "4317:4317"    # OTLP gRPC

volumes:
  postgres_data:
  redis_data:
  minio_data:
```

### `infra/Dockerfile` — imagen de producción

```dockerfile
# Etapa 1: planificador de dependencias (cargo-chef)
# Construye solo las dependencias, aprovechando el caché de Docker layers.
# Clave: las dependencias cambian raramente; el código cambia a cada commit.
FROM rust:1.81-slim-bookworm AS planner
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Etapa 2: constructor de dependencias (caché)
FROM rust:1.81-slim-bookworm AS deps
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY --from=planner /app/recipe.json recipe.json
# Esta capa solo se invalida si Cargo.lock/Cargo.toml cambia
RUN cargo chef cook --release --recipe-path recipe.json

# Etapa 3: constructor del binario
FROM rust:1.81-slim-bookworm AS builder
WORKDIR /app

# Dependencias del sistema para SQLx (libssl, libpq)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev \
    && rm -rf /var/lib/apt/lists/*

# Copiar deps pre-compiladas de la etapa anterior
COPY --from=deps /app/target target
COPY --from=deps /usr/local/cargo /usr/local/cargo

# Ahora copiar el código fuente (esta capa se invalida con cada cambio)
COPY . .

# Compilar en modo SQLX_OFFLINE (usa el cache .sqlx/ commiteado)
RUN SQLX_OFFLINE=true cargo build --release --bin linkmetrics-server

# Etapa 4: imagen final — Distroless (sin shell, sin apt, sin root)
FROM gcr.io/distroless/cc-debian12 AS runtime
WORKDIR /app

# Copiar solo el binario compilado
COPY --from=builder /app/target/release/linkmetrics-server /app/server

# Usuario no-root (distroless usa uid 65532 por defecto)
USER 65532:65532

# Variables de entorno configurables en runtime
ENV LM_HOST=0.0.0.0
ENV LM_PORT=3000

EXPOSE 3000

ENTRYPOINT ["/app/server"]
```

```dockerfile
# infra/Dockerfile.dev — para desarrollo local con hot-reload
FROM rust:1.81-slim-bookworm
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libpq-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-watch --locked

COPY . .

# Hot-reload: recompila y reinicia al cambiar .rs
CMD ["cargo", "watch", "-x", "run --bin linkmetrics-server"]
```

---

## `.github/workflows/ci.yml` — la puerta de calidad

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  SQLX_OFFLINE: "true"  # CI compila sin DB real

jobs:
  # ── Formato y lints ────────────────────────────────────────────────────
  lint:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2

      - name: Formato
        run: cargo fmt --all -- --check

      - name: Clippy (deny warnings)
        run: cargo clippy --workspace --all-features -- -D warnings

  # ── Tests del workspace ────────────────────────────────────────────────
  test:
    name: Tests
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_USER:     linkmetrics
          POSTGRES_PASSWORD: linkmetrics_dev
          POSTGRES_DB:       linkmetrics_test
        ports: ["5432:5432"]
        options: >-
          --health-cmd pg_isready
          --health-interval 5s
          --health-timeout 5s
          --health-retries 10
    env:
      DATABASE_URL: postgres://linkmetrics:linkmetrics_dev@localhost/linkmetrics_test
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Instalar sqlx-cli
        run: cargo install sqlx-cli --no-default-features --features postgres --locked

      - name: Migraciones
        run: cargo sqlx migrate run --source crates/db/migrations

      - name: Verificar cache SQLx actualizado
        run: cargo sqlx prepare --workspace --check

      - name: Tests
        run: cargo test --workspace

  # ── Auditoría de seguridad ─────────────────────────────────────────────
  security:
    name: Security
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Cargo audit (CVEs)
        run: |
          cargo install cargo-audit --locked
          cargo audit

      - name: Cargo deny (licencias, fuentes, bans)
        run: |
          cargo install cargo-deny --locked
          cargo deny check

  # ── Build & push imagen ─────────────────────────────────────────────────
  docker:
    name: Docker build
    runs-on: ubuntu-latest
    needs: [lint, test, security]
    if: github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v4

      - name: Build imagen
        run: |
          docker build \
            --file infra/Dockerfile \
            --tag ghcr.io/${{ github.repository }}:${{ github.sha }} \
            --tag ghcr.io/${{ github.repository }}:latest \
            .

      - name: Login GHCR
        uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Push
        run: docker push ghcr.io/${{ github.repository }} --all-tags
```

---

## Errores de compilación más frecuentes esta semana

### Error 1: dependency cycle entre crates

```text
error[E0523]: found both `linkmetrics-core v0.1.0 (./crates/core)` and
              `linkmetrics-core v0.1.0 (./crates/core)` as dependencies of
              `linkmetrics-db v0.1.0 (./crates/db)`

CAUSA: crates/core tiene un [dependency] que depende de crates/db.
SOLUCIÓN: core NUNCA puede importar db. Solo traits (ports) en core.
```

### Error 2: SQLx sin DATABASE_URL y sin offline mode

```text
error: `DATABASE_URL` must be set, or `SQLX_OFFLINE=true` must be set

CAUSA: estás compilando con query_as! pero no hay DB ni cache .sqlx/
SOLUCIÓN OPCIÓN A: docker compose up -d postgres && DATABASE_URL="..." cargo build
SOLUCIÓN OPCIÓN B: SQLX_OFFLINE=true cargo build  (si ya hiciste cargo sqlx prepare)
```

### Error 3: tipo Rust no coincide con tipo PostgreSQL

```text
error[E0277]: the trait bound `Option<String>: sqlx::Decode<'_, sqlx::Postgres>` is not satisfied
  --> crates/db/src/pg_link_repo.rs:45:9
   |
45 |     let row = sqlx::query_as!(LinkRow, "SELECT title FROM links ...").fetch_one(&pool)
   |               ^^^^^^^^^^ the trait `sqlx::Decode` is not implemented for `Option<String>`

CAUSA: la columna `title` es TEXT NOT NULL en el schema pero tienes Option<String> en LinkRow.
SOLUCIÓN: o cambias el SQL a `title TEXT` (nullable) o cambias a `title: String` en el struct.
```

### Error 4: método `target_url()` en Link<Expired>

```text
error[E0599]: no method named `target_url` found for struct `Link<Expired>`
  --> crates/server/src/handlers/redirect.rs:23:20
   |
23 |     let url = link.target_url();
   |                    ^^^^^^^^^^ method not found in `Link<Expired>`

CAUSA: intentas redirigir con un link expirado.
SOLUCIÓN: verificar el estado antes — el typestate hace imposible llamar
          target_url() en un link expirado. Revisa la lógica de activación.
```

---

## `deny.toml` — política de supply chain

```toml
# cargo deny check: licencias, fuentes, crates prohibidos

[licenses]
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "ISC", "BSD-3-Clause"]
deny  = ["GPL-2.0", "GPL-3.0", "AGPL-3.0"]

[bans]
multiple-versions = "warn"
deny = [
    { name = "openssl",  reason = "usar rustls" },
    { name = "openssl-sys", reason = "usar rustls" },
]

[sources]
unknown-registry = "deny"
unknown-git      = "warn"
```

---

## ✅ Checklist de la Semana 22

- [ ] El workspace `Cargo.toml` raíz tiene `resolver = "2"`, `workspace.dependencies`
  para TODAS las crates compartidas y `workspace.lints` con clippy pedantic.
- [ ] `crates/core` compila con `no_std` (al menos `cfg_attr(not(test), no_std)`).
  Verificación: `cargo build -p linkmetrics-core --target thumbv7m-none-eabi`.
- [ ] El dominio usa typestates: `Link<Draft>`, `Link<Active>`, `Link<Expired>`.
  `Link<Expired>` no tiene el método `target_url()`.
- [ ] `validate_code` y `validate_url` tienen tests `proptest` con al menos 3 estrategias
  cada una (válidos, inválidos por longitud, inválidos por charset).
- [ ] `cargo test -p linkmetrics-core` pasa 0 warnings, 0 errores, ≥ 15 tests.
- [ ] Las migraciones SQL están en `crates/db/migrations/` y son idempotentes
  (`CREATE TABLE IF NOT EXISTS`, `CREATE INDEX IF NOT EXISTS`).
- [ ] `PgLinkRepo` y `PgClickStore` usan únicamente `query!`, `query_as!` o
  `query_scalar!`. Ningún `format!()` mezclado con SQL.
- [ ] `cargo sqlx prepare --workspace` genera archivos en `.sqlx/` y estos están
  en el repositorio (`.gitignore` NO los excluye).
- [ ] `SQLX_OFFLINE=true cargo build --workspace` compila sin errores.
- [ ] `docker compose up -d` arranca PostgreSQL y Redis con healthchecks que pasan
  antes de que el servicio sea considerado healthy.
- [ ] La imagen Docker multi-stage compila: `docker build -f infra/Dockerfile .`
  La imagen final pesa menos de 30 MB: `docker image ls | grep linkmetrics`.
- [ ] `cargo audit` no reporta vulnerabilidades críticas o altas.
- [ ] `cargo deny check` pasa con las políticas de licencias definidas.
- [ ] El CLI `lm migrate` ejecuta las migraciones contra la DB de dev sin error.
- [ ] El CLI `lm check` reporta `postgres: ✓ OK` con Docker Compose corriendo.

> **Siguiente sección:** [Semana 23 — Servidor HTTP, autenticación y tests E2E](section_03.md)
