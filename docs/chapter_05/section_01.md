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
- **Proyecto**: URL Shortener v4 — refactorización completa con Typestate URLs,
  actor contador sharded y DI por traits.

!!! quote ""

    *"Hacer imposibles los estados inválidos" no es solo un eslogan de marketing —
    es la única forma de eliminar una categoría entera de bugs sin un solo test.
    En Rust, el compilador puede ser tu QA más exigente si diseñas bien los tipos.*
    — Yaron Minsky (adaptado al mundo Rust)

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

// `async-trait` sigue siendo necesario aquí porque usamos `Arc<dyn AlmacenUrls>`:
// los `async fn` nativos en traits (Rust 1.75+) aún no son dyn-compatibles.
// Si solo usaras generics, bastaría con `async fn` nativo sin el atributo.
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

## Proyecto: URL Shortener v4

Refactoriza el URL Shortener de las Semanas 10–12 con la arquitectura aprendida
esta semana: Newtype para los códigos, Typestate para el ciclo de vida de cada URL,
un actor sharded para los clicks, DI por traits para el almacén y `figment` para la
configuración. El código completo está en
[`url_shortener_v4`](https://github.com/pepemxl/Rust-Notes/tree/master/src/chapter_05/url_shortener_v4).

| Ruta | Respuesta |
| :--- | :--- |
| `POST /url` | `201` con el código; `400` si el código es inválido; `409` si ya existe |
| `GET /{code}` | `307` al destino si la URL está activa; `404` si no existe o expiró |
| `DELETE /{code}` | `204` al pasar la URL a `Expired`; `404` si no estaba activa |
| `GET /admin/stats` | Total de URLs y clicks por código (leídos del actor) |

### Estructura

```text
url_shortener_v4/
├── Cargo.toml
├── src/
│   ├── lib.rs             ← módulos + router()
│   ├── main.rs            ← carga la config y arranca el servidor
│   ├── config.rs          ← figment: defaults → config.toml → APP_*
│   ├── domain/
│   │   └── url_entry.rs   ← Newtype + Typestate
│   ├── actor/
│   │   └── contador.rs    ← actor sharded
│   ├── store/
│   │   └── memoria.rs     ← trait AlmacenUrls + impl con DashMap
│   └── api/
│       ├── estado.rs      ← AppState (DI dinámico)
│       └── handlers.rs    ← handlers de Axum
└── tests/
    ├── typestate_test.rs
    ├── actor_test.rs
    └── api_test.rs
```

### `Cargo.toml`

```toml
--8<-- "src/chapter_05/url_shortener_v4/Cargo.toml"
```

### `src/lib.rs`

El `Router` vive en la librería, no en `main.rs`: así los tests de integración
pueden construirlo y llamarlo sin abrir un puerto.

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/lib.rs"
```

### `src/domain/url_entry.rs`

Dos diferencias con los ejemplos de arriba. El campo de `UrlCode` es **privado**:
fuera del módulo, la única forma de tener un `UrlCode` es `parse()`, así que tenerlo
ya prueba que es válido. Y los ejemplos que no deben compilar son doctests
` ```compile_fail `: `cargo test` falla si algún día **sí** compilan.

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/domain/url_entry.rs"
```

### `src/actor/contador.rs`

`ContadorHandle` y el protocolo `Mensaje` son privados; el resto del sistema solo ve
`ContadorSharded`. Como cada código va siempre al mismo shard, cada conteo vive en un
solo actor y `snapshot()` puede unir los mapas con `extend` sin sumar.

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/actor/contador.rs"
```

### `src/store/memoria.rs`

`EntradaUrl` es `Clone`, así que `obtener` devuelve una copia en lugar de mantener
bloqueado un shard del `DashMap`. Las operaciones que leen y escriben
(`guardar`, `registrar_click`, `expirar`) lo hacen bajo **un solo** guard, para que
otra petición no pueda colarse entre la comprobación y el cambio.

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/store/memoria.rs"
```

### `src/api/estado.rs` y `src/api/handlers.rs`

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/api/estado.rs"
```

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/api/handlers.rs"
```

### `src/config.rs`

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/config.rs"
```

### `src/main.rs`

```rust
--8<-- "src/chapter_05/url_shortener_v4/src/main.rs"
```

### Probarlo

```bash
cargo run                                   # o: APP_PORT=9090 APP_ACTOR_SHARDS=32 cargo run

curl -X POST localhost:8080/url -H 'content-type: application/json' \
     -d '{"target":"https://www.rust-lang.org","code":"rust"}'
curl -i localhost:8080/rust                 # redirige y cuenta el click
curl -X DELETE localhost:8080/rust          # Active → Expired
curl localhost:8080/admin/stats
```

```text
{"code":"rust","short_url":"http://localhost:8080/rust"}

HTTP/1.1 307 Temporary Redirect
location: https://www.rust-lang.org
content-length: 0

{"clicks":{"rust":1},"total_urls":1}
```

Repetir el `POST` devuelve `409` con `el código 'rust' ya existe`, y después del
`DELETE` el `GET /rust` responde `404`: la URL sigue guardada (cuenta en
`total_urls`), pero en estado `Expired` ya no tiene destino.

---

## Tests

`tests/typestate_test.rs` prueba el dominio sin runtime async:

```rust
--8<-- "src/chapter_05/url_shortener_v4/tests/typestate_test.rs"
```

`tests/actor_test.rs` necesita Tokio (`#[tokio::test]`) porque el actor es una tarea:

```rust
--8<-- "src/chapter_05/url_shortener_v4/tests/actor_test.rs"
```

`tests/api_test.rs` recorre la API completa con `tower::ServiceExt::oneshot`, que
llama al `Router` como a una función: sin puerto, sin `reqwest`, sin servidor.

```rust
--8<-- "src/chapter_05/url_shortener_v4/tests/api_test.rs"
```

---

## Errores de compilación que el Typestate previene

Esto es lo que responde el compilador (Rust 1.96) si intentas usar un estado de forma
inválida desde un binario que depende de la librería:

```rust
// ❌ NO COMPILA
use url_shortener_v4::domain::url_entry::{UrlCode, UrlEntry};

fn main() {
    let code = UrlCode::parse("abc").unwrap();
    let mut draft = UrlEntry::nueva(code, "https://ejemplo.com");
    draft.registrar_click();

    let activa = draft.publicar();
    activa.publicar();
}
```

Los dos errores son el mismo `E0599`: el método existe, pero para **otro** estado, y
la nota lo dice:

```text
error[E0599]: no method named `registrar_click` found for struct `UrlEntry<url_shortener_v4::domain::url_entry::Draft>` in the current scope
 --> src/main.rs:6:11
  |
6 |     draft.registrar_click();
  |           ^^^^^^^^^^^^^^^ method not found in `UrlEntry<url_shortener_v4::domain::url_entry::Draft>`
  |
  = note: the method was found for `UrlEntry<url_shortener_v4::domain::url_entry::Active>`

error[E0599]: no method named `publicar` found for struct `UrlEntry<url_shortener_v4::domain::url_entry::Active>` in the current scope
 --> src/main.rs:9:12
  |
9 |     activa.publicar();
  |            ^^^^^^^^ method not found in `UrlEntry<url_shortener_v4::domain::url_entry::Active>`
  |
  = note: the method was found for `UrlEntry<url_shortener_v4::domain::url_entry::Draft>`
```

Publicar dos veces el mismo `Draft` lo detecta el borrow checker: `publicar(self)`
consume el valor.

```rust
// ❌ NO COMPILA
use url_shortener_v4::domain::url_entry::{UrlCode, UrlEntry};

fn main() {
    let code = UrlCode::parse("abc").unwrap();
    let draft = UrlEntry::nueva(code, "https://ejemplo.com");
    let _activa = draft.publicar();
    let _otra = draft.publicar();
}
```

```text
error[E0382]: use of moved value: `draft`
 --> src/main.rs:7:17
  |
5 |     let draft = UrlEntry::nueva(code, "https://ejemplo.com");
  |         ----- move occurs because `draft` has type `UrlEntry<url_shortener_v4::domain::url_entry::Draft>`, which does not implement the `Copy` trait
6 |     let _activa = draft.publicar();
  |                         ---------- `draft` moved due to this method call
7 |     let _otra = draft.publicar();
  |                 ^^^^^ value used here after move
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
- [ ] `cargo test` pasa los 8 tests (4 de typestate, 2 del actor, 2 de la API) y los
  2 doctests `compile_fail`.

!!! abstract "Siguiente sección"

    [Semana 18 — Concurrencia avanzada y lock-free](section_02.md)
