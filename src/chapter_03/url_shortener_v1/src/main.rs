mod almacen;
mod error;
mod estado;
mod handlers;
mod models;

use almacen::AlmacenMemoria;
use estado::EstadoApp;
use handlers::*;

use axum::{
    Router,
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

// ── Middleware de telemetría ───────────────────────────────────────────────

async fn telemetria(req: Request, next: Next) -> Response {
    let inicio = Instant::now();
    let metodo = req.method().clone();
    let uri = req.uri().path().to_owned();

    let resp = next.run(req).await;

    tracing::info!(
        metodo = %metodo,
        ruta   = %uri,
        estado = resp.status().as_u16(),
        ms     = inicio.elapsed().as_millis(),
        "petición"
    );

    resp
}

// ── Entry point ───────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // Logs estructurados: RUST_LOG=debug cargo run
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    let estado = EstadoApp::nuevo(AlmacenMemoria::nuevo(), base_url);

    let app = Router::new()
        .route("/health", get(chequeo_salud))
        .route("/shorten", post(acortar_url::<AlmacenMemoria>))
        .route("/urls", get(listar_urls::<AlmacenMemoria>))
        .route("/{codigo}", get(redirigir::<AlmacenMemoria>))
        .route("/{codigo}/stats", get(estadisticas::<AlmacenMemoria>))
        .layer(middleware::from_fn(telemetria))
        .layer(CorsLayer::permissive())
        .with_state(estado);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("servidor escuchando en {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
