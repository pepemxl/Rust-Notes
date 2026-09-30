// Los módulos de negocio son los de la Semana 11 (librería `url_shortener`);
// esta semana solo añade observabilidad, health checks y apagado graceful.
use url_shortener::almacen::AlmacenPostgres;
use url_shortener::estado::EstadoApp;
use url_shortener::handlers::{acortar_url, estadisticas, listar_urls, redirigir};

use axum::{
    Router,
    extract::{MatchedPath, Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sqlx::PgPool;
use std::time::Instant;
use tokio::signal;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

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
    PrometheusBuilder::new()
        .install_recorder()
        .expect("no se pudo instalar el recorder de Prometheus")
}

async fn handler_metricas(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

// ── Health Checks ─────────────────────────────────────────────────────────

async fn liveness() -> &'static str {
    "OK"
}

async fn readiness(State(pool): State<PgPool>) -> impl IntoResponse {
    use axum::http::StatusCode;
    use serde_json::json;
    match pool.acquire().await {
        Ok(_) => (
            StatusCode::OK,
            axum::Json(json!({"status":"ready","db":"ok"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(json!({"status":"not_ready","db":e.to_string()})),
        )
            .into_response(),
    }
}

// ── Middleware ────────────────────────────────────────────────────────────

async fn middleware_observabilidad(req: Request, next: Next) -> Response {
    let inicio = Instant::now();
    let metodo = req.method().to_string();
    // La plantilla de la ruta ("/{codigo}"), no la URL real ("/L4fz9R14"): si cada
    // código fuera una etiqueta distinta, Prometheus crearía una serie por URL.
    let ruta = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "sin_ruta".into());

    let resp = next.run(req).await;

    let estado = resp.status().as_u16().to_string();
    let latencia = inicio.elapsed().as_secs_f64();

    tracing::info!(
        metodo = %metodo, ruta = %ruta,
        estado = %estado, ms = (latencia * 1000.0) as u64,
        "petición completada"
    );

    counter!("http_requests_total",
        "method" => metodo.clone(), "route" => ruta.clone(), "status" => estado
    )
    .increment(1);

    histogram!("http_request_duration_seconds",
        "method" => metodo, "route" => ruta
    )
    .record(latencia);

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
            .recv()
            .await;
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

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL debe estar definida");
    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    // Pool con tamaño apropiado para el número de workers del runtime
    let pool = sqlx::postgres::PgPoolOptions::new()
        .min_connections(2)
        .max_connections(20)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("migraciones aplicadas");

    let almacen = AlmacenPostgres::nuevo(pool.clone());
    let estado = EstadoApp::nuevo(almacen, base_url);

    let app = Router::new()
        // Endpoints de negocio
        .route("/shorten", post(acortar_url::<AlmacenPostgres>))
        .route("/urls", get(listar_urls::<AlmacenPostgres>))
        .route("/{codigo}", get(redirigir::<AlmacenPostgres>))
        .route("/{codigo}/stats", get(estadisticas::<AlmacenPostgres>))
        .with_state(estado)
        // Endpoints de infraestructura (estado propio, no el del negocio)
        .route("/health", get(liveness))
        .route("/ready", get(readiness).with_state(pool))
        .route(
            "/metrics",
            get(handler_metricas).with_state(metricas_handle),
        )
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
