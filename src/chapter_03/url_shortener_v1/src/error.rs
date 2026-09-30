use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ErrorApp {
    #[error("URL no encontrada")]
    NoEncontrado,
    #[error("URL inválida: {0}")]
    UrlInvalida(String),
    #[error("error de almacenamiento")]
    Almacenamiento,
}

impl IntoResponse for ErrorApp {
    fn into_response(self) -> Response {
        let (estado, msg) = match &self {
            ErrorApp::NoEncontrado => (StatusCode::NOT_FOUND, self.to_string()),
            ErrorApp::UrlInvalida(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ErrorApp::Almacenamiento => (StatusCode::INTERNAL_SERVER_ERROR, "error interno".into()),
        };
        (estado, Json(json!({ "error": msg }))).into_response()
    }
}
