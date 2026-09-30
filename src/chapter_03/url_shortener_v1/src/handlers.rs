use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Redirect,
};
use std::sync::Arc;

use crate::{
    almacen::AlmacenUrls,
    error::ErrorApp,
    estado::EstadoApp,
    models::{CodigoCorto, EntradaUrl, EstadisticasUrl, RespuestaAcortar, SolicitudAcortar},
};

/// GET /health — Kubernetes liveness probe
pub async fn chequeo_salud() -> &'static str {
    "OK"
}

/// POST /shorten — Crea una URL corta
pub async fn acortar_url<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Json(cuerpo): Json<SolicitudAcortar>,
) -> Result<(StatusCode, Json<RespuestaAcortar>), ErrorApp> {
    // Validación básica de URL
    if !cuerpo.url.starts_with("http://") && !cuerpo.url.starts_with("https://") {
        return Err(ErrorApp::UrlInvalida(
            "la URL debe empezar con http:// o https://".into(),
        ));
    }

    let entrada = EntradaUrl::nueva(cuerpo.url);
    let codigo = entrada.codigo.clone();

    estado
        .almacen
        .guardar(entrada)
        .map_err(|_| ErrorApp::Almacenamiento)?;

    let url_corta = format!("{}/{}", estado.base_url, codigo);

    Ok((
        StatusCode::CREATED,
        Json(RespuestaAcortar { codigo, url_corta }),
    ))
}

/// GET /{codigo} — Redirige a la URL original
pub async fn redirigir<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Path(codigo_str): Path<String>,
) -> Result<Redirect, ErrorApp> {
    let codigo = CodigoCorto(codigo_str);

    let entrada = estado
        .almacen
        .buscar(&codigo)
        .ok_or(ErrorApp::NoEncontrado)?;

    // Incrementar contador en background (no bloquea la respuesta)
    let almacen = estado.almacen.clone();
    let codigo_clone = codigo.clone();
    tokio::spawn(async move {
        almacen.incrementar_clics(&codigo_clone);
    });

    Ok(Redirect::permanent(&entrada.url_orig))
}

/// GET /{codigo}/stats — Estadísticas de una URL
pub async fn estadisticas<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Path(codigo_str): Path<String>,
) -> Result<Json<EstadisticasUrl>, ErrorApp> {
    let codigo = CodigoCorto(codigo_str);
    let entrada = estado
        .almacen
        .buscar(&codigo)
        .ok_or(ErrorApp::NoEncontrado)?;
    Ok(Json(entrada.into()))
}

/// GET /urls — Lista todas las URLs (admin)
pub async fn listar_urls<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
) -> Json<Vec<EstadisticasUrl>> {
    let lista = estado
        .almacen
        .listar_todo()
        .into_iter()
        .map(EstadisticasUrl::from)
        .collect();
    Json(lista)
}
