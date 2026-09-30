use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Redirect,
};
use chrono::{Duration, Utc};
use std::sync::Arc;

use crate::{
    almacen::AlmacenUrls,
    error::ErrorApp,
    estado::EstadoApp,
    models::{CodigoCorto, EntradaUrl, EstadisticasUrl, RespuestaAcortar, SolicitudAcortar},
};

pub async fn chequeo_salud() -> &'static str {
    "OK"
}

pub async fn acortar_url<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Json(cuerpo): Json<SolicitudAcortar>,
) -> Result<(StatusCode, Json<RespuestaAcortar>), ErrorApp> {
    if !cuerpo.url.starts_with("http://") && !cuerpo.url.starts_with("https://") {
        return Err(ErrorApp::UrlInvalida(
            "debe empezar con http:// o https://".into(),
        ));
    }

    let expira_en = cuerpo
        .expira_en_segundos
        .map(|s| Utc::now() + Duration::seconds(s as i64));

    let mut entrada = EntradaUrl::nueva(cuerpo.url);
    entrada.expira_en = expira_en;

    let codigo = entrada.codigo.clone();
    estado
        .almacen
        .guardar(entrada)
        .await
        .map_err(|_| ErrorApp::Almacenamiento)?;

    let url_corta = format!("{}/{}", estado.base_url, codigo);
    let expira_ts = expira_en.map(|dt| dt.timestamp());

    Ok((
        StatusCode::CREATED,
        Json(RespuestaAcortar {
            codigo,
            url_corta,
            expira_en: expira_ts,
        }),
    ))
}

pub async fn redirigir<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Path(codigo_str): Path<String>,
) -> Result<Redirect, ErrorApp> {
    let codigo = CodigoCorto(codigo_str);
    let entrada = estado
        .almacen
        .buscar(&codigo)
        .await
        .ok_or(ErrorApp::NoEncontrado)?;

    // Verificar expiración
    if let Some(exp) = entrada.expira_en {
        if Utc::now() > exp {
            return Err(ErrorApp::NoEncontrado);
        }
    }

    let almacen = estado.almacen.clone();
    let cod = codigo.clone();
    tokio::spawn(async move {
        almacen.incrementar_clics(&cod).await;
    });

    Ok(Redirect::permanent(&entrada.url_orig))
}

pub async fn estadisticas<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
    Path(codigo_str): Path<String>,
) -> Result<Json<EstadisticasUrl>, ErrorApp> {
    let codigo = CodigoCorto(codigo_str);
    let entrada = estado
        .almacen
        .buscar(&codigo)
        .await
        .ok_or(ErrorApp::NoEncontrado)?;
    Ok(Json(entrada.into()))
}

pub async fn listar_urls<S: AlmacenUrls>(
    State(estado): State<Arc<EstadoApp<S>>>,
) -> Json<Vec<EstadisticasUrl>> {
    let lista = estado
        .almacen
        .listar_todo()
        .await
        .into_iter()
        .map(EstadisticasUrl::from)
        .collect();
    Json(lista)
}
