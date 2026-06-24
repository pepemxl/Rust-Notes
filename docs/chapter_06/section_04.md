# Fase 3: Especialización y pulido — elige tu arma

La Semana 24 cierra el capstone y el curso. El sistema está funcionando en
staging. Esta semana añades la capa que lo convierte en un trabajo de
portfolio memorable: una especialización técnica profunda que demuestra
dominio real, documentación de nivel profesional y el proceso de release
`v1.0.0`. No intentes cubrir las cinco rutas — elige una y ve tan profundo
como puedas.

En esta sección aprenderemos:

- Cómo elegir la ruta de especialización que mejor encaja con tu perfil y
  el tipo de trabajo que quieres.
- **Ruta A**: gRPC con `tonic`, Kubernetes Operator con `kube-rs` y traces
  distribuidos cruzando servicios.
- **Ruta B**: sistema de plugins dinámicos con `libloading` + ABI estable,
  self-updater verificado con `cosign`.
- **Ruta C**: SSR + hydration con Leptos, optimización de bundle WASM con
  `wasm-opt` y `twiggy`, Lighthouse > 95.
- **Ruta D**: ETL con `polars` Lazy API → Parquet, SQL analytics con
  `datafusion`, inferencia ML con `candle`.
- **Ruta E**: tareas async en Embassy sobre hardware real o QEMU, `defmt`
  como sistema de logging zero-cost para embedded.
- Cómo escribir la documentación final al nivel de un crate del Top 1%.
- Cómo hacer el release `v1.0.0` con `cargo-release` y `cargo-dist`.
- Cómo preparar y grabar la charla técnica de 10 minutos.

> *"El experto en cualquier cosa fue una vez un principiante. La diferencia
> es que el experto eligió una cosa y no paró."*
> — Helen Hayes (adaptado)

---

## Árbol de decisión: ¿qué ruta elegir?

```text
¿Cuál es tu objetivo profesional principal?
│
├─► Sistemas distribuidos, cloud, microservicios, plataforma
│       → RUTA A: Backend / Cloud Native (gRPC + K8s Operator + OTel)
│
├─► Herramientas de desarrollador, CLIs, editor tooling, SDKs
│       → RUTA B: Systems / CLI Extremo (Plugins + Self-update)
│
├─► Web frontend, SSR, apps interactivas, Wasm en navegador
│       → RUTA C: WASM / Fullstack (Leptos SSR + Wasm optimization)
│
├─► Data engineering, ML inference, analytics, pipelines
│       → RUTA D: Data / ML (Polars + DataFusion + Candle)
│
└─► Firmware, IoT, sistemas empotrados, tiempo real
        → RUTA E: Embedded (Embassy + Defmt + no_std)

SEÑALES DE QUE ELEGISTE BIEN:
  ✅ El deliverable de la ruta te entusiasma más que los otros cuatro
  ✅ Tienes al menos algo de contexto previo en ese dominio (aunque sea poco)
  ✅ Puedes hacer la demo en 60 segundos sin explicar 10 conceptos nuevos

SEÑALES DE QUE DEBES RECONSIDERAR:
  ❌ "La elijo porque parece la más impresionante"
  ❌ "No tengo hardware para E y no me gusta web, pero elijo C"
  ❌ No entiendes el entregable de la ruta después de leer esta sección
```

---

## Ruta A: Backend / Cloud Native

### Cuándo elegirla
Quieres trabajar en plataformas, infraestructura de backend, microservicios
o sistemas que se despliegan en Kubernetes. Conoces o quieres aprender
gRPC, service meshes y observabilidad distribuida.

### gRPC con `tonic`

```protobuf
// proto/links.proto
syntax = "proto3";
package links;

service LinksService {
  // CRUD tipado, generado desde este .proto
  rpc CreateLink  (CreateLinkRequest)  returns (LinkResponse);
  rpc GetLink     (GetLinkRequest)     returns (LinkResponse);
  rpc DeleteLink  (DeleteLinkRequest)  returns (DeleteLinkResponse);
  // Streaming: analytics en tiempo real (servidor → cliente)
  rpc WatchClicks (WatchClicksRequest) returns (stream ClickEvent);
}

message CreateLinkRequest {
  string target_url = 1;
  string code       = 2;
  string user_id    = 3;
}

message LinkResponse {
  string id         = 1;
  string code       = 2;
  string target_url = 3;
  string short_url  = 4;
}

message GetLinkRequest     { string code = 1; }
message DeleteLinkRequest  { string code = 1; string user_id = 2; }
message DeleteLinkResponse { bool   ok   = 1; }

message WatchClicksRequest { string link_code = 1; }
message ClickEvent {
  string link_id   = 1;
  string country   = 2;
  string referrer  = 3;
  int64  timestamp = 4;
}
```

```toml
# crates/proto/build.rs — generar código Rust desde .proto en tiempo de compilación

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        // Generar derives adicionales para serde (útil para tests)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile(
            &["proto/links.proto"],
            &["proto"],
        )?;
    Ok(())
}
```

```rust
// crates/server/src/grpc/links_service.rs

use tonic::{Request, Response, Status};
use linkmetrics_core::{domain::UserId, services::link_service::create_link};
use uuid::Uuid;

// El código se genera en build.rs y se incluye con include_proto!
pub mod proto {
    tonic::include_proto!("links");
}
use proto::{
    links_service_server::LinksService,
    CreateLinkRequest, LinkResponse,
    GetLinkRequest, DeleteLinkRequest, DeleteLinkResponse,
    WatchClicksRequest, ClickEvent,
};

pub struct GrpcLinksService {
    state: crate::state::AppState,
}

impl GrpcLinksService {
    pub fn new(state: crate::state::AppState) -> Self { Self { state } }
}

#[tonic::async_trait]
impl LinksService for GrpcLinksService {
    async fn create_link(
        &self,
        request: Request<CreateLinkRequest>,
    ) -> Result<Response<LinkResponse>, Status> {
        // Extraer metadatos de autenticación del gRPC request
        let metadata = request.metadata();
        let user_id  = extract_user_from_metadata(metadata)?;

        let req = request.into_inner();
        let mut repo = self.state.link_repo();

        let link = create_link(&mut repo, user_id, req.code, req.target_url)
            .await
            .map_err(|e| Status::from(e))?;  // DomainError → gRPC Status

        Ok(Response::new(LinkResponse {
            id:         link.id.0.to_string(),
            code:       link.code,
            target_url: link.target_url,
            short_url:  format!("{}/s/{}", self.state.config.base_url, link.code),
        }))
    }

    async fn get_link(
        &self,
        request: Request<GetLinkRequest>,
    ) -> Result<Response<LinkResponse>, Status> {
        let code = request.into_inner().code;
        let repo = self.state.link_repo();
        let link = repo.find_by_code_async(&code)
            .await
            .map_err(|e| Status::from(e))?
            .ok_or_else(|| Status::not_found(format!("link '{code}' no encontrado")))?;

        Ok(Response::new(LinkResponse {
            id:         link.id.0.to_string(),
            code:       link.code.clone(),
            target_url: link.target_url.clone(),
            short_url:  format!("{}/s/{}", self.state.config.base_url, link.code),
        }))
    }

    // Server-streaming: emite clicks en tiempo real via tokio::sync::broadcast
    type WatchClicksStream = std::pin::Pin<
        Box<dyn futures::Stream<Item = Result<ClickEvent, Status>> + Send>
    >;

    async fn watch_clicks(
        &self,
        request: Request<WatchClicksRequest>,
    ) -> Result<Response<Self::WatchClicksStream>, Status> {
        let _code = request.into_inner().link_code;
        // Suscribirse al canal de eventos de clicks del ActorModel (Semana 22)
        // y retransmitirlos como stream gRPC
        let stream = futures::stream::pending(); // placeholder
        Ok(Response::new(Box::pin(stream)))
    }

    async fn delete_link(
        &self,
        _request: Request<DeleteLinkRequest>,
    ) -> Result<Response<DeleteLinkResponse>, Status> {
        Ok(Response::new(DeleteLinkResponse { ok: true }))
    }
}

// Mapear DomainError a gRPC Status
impl From<linkmetrics_core::domain::DomainError> for Status {
    fn from(e: linkmetrics_core::domain::DomainError) -> Self {
        use linkmetrics_core::domain::DomainError::*;
        match e {
            Validation(msg)  => Status::invalid_argument(msg),
            Conflict(code)   => Status::already_exists(format!("código '{code}' ya existe")),
            NotFound         => Status::not_found("recurso no encontrado"),
            Unauthorized     => Status::permission_denied("sin autorización"),
            Internal(msg)    => Status::internal(msg),
        }
    }
}

fn extract_user_from_metadata(
    metadata: &tonic::metadata::MetadataMap
) -> Result<UserId, Status> {
    let token = metadata
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| Status::unauthenticated("Bearer token requerido"))?;
    // Validación JWT real igual que el extractor HTTP (Semana 23)
    let _ = token;
    Ok(UserId(Uuid::now_v7()))
}
```

```rust
// Servir REST + gRPC en el mismo proceso con distintos puertos
// crates/server/src/main.rs (fragmento)

// Puerto 3000: Axum REST + Swagger UI
let rest_server = axum::serve(
    tokio::net::TcpListener::bind("0.0.0.0:3000").await?,
    rest_app,
).with_graceful_shutdown(shutdown.clone());

// Puerto 50051: Tonic gRPC
let grpc_server = tonic::transport::Server::builder()
    .add_service(proto::links_service_server::LinksServiceServer::new(
        GrpcLinksService::new(state.clone())
    ))
    .serve_with_shutdown(
        "0.0.0.0:50051".parse()?,
        shutdown.clone(),
    );

// Ambos en paralelo
tokio::try_join!(rest_server, grpc_server)?;
```

### Kubernetes Operator con `kube-rs`

```rust
// crates/operator/src/main.rs
// Un operator que reconcilia CRDs de tipo "LinkProject"

use k8s_openapi::api::core::v1::ConfigMap;
use kube::{
    api::{Api, Patch, PatchParams},
    Client, CustomResource, ResourceExt,
};
use kube::runtime::{controller::Action, Controller};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

/// El CRD que el operator gestiona.
/// `cargo run --bin crdgen` genera el YAML para aplicar en K8s.
#[derive(CustomResource, Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[kube(
    group   = "linkmetrics.io",
    version = "v1alpha1",
    kind    = "LinkProject",
    namespaced,
    status  = "LinkProjectStatus",
)]
pub struct LinkProjectSpec {
    pub name:          String,
    pub rate_limit_rpm: u32,
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
pub struct LinkProjectStatus {
    pub phase:   String,   // Pending | Active | Error
    pub api_key: Option<String>,
}

/// El reconciliador: K8s llama aquí cada vez que algo cambia.
async fn reconcile(
    project: Arc<LinkProject>,
    ctx:     Arc<Data>,
) -> Result<Action, kube::Error> {
    let ns   = project.namespace().unwrap_or_default();
    let name = project.name_any();

    tracing::info!(project = %name, ns = %ns, "Reconciliando LinkProject");

    // 1. Crear/actualizar ConfigMap con la configuración del proyecto
    let config_map = build_config_map(&project);
    let cm_api: Api<ConfigMap> = Api::namespaced(ctx.client.clone(), &ns);
    cm_api.patch(
        &format!("linkmetrics-{name}"),
        &PatchParams::apply("linkmetrics-operator"),
        &Patch::Apply(config_map),
    ).await?;

    // 2. Actualizar status del CRD
    let projects: Api<LinkProject> = Api::namespaced(ctx.client.clone(), &ns);
    let status = serde_json::json!({
        "status": { "phase": "Active", "api_key": null }
    });
    projects.patch_status(
        &name,
        &PatchParams::default(),
        &Patch::Merge(status),
    ).await?;

    // Revisar cada 5 minutos (o antes si hay cambios)
    Ok(Action::requeue(Duration::from_secs(300)))
}

fn error_policy(
    project: Arc<LinkProject>,
    error:   &kube::Error,
    _ctx:    Arc<Data>,
) -> Action {
    tracing::error!(project = %project.name_any(), error = %error, "Error reconciliando");
    Action::requeue(Duration::from_secs(30))
}

struct Data { client: Client }

fn build_config_map(project: &LinkProject) -> ConfigMap {
    use std::collections::BTreeMap;
    let mut data = BTreeMap::new();
    data.insert("rate_limit_rpm".into(), project.spec.rate_limit_rpm.to_string());
    data.insert("allowed_domains".into(), project.spec.allowed_domains.join(","));
    ConfigMap {
        metadata: kube::core::ObjectMeta {
            name:      Some(format!("linkmetrics-{}", project.name_any())),
            namespace: project.namespace(),
            ..Default::default()
        },
        data: Some(data),
        ..Default::default()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().json().init();
    let client = Client::try_default().await?;
    let ctx    = Arc::new(Data { client: client.clone() });
    let api: Api<LinkProject> = Api::all(client);

    Controller::new(api, Default::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|result| async move {
            match result {
                Ok((obj, _))   => tracing::info!(obj = %obj.name, "Reconciliado"),
                Err(err)       => tracing::error!(error = %err, "Error en controller"),
            }
        })
        .await;
    Ok(())
}
```

---

## Ruta B: Systems / CLI Extremo

### Cuándo elegirla
Quieres construir herramientas de desarrollador, SDKs, CLIs que otros usan
en su terminal, o sistemas que se distribuyen como binarios únicos.

### Sistema de plugins con `libloading`

```rust
// La interfaz del plugin: C-ABI estable entre versiones del host.
// Este header se publica para que autores de plugins lo implementen.

// crates/plugin-api/src/lib.rs
// (Este crate es el que publicas en crates.io para autores de plugins)

/// Versión del ABI. Si cambia → incompatibilidad. Plugins deben verificar.
pub const ABI_VERSION: u32 = 1;

/// La función que el host llama para obtener el plugin.
/// Nombre fijo por convención: `lm_plugin_init`
/// Declaración C-ABI para cruzar la barrera de .so/.dylib/.dll
pub type PluginInitFn = unsafe extern "C" fn() -> *mut dyn PluginVTable;

/// VTable: punteros a funciones. Equivalente a un trait object estable en C-ABI.
/// IMPORTANTE: nunca añadir campos al final de un trait en Rust puro —
/// el layout del vtable no está garantizado. Usamos este struct explícito.
#[repr(C)]
pub struct PluginVTable {
    pub abi_version: u32,
    pub name:        extern "C" fn() -> *const std::ffi::c_char,
    pub version:     extern "C" fn() -> *const std::ffi::c_char,
    pub description: extern "C" fn() -> *const std::ffi::c_char,
    pub execute:     extern "C" fn(args: *const *const std::ffi::c_char, len: usize) -> i32,
    pub destroy:     unsafe extern "C" fn(this: *mut PluginVTable),
}

// Plugin de ejemplo: crates/plugin-example/src/lib.rs
#[no_mangle]
pub unsafe extern "C" fn lm_plugin_init() -> *mut PluginVTable {
    let vtable = Box::new(PluginVTable {
        abi_version: linkmetrics_plugin_api::ABI_VERSION,
        name:        plugin_name,
        version:     plugin_version,
        description: plugin_description,
        execute:     plugin_execute,
        destroy:     plugin_destroy,
    });
    Box::into_raw(vtable)
}

extern "C" fn plugin_name()        -> *const std::ffi::c_char {
    b"geolookup\0".as_ptr() as _
}
extern "C" fn plugin_version()     -> *const std::ffi::c_char {
    b"0.1.0\0".as_ptr() as _
}
extern "C" fn plugin_description() -> *const std::ffi::c_char {
    b"Enriquece clicks con datos de geolocalizacion\0".as_ptr() as _
}
extern "C" fn plugin_execute(args: *const *const std::ffi::c_char, _len: usize) -> i32 {
    // Leer args[0] como código de link, buscar en DB, etc.
    let _ = args;
    println!("[geolookup] ejecutando...");
    0   // 0 = éxito
}
unsafe extern "C" fn plugin_destroy(this: *mut PluginVTable) {
    drop(Box::from_raw(this));
}
```

```rust
// crates/cli/src/plugins/loader.rs

use libloading::{Library, Symbol};
use linkmetrics_plugin_api::{ABI_VERSION, PluginInitFn, PluginVTable};
use std::{ffi::CStr, path::Path};

pub struct LoadedPlugin {
    vtable:  *mut PluginVTable,
    _lib:    Library,  // debe vivir tanto como vtable
}

unsafe impl Send for LoadedPlugin {}

impl LoadedPlugin {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        // SAFETY: el .so puede contener código arbitrario.
        // Verificamos ABI_VERSION antes de llamar cualquier función del plugin.
        let lib: Library = unsafe { Library::new(path)? };

        let init_fn: Symbol<PluginInitFn> = unsafe {
            lib.get(b"lm_plugin_init\0")?
        };

        let vtable = unsafe { init_fn() };
        if vtable.is_null() {
            anyhow::bail!("plugin {:?} devolvió null", path);
        }

        let abi = unsafe { (*vtable).abi_version };
        if abi != ABI_VERSION {
            unsafe { ((*vtable).destroy)(vtable) };
            anyhow::bail!(
                "plugin {:?} usa ABI {} pero el host requiere {}",
                path, abi, ABI_VERSION
            );
        }

        Ok(LoadedPlugin { vtable, _lib: lib })
    }

    pub fn name(&self) -> &str {
        let ptr = unsafe { ((*self.vtable).name)() };
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("unknown")
    }

    pub fn execute(&self, args: &[&str]) -> i32 {
        let c_args: Vec<std::ffi::CString> = args.iter()
            .map(|s| std::ffi::CString::new(*s).unwrap())
            .collect();
        let ptrs: Vec<*const std::ffi::c_char> = c_args.iter()
            .map(|s| s.as_ptr())
            .collect();
        unsafe { ((*self.vtable).execute)(ptrs.as_ptr(), ptrs.len()) }
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        unsafe { ((*self.vtable).destroy)(self.vtable) };
    }
}

/// Escanear ~/.config/linkmetrics/plugins/*.so (Linux/Mac) y cargar todos.
pub fn discover_and_load_plugins() -> Vec<LoadedPlugin> {
    let plugin_dir = dirs::config_dir()
        .unwrap_or_default()
        .join("linkmetrics")
        .join("plugins");

    let ext = if cfg!(target_os = "macos") { "dylib" } else { "so" };

    std::fs::read_dir(&plugin_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == ext).unwrap_or(false))
        .filter_map(|e| {
            LoadedPlugin::load(&e.path())
                .map_err(|err| tracing::warn!(path = ?e.path(), %err, "Plugin no cargado"))
                .ok()
        })
        .collect()
}
```

### Self-updater con verificación de firma

```rust
// crates/cli/src/update.rs

use anyhow::Result;

const GITHUB_REPO: &str = "usuario/linkmetrics";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Verificar si hay una versión más nueva y preguntar al usuario.
pub async fn check_and_apply_update(force: bool) -> Result<()> {
    println!("Versión actual: {CURRENT_VERSION}");
    println!("Buscando actualizaciones en github.com/{GITHUB_REPO}...");

    let status = self_update::backends::github::Update::configure()
        .repo_owner("usuario")
        .repo_name("linkmetrics")
        .bin_name("lm")
        .show_download_progress(true)
        .current_version(CURRENT_VERSION)
        // Verificar firma cosign antes de aplicar (supply chain security)
        .verifier(cosign_verify)
        .build()?
        .update_extended()?;

    match status {
        self_update::Status::UpToDate(v) => {
            println!("✓ Ya estás en la versión más reciente ({v})");
        }
        self_update::Status::Updated(v) => {
            println!("✓ Actualizado a {v}. Reinicia el CLI para usar la nueva versión.");
        }
    }
    Ok(())
}

fn cosign_verify(data: &[u8], _sig: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    // En producción: verificar la firma cosign con la clave pública del proyecto.
    // cosign verify-blob --key cosign.pub --signature sig.b64 binary
    // Aquí simplificado — ver la documentación de sigstore/cosign-rs
    let _ = data;
    Ok(())
}
```

---

## Ruta C: WASM / Fullstack

### Cuándo elegirla
Quieres construir interfaces web con Rust compartiendo tipos entre cliente
y servidor, o necesitas integrar Rust en el navegador.

### Leptos SSR con server functions

```rust
// crates/frontend/src/lib.rs
// Compilado tanto para el servidor (SSR) como para el cliente (WASM)

use leptos::*;

/// Componente del dashboard: renderiza en el servidor, se hidrata en el cliente.
#[component]
pub fn Dashboard() -> impl IntoView {
    // server_action! — RPC type-safe que funciona tanto en SSR como en WASM
    let get_stats = create_resource(
        || (),
        |_| async move { fetch_dashboard_stats().await.ok() },
    );

    view! {
        <main class="dashboard">
            <h1>"LinkMetrics Dashboard"</h1>
            <Suspense fallback=move || view! { <p>"Cargando..."</p> }>
                {move || {
                    get_stats.get().flatten().map(|stats| view! {
                        <div class="stats-grid">
                            <StatCard
                                label="Links activos"
                                value=stats.total_links.to_string()
                            />
                            <StatCard
                                label="Clicks (24h)"
                                value=stats.clicks_24h.to_string()
                            />
                        </div>
                    })
                }}
            </Suspense>
        </main>
    }
}

#[component]
fn StatCard(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="stat-card">
            <span class="stat-label">{label}</span>
            <span class="stat-value">{value}</span>
        </div>
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DashboardStats {
    pub total_links: u64,
    pub clicks_24h:  u64,
}

/// Server function: se ejecuta en el servidor, se llama desde el cliente vía HTTP.
/// El compilador garantiza que los tipos coinciden entre cliente y servidor.
#[server(FetchDashboardStats, "/api")]
pub async fn fetch_dashboard_stats() -> Result<DashboardStats, ServerFnError> {
    use leptos_axum::extract;
    // Extraer el AppState de Axum (inyectado por LeptosAxumRouter)
    let state = extract::<axum::extract::State<crate::state::AppState>>().await?;

    let total_links: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM links WHERE is_active = true"
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ServerFnError::ServerError(e.to_string()))?
    .unwrap_or(0);

    let clicks_24h: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM clicks WHERE clicked_at > now() - interval '24 hours'"
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| ServerFnError::ServerError(e.to_string()))?
    .unwrap_or(0);

    Ok(DashboardStats {
        total_links: total_links as u64,
        clicks_24h:  clicks_24h as u64,
    })
}
```

### Optimización del bundle WASM

```bash
# 1. Compilar en modo release con optimizaciones
wasm-pack build --release --target web crates/frontend

# 2. Optimizar el .wasm generado con Binaryen
wasm-opt -Oz \
  --enable-mutable-globals \
  --enable-reference-types \
  --strip-debug \
  pkg/frontend_bg.wasm \
  -o pkg/frontend_bg_opt.wasm

# 3. Analizar qué funciones ocupan más espacio (twiggy)
twiggy top      pkg/frontend_bg.wasm -n 20
twiggy dominators pkg/frontend_bg.wasm | head -30

# Salida típica de twiggy:
#  Retained Bytes │ Retained % │ Item
# ────────────────┼────────────┼─────────────────────────────────
#          45,123 │     23.4%  │ leptos_dom::mount_to
#          32,891 │     17.1%  │ serde_json::ser::Serializer
#          ...
#
# Si serde_json aparece alto → considerar serde-wasm-bindgen o rmp-serde

# 4. Comparar tamaños
ls -lh pkg/*.wasm
# frontend_bg.wasm:       312K  (sin optimizar)
# frontend_bg_opt.wasm:   187K  (con wasm-opt -Oz)
# Con gzip en servidor:    52K  (lo que descarga el navegador)

# 5. Audit de Lighthouse (requiere Chrome headless)
npx lighthouse http://localhost:3000 \
  --only-categories=performance,best-practices,accessibility,seo \
  --output=json \
  --output-path=lighthouse-report.json
cat lighthouse-report.json | jq '.categories | to_entries[] | {(.key): .value.score}'
# Target: performance ≥ 0.95 (= Lighthouse score 95)
```

```toml
# Cargo.toml del crate frontend — configuración para tamaño mínimo
[profile.release]
opt-level = "z"        # optimizar para tamaño, no velocidad
lto       = true
codegen-units = 1

# Si usas wee_alloc (allocator más pequeño para WASM):
[dependencies]
wee_alloc = { version = "0.4", optional = true }

# En lib.rs:
# #[cfg(feature = "wee_alloc")]
# #[global_allocator]
# static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
```

---

## Ruta D: Data / ML

### Cuándo elegirla
Quieres trabajar con grandes volúmenes de datos, pipelines ETL, queries
analíticas o inferencia de modelos en Rust.

### ETL con Polars

```rust
// crates/analytics/src/etl.rs
// Pipeline: logs de clicks en CSV → análisis → Parquet particionado

use polars::prelude::*;
use std::path::Path;

/// Procesar archivo de clicks y producir Parquet agregado por día y país.
pub fn procesar_clicks(input_path: &Path, output_dir: &Path) -> anyhow::Result<()> {
    // API Lazy: no ejecuta nada hasta collect() — permite optimizaciones
    let df = LazyFrame::scan_csv(input_path, Default::default())?
        // Filtrar clicks sin país conocido
        .filter(col("country").is_not_null())
        // Parsear timestamp string a Date
        .with_column(
            col("clicked_at")
                .str()
                .to_date(StrptimeOptions {
                    format: Some("%Y-%m-%dT%H:%M:%SZ".into()),
                    ..Default::default()
                })
                .alias("date"),
        )
        // Agregar: clicks por día y país
        .group_by([col("date"), col("country"), col("link_id")])
        .agg([
            count().alias("total_clicks"),
            col("ip_hash").n_unique().alias("unique_visitors"),
        ])
        // Ordenar para mejor compresión en Parquet
        .sort(["date", "country"], Default::default())
        .collect()?;

    println!(
        "Procesadas {} filas → {} registros agregados",
        df.height(),
        df.height()
    );

    // Particionar por fecha y escribir Parquet (mejor para range queries)
    let fechas = df.column("date")?
        .unique(None)?
        .cast(&DataType::String)?;

    for fecha in fechas.str()?.into_iter().flatten() {
        let particion = df.clone().lazy()
            .filter(col("date").cast(DataType::String).eq(lit(fecha)))
            .collect()?;

        let salida = output_dir.join(format!("fecha={fecha}")).join("data.parquet");
        std::fs::create_dir_all(salida.parent().unwrap())?;

        let mut file = std::fs::File::create(&salida)?;
        ParquetWriter::new(&mut file)
            .with_statistics(StatisticsOptions::full())
            .with_compression(ParquetCompression::Zstd(Some(ZstdLevel::try_new(6)?)))
            .finish(&mut particion.clone())?;
    }

    Ok(())
}

/// Leer múltiples Parquets y responder una query SQL con DataFusion
pub async fn query_analytics(
    parquet_dir: &Path,
    sql:         &str,
) -> anyhow::Result<datafusion::dataframe::DataFrame> {
    use datafusion::prelude::*;

    let ctx = SessionContext::new();

    // Registrar todos los Parquets del directorio como una tabla virtual
    ctx.register_parquet(
        "clicks",
        parquet_dir.to_str().unwrap(),
        ParquetReadOptions::default(),
    ).await?;

    // Ejecutar SQL arbitrario — DataFusion lo planifica y ejecuta en Arrow
    let df = ctx.sql(sql).await?;
    Ok(df)
}

#[tokio::test]
async fn top_paises_por_clicks() {
    // Crear datos de prueba
    let tmp = tempfile::tempdir().unwrap();
    // (crear CSV de test...)

    let resultado = query_analytics(
        tmp.path(),
        "SELECT country, SUM(total_clicks) as clicks
         FROM clicks
         GROUP BY country
         ORDER BY clicks DESC
         LIMIT 10",
    ).await.unwrap();

    resultado.show().await.unwrap();
}
```

### Inferencia ML con `candle`

```rust
// crates/analytics/src/ml/spam_detector.rs
// Clasificar si una URL de destino parece spam o phishing

use candle_core::{Device, Tensor};
use candle_nn::{linear, Linear, Module, VarBuilder};

/// Red neuronal simple para clasificación binaria (spam/no-spam).
/// Entrenada offline con Python/PyTorch y exportada como SafeTensors.
struct SpamDetector {
    layer1: Linear,
    layer2: Linear,
}

impl SpamDetector {
    fn new(vb: VarBuilder) -> candle_core::Result<Self> {
        Ok(Self {
            layer1: linear(128, 64, vb.pp("layer1"))?,
            layer2: linear(64,  2,  vb.pp("layer2"))?,
        })
    }

    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        let x = self.layer1.forward(x)?;
        let x = x.relu()?;
        self.layer2.forward(&x)
    }

    /// Cargar desde archivo .safetensors (exportado desde Python)
    pub fn load(model_path: &str) -> candle_core::Result<Self> {
        let device = Device::Cpu;
        let tensors = candle_core::safetensors::load(model_path, &device)?;
        let vb = VarBuilder::from_tensors(tensors, candle_core::DType::F32, &device);
        Self::new(vb)
    }

    /// Clasificar una URL. Retorna probabilidad de spam (0.0–1.0).
    pub fn classify(&self, features: Vec<f32>) -> candle_core::Result<f32> {
        let device = Device::Cpu;
        let input = Tensor::from_vec(features, (1, 128), &device)?;
        let logits = self.forward(&input)?;
        // Softmax sobre los 2 logits → [prob_legit, prob_spam]
        let probs = candle_nn::ops::softmax(&logits, 1)?;
        let spam_prob: f32 = probs.i((0, 1))?.to_scalar()?;
        Ok(spam_prob)
    }
}

/// Extraer features de texto de una URL (simplificado)
pub fn url_features(url: &str) -> Vec<f32> {
    let mut features = vec![0.0f32; 128];
    features[0]  = url.len() as f32 / 2048.0;
    features[1]  = url.chars().filter(|c| *c == '-').count() as f32;
    features[2]  = if url.contains("https") { 1.0 } else { 0.0 };
    features[3]  = url.chars().filter(|c| c.is_numeric()).count() as f32;
    // ... 124 features más extraídas del dominio, TLD, patrones, etc.
    features
}

/// Llamar desde el handler de creación de link (en spawn_blocking para no bloquear async)
pub async fn es_url_spam(
    detector: std::sync::Arc<SpamDetector>,
    url:      String,
) -> bool {
    tokio::task::spawn_blocking(move || {
        let features = url_features(&url);
        detector.classify(features)
            .map(|prob| prob > 0.85)
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}
```

---

## Ruta E: Embedded (Embassy)

### Cuándo elegirla
Quieres trabajar con firmware, IoT, sistemas de tiempo real o microcontroladores.
Tienes acceso a hardware (RP2040, ESP32, STM32) o puedes usar QEMU.

### Compartir lógica de dominio entre embedded y servidor

```rust
// crates/core/src/lib.rs — añadir compatibilidad no_std

// La lógica de dominio de core es tan pura que funciona en no_std.
// Esto no es teórico: el mismo crate valida eventos en el firmware Y en el servidor.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use alloc::string::String;

// Todo lo que ya teníamos funciona: Domain types, validation, use cases.
// Solo los ports que usan I/O (LinkRepo, ClickStore) quedan excluidos en no_std.
```

```toml
# crates/core/Cargo.toml — hacer no_std opcional
[features]
default = ["std"]
std     = []

[dependencies]
serde = { workspace = true, default-features = false, features = ["derive", "alloc"] }
```

### Aplicación Embassy con tareas async

```rust
// crates/firmware/src/main.rs
// Target: RP2040 (Raspberry Pi Pico) con Embassy

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use embassy_executor::Spawner;
use embassy_rp::{
    gpio::{Input, Level, Output, Pull},
    peripherals::{PIN_25, PIN_0, UART0},
    uart::{self, Uart},
    i2c::{self, I2c},
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use defmt::{info, warn, error};
use defmt_rtt as _;    // defmt a través de RTT (Real Time Transfer)
use panic_probe as _;

/// Canal para comunicar lecturas del sensor al task de envío WiFi.
/// Capacity 4: si el sender es más rápido que el receiver, bloquea el sender.
static SENSOR_CHANNEL: Channel<ThreadModeRawMutex, SensorReading, 4> = Channel::new();

#[derive(defmt::Format)]
struct SensorReading {
    temperature_c: i16,   // en décimas de grado (sin floats para ahorrar espacio)
    humidity_pct:  u8,
    link_clicks:   u32,   // contador local de clicks físicos en el botón
}

/// Task 1: leer sensor de temperatura/humedad BMP280 cada 10 segundos.
#[embassy_executor::task]
async fn task_sensor(i2c: I2c<'static, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Async>) {
    let mut clicks = 0u32;

    loop {
        // Leer BMP280 via I2C (simplificado — la librería real maneja el protocolo)
        let temperatura = leer_bmp280_temperatura(&i2c).await.unwrap_or(0);
        let humedad     = leer_bmp280_humedad(&i2c).await.unwrap_or(0);

        let reading = SensorReading {
            temperature_c: temperatura,
            humidity_pct:  humedad,
            link_clicks:   clicks,
        };

        info!("Lectura sensor: {:?}", reading);

        // Enviar al canal (si está lleno, esperar — no bloquear el executor)
        SENSOR_CHANNEL.send(reading).await;

        Timer::after(Duration::from_secs(10)).await;
    }
}

/// Task 2: enviar lecturas via UART al host (o WiFi con embassy-net).
#[embassy_executor::task]
async fn task_comunicacion(mut uart: Uart<'static, UART0, uart::Async>) {
    loop {
        let reading = SENSOR_CHANNEL.receive().await;

        // Serializar en formato compacto (sin serde_json — demasiado para flash)
        // Formato: "T:{temp},H:{hum},C:{clicks}\n"
        let mut buf = [0u8; 64];
        let len = escribir_csv(&reading, &mut buf);

        if let Err(e) = uart.write(&buf[..len]).await {
            error!("Error UART: {:?}", e);
        }
    }
}

/// Task 3: LED blinky como indicador de vida (watchdog visual).
#[embassy_executor::task]
async fn task_blinky(mut led: Output<'static, PIN_25>) {
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(900)).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    info!("LinkMetrics Firmware v{}", env!("CARGO_PKG_VERSION"));

    // Inicializar I2C para el sensor BMP280
    let i2c = I2c::new_async(
        p.I2C0, p.PIN_4, p.PIN_5,
        embassy_rp::bind_interrupts!(struct Irqs { I2C0_IRQ => i2c::InterruptHandler<embassy_rp::peripherals::I2C0>; }),
        Default::default(),
    );

    // Inicializar UART para comunicación con host
    let uart = Uart::new_with_rtscts_async(
        p.UART0, p.PIN_0, p.PIN_1, p.PIN_2, p.PIN_3,
        embassy_rp::bind_interrupts!(struct UartIrqs { UART0_IRQ => uart::InterruptHandler<UART0>; }),
        Default::default(),
    );

    // Lanzar todas las tareas — el executor de Embassy las gestiona
    spawner.spawn(task_sensor(i2c)).unwrap();
    spawner.spawn(task_comunicacion(uart)).unwrap();
    spawner.spawn(task_blinky(Output::new(p.PIN_25, Level::Low))).unwrap();

    // main nunca retorna en embedded
}

async fn leer_bmp280_temperatura(_i2c: &I2c<'_, _, _>) -> Option<i16> {
    Some(235)  // 23.5°C en décimas — placeholder para compilación
}
async fn leer_bmp280_humedad(_i2c: &I2c<'_, _, _>) -> Option<u8> {
    Some(62)   // 62% — placeholder
}
fn escribir_csv(r: &SensorReading, buf: &mut [u8]) -> usize {
    let s = format_args!("T:{},H:{},C:{}\n", r.temperature_c, r.humidity_pct, r.link_clicks);
    let _ = (s, buf);
    0 // placeholder
}
```

```toml
# .cargo/config.toml — configuración de compilación cruzada
[build]
target = "thumbv6m-none-eabi"  # RP2040 (Cortex-M0+)

[target.thumbv6m-none-eabi]
runner = "probe-rs run --chip RP2040"  # flashear automáticamente con probe-rs

[alias]
flash = "run --release"   # cargo flash → compila y flashea
```

```bash
# Compilar para RP2040
cargo build --release --target thumbv6m-none-eabi

# Flashear con probe-rs (requiere depurador SWD conectado)
cargo flash

# Ver logs defmt en tiempo real
cargo flash && probe-rs attach --chip RP2040 --protocol swd

# Sin hardware — QEMU para thumbv7m (Cortex-M3, compatible)
cargo run --target thumbv7m-none-eabi --features qemu
# Salida defmt en terminal vía QEMU semihosting
```

---

## Documentación final — estándar Top 1%

```text
ARCHIVOS MÍNIMOS EN EL REPOSITORIO PÚBLICO:

  README.md          ← La portada del proyecto. Importa más que el código.
  ARCHITECTURE.md    ← Decisiones técnicas y trade-offs (los ADRs expandidos)
  CONTRIBUTING.md    ← Cómo contribuir: setup, convenciones, PR process
  CHANGELOG.md       ← Historial de cambios (Keep a Changelog)
  LICENSE            ← MIT o Apache-2.0 (elige conscientemente)
  SECURITY.md        ← Cómo reportar vulnerabilidades
  docs/              ← openapi.yaml, adr/, guías
```

### `README.md` — la portada

````markdown
# LinkMetrics

[![CI](https://github.com/usuario/linkmetrics/actions/workflows/ci.yml/badge.svg)](...)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-1.81%2B-orange.svg)](...)

> Plataforma self-hosted de URL shortening con analytics en tiempo real.
> Construida en Rust con Axum, SQLx y Redis.

## Quickstart

```bash
# 1. Clonar e iniciar infraestructura
git clone https://github.com/usuario/linkmetrics
cd linkmetrics
docker compose up -d

# 2. Aplicar migraciones y arrancar
cargo run --bin linkmetrics-server
# → Servidor en http://localhost:3000
# → Swagger UI en http://localhost:3000/docs

# 3. Crear un link
curl -X POST http://localhost:3000/api/v1/links \
  -H "Authorization: Bearer TU_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"target_url": "https://ejemplo.com", "code": "mi-link"}'

# 4. Redirigir
curl -L http://localhost:3000/s/mi-link
```

## Arquitectura

```
┌──────────────┐     ┌──────────────────────────────────────────────────┐
│ REST Client  │────▶│  crates/server  (Axum + Tower)                   │
│ gRPC Client  │────▶│  crates/server  (Tonic)              :3000/:50051│
└──────────────┘     │                                                  │
                     │  ┌────────────┐  ┌─────────┐  ┌─────────────┐  │
                     │  │crates/core │  │crates/db│  │crates/cli   │  │
                     │  │ (dominio   │◀─│ (SQLx   │  │ (Clap admin)│  │
                     │  │  puro)     │  │  repos) │  │             │  │
                     │  └────────────┘  └────┬────┘  └──────┬──────┘  │
                     └─────────────────────────────────────────────────┘
                                             │               │
                                    ┌────────▼───────────────▼──────┐
                                    │  PostgreSQL 16  │  Redis 7    │
                                    └─────────────────────────────┘
```

## Características

- **Redirección < 2ms** (Redis cache + Actor Model para clicks)
- **Multi-tenant** con API Keys y rate limiting por clave
- **Analytics** en tiempo real: clicks por día, país, referrer
- **OpenAPI 3.1** con Swagger UI (dev/staging)
- **Observabilidad**: structured JSON logs + OpenTelemetry traces
- **Seguridad**: JWT + JWKS rotation, STRIDE threat model, SBOM

## Links

- [Documentación de la API](docs/openapi.yaml)
- [Arquitectura](ARCHITECTURE.md)
- [Demo video (10 min)](https://youtube.com/...)
- [Desplegado en staging](https://linkmetrics-staging.fly.dev)
````

### `ARCHITECTURE.md` — para un Senior Engineer

````markdown
# Architecture

## Por qué este stack

### Hexagonal (Ports & Adapters)
`crates/core` no importa ninguna librería de I/O. Esto significa:
- Los tests del dominio son instantáneos (sin DB, sin red).
- Cambiar de PostgreSQL a CockroachDB solo requiere un nuevo adapter en `crates/db`.
- Los handlers HTTP son delgados: solo traducen HTTP → use case → HTTP.

### Por qué Actor Model para clicks (ADR-001)
La redirección maneja ráfagas de 50k+ req/s en campañas virales. Un
`UPDATE ... click_count = click_count + 1` crea hot-spot en la fila de
PostgreSQL. El Actor sharded (16 shards × tokio::sync::mpsc) acumula
clicks en memoria y los persiste en batch cada 5s o 1000 clicks — eventual
consistency de < 1s, sin contención en DB.

**Coste**: si el proceso muere, perdemos hasta 1000 clicks (< 0.01% a 100k RPM).
**Beneficio**: latencia de redirección < 2ms p99 bajo carga.

### Por qué SQLx sobre Diesel
[Ver ADR-002 en docs/adr/002-sqlx-over-diesel.md]

### Por qué UUIDv7 como clave primaria
[Ver ADR-003 en docs/adr/003-uuid-v7-primary-key.md]

## Trade-offs conscientes

| Decisión | Alternativa descartada | Razón |
|----------|----------------------|-------|
| Eventual consistency en clicks | Fuerte consistencia (DB sync) | 50x mayor throughput |
| JWT stateless | Sessions en Redis | Sin estado compartido entre instancias |
| GDPR: hash de IPs | Almacenar IP | Cumplimiento legal, reversibilidad imposible |
| Rust stable (1.81) | Nightly | Reproducibilidad en CI, sin sorpresas |

## Cómo añadir un nuevo endpoint

1. Definir la use case en `crates/core/src/services/`
2. Si necesita DB: añadir método en `crates/db/src/pg_*_repo.rs` con `query_as!`
3. Ejecutar `cargo sqlx prepare --workspace` si hay queries nuevas
4. Añadir handler en `crates/server/src/handlers/` con `#[utoipa::path]`
5. Registrar la ruta en `crates/server/src/router.rs`
6. Añadir tests de integración en `crates/server/tests/integration/`
````

---

## Release `v1.0.0`

### Proceso con `cargo-release`

```toml
# release.toml — configuración de cargo-release
[package]
consolidate-commits  = true
pre-release-commit-message = "chore: release {{version}}"
tag-name             = "v{{version}}"
tag-message          = "LinkMetrics {{version}}"

# Actualizar automáticamente el CHANGELOG.md
pre-release-replacements = [
    { file = "CHANGELOG.md", search = "## \\[Unreleased\\]", replace = "## [Unreleased]\n\n## [{{version}}] - {{date}}" },
]

# Publicar en crates.io si es una crate de librería (no para bins)
publish = false  # true si publicas crates/core en crates.io
```

```bash
# 1. Verificar que CI pasa en main
gh run list --branch main --limit 5

# 2. Ver qué va a hacer release antes de ejecutar
cargo release 1.0.0 --dry-run

# 3. Ejecutar release (actualiza versión, commit, tag, push)
cargo release 1.0.0 --execute

# 4. cargo-dist: construir binarios release para todas las plataformas
cargo dist build --artifacts=all
# Genera: dist/artifacts/lm-v1.0.0-x86_64-unknown-linux-gnu.tar.gz
#                          lm-v1.0.0-aarch64-unknown-linux-gnu.tar.gz
#                          lm-v1.0.0-x86_64-apple-darwin.tar.gz
#                          etc.

# 5. Crear GitHub Release con los binarios y el SBOM
gh release create v1.0.0 \
  --title "LinkMetrics v1.0.0" \
  --notes-file CHANGELOG.md \
  dist/artifacts/*.tar.gz \
  sbom.json

# 6. Firmar la imagen Docker con cosign
cosign sign --key cosign.key ghcr.io/usuario/linkmetrics:v1.0.0

# 7. Verificar la firma (lo que haría un usuario)
cosign verify --key cosign.pub ghcr.io/usuario/linkmetrics:v1.0.0
```

### CHANGELOG.md

```markdown
# Changelog
Formato: [Keep a Changelog](https://keepachangelog.com/es/1.0.0/).
Versionado: [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2026-06-24

### Added
- API REST completa: CRUD de links, analytics, health/readiness, métricas Prometheus
- Autenticación JWT con rotación de claves JWKS
- Rate limiting por API Key con Redis Token Bucket (Lua atómico)
- Actor Model sharded para registro de clicks de alta frecuencia
- CLI admin: migrate, stats, check, user create/delete
- Observabilidad completa: JSON logs, OpenTelemetry → Jaeger, métricas RED/USE
- Swagger UI en /docs (dev/staging)
- Imagen Docker distroless < 25 MB firmada con cosign

### Security
- IPs hasheadas con SHA-256 + salt diario (GDPR art. 5)
- STRIDE threat model documentado en docs/threat-model.md
- SBOM generado y publicado con cada release
- cargo audit + cargo deny en CI

[Unreleased]: https://github.com/usuario/linkmetrics/compare/v1.0.0...HEAD
[1.0.0]:      https://github.com/usuario/linkmetrics/releases/tag/v1.0.0
```

---

## La charla técnica de 10 minutos

### Guion

```text
ESTRUCTURA (10 minutos exactos):

  00:00 – 00:30  INTRO (30 segundos)
  ────────────────────────────────────
  "Hola, soy [nombre]. Construí LinkMetrics: una plataforma de URL shortening
  con analytics en tiempo real. Stack: Rust 1.81, Axum, SQLx, PostgreSQL,
  Redis, Docker. Voy a mostrar las 3 decisiones técnicas más interesantes."
  
  No describas features — describe DECISIONES.

  00:30 – 02:30  ARQUITECTURA (2 minutos)
  ────────────────────────────────────────
  Abre ARCHITECTURE.md o el diagrama C4.
  "La decisión más importante: crates/core no importa sqlx, axum ni tokio.
  El dominio es independiente del framework. ¿Por qué?"
  Muestra un test de core que corre en < 50ms sin DB.
  "Esto me permite cambiar de PostgreSQL a cualquier otra DB en < 4 horas."

  02:30 – 06:30  DEEP DIVE (4 minutos) — el 'meat'
  ────────────────────────────────────────────────────
  Elige DOS de estas opciones según tu ruta:
  
  OPCIÓN 1: Typestate en core
    Abre domain/link.rs.
    "Link<Draft> no tiene target_url(). Link<Expired> tampoco. El compilador
    impide redirigir con un link inactivo. No hay runtime check, es el tipo."
    Muestra el error de compilación si intentas llamarlo.
  
  OPCIÓN 2: Actor model + loom
    Abre el actor sharded de click counter.
    "En campañas virales, 50k clicks/segundo crean hot-spot en PostgreSQL.
    El actor acumula en memoria, batch de 1000 clicks o 5s. Demostración:"
    Corre el benchmark de criterion en vivo.
  
  OPCIÓN 3: JWT + JWKS rotation
    Abre auth/extractor.rs.
    "El extractor AuthUser valida kid del header, busca en JWKS cache,
    refresca si el kid es desconocido (permite rotación sin downtime)."
    Muestra el test que verifica token expirado → 401.
  
  OPCIÓN 4: Observabilidad
    Abre Grafana/Jaeger en vivo.
    "Cada request tiene trace_id correlacionado entre logs, métricas y traces.
    POST /api/v1/links: veo el span de PostgreSQL, el span de Redis y la
    latencia total. Todo sin un solo print! en el código."

  06:30 – 08:00  ESPECIALIZACIÓN (1.5 minutos)
  ──────────────────────────────────────────────
  Muestra tu Ruta (A/B/C/D/E) en funcionamiento:
  - Ruta A: llamada gRPC desde grpcurl, Operator reconciliando en K8s
  - Ruta B: lm plugin install → lm geolookup --help funciona
  - Ruta C: demo en Chrome, DevTools → WASM 52KB comprimido, Lighthouse 97
  - Ruta D: polars procesar 10M de clicks → Parquet → query SQL en < 2s
  - Ruta E: RP2040 conectado, terminal con defmt: "T:235,H:62,C:847"

  08:00 – 09:00  BUG O PROBLEMA DIFÍCIL (1 minuto)
  ────────────────────────────────────────────────
  Cuenta UN bug real que tuviste y cómo lo resolviste.
  Ejemplos reales del curso:
  - "El actor counter tenía race condition bajo carga — loom lo detectó
    en 3 segundos. La solución fue usar SeqCst en vez de Relaxed."
  - "SQLx offline mode no incluía las queries de tests, fallaba en CI.
    Solución: SQLX_OFFLINE=false solo en el job de test con postgres service."
  - "La imagen Docker pesaba 420MB. Cambié a distroless + cargo-chef: 23MB."
  
  Esto demuestra que el proyecto fue real, no un tutorial copiado.

  09:00 – 10:00  LECCIONES Y PRÓXIMOS PASOS (1 minuto)
  ───────────────────────────────────────────────────────
  "Lo que haría distinto desde el principio:"
  - sqlx prepare --check en CI desde el día 1 (lo añadí en la semana 3)
  - cargo-hack para testear feature combinations
  - Lighthouse audit antes de escribir el primer componente (Ruta C)
  
  "Próximos pasos:"
  - Contribuir a [tokio/axum/leptos] con un PR de [issue que encontré]
  - Implementar la Ruta [la que no elegiste que más te llama]
  - Publicar crates/core en crates.io
```

### Tips para la grabación

```text
SETUP RECOMENDADO:
  ✅ Fuente: JetBrains Mono / Fira Code, tamaño ≥ 18px
  ✅ Terminal: colores de alto contraste (Dracula/Catppuccin)
  ✅ Grabación: OBS / Asciinema (solo terminal) / Loom (pantalla + cámara)
  ✅ Audio: headset con cancelación de ruido — el audio importa más que el video
  ✅ Resolución: 1920×1080 mínimo
  ✅ Duración: 10 minutos ± 30 segundos (ni 8 ni 14)

ERRORES COMUNES:
  ❌ "Voy a compilar... esperen un momento..." [silencio de 45s]
     → Pre-compilar todo antes de grabar. Usa `cargo build` primero.
  ❌ Mostrar archivos de configuración línea por línea
     → Resalta las 3 líneas importantes, el resto en blur/scroll rápido
  ❌ No mostrar el sistema funcionando en producción/staging
     → Siempre termina con un `curl` real contra staging
  ❌ Pedir disculpas por el código ("esto está un poco sucio")
     → El código ya pasó CI. No te disculpes, explica las decisiones.
```

---

## ✅ Checklist global — "Ship It" (Definition of Done)

### Código y arquitectura

- [ ] `cargo test --workspace --all-features` pasa con 0 warnings y 0 errores.
- [ ] `crates/core` no importa `sqlx`, `axum`, `tokio`, `redis` ni `reqwest`.
  Verificar: `grep -r "sqlx\|axum\|tokio" crates/core/src/ | grep -v test`.
- [ ] Todos los handlers de Axum retornan `Result<_, DomainError>`. Ninguno
  tiene `StatusCode::CONFLICT` hardcodeado — todo pasa por `IntoResponse`.
- [ ] `crates/db`: `grep -r "query(" crates/db/src/ | grep -v query_as\|query_scalar\|query!`
  devuelve vacío. Solo macros verificadas.
- [ ] Migraciones idempotentes: `cargo sqlx migrate run` ejecutado dos veces
  no produce error.
- [ ] `SQLX_OFFLINE=true cargo build --workspace` compila sin DB activa.
- [ ] `cargo sqlx prepare --workspace --check` pasa en CI.
- [ ] 0 `unwrap()` en código de producción fuera de tests. Usar `?`, `expect()`
  solo donde el invariante es documentado, o mapear a `DomainError`.

### Tests

- [ ] `crates/core`: ≥ 15 tests, incluyendo ≥ 3 estrategias `proptest`.
- [ ] `crates/db`: tests con `#[sqlx::test]` o `testcontainers` contra DB real.
- [ ] `crates/server`: ≥ 5 tests de integración. Cubren: 200, 401, 404, 409, 201.
- [ ] Test E2E: `cargo test --test e2e` levanta servidor + DB reales y verifica
  el flujo crear → redirigir → analytics.
- [ ] `crates/cli`: `assert_cmd` tests para `--help`, argumentos inválidos y
  al menos un comando exitoso.
- [ ] Cobertura de `crates/core` > 80%: `cargo llvm-cov --package linkmetrics-core`.

### Observabilidad y operabilidad

- [ ] Logs JSON con `trace_id`, `request_id`, `latency_ms`, `status` en cada
  request. Verificar: `cargo run | head -20 | python3 -m json.tool`.
- [ ] `/metrics` expone métricas Prometheus: `http_requests_total`,
  `http_request_duration_seconds`, `db_pool_size`.
- [ ] `/health` → 200 siempre que el proceso viva.
  `/ready` → 503 si PostgreSQL o Redis no responden.
- [ ] Graceful shutdown verificado: `kill -15 $(pgrep linkmetrics-server)` →
  completa requests en vuelo → loguea "Servidor apagado limpiamente" → exit 0.

### DevOps y supply chain

- [ ] CI completo en < 15 minutos: fmt + clippy + test + audit + deny +
  sqlx check + build + docker push.
- [ ] `cargo audit` — 0 vulnerabilidades críticas o altas.
- [ ] `cargo deny check` — todas las licencias en la lista de permitidas.
- [ ] Imagen Docker ≤ 30 MB: `docker image inspect ... | jq '.[0].Size'`.
- [ ] Imagen Docker firmada con cosign: `cosign verify` exitoso.
- [ ] SBOM generado y publicado como artefacto del release.
- [ ] `v1.0.0` tag en git, GitHub Release con binarios para ≥ 2 plataformas.

### Especialización (tu ruta elegida)

- [ ] **Ruta A**: gRPC `tonic` funcional y verificado con `grpcurl`. Operator
  `kube-rs` reconcilia el CRD en un cluster local (kind/minikube).
- [ ] **Ruta B**: `lm plugin install` instala un plugin externo. El plugin
  aparece en `lm --help`. `lm update` verifica firma cosign antes de aplicar.
- [ ] **Ruta C**: Lighthouse score ≥ 95 en Performance. Bundle WASM < 100 KB
  comprimido. Informe de `twiggy dominators` en `docs/wasm-analysis.md`.
- [ ] **Ruta D**: Pipeline `polars` procesa ≥ 1M filas → Parquet en < 10s.
  Benchmark vs pandas documentado en README. Endpoint `/api/v1/embed` funcional.
- [ ] **Ruta E**: Binario flasheado en hardware real o QEMU. `defmt` emite
  logs legibles via probe-rs. Core sin `std` compila:
  `cargo build -p linkmetrics-core --target thumbv6m-none-eabi`.

### Portfolio y comunicación

- [ ] Repositorio público en GitHub. `README.md` tiene badges de CI, licencia
  y versión. Quickstart funciona en una terminal limpia en < 5 minutos.
- [ ] `ARCHITECTURE.md` explica al menos 3 decisiones técnicas con alternativas
  descartadas y trade-offs cuantificados donde sea posible.
- [ ] Video de demo de 10 minutos grabado, editado y enlazado en `README.md`.
  Audio claro, sin silencios de compilación, sistema funcionando en staging.
- [ ] `CHANGELOG.md` en formato Keep a Changelog con la entrada `v1.0.0`.
- [ ] `SECURITY.md` con email de contacto y proceso de disclosure.

---

## Cierre del curso: ¿y ahora qué?

Has completado seis meses de ingeniería de software con Rust. No eres
"alguien que está aprendiendo Rust". Eres un ingeniero que puede diseñar,
implementar, testear y desplegar sistemas reales con Rust.

```text
TU TOOLKIT MENTAL — AHORA INCLUYE:

  Ownership & Borrowing   → modelo mental por defecto, no sintaxis memorizada
  Async Rust              → State Machines + Executors, entendido desde adentro
  Traits & Generics       → abstracción de costo cero, sin herencia de clases
  Concurrencia            → Send/Sync, Mutex vs Actor vs Atomics, lock-free
  FFI / Wasm / Embedded   → cruzar fronteras con seguridad
  Observabilidad          → tracing + metrics + OTel como ciudadanos de primera clase
  Calidad industrial      → clippy, miri, loom, proptest, criterion, cargo-audit
  Arquitectura            → Typestate, Hexagonal, Actor Model, ADRs, Monorepo

PRÓXIMOS PASOS REALES (en orden de impacto):

  1. CONTRIBUYE A OPEN SOURCE EN LAS PRÓXIMAS 2 SEMANAS
     Busca `good first issue` + `help wanted` en:
     - tokio-rs/tokio, tokio-rs/axum
     - launchbadge/sqlx
     - pola-rs/polars
     - embassy-rs/embassy
     Tu primer PR merged es tu certificado real.

  2. PUBLICA UN CRATE EN CRATES.IO ESTE MES
     Pequeño, enfocado, bien documentado. Un extractor de Axum, un
     middleware de Tower, un helper de SQLx. La documentación importa
     más que el código a este punto.

  3. ESCRIBE UN POST TÉCNICO
     "Cómo resolví [el bug más interesante del capstone]"
     500-800 palabras, con código real. Publica en dev.to o tu blog.
     Lo que escribes consolida lo que sabes.

  4. BUSCA TRABAJO / PROYECTOS EN EL MERCADO RUST
     Términos de búsqueda útiles:
     - "Rust Backend Engineer" / "Systems Engineer Rust"
     - "Embedded Rust" / "Rust Firmware Engineer"
     - "Blockchain / Infra Rust"
     Empresas activas en Rust: Cloudflare, AWS, Microsoft, Embark Studios,
     Ferrous Systems, Oxide Computer, Linear, InfluxData, Svix, Shuttle.

  5. MANTENTE AL DÍA SIN BURNOUT
     - "This Week in Rust" (newsletter semanal, 15 minutos)
     - RustConf / EuroRust / RustLab videos (YouTube, gratuitos)
     - rust-lang.org/blog para cambios en el lenguaje
     - EVITA: leer todos los threads de Twitter/Reddit. Eso es ruido.

  6. ENSEÑA — ES LA CONSOLIDACIÓN DEFINITIVA
     Ayuda a alguien en el Discord de Rust con una pregunta de Semana 1.
     Explica ownership a un colega en 5 minutos.
     Mentorea a alguien que empieza.
     Enseñar revela exactamente qué entiendes y qué crees entender.
```

> **Habilidades demostradas al completar este curso:**
>
> ✅ Systems Programming — Ownership, Borrowing, Lifetimes, unsafe con disciplina  
> ✅ Async & Concurrencia — Tokio, Actor Model, Atomics, Lock-Free, loom  
> ✅ Backend Engineering — Axum, SQLx, PostgreSQL, Redis, JWT, OpenAPI  
> ✅ CLI & Developer Experience — Clap, Plugins, Self-update, Completions  
> ✅ WebAssembly — wasm-bindgen, Leptos SSR, Bundle optimization  
> ✅ High Performance — Profiling, SIMD, Cache-aware, Custom Allocators  
> ✅ Software Architecture — Typestate, Hexagonal, ADRs, Monorepo, STRIDE  
> ✅ Production Readiness — Docker, CI/CD, Observabilidad, Supply Chain Security  
> ✅ Specialization Depth — Cloud / Systems / WASM / Data / Embedded

---

> **Siguiente sección:** esta es la sección final del curso. El repositorio
> del capstone es tu siguiente paso.