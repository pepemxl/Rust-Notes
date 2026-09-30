use super::estado::AppState;
use crate::domain::url_entry::{EntradaUrl, UrlCode};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Redirect, Response},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CrearUrlRequest {
    pub target: String,
    pub code: Option<String>,
}

#[derive(Serialize)]
pub struct CrearUrlResponse {
    pub code: String,
    pub short_url: String,
}

/// `POST /url` → 201 con el código, 400 si el código es inválido, 409 si ya existe.
pub async fn crear_url(
    State(state): State<AppState>,
    Json(body): Json<CrearUrlRequest>,
) -> Response {
    let code = match body.code {
        Some(c) => match UrlCode::parse(c) {
            Ok(c) => c,
            Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
        },
        None => UrlCode::nueva_aleatoria(),
    };

    let entrada = EntradaUrl::nueva_activa(code.clone(), body.target);

    match state.almacen.guardar(entrada).await {
        Ok(()) => {
            let respuesta = CrearUrlResponse {
                short_url: format!("{}/{code}", state.base_url),
                code: code.to_string(),
            };
            (StatusCode::CREATED, Json(respuesta)).into_response()
        }
        Err(e) => (StatusCode::CONFLICT, e.to_string()).into_response(),
    }
}

/// `GET /{code}` → 307 al destino si la URL está activa; 404 si no existe o expiró.
pub async fn redirigir(State(state): State<AppState>, Path(code): Path<String>) -> Response {
    let Ok(code) = UrlCode::parse(code) else {
        return StatusCode::BAD_REQUEST.into_response();
    };

    match state.almacen.registrar_click(&code).await {
        Some(destino) => {
            state.contador.incrementar(code).await;
            Redirect::temporary(&destino).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `DELETE /{code}` → 204 si la URL pasó a `Expired`; 404 si no estaba activa.
pub async fn expirar(State(state): State<AppState>, Path(code): Path<String>) -> StatusCode {
    let Ok(code) = UrlCode::parse(code) else {
        return StatusCode::BAD_REQUEST;
    };

    if state.almacen.expirar(&code).await {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// `GET /admin/stats` → total de URLs y clicks por código (del actor sharded).
pub async fn estadisticas(State(state): State<AppState>) -> Json<serde_json::Value> {
    let total_urls = state.almacen.total_urls().await;
    let clicks = state.contador.snapshot().await;

    Json(serde_json::json!({
        "total_urls": total_urls,
        "clicks":     clicks,
    }))
}
