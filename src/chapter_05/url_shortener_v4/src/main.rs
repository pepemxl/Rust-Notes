use std::sync::Arc;

use tracing_subscriber::EnvFilter;
use url_shortener_v4::{
    actor::contador::ContadorSharded, api::estado::AppState, config::Config, router,
    store::memoria::AlmacenMemoria,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::cargar()?;

    // Aquí se decide la implementación concreta: cambiar a Postgres es cambiar esta línea
    let estado = AppState {
        almacen: Arc::new(AlmacenMemoria::default()),
        contador: ContadorSharded::iniciar(config.actor_shards),
        base_url: config.base_url.clone(),
    };

    let listener = tokio::net::TcpListener::bind((config.host.as_str(), config.port)).await?;
    tracing::info!(
        "URL Shortener v4 en {} ({} shards)",
        listener.local_addr()?,
        config.actor_shards
    );
    axum::serve(listener, router(estado)).await?;
    Ok(())
}
