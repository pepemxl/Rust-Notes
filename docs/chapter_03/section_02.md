# Tokio Ecosistema & Axum: construyendo el servidor

La Semana 9 nos mostró cómo funciona async por dentro. La Semana 10 aprovecha ese
conocimiento para construir un servidor web real: un acortador de URLs que aplica todos
los patrones idiomáticos del stack async de producción en Rust.

En esta sección aprenderemos:

- Los canales de Tokio (`mpsc`, `oneshot`, `watch`, `broadcast`) y cuándo usar cada uno.
- Primitivas de sincronización async-aware: `Mutex`, `RwLock`, `Semaphore`.
- `JoinSet` para concurrencia estructurada.
- La arquitectura "extractor-centric" de Axum.
- Cómo fluye una petición a través de middleware Tower.
- State compartido con `Arc<AppState>`.
- Manejo de errores con `AppError` e `IntoResponse`.
- El proyecto completo **Url Shortener v1**.

!!! quote "Filosofía de la Semana 10"

    *En Rust async no hay magia. Axum es simplemente
    tipos que implementan traits de Tower; sus extractores son simplemente structs que
    implementan `FromRequest`. Cuando entiendas esa mecánica, podrás extender el
    framework sin buscar si "tiene soporte para X".*

---

## Canales de Tokio: comunicación entre tareas

Los canales son la forma idiomática de comunicar tareas async. Tokio ofrece cuatro
variantes para cuatro patrones distintos.

```text
                  CANALES DE TOKIO

mpsc  (muchos → uno)     Tx──┐
                         Tx──┼──► Rx     Fan-in, actor pattern
                         Tx──┘

oneshot (uno → uno)      Tx ─────► Rx    Request/Response interno

watch (uno → muchos)     Tx ──────► Rx   Config hot-reload, estado global
     (solo último valor)           Rx
                                   Rx

broadcast (uno → muchos) Tx ──────► Rx   Eventos, notificaciones, chat
         (todos los val.)          Rx
                                   Rx
```

### `mpsc`: el patrón Actor

*Multi-Producer Single-Consumer*. Un receptor, N transmisores. El buffer tiene
capacidad fija: `.send().await` hace backpressure cuando está lleno.

```rust
use tokio::sync::mpsc;

#[derive(Debug)]
enum ComandoLogger {
    Log(String),
    Cerrar,
}

// Actor: tarea que gestiona un recurso de forma exclusiva
async fn logger_actor(mut rx: mpsc::Receiver<ComandoLogger>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            ComandoLogger::Log(msg)  => eprintln!("[LOG] {msg}"),
            ComandoLogger::Cerrar    => break,
        }
    }
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<ComandoLogger>(32); // buffer de 32 mensajes

    tokio::spawn(logger_actor(rx));

    // Varias tareas comparten el transmisor (clone barato)
    let tx2 = tx.clone();
    tokio::spawn(async move {
        tx2.send(ComandoLogger::Log("desde tarea 2".into())).await.unwrap();
    });

    tx.send(ComandoLogger::Log("desde main".into())).await.unwrap();
    tx.send(ComandoLogger::Cerrar).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}
```

El patrón Actor es especialmente valioso para recursos que **no son** `Send` o que
deben ser accedidos de forma secuencializada (conexiones a bases de datos, archivos,
etc.). En lugar de envolver el recurso en `Mutex`, se mueve al actor y se comunica
mediante mensajes.

### `oneshot`: Request/Response interno

Para responder una sola vez a una sola pregunta. Muy útil cuando una tarea necesita
esperar la respuesta de un actor:

```rust
use tokio::sync::{mpsc, oneshot};

enum ComandoCalculo {
    Factorial { n: u64, responder: oneshot::Sender<u64> },
}

async fn actor_calculo(mut rx: mpsc::Receiver<ComandoCalculo>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            ComandoCalculo::Factorial { n, responder } => {
                let resultado = (1..=n).product();
                let _ = responder.send(resultado); // ignorar error si el receptor cayó
            }
        }
    }
}

async fn calcular_factorial(tx: &mpsc::Sender<ComandoCalculo>, n: u64) -> u64 {
    let (resp_tx, resp_rx) = oneshot::channel();
    tx.send(ComandoCalculo::Factorial { n, responder: resp_tx }).await.unwrap();
    resp_rx.await.unwrap()
}
```

### `watch`: estado compartido mutable con pub/sub

Publica un valor y todos los subscriptores ven el **último** valor. Perfecto para
configuración en caliente:

```rust
use tokio::sync::watch;

#[derive(Clone, Debug)]
struct Config {
    limite_conexiones: usize,
    nivel_log: &'static str,
}

#[tokio::main]
async fn main() {
    let (tx, rx) = watch::channel(Config {
        limite_conexiones: 100,
        nivel_log: "info",
    });

    // Tarea que observa cambios de configuración
    tokio::spawn(async move {
        let mut rx = rx;
        loop {
            rx.changed().await.unwrap();
            let cfg = rx.borrow_and_update().clone();
            println!("nueva config: limite={}", cfg.limite_conexiones);
        }
    });

    // Publicar nueva configuración (hot-reload)
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    tx.send(Config { limite_conexiones: 50, nivel_log: "debug" }).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}
```

### `broadcast`: todos los receptores reciben todos los mensajes

Útil para notificaciones de shutdown, eventos de negocio, o sistemas de chat:

```rust
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel::<String>(16);

    // Cada subscriptor recibe su propia copia
    for i in 0..3 {
        let mut rx = tx.subscribe();
        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                println!("receptor {i}: {msg}");
            }
        });
    }

    tx.send("evento 1".into()).unwrap();
    tx.send("evento 2".into()).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}
```

**Cuidado**: si un receptor va demasiado lento y el buffer se llena, pierde mensajes
(`lagged` error). Dimensiona el buffer o usa `mpsc` si no puedes perder mensajes.

### Tabla de canales

| Canal | Productores | Consumidores | Persiste | Cuándo usar |
| :--- | :--- | :--- | :--- | :--- |
| `mpsc` | N (clone) | 1 | No | Actor pattern, fan-in, logging |
| `oneshot` | 1 | 1 | No | Request/Response, resultado único |
| `watch` | 1 | N (subscribe) | Último valor | Config hot-reload, estado global |
| `broadcast` | N (clone) | N (subscribe) | No (buffer) | Eventos, shutdown, notificaciones |

---

## Primitivas de sincronización async-aware

### La regla fundamental

!!! note "Idea clave"

    **Nunca uses `std::sync::Mutex::lock()` si vas a hacer `.await` mientras tienes el
    guard activo.** Bloquea un worker thread del runtime, lo que puede producir un
    deadlock si todos los workers quedan bloqueados esperando.

```rust
// ❌ PELIGROSO: std Mutex a través de .await
async fn mal(m: &std::sync::Mutex<Vec<i32>>) {
    let mut guard = m.lock().unwrap();   // bloquea el hilo OS
    guard.push(1);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await; // hilo OS sigue bloqueado
    guard.push(2);
}

// ✅ CORRECTO: tokio Mutex — el guard es Send y el .lock() es async
async fn bien(m: &tokio::sync::Mutex<Vec<i32>>) {
    let mut guard = m.lock().await;      // cede el worker si está ocupado
    guard.push(1);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await; // OK
    guard.push(2);
}
```

**Excepción válida**: si el guard de `std::sync::Mutex` se libera **antes** del primer
`.await`, puedes usarlo sin problema. Scopeado con `{}` o `drop(guard)` explícito.

### `tokio::sync::RwLock`: lecturas concurrentes

Permite múltiples lectores simultáneos pero escritura exclusiva. Ideal cuando lees con
mucha más frecuencia de lo que escribes:

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

let datos: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(vec![]));

// Múltiples lectores concurrentes
let d = datos.clone();
tokio::spawn(async move {
    let guard = d.read().await;      // RwLockReadGuard — comparte el lock
    println!("longitud: {}", guard.len());
});

// Un escritor exclusivo
{
    let mut guard = datos.write().await; // RwLockWriteGuard — exclusivo
    guard.push("nuevo".into());
}
```

### `tokio::sync::Semaphore`: limitar concurrencia

Esencial para limitar el número de operaciones simultáneas (consultas a base de datos,
llamadas a APIs externas, archivos abiertos):

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

let sem = Arc::new(Semaphore::new(5)); // máximo 5 operaciones simultáneas

let mut handles = vec![];
for i in 0..20 {
    let permit = sem.clone().acquire_owned().await.unwrap();
    handles.push(tokio::spawn(async move {
        // permit se libera al hacer drop (al salir de este bloque)
        let _permit = permit;
        println!("tarea {i} ejecutándose");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }));
}

for h in handles { h.await.unwrap(); }
```

### `JoinSet`: concurrencia estructurada

Colección de `JoinHandle` que gestiona su ciclo de vida. Cuando el `JoinSet` se
dropea, aborta todas las tareas pendientes automáticamente:

```rust
use tokio::task::JoinSet;

async fn procesar_lote(ids: Vec<u64>) -> Vec<String> {
    let mut set = JoinSet::new();

    for id in ids {
        set.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            format!("resultado_{id}")
        });
    }

    let mut resultados = Vec::new();
    while let Some(res) = set.join_next().await {
        resultados.push(res.unwrap());
    }
    resultados
}
```

`JoinSet` es superior a mantener un `Vec<JoinHandle>` manualmente porque:
- Cancela automáticamente al hacer drop.
- `.join_next()` retorna el **primero que termina**, no necesariamente en orden.
- Gestiona correctamente el caso en que una tarea hace `panic!`.

---

## Axum: arquitectura extractor-centric

Axum está construido sobre Tower (un framework de middleware genérico) y Hyper (HTTP
de bajo nivel). Para el programador, la capa de Axum ofrece tres abstracciones:

```text
         Petición HTTP
              │
              ▼
    ┌─────────────────────┐
    │   Tower Middleware  │  ← CorsLayer, TraceLayer, RateLimitLayer...
    │   (capas apiladas)  │
    └─────────┬───────────┘
              │
              ▼
    ┌─────────────────────┐
    │    Axum Router      │  ← .route("/path", get(handler))
    │  (coincidencia de   │
    │     rutas)          │
    └─────────┬───────────┘
              │
              ▼
    ┌─────────────────────┐
    │     Extractores     │  ← Path, Query, State, Json...
    │  (inyección de      │     implementan FromRequest
    │    dependencias)    │
    └─────────┬───────────┘
              │
              ▼
    ┌─────────────────────┐
    │      Handler        │  ← async fn handler(ext1, ext2, ...) -> impl IntoResponse
    └─────────────────────┘
```

### Extractores: cómo Axum inyecta dependencias

Un extractor es cualquier tipo que implementa `FromRequestParts<S>` (para acceder a
headers, path, query, etc.) o `FromRequest<S>` (para consumir el cuerpo):

```rust
use axum::{
    extract::{Path, Query, State, Json},
    http::StatusCode,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct Filtros {
    pagina: Option<u32>,
    limite: Option<u32>,
}

// Axum extrae automáticamente cada argumento por su tipo:
async fn listar_urls(
    State(estado): State<Arc<EstadoApp>>,  // (1) estado compartido — FromRequestParts
    Query(filtros): Query<Filtros>,         // (2) parámetros de query — FromRequestParts
) -> Json<Vec<String>> {
    // ...
    Json(vec![])
}

async fn redirigir(
    State(estado): State<Arc<EstadoApp>>,  // (1) FromRequestParts
    Path(codigo): Path<String>,             // (2) FromRequestParts
) -> Result<axum::response::Redirect, StatusCode> {
    // ...
    Ok(axum::response::Redirect::permanent("https://rust-lang.org"))
}

async fn crear_url(
    State(estado): State<Arc<EstadoApp>>,   // (1) FromRequestParts
    Json(cuerpo): Json<SolicitudCrear>,     // (2) body — FromRequest (consume body)
) -> Result<(StatusCode, Json<RespuestaUrl>), StatusCode> {
    // ...
    Ok((StatusCode::CREATED, Json(RespuestaUrl { codigo: "abc123".into() })))
}

struct EstadoApp; struct SolicitudCrear; struct RespuestaUrl { codigo: String }
```

**Regla de orden**: los extractores que consumen el body (`Json`, `Form`, `Bytes`,
`String`) **deben ser el último argumento** porque Hyper solo puede leer el body una
vez. Los extractores de partes (`Path`, `Query`, `State`, `Extension`) van antes.

### Estado compartido: `State<S>`

El estado se registra en el router y Axum lo inyecta clonando (el clone de `Arc` es
barato — solo incrementa un contador):

```rust
use axum::{Router, routing::{get, post}};
use std::sync::Arc;

#[derive(Clone)]
struct EstadoApp {
    almacen: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    config:  Arc<Configuracion>,
}

struct Configuracion {
    base_url: String,
}

// En main:
let estado = Arc::new(EstadoApp {
    almacen: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    config:  Arc::new(Configuracion { base_url: "http://localhost:3000".into() }),
});

let app: Router = Router::new()
    .route("/health",   get(chequeo_salud))
    .route("/shorten",  post(acortar_url))
    .route("/{codigo}",  get(redirigir))
    .with_state(estado);  // Registra el estado; cada handler recibe State(Arc<...>)

async fn chequeo_salud() -> &'static str { "OK" }
async fn acortar_url()  -> &'static str { "creado" }
async fn redirigir()    -> &'static str { "redirect" }
```

### Middleware Tower con `middleware::from_fn`

Tower define el trait `Service<Request>`. Un middleware es simplemente una función
que envuelve al siguiente handler, pudiendo inspeccionar y modificar petición y
respuesta:

```rust
use axum::{middleware::{self, Next}, extract::Request, response::Response};
use std::time::Instant;

// Middleware de latencia: mide cuánto tarda cada petición
async fn medir_latencia(req: Request, next: Next) -> Response {
    let inicio = Instant::now();
    let uri = req.uri().path().to_owned();
    let metodo = req.method().clone();

    let respuesta = next.run(req).await;

    println!(
        "{} {} → {} ({:?})",
        metodo, uri,
        respuesta.status().as_u16(),
        inicio.elapsed()
    );

    respuesta
}

// Registro en el router:
// Router::new()
//     .route(...)
//     .layer(middleware::from_fn(medir_latencia))
//     .with_state(estado)
```

Para middleware más complejos (con estado, o que necesitan el estado de la app), usa
`middleware::from_fn_with_state`:

```rust
use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use std::sync::Arc;

struct ConfigSeguridad {
    api_key: String,
}

async fn verificar_clave(
    State(cfg): State<Arc<ConfigSeguridad>>,
    req: Request,
    next: Next,
) -> Response {
    let clave = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());
    if clave == Some(cfg.api_key.as_str()) {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

// route_layer: el middleware solo se aplica a las rutas ya registradas en `rutas`
// (una ruta inexistente sigue dando 404, no 401).
fn proteger(rutas: Router, cfg: Arc<ConfigSeguridad>) -> Router {
    rutas.route_layer(from_fn_with_state(cfg, verificar_clave))
}
```

### `IntoResponse`: convertir cualquier tipo en respuesta HTTP

Implementar `IntoResponse` es lo que permite usar `?` en los handlers para propagar
errores y que Axum los convierta automáticamente en respuestas HTTP con el código
correcto:

```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ErrorApp {
    #[error("recurso no encontrado")]
    NoEncontrado,
    #[error("solicitud inválida: {0}")]
    Invalido(String),
    #[error("error interno")]
    Interno(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl IntoResponse for ErrorApp {
    fn into_response(self) -> Response {
        let (estado, mensaje) = match &self {
            ErrorApp::NoEncontrado    => (StatusCode::NOT_FOUND,             self.to_string()),
            ErrorApp::Invalido(_)     => (StatusCode::BAD_REQUEST,           self.to_string()),
            ErrorApp::Interno(_)      => (StatusCode::INTERNAL_SERVER_ERROR, "error interno".into()),
        };

        (estado, Json(json!({ "error": mensaje }))).into_response()
    }
}

// Ahora los handlers pueden devolver Result<T, ErrorApp> y usar ?
async fn handler_ejemplo() -> Result<Json<String>, ErrorApp> {
    let valor = buscar_algo().ok_or(ErrorApp::NoEncontrado)?;
    Ok(Json(valor))
}

fn buscar_algo() -> Option<String> { None }
```

---

## Proyecto: Url Shortener v1

Construyamos un acortador de URLs con en-memoria storage, Axum, y los patrones
aprendidos. Este proyecto es la base que en la Semana 11 migraremos a PostgreSQL.

### Estructura del proyecto

```bash
cargo new url_shortener --bin
cd url_shortener
```

`Cargo.toml`:

```toml
[package]
name = "url_shortener"
version = "0.1.0"
edition = "2024"

[dependencies]
axum        = "0.8"
tokio       = { version = "1", features = ["full"] }
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
dashmap     = "6"
tower-http  = { version = "0.6", features = ["trace", "cors"] }
tracing             = "0.1"
tracing-subscriber  = { version = "0.3", features = ["env-filter"] }
uuid        = { version = "1", features = ["v4"] }
thiserror   = "2"

[dev-dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

### `src/models.rs`

```rust
--8<-- "src/chapter_03/url_shortener_v1/src/models.rs"
```

### `src/error.rs`

```rust
--8<-- "src/chapter_03/url_shortener_v1/src/error.rs"
```

### `src/almacen.rs`

Usamos `DashMap` — un `HashMap` concurrente sin necesidad de `Mutex`:

```rust
--8<-- "src/chapter_03/url_shortener_v1/src/almacen.rs"
```

### `src/estado.rs`

```rust
--8<-- "src/chapter_03/url_shortener_v1/src/estado.rs"
```

### `src/handlers.rs`

```rust
--8<-- "src/chapter_03/url_shortener_v1/src/handlers.rs"
```

### `src/main.rs`

```rust
--8<-- "src/chapter_03/url_shortener_v1/src/main.rs"
```

### Probar el servidor

```bash
# Terminal 1: arrancar
RUST_LOG=info cargo run

# Terminal 2: crear una URL corta
curl -s -X POST http://localhost:3000/shorten \
     -H "Content-Type: application/json" \
     -d '{"url": "https://www.rust-lang.org"}'
# {"codigo":"bVgU5t16","url_corta":"http://localhost:3000/bVgU5t16"}

# Redirigir (muestra la respuesta sin seguir el redirect):
curl -s -D - -o /dev/null http://localhost:3000/bVgU5t16 | grep -iE "^(HTTP|location)"
# HTTP/1.1 308 Permanent Redirect
# location: https://www.rust-lang.org

# Ver estadísticas:
curl -s http://localhost:3000/bVgU5t16/stats
# {"codigo":"bVgU5t16","url_orig":"https://www.rust-lang.org","creada_en":1790724276,"clics":1}

# URL inválida → 400; código inexistente → 404:
curl -s -X POST http://localhost:3000/shorten \
     -H "Content-Type: application/json" -d '{"url": "ftp://x"}'
# {"error":"URL inválida: la URL debe empezar con http:// o https://"}
curl -s http://localhost:3000/NOEXISTE
# {"error":"URL no encontrada"}

# Listar todo:
curl -s http://localhost:3000/urls
```

El código es aleatorio, así que el tuyo será distinto. `CodigoCorto` es un *newtype*
(`struct CodigoCorto(String)`) y Serde lo serializa como el `String` que envuelve, por
eso aparece como texto y no como objeto.

!!! note "¿308 o 301?"

    `Redirect::permanent` de Axum responde **308 Permanent Redirect**, no 301. Ambos son
    redirecciones permanentes que el navegador guarda en caché; la diferencia es que 308
    obliga a repetir la petición con el mismo método y cuerpo (un `POST` sigue siendo
    `POST`), mientras que con 301 los clientes suelen cambiarlo a `GET`. Para un
    acortador cualquiera de los dos sirve. Si necesitas exactamente 301, devuelve
    `(StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, url)])`.

---

## Patrones avanzados de Axum

### Extractor personalizado

Puedes crear extractores propios implementando `FromRequestParts`. Por ejemplo, un
extractor que verifica un header de API key:

```rust
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

pub struct ClaveApi(pub String);

// Axum 0.8 usa `async fn` nativo en traits: ya no se necesita `#[async_trait]`.
impl<S: Send + Sync> FromRequestParts<S> for ClaveApi {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let clave = parts
            .headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        Ok(ClaveApi(clave.to_owned()))
    }
}

// Uso:
// async fn endpoint_protegido(ClaveApi(k): ClaveApi, ...) -> ... {
//     // si llega aquí, la clave es válida
// }
```

### Rutas anidadas con sub-routers

```rust
use axum::Router;

fn rutas_admin<S: AlmacenUrls>() -> Router<std::sync::Arc<EstadoApp<S>>> {
    Router::new()
        .route("/urls",   get(listar_urls::<S>))
        // .route("/urls/{id}", delete(borrar_url::<S>))
}

fn construir_app<S: AlmacenUrls>(
    estado: std::sync::Arc<EstadoApp<S>>,
) -> Router {
    Router::new()
        .route("/health",        get(chequeo_salud))
        .route("/shorten",       post(acortar_url::<S>))
        .route("/{codigo}",       get(redirigir::<S>))
        .route("/{codigo}/stats", get(estadisticas::<S>))
        .nest("/admin", rutas_admin())          // ← sub-router /admin/urls
        .with_state(estado)
}

use almacen::AlmacenMemoria; use estado::EstadoApp;
```

### Apagado graceful con `JoinSet` y `watch`

```rust
use tokio::{sync::watch, task::JoinSet};
use std::sync::Arc;

async fn servidor_con_apagado() {
    let (tx_shutdown, rx_shutdown) = watch::channel(false);

    let mut set = JoinSet::new();

    // Tarea: servidor HTTP
    {
        let rx = rx_shutdown.clone();
        set.spawn(async move {
            // axum::serve(...).with_graceful_shutdown(async move {
            //     let mut rx = rx;
            //     rx.changed().await.ok();
            // }).await.unwrap();
            let _ = rx;   // placeholder
            println!("servidor HTTP arrancando");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });
    }

    // Tarea: esperar Ctrl+C y ordenar el apagado
    set.spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        println!("Ctrl+C recibido, iniciando apagado graceful...");
        let _ = tx_shutdown.send(true);
    });

    // Espera la primera tarea que termine (Ctrl+C o error del servidor)
    set.join_next().await;

    // JoinSet::drop() aborta las demás tareas automáticamente
    println!("apagado completo");
}
```

---

## ✅ Checklist de la Semana 10

- [ ] Conozco los cuatro canales de Tokio y elijo el correcto según el patrón de
  comunicación: `mpsc` (fan-in/actor), `oneshot` (petición/respuesta),
  `watch` (estado global), `broadcast` (eventos).
- [ ] Nunca uso `std::sync::Mutex::lock()` con `.await` activo sobre el guard.
  Uso `tokio::sync::Mutex` o suelto el guard antes del `.await`.
- [ ] Uso `Semaphore` para limitar la concurrencia máxima en operaciones costosas.
- [ ] Uso `JoinSet` en lugar de `Vec<JoinHandle>` para gestión de tareas hijos.
- [ ] Entiendo el flujo `petición → middleware → extractor → handler → IntoResponse`.
- [ ] Sé que los extractores que consumen el body deben ir como último argumento.
- [ ] Implementé `IntoResponse` en `ErrorApp` para que `?` funcione en handlers.
- [ ] El proyecto Url Shortener v1 arranca, acepta peticiones en `/shorten`,
  redirige en `/{codigo}` y devuelve estadísticas en `/{codigo}/stats`.
- [ ] El middleware de telemetría registra método, ruta, estado y latencia.
- [ ] `RUST_LOG=debug cargo run` muestra logs estructurados.

!!! abstract "Siguiente paso"

    Semana 11 — [Bases de datos con SQLx y Serde avanzado](section_03.md).
