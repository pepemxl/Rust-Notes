use url_shortener::almacen::AlmacenPostgres;
use url_shortener::estado::EstadoApp;
use url_shortener::handlers::*;

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

async fn telemetria(req: Request, next: Next) -> Response {
    let inicio = Instant::now();
    let metodo = req.method().clone();
    let uri = req.uri().path().to_owned();
    let resp = next.run(req).await;
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

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL debe estar definida");
    let base_url = std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".into());

    // Crear pool y aplicar migraciones
    let pool = sqlx::PgPool::connect(&database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let almacen = AlmacenPostgres::nuevo(pool.clone());
    let estado = EstadoApp::nuevo(almacen, base_url);

    let app = Router::new()
        .route("/health", get(chequeo_salud))
        .route("/shorten", post(acortar_url::<AlmacenPostgres>))
        .route("/urls", get(listar_urls::<AlmacenPostgres>))
        .route("/{codigo}", get(redirigir::<AlmacenPostgres>))
        .route("/{codigo}/stats", get(estadisticas::<AlmacenPostgres>))
        .layer(middleware::from_fn(telemetria))
        .layer(CorsLayer::permissive())
        .with_state(estado);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("servidor en {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
