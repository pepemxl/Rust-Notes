# Patrones de diseño: modelado de estado y arquitectura

La Semana 17 introduce los patrones que separan un Rustacean principiante de uno
senior: usar el sistema de tipos para que los estados inválidos sean
**inexpresables** en lugar de protegerlos con ifs y flags en runtime.

En esta sección aprenderemos:

- **Newtype Pattern**: tipos fuertes para IDs, emails y unidades; Orphan Rule.
- **Builder Pattern con Typestate**: campos requeridos verificados en compilación.
- **Typestate Pattern**: el estado vive en el parámetro genérico, no en un enum.
  `PhantomData<S>` = cero bytes en runtime, máxima garantía en compilación.
- **Actor Model con Tokio**: `mpsc::channel` + `tokio::spawn` como alternativa
  a `Arc<Mutex<T>>`. Estado privado, concurrencia por mensajes.
- **Dependency Injection**: dispatch estático (generics) vs dinámico (`dyn Trait`).
  Configuración multicapa con `figment`.
- **Proyecto**: URL Shortener v3 — refactorización completa con Typestate URLs,
  actor contador sharded y DI por traits.

> *"Hacer imposibles los estados inválidos" no es solo un eslogan de marketing —
> es la única forma de eliminar una categoría entera de bugs sin un solo test.
> En Rust, el compilador puede ser tu QA más exigente si diseñas bien los tipos.*
> — Yaron Minsky (adaptado al mundo Rust)

---

## El problema que resuelven estos patrones

```text
SIN PATRONES (estilo "traducción directa de Java/Python"):

struct UrlEntry {
    code:    String,
    target:  String,
    estado:  String,   // "draft" | "active" | "expired"
    clicks:  u64,
}

impl UrlEntry {
    fn click(&mut self) {
        if self.estado != "active" {   // verificación en runtime
            panic!("¡URL no activa!");  // error descubierto en producción
        }
        self.clicks += 1;
    }
}

CON PATRONES RUST (estilo senior):

struct UrlEntry<S> { code: String, target: String, clicks: u64, _s: PhantomData<S> }

impl UrlEntry<Active> {
    fn click(&mut self) { self.clicks += 1; }  // no hay if; el tipo garantiza
}

// UrlEntry<Draft>::click() → ERROR EN COMPILACIÓN, no en producción
```

---

## 1. Newtype Pattern

Envuelve un tipo primitivo en una struct de un campo. El resultado es un tipo
nuevo, incompatible con el original, que puede tener su propia lógica de
validación e implementar traits externos.

```rust
use std::fmt;

// Cada dominio tiene su propio tipo de ID: no se confunden entre sí
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UrlCode(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(u64);

// Email: validación en construcción, imposible tener un Email inválido
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    pub fn parse(s: impl Into<String>) -> Result<Self, String> {
        let s = s.into();
        if s.contains('@') && s.len() > 3 {
            Ok(Email(s))
        } else {
            Err(format!("email inválido: '{s}'"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// UrlCode: generación y validación
impl UrlCode {
    pub fn nueva() -> Self {
        // En producción usaríamos uuid o nanoid
        UrlCode("abc123".to_string())
    }

    pub fn parse(s: impl Into<String>) -> Result<Self, String> {
        let s = s.into();
        if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') && !s.is_empty() {
            Ok(UrlCode(s))
        } else {
            Err(format!("código inválido: '{s}'"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UrlCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Orphan Rule: puedes implementar tus traits sobre tipos ajenos,
// pero no traits ajenos sobre tipos ajenos.
// Newtype resuelve esto: defines el tipo tú → puedes implementar cualquier trait.
//
// impl serde::Serialize for String { ... }  // ❌ E0210: orphan rule
// impl serde::Serialize for Email  { ... }  // ✅ Email es TUYO
```

### Tabla: cuándo usar Newtype

| Caso | Ejemplo | Beneficio |
|------|---------|-----------|
| IDs distintos que no deben mezclarse | `UserId(u64)`, `ProductId(u64)` | Imposible pasar `UserId` donde se espera `ProductId` |
| Validación en construcción | `Email(String)`, `Url(String)` | El tipo es la prueba de validez |
| Unidades físicas incompatibles | `Metros(f64)`, `Segundos(f64)` | Sin suma metros + segundos |
| Orphan Rule | `MiJson(serde_json::Value)` | Implementar traits externos sobre tipos ajenos |
| Ocultar representación interna | `Token(String)` con Display que imprime `***` | Encapsulación real |

---

## 2. Builder Pattern con Typestate

El Builder clásico descubre errores (campos faltantes) en runtime con `unwrap()`.
El Builder con Typestate los descubre **en compilación**:

```text
BUILDER CLÁSICO (error en runtime):

let cfg = Config::builder()
    .timeout(30)
    // .url("...")  ← olvidado
    .build()        // panic! o Err en runtime
    
BUILDER TYPESTATE (error en compilación):

let cfg = ConfigBuilder::new()
    .timeout(Duration::from_secs(30))
    // .url(...)  ← falta
    .build()
// error[E0599]: no method named `build` found for struct `ConfigBuilder<Unset, Set>`
//       |        ^^^^^ method not found in `ConfigBuilder<Unset, Set>`
```

### Implementación

```rust
use std::marker::PhantomData;
use std::time::Duration;

// Marcadores de estado: son unit structs (0 bytes)
pub struct Unset;
pub struct Set;

// El tipo lleva en sus parámetros genéricos qué campos están configurados
pub struct ConfigBuilder<HasUrl, HasTimeout> {
    url:     Option<String>,
    timeout: Option<Duration>,
    reintentos: u32,
    _estado: PhantomData<(HasUrl, HasTimeout)>,
}

// Estado inicial: ningún campo requerido está puesto
impl ConfigBuilder<Unset, Unset> {
    pub fn new() -> Self {
        ConfigBuilder {
            url:        None,
            timeout:    None,
            reintentos: 3,
            _estado:    PhantomData,
        }
    }
}

// Solo se puede llamar url() si HasUrl = Unset
impl<HasTimeout> ConfigBuilder<Unset, HasTimeout> {
    pub fn url(self, url: impl Into<String>) -> ConfigBuilder<Set, HasTimeout> {
        ConfigBuilder {
            url:        Some(url.into()),
            timeout:    self.timeout,
            reintentos: self.reintentos,
            _estado:    PhantomData,
        }
    }
}

// Solo se puede llamar timeout() si HasTimeout = Unset
impl<HasUrl> ConfigBuilder<HasUrl, Unset> {
    pub fn timeout(self, t: Duration) -> ConfigBuilder<HasUrl, Set> {
        ConfigBuilder {
            url:        self.url,
            timeout:    Some(t),
            reintentos: self.reintentos,
            _estado:    PhantomData,
        }
    }
}

// Campo opcional: disponible en cualquier estado
impl<HasUrl, HasTimeout> ConfigBuilder<HasUrl, HasTimeout> {
    pub fn reintentos(mut self, n: u32) -> Self {
        self.reintentos = n;
        self
    }
}

// build() solo existe cuando AMBOS campos requeridos están configurados
impl ConfigBuilder<Set, Set> {
    pub fn build(self) -> Config {
        Config {
            url:        self.url.unwrap(),      // unwrap SEGURO: el tipo lo garantiza
            timeout:    self.timeout.unwrap(),
            reintentos: self.reintentos,
        }
    }
}

#[derive(Debug)]
pub struct Config {
    pub url:        String,
    pub timeout:    Duration,
    pub reintentos: u32,
}

// Uso correcto:
// let cfg = ConfigBuilder::new()
//     .url("https://api.ejemplo.com")
//     .timeout(Duration::from_secs(30))
//     .reintentos(5)
//     .build();
//
// Uso incorrecto — falla en compilación:
// let cfg = ConfigBuilder::new()
//     .url("https://api.ejemplo.com")
//     .build();  // ❌ E0599: build no existe en ConfigBuilder<Set, Unset>
```

---

## 3. Typestate Pattern

El Typestate es el Builder llevado al extremo: el estado de un objeto **es** su tipo.
El compilador verifica que solo llames los métodos válidos para cada estado.

```text
MÁQUINA DE ESTADOS TYPESTATE PARA URL SHORTENER

                 ┌──────────────┐
                 │  Url<Draft>  │  new(code, target)
                 │              │
                 │  info()      │  ← solo lectura
                 └──────┬───────┘
                        │ publish()  — consume el Draft, devuelve Active
                        ▼
                 ┌──────────────┐
                 │ Url<Active>  │
                 │              │
                 │  click()  ←─┤  modifica &mut self
                 │  info()      │  ← solo lectura
                 └──────┬───────┘
                        │ expire()  — consume el Active, devuelve Expired
                        ▼
                 ┌──────────────┐
                 │ Url<Expired> │
                 │              │
                 │  info()      │  ← solo lectura
                 │  clicks_     │
                 │  finales()   │
                 └──────────────┘

GARANTÍAS DEL COMPILADOR:
  ❌ Url<Draft>::click()   → E0599: no method `click` for `Url<Draft>`
  ❌ Url<Expired>::click() → E0599: no method `click` for `Url<Expired>`
  ❌ Url<Active>::publish()→ E0599: no method `publish` for `Url<Active>`
  ✅ Cero overhead: PhantomData<S> = 0 bytes
```

### Implementación completa

```rust
use std::marker::PhantomData;
use std::time::SystemTime;

// Marcadores de estado (zero-sized types)
pub struct Draft;
pub struct Active;
pub struct Expired;

// La struct tiene el estado en el tipo, no en un campo enum
#[derive(Debug)]
pub struct UrlEntry<S> {
    pub code:    UrlCode,
    pub target:  String,
    pub clicks:  u64,
    creada_en:   SystemTime,
    _estado:     PhantomData<S>,
}

// Solo Draft puede crearse desde cero
impl UrlEntry<Draft> {
    pub fn nueva(code: UrlCode, target: impl Into<String>) -> Self {
        UrlEntry {
            code,
            target:   target.into(),
            clicks:   0,
            creada_en: SystemTime::now(),
            _estado:  PhantomData,
        }
    }

    // Consume self (Draft desaparece) y retorna Active
    pub fn publicar(self) -> UrlEntry<Active> {
        UrlEntry {
            code:     self.code,
            target:   self.target,
            clicks:   0,
            creada_en: self.creada_en,
            _estado:  PhantomData,
        }
    }
}

// Solo Active puede recibir clicks o expirar
impl UrlEntry<Active> {
    pub fn registrar_click(&mut self) {
        self.clicks += 1;
    }

    // Consume Active y retorna Expired
    pub fn expirar(self) -> UrlEntry<Expired> {
        UrlEntry {
            code:     self.code,
            target:   self.target,
            clicks:   self.clicks,
            creada_en: self.creada_en,
            _estado:  PhantomData,
        }
    }

    pub fn url_destino(&self) -> &str {
        &self.target
    }
}

// Expired: solo lectura
impl UrlEntry<Expired> {
    pub fn clicks_finales(&self) -> u64 {
        self.clicks
    }

    pub fn url_destino(&self) -> Option<&str> {
        None  // Las URLs expiradas ya no redirigen
    }
}

// Método disponible en TODOS los estados
impl<S> UrlEntry<S> {
    pub fn codigo(&self) -> &UrlCode {
        &self.code
    }

    pub fn creada_en(&self) -> SystemTime {
        self.creada_en
    }
}

// Para el almacenamiento necesitamos un tipo borrado (sin parámetro genérico)
// porque no podemos tener Vec<UrlEntry<?>>
#[derive(Debug)]
pub enum EstadoUrl {
    Draft(UrlEntry<Draft>),
    Active(UrlEntry<Active>),
    Expired(UrlEntry<Expired>),
}

impl EstadoUrl {
    pub fn codigo(&self) -> &UrlCode {
        match self {
            EstadoUrl::Draft(u)   => u.codigo(),
            EstadoUrl::Active(u)  => u.codigo(),
            EstadoUrl::Expired(u) => u.codigo(),
        }
    }

    pub fn url_destino_activa(&self) -> Option<&str> {
        match self {
            EstadoUrl::Active(u) => Some(u.url_destino()),
            _ => None,
        }
    }

    pub fn publicar(self) -> Self {
        match self {
            EstadoUrl::Draft(u) => EstadoUrl::Active(u.publicar()),
            otro => otro,
        }
    }

    pub fn expirar(self) -> Self {
        match self {
            EstadoUrl::Active(u) => EstadoUrl::Expired(u.expirar()),
            otro => otro,
        }
    }

    pub fn registrar_click(&mut self) {
        if let EstadoUrl::Active(u) = self {
            u.registrar_click();
        }
    }
}
```

### Prueba de cero overhead

```rust
use std::mem::size_of;

fn verificar_tamanos() {
    // PhantomData<S> no ocupa espacio: todas las versiones miden igual
    assert_eq!(
        size_of::<UrlEntry<Draft>>(),
        size_of::<UrlEntry<Active>>()
    );
    assert_eq!(
        size_of::<UrlEntry<Active>>(),
        size_of::<UrlEntry<Expired>>()
    );
    // PhantomData<()> también es 0 bytes
    assert_eq!(size_of::<PhantomData<Draft>>(), 0);
}
```

---

## 4. Actor Model con Tokio

En lugar de `Arc<Mutex<HashMap<Code, u64>>>`, cada actor posee su estado
**en exclusiva** y se comunica únicamente por mensajes:

```text
MODELO ACTOR vs MUTEX

  ┌─────────────────────────────────────────────────────────────────────┐
  │  CON Mutex                                                          │
  │                                                                     │
  │  Thread A ──────────► lock() ──► actualiza HashMap ──► unlock()    │
  │  Thread B ──────────► lock() ──► ESPERA (bloqueado) ──────────────►│
  │  Thread C ──────────► lock() ──► ESPERA ────────────────────────►  │
  │                                                                     │
  │  Problemas: contención, inversión de prioridades, deadlock potencial│
  └─────────────────────────────────────────────────────────────────────┘

  ┌─────────────────────────────────────────────────────────────────────┐
  │  CON ACTOR                                                          │
  │                                                                     │
  │  CounterHandle  ──Msg::Increment──►  ┌──────────────────────────┐  │
  │  (clonable)     ──Msg::Get(reply)──► │   ClickCounter (task)    │  │
  │  (Send)         ──Msg::Increment──► │   count: u64  (privado)  │  │
  │                                      │   rx.recv().await        │  │
  │  Cualquier tarea puede enviar         └──────────────────────────┘  │
  │  mensajes sin bloquear                    ▲                          │
  │                                      tokio::spawn                   │
  │  No hay lock. No hay deadlock.                                       │
  │  El estado es PRIVADO del actor.                                     │
  └─────────────────────────────────────────────────────────────────────┘
```

### Actor básico: contador de clicks

```rust
use tokio::sync::{mpsc, oneshot};

// El protocolo de comunicación del actor
pub enum MensajeContador {
    Incrementar(UrlCode),
    Obtener { codigo: UrlCode, respuesta: oneshot::Sender<u64> },
    ObtenerTodos(oneshot::Sender<std::collections::HashMap<String, u64>>),
    Detener,
}

// El actor mismo: struct privada, nadie más la ve
struct ContadorActor {
    conteos: std::collections::HashMap<String, u64>,
    rx:      mpsc::Receiver<MensajeContador>,
}

impl ContadorActor {
    fn nuevo(rx: mpsc::Receiver<MensajeContador>) -> Self {
        ContadorActor {
            conteos: std::collections::HashMap::new(),
            rx,
        }
    }

    async fn ejecutar(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                MensajeContador::Incrementar(code) => {
                    *self.conteos.entry(code.as_str().to_string()).or_default() += 1;
                }
                MensajeContador::Obtener { codigo, respuesta } => {
                    let n = self.conteos.get(codigo.as_str()).copied().unwrap_or(0);
                    let _ = respuesta.send(n);
                }
                MensajeContador::ObtenerTodos(respuesta) => {
                    let _ = respuesta.send(self.conteos.clone());
                }
                MensajeContador::Detener => break,
            }
        }
    }
}

// El handle: lo que el resto del sistema ve
#[derive(Clone)]
pub struct ContadorHandle {
    tx: mpsc::Sender<MensajeContador>,
}

impl ContadorHandle {
    pub fn iniciar() -> Self {
        let (tx, rx) = mpsc::channel(256);  // bounded: backpressure automático
        let actor = ContadorActor::nuevo(rx);
        tokio::spawn(actor.ejecutar());
        ContadorHandle { tx }
    }

    pub async fn incrementar(&self, code: UrlCode) {
        // fire-and-forget; si el canal está lleno hay backpressure implícito
        let _ = self.tx.send(MensajeContador::Incrementar(code)).await;
    }

    pub async fn obtener(&self, codigo: &UrlCode) -> u64 {
        let (tx, rx) = oneshot::channel();
        let msg = MensajeContador::Obtener {
            codigo: codigo.clone(),
            respuesta: tx,
        };
        if self.tx.send(msg).await.is_err() {
            return 0;
        }
        rx.await.unwrap_or(0)
    }

    pub async fn todos_los_conteos(&self) -> std::collections::HashMap<String, u64> {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(MensajeContador::ObtenerTodos(tx)).await.is_err() {
            return Default::default();
        }
        rx.await.unwrap_or_default()
    }
}
```

### Actor sharded: escala horizontal

Un solo actor se convierte en cuello de botella con muchas escrituras concurrentes.
La solución: N actores, cada URL se dirige al actor `hash(code) % N`:

```rust
/// Sharded counter: N actores, cada uno gestiona 1/N de las URLs.
/// El sharding elimina la contención de mailbox para cargas de alta escritura.
#[derive(Clone)]
pub struct ContadorSharded {
    shards: Vec<ContadorHandle>,
}

impl ContadorSharded {
    pub fn iniciar(n_shards: usize) -> Self {
        let shards = (0..n_shards).map(|_| ContadorHandle::iniciar()).collect();
        ContadorSharded { shards }
    }

    fn shard_para(&self, code: &UrlCode) -> &ContadorHandle {
        let idx = self.hash_shard(code.as_str());
        &self.shards[idx]
    }

    fn hash_shard(&self, s: &str) -> usize {
        // FNV-1a manual (no requiere dependencia)
        let h = s.bytes().fold(2166136261u64, |acc, b| {
            (acc ^ b as u64).wrapping_mul(16777619)
        });
        (h as usize) % self.shards.len()
    }

    pub async fn incrementar(&self, code: UrlCode) {
        self.shard_para(&code).incrementar(code).await;
    }

    pub async fn obtener(&self, codigo: &UrlCode) -> u64 {
        self.shard_para(codigo).obtener(codigo).await
    }
}
```

---

## 5. Dependency Injection

### DI estático: generics (preferido)

```rust
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait AlmacenUrls: Send + Sync {
    async fn guardar(&self, url: &EstadoUrl) -> Result<(), String>;
    async fn obtener(&self, code: &UrlCode) -> Option<EstadoUrl>;
    async fn actualizar_estado(&self, code: &UrlCode, url: EstadoUrl) -> Result<(), String>;
}

// DI ESTÁTICO: el tipo del almacen queda fijo en compilación
// Ventaja: cero overhead de dispatch, el compilador puede inline todo
pub struct ServicioUrls<A: AlmacenUrls> {
    almacen:  A,
    contador: ContadorSharded,
}

impl<A: AlmacenUrls> ServicioUrls<A> {
    pub fn nuevo(almacen: A, contador: ContadorSharded) -> Self {
        ServicioUrls { almacen, contador }
    }

    pub async fn crear_url(
        &self,
        code: UrlCode,
        target: String,
    ) -> Result<(), String> {
        let entrada = UrlEntry::<Draft>::nueva(code, target);
        let estado  = EstadoUrl::Active(entrada.publicar());
        self.almacen.guardar(&estado).await
    }

    pub async fn redirigir(&self, code: &UrlCode) -> Option<String> {
        let mut url = self.almacen.obtener(code).await?;
        url.registrar_click();
        let destino = url.url_destino_activa()?.to_string();
        self.contador.incrementar(code.clone()).await;
        let _ = self.almacen.actualizar_estado(code, url).await;
        Some(destino)
    }
}

// DI DINÁMICO: útil para tests con mocks o plugins en runtime
// Desventaja: vtable dispatch (~1-3 ns extra por llamada)
pub struct ServicioUrlsDyn {
    almacen:  Arc<dyn AlmacenUrls>,
    contador: ContadorSharded,
}
```

### Comparación

```text
┌──────────────────────┬────────────────────────┬──────────────────────┐
│ Criterio             │ Generics (estático)     │ dyn Trait (dinámico) │
├──────────────────────┼────────────────────────┼──────────────────────┤
│ Dispatch             │ Monomorphized (inline)  │ Vtable (~3 ns)       │
│ Tipo en compilación  │ Fijo                    │ Borrado              │
│ Binario              │ +tamaño (por instancia) │ Compartido           │
│ Tests con mock       │ Impl el trait           │ Box<dyn> o Arc<dyn>  │
│ Plugins/carga dinámica│ ❌ imposible            │ ✅ natural            │
│ Recomendación        │ Hot paths, bibliotecas  │ Handlers/app layer   │
└──────────────────────┴────────────────────────┴──────────────────────┘
```

---

## 6. Configuración multicapa con `figment`

`figment` aplica capas de configuración: valores por defecto → archivo → variables
de entorno → flags CLI. Cada capa sobreescribe solo los campos que define:

```rust
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub host:           String,
    pub port:           u16,
    pub database_url:   String,
    pub max_urls:       usize,
    #[serde(with = "figment::value::magic::RelativePathBuf", default)]
    pub log_level:      String,
    pub actor_shards:   usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host:         "0.0.0.0".to_string(),
            port:         8080,
            database_url: "sqlite::memory:".to_string(),
            max_urls:     100_000,
            log_level:    "info".to_string(),
            actor_shards: 16,
        }
    }
}

impl Config {
    pub fn cargar() -> Result<Self, figment::Error> {
        use figment::{Figment, providers::{Env, Format, Serialized, Toml}};

        Figment::new()
            .merge(Serialized::defaults(Config::default()))  // 1. defaults
            .merge(Toml::file("config.toml"))                // 2. archivo (opcional)
            .merge(Env::prefixed("APP_"))                    // 3. APP_PORT=9090, etc.
            .extract()
    }
}

// config.toml (ejemplo):
// port = 9090
// actor_shards = 32
//
// Variables de entorno (prioridad máxima):
// APP_DATABASE_URL=postgres://... APP_PORT=443 cargo run
```

---

## Proyecto: URL Shortener v3

Refactoriza el URL Shortener de las Semanas 10 y 11 con la arquitectura aprendida
esta semana: Typestate para URLs, Actor sharded para clicks, DI por traits.

### Estructura

```
url_shortener_v3/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── config.rs
    ├── domain/
    │   ├── mod.rs
    │   └── url_entry.rs   ← Typestate
    ├── actor/
    │   ├── mod.rs
    │   └── contador.rs    ← Actor sharded
    ├── store/
    │   ├── mod.rs
    │   └── memoria.rs     ← impl AlmacenUrls en DashMap
    └── api/
        ├── mod.rs
        ├── estado.rs      ← AppState
        └── handlers.rs    ← Axum handlers
```

### `Cargo.toml`

```toml
[package]
name    = "url-shortener-v3"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio        = { version = "1", features = ["full"] }
axum         = "0.7"
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
async-trait  = "0.1"
dashmap      = "6"
thiserror    = "2"
anyhow       = "1"
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tokio-test  = "0.4"
```

### `src/domain/url_entry.rs`

```rust
use std::marker::PhantomData;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UrlCode(pub String);

impl UrlCode {
    pub fn nueva_aleatoria() -> Self {
        use std::time::UNIX_EPOCH;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        UrlCode(format!("{nanos:x}"))
    }

    pub fn parse(s: impl Into<String>) -> Result<Self, String> {
        let s = s.into();
        if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && !s.is_empty()
            && s.len() <= 32
        {
            Ok(UrlCode(s))
        } else {
            Err(format!("código de URL inválido: '{s}'"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for UrlCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Marcadores de estado ──────────────────────────────────────────────────

pub struct Draft;
pub struct Active;
pub struct Expired;

// ── Struct principal ──────────────────────────────────────────────────────

#[derive(Debug)]
pub struct UrlEntry<S> {
    pub code:   UrlCode,
    pub target: String,
    pub clicks: u64,
    _estado:    PhantomData<S>,
}

impl UrlEntry<Draft> {
    pub fn nueva(code: UrlCode, target: impl Into<String>) -> Self {
        UrlEntry { code, target: target.into(), clicks: 0, _estado: PhantomData }
    }

    pub fn publicar(self) -> UrlEntry<Active> {
        UrlEntry { code: self.code, target: self.target, clicks: 0, _estado: PhantomData }
    }
}

impl UrlEntry<Active> {
    pub fn registrar_click(&mut self) {
        self.clicks += 1;
    }

    pub fn url_destino(&self) -> &str {
        &self.target
    }

    pub fn expirar(self) -> UrlEntry<Expired> {
        UrlEntry { code: self.code, target: self.target, clicks: self.clicks, _estado: PhantomData }
    }
}

impl UrlEntry<Expired> {
    pub fn clicks_finales(&self) -> u64 {
        self.clicks
    }
}

impl<S> UrlEntry<S> {
    pub fn codigo(&self) -> &UrlCode { &self.code }
}

// ── Tipo borrado para almacenamiento ─────────────────────────────────────

#[derive(Debug)]
pub enum EntradaUrl {
    Draft(UrlEntry<Draft>),
    Active(UrlEntry<Active>),
    Expired(UrlEntry<Expired>),
}

impl EntradaUrl {
    pub fn nueva_activa(code: UrlCode, target: impl Into<String>) -> Self {
        let draft = UrlEntry::<Draft>::nueva(code, target);
        EntradaUrl::Active(draft.publicar())
    }

    pub fn url_destino_activa(&self) -> Option<&str> {
        match self {
            EntradaUrl::Active(u) => Some(u.url_destino()),
            _ => None,
        }
    }

    pub fn registrar_click(&mut self) -> bool {
        match self {
            EntradaUrl::Active(u) => { u.registrar_click(); true }
            _ => false,
        }
    }

    pub fn expirar(self) -> Self {
        match self {
            EntradaUrl::Active(u) => EntradaUrl::Expired(u.expirar()),
            otro => otro,
        }
    }

    pub fn codigo(&self) -> &UrlCode {
        match self {
            EntradaUrl::Draft(u)   => u.codigo(),
            EntradaUrl::Active(u)  => u.codigo(),
            EntradaUrl::Expired(u) => u.codigo(),
        }
    }

    pub fn clicks(&self) -> u64 {
        match self {
            EntradaUrl::Draft(u)   => u.clicks,
            EntradaUrl::Active(u)  => u.clicks,
            EntradaUrl::Expired(u) => u.clicks,
        }
    }
}
```

### `src/actor/contador.rs`

```rust
use crate::domain::url_entry::UrlCode;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

enum Mensaje {
    Incrementar(UrlCode),
    Obtener { codigo: UrlCode, tx: oneshot::Sender<u64> },
    Snapshot(oneshot::Sender<HashMap<String, u64>>),
}

struct ContadorInterno {
    conteos: HashMap<String, u64>,
    rx:      mpsc::Receiver<Mensaje>,
}

impl ContadorInterno {
    async fn ejecutar(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                Mensaje::Incrementar(code) => {
                    *self.conteos.entry(code.0).or_default() += 1;
                }
                Mensaje::Obtener { codigo, tx } => {
                    let n = self.conteos.get(&codigo.0).copied().unwrap_or(0);
                    let _ = tx.send(n);
                }
                Mensaje::Snapshot(tx) => {
                    let _ = tx.send(self.conteos.clone());
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct ContadorHandle {
    tx: mpsc::Sender<Mensaje>,
}

impl ContadorHandle {
    fn iniciar() -> Self {
        let (tx, rx) = mpsc::channel(512);
        let actor = ContadorInterno { conteos: HashMap::new(), rx };
        tokio::spawn(actor.ejecutar());
        ContadorHandle { tx }
    }

    async fn incrementar(&self, code: UrlCode) {
        let _ = self.tx.send(Mensaje::Incrementar(code)).await;
    }

    async fn obtener(&self, codigo: &UrlCode) -> u64 {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.send(Mensaje::Obtener { codigo: codigo.clone(), tx }).await;
        rx.await.unwrap_or(0)
    }
}

// ── Sharded: N actores para alta concurrencia ─────────────────────────────

#[derive(Clone)]
pub struct ContadorSharded {
    shards: Vec<ContadorHandle>,
}

impl ContadorSharded {
    pub fn iniciar(n: usize) -> Self {
        let shards = (0..n.max(1)).map(|_| ContadorHandle::iniciar()).collect();
        ContadorSharded { shards }
    }

    fn shard(&self, code: &UrlCode) -> &ContadorHandle {
        let h = code.0.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        &self.shards[(h as usize) % self.shards.len()]
    }

    pub async fn incrementar(&self, code: UrlCode) {
        self.shard(&code).incrementar(code).await;
    }

    pub async fn obtener(&self, codigo: &UrlCode) -> u64 {
        self.shard(codigo).obtener(codigo).await
    }

    pub async fn snapshot(&self) -> HashMap<String, u64> {
        let mut total = HashMap::new();
        for shard in &self.shards {
            let (tx, rx) = oneshot::channel();
            let _ = shard.tx.send(Mensaje::Snapshot(tx)).await;
            if let Ok(mapa) = rx.await {
                for (k, v) in mapa {
                    *total.entry(k).or_default() += v;
                }
            }
        }
        total
    }
}
```

### `src/store/memoria.rs`

```rust
use crate::domain::url_entry::{EntradaUrl, UrlCode};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

#[async_trait]
pub trait AlmacenUrls: Send + Sync {
    async fn guardar(&self, url: EntradaUrl) -> Result<(), String>;
    async fn obtener(&self, code: &UrlCode) -> Option<EntradaUrl>;
    async fn registrar_click(&self, code: &UrlCode) -> bool;
    async fn total_urls(&self) -> usize;
}

#[derive(Clone, Default)]
pub struct AlmacenMemoria {
    mapa: Arc<DashMap<String, EntradaUrl>>,
}

#[async_trait]
impl AlmacenUrls for AlmacenMemoria {
    async fn guardar(&self, url: EntradaUrl) -> Result<(), String> {
        let code = url.codigo().0.clone();
        if self.mapa.contains_key(&code) {
            return Err(format!("código '{code}' ya existe"));
        }
        self.mapa.insert(code, url);
        Ok(())
    }

    async fn obtener(&self, code: &UrlCode) -> Option<EntradaUrl> {
        // Nota: no podemos devolver referencia al interior de DashMap fácilmente
        // sin clonar, así que definimos clone en EntradaUrl o usamos Ref guard.
        // Para simplificar, tomamos el valor y lo re-insertamos.
        // En producción usaríamos Arc<RwLock<EntradaUrl>> o Ref guard.
        self.mapa.get(&code.0).map(|r| {
            let e = r.value();
            // Construcción espejo (sin clone en traits, usamos helper)
            match e {
                EntradaUrl::Active(u) => {
                    EntradaUrl::nueva_activa(u.code.clone(), u.target.clone())
                }
                _ => EntradaUrl::nueva_activa(
                    e.codigo().clone(),
                    "expired".to_string(),
                ),
            }
        })
    }

    async fn registrar_click(&self, code: &UrlCode) -> bool {
        if let Some(mut entrada) = self.mapa.get_mut(&code.0) {
            entrada.registrar_click()
        } else {
            false
        }
    }

    async fn total_urls(&self) -> usize {
        self.mapa.len()
    }
}
```

### `src/api/handlers.rs`

```rust
use crate::{actor::contador::ContadorSharded, domain::url_entry::UrlCode};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Redirect},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::super::store::memoria::AlmacenUrls;

#[derive(Clone)]
pub struct AppState {
    pub almacen: Arc<dyn AlmacenUrls>,
    pub contador: ContadorSharded,
}

#[derive(Deserialize)]
pub struct CrearUrlRequest {
    pub target:  String,
    pub code:    Option<String>,
}

#[derive(Serialize)]
pub struct CrearUrlResponse {
    pub code:       String,
    pub short_url:  String,
}

pub async fn crear_url(
    State(state): State<AppState>,
    Json(body): Json<CrearUrlRequest>,
) -> impl IntoResponse {
    use crate::domain::url_entry::{EntradaUrl, UrlCode};

    let code = match body.code {
        Some(c) => match UrlCode::parse(c) {
            Ok(c) => c,
            Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
        },
        None => UrlCode::nueva_aleatoria(),
    };

    let entrada = EntradaUrl::nueva_activa(code.clone(), &body.target);

    match state.almacen.guardar(entrada).await {
        Ok(_) => Json(CrearUrlResponse {
            short_url: format!("http://localhost:8080/{}", code.as_str()),
            code:      code.0,
        }).into_response(),
        Err(e) => (StatusCode::CONFLICT, e).into_response(),
    }
}

pub async fn redirigir(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> impl IntoResponse {
    let Ok(url_code) = UrlCode::parse(&code) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    // Registrar click en el almacen y en el actor sharded
    let hubo_click = state.almacen.registrar_click(&url_code).await;

    if !hubo_click {
        return StatusCode::NOT_FOUND.into_response();
    }

    state.contador.incrementar(url_code.clone()).await;

    // Obtener URL destino
    match state.almacen.obtener(&url_code).await {
        Some(entrada) if entrada.url_destino_activa().is_some() => {
            Redirect::temporary(entrada.url_destino_activa().unwrap()).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn estadisticas(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let total_urls  = state.almacen.total_urls().await;
    let conteos     = state.contador.snapshot().await;

    Json(serde_json::json!({
        "total_urls":  total_urls,
        "top_clicks":  conteos,
    }))
}
```

### `src/main.rs`

```rust
mod actor;
mod api;
mod domain;
mod store;

use actor::contador::ContadorSharded;
use api::handlers::{AppState, crear_url, estadisticas, redirigir};
use store::memoria::AlmacenMemoria;

use axum::{Router, routing::{get, post}};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let almacen = Arc::new(AlmacenMemoria::default());
    let contador = ContadorSharded::iniciar(16);  // 16 shards

    let estado = AppState { almacen, contador };

    let app = Router::new()
        .route("/url", post(crear_url))
        .route("/{code}", get(redirigir))
        .route("/admin/stats", get(estadisticas))
        .with_state(estado);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!("URL Shortener v3 escuchando en :8080");
    axum::serve(listener, app).await.unwrap();
}
```

---

## Tests

```rust
// tests/typestate_test.rs
use url_shortener_v3::domain::url_entry::{
    Active, Draft, EntradaUrl, Expired, UrlCode, UrlEntry,
};

#[test]
fn ciclo_de_vida_completo() {
    let code   = UrlCode::parse("mi-url").unwrap();
    let draft  = UrlEntry::<Draft>::nueva(code.clone(), "https://ejemplo.com");

    // Draft → Active
    let mut activa = draft.publicar();
    assert_eq!(activa.clicks, 0);
    assert_eq!(activa.url_destino(), "https://ejemplo.com");

    // Active: registrar clicks
    activa.registrar_click();
    activa.registrar_click();
    activa.registrar_click();
    assert_eq!(activa.clicks, 3);

    // Active → Expired
    let expirada = activa.expirar();
    assert_eq!(expirada.clicks_finales(), 3);
}

#[test]
fn tamanos_iguales_por_phantomdata() {
    use std::mem::size_of;
    assert_eq!(size_of::<UrlEntry<Draft>>(), size_of::<UrlEntry<Active>>());
    assert_eq!(size_of::<UrlEntry<Active>>(), size_of::<UrlEntry<Expired>>());
}

#[test]
fn tipo_borrado_registra_clicks() {
    let code   = UrlCode::parse("abc").unwrap();
    let mut e  = EntradaUrl::nueva_activa(code, "https://ejemplo.com");

    assert!(e.registrar_click());   // activa: OK
    assert_eq!(e.clicks(), 1);

    let expirada = e.expirar();
    // ya no es activa → registrar_click devuelve false
    // (no podemos mutar expirada.registrar_click() para probar false sin convertir)
    assert!(expirada.url_destino_activa().is_none());
}

#[test]
fn url_code_valida_caracteres() {
    assert!(UrlCode::parse("hola-mundo_123").is_ok());
    assert!(UrlCode::parse("").is_err());
    assert!(UrlCode::parse("url con espacio").is_err());
    assert!(UrlCode::parse("url/slash").is_err());
}

// tests/actor_test.rs
#[tokio::test]
async fn actor_sharded_contadores_correctos() {
    use url_shortener_v3::actor::contador::ContadorSharded;
    use url_shortener_v3::domain::url_entry::UrlCode;

    let contador = ContadorSharded::iniciar(4);
    let code_a   = UrlCode::parse("url-a").unwrap();
    let code_b   = UrlCode::parse("url-b").unwrap();

    // Incrementar desde múltiples tareas concurrentes
    let mut tareas = Vec::new();
    for _ in 0..10 {
        let c   = contador.clone();
        let ca  = code_a.clone();
        let cb  = code_b.clone();
        tareas.push(tokio::spawn(async move {
            c.incrementar(ca).await;
            c.incrementar(cb).await;
            c.incrementar(cb).await;
        }));
    }
    for t in tareas { t.await.unwrap(); }

    assert_eq!(contador.obtener(&code_a).await, 10);
    assert_eq!(contador.obtener(&code_b).await, 20);
}

#[tokio::test]
async fn snapshot_agrega_todos_los_shards() {
    use url_shortener_v3::actor::contador::ContadorSharded;
    use url_shortener_v3::domain::url_entry::UrlCode;

    let contador = ContadorSharded::iniciar(8);
    let codes: Vec<_> = (0..8)
        .map(|i| UrlCode::parse(format!("url-{i}")).unwrap())
        .collect();

    for code in &codes {
        contador.incrementar(code.clone()).await;
        contador.incrementar(code.clone()).await;
    }

    let snap = contador.snapshot().await;
    let total: u64 = snap.values().sum();
    assert_eq!(total, 16);  // 8 URLs × 2 clicks cada una
}
```

---

## Errores de compilación que el Typestate previene

```text
error[E0599]: no method named `registrar_click` found for struct
              `UrlEntry<Draft>` in the current scope
  --> src/main.rs:42:13
   |
42 |     draft.registrar_click();
   |           ^^^^^^^^^^^^^^^ method not found in `UrlEntry<Draft>`
   |
   = note: the method exists for `UrlEntry<Active>` but not for `UrlEntry<Draft>`

error[E0599]: no method named `publicar` found for struct
              `UrlEntry<Active>` in the current scope
  --> src/main.rs:48:14
   |
48 |     activa.publicar();
   |            ^^^^^^^^ method not found in `UrlEntry<Active>`

error[E0382]: use of moved value: `draft`
  --> src/main.rs:51:5
   |
45 |     let activa = draft.publicar();
   |                        --------- value moved here
51 |     draft.publicar();
   |     ^^^^^ value used here after move
   |
   = help: `publicar` consumes `self`, no hay segunda publicación
```

---

## ✅ Checklist de la Semana 17

- [ ] Creo tipos Newtype para IDs, emails y unidades: `UserId(u64)`, `Email(String)`,
  `Metros(f64)`. No mezclo tipos primitivos donde se necesitan tipos de dominio.
- [ ] El Builder con Typestate solo expone `build()` cuando todos los campos
  requeridos están configurados. El compilador rechaza `build()` con campos faltantes.
- [ ] `PhantomData<S>` tiene tamaño cero en runtime: `size_of::<UrlEntry<Draft>>()`
  es igual a `size_of::<UrlEntry<Active>>()`.
- [ ] El Typestate previene llamadas inválidas en compilación: `click()` en
  `Url<Draft>` o `Url<Expired>` es un error `E0599`, no un panic en producción.
- [ ] El Actor guarda su estado en variables locales del closure/struct. No usa
  `Arc<Mutex<T>>` en el hot path. El handle es `Clone + Send`.
- [ ] El canal del Actor es bounded (`mpsc::channel(N)`) para que haya backpressure
  automático cuando el actor no da abasto.
- [ ] `ContadorSharded` distribuye las escrituras entre N actores usando hash del
  código de URL. Elijo N basándome en el número de núcleos, no un valor arbitrario.
- [ ] DI estático (generics) en el núcleo de negocio; DI dinámico (`Arc<dyn Trait>`)
  en el estado de Axum para facilitar tests con mocks.
- [ ] `cargo test` pasa los 7 tests (5 de typestate + 2 de actor).

> **Siguiente sección:** [Semana 18 — Concurrencia avanzada y lock-free](section_02.md)
