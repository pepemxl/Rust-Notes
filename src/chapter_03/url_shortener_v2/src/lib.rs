//! Url Shortener v2: los módulos viven en la librería para que los tests de
//! integración (`tests/`) puedan usarlos; `main.rs` solo arma el servidor.
pub mod almacen;
pub mod error;
pub mod estado;
pub mod handlers;
pub mod models;
