//! URL Shortener v4: el servicio de las Semanas 10–12 refactorizado con
//! Newtype + Typestate (dominio), un actor sharded (clicks) y DI por traits (almacén).

pub mod actor;
pub mod api;
pub mod config;
pub mod domain;
pub mod store;

use axum::{
    Router,
    routing::{get, post},
};

use api::estado::AppState;
use api::handlers::{crear_url, estadisticas, expirar, redirigir};

/// El Router completo. Vive en la librería para que los tests lo usen sin abrir un puerto.
pub fn router(estado: AppState) -> Router {
    Router::new()
        .route("/url", post(crear_url))
        .route("/{code}", get(redirigir).delete(expirar))
        .route("/admin/stats", get(estadisticas))
        .with_state(estado)
}
