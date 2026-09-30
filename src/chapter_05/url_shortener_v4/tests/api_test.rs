use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use tower::ServiceExt; // oneshot()
use url_shortener_v4::{
    actor::contador::ContadorSharded, api::estado::AppState, router, store::memoria::AlmacenMemoria,
};

fn app() -> Router {
    router(AppState {
        almacen: Arc::new(AlmacenMemoria::default()),
        contador: ContadorSharded::iniciar(2),
        base_url: "http://test".into(),
    })
}

fn crear(code: &str) -> Request<Body> {
    let json = format!(r#"{{"target":"https://www.rust-lang.org","code":"{code}"}}"#);
    Request::post("/url")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json))
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn crear_redirigir_y_expirar() {
    let app = app(); // Router es Clone y comparte el mismo estado

    let resp = app.clone().oneshot(crear("rust")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let cuerpo = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        cuerpo,
        r#"{"code":"rust","short_url":"http://test/rust"}"#.as_bytes()
    );

    // Código repetido → 409
    let resp = app.clone().oneshot(crear("rust")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Activa → redirige
    let resp = app.clone().oneshot(get("/rust")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        resp.headers()[header::LOCATION],
        "https://www.rust-lang.org"
    );

    // Expirar → 204; una segunda vez ya no está activa → 404
    let borrar = || Request::delete("/rust").body(Body::empty()).unwrap();
    assert_eq!(
        app.clone().oneshot(borrar()).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.clone().oneshot(borrar()).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    // Expirada → ya no redirige
    let resp = app.clone().oneshot(get("/rust")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // Las estadísticas vienen del actor: solo contó el click que sí redirigió
    let resp = app.oneshot(get("/admin/stats")).await.unwrap();
    let cuerpo = resp.into_body().collect().await.unwrap().to_bytes();
    let stats: serde_json::Value = serde_json::from_slice(&cuerpo).unwrap();
    assert_eq!(
        stats,
        serde_json::json!({ "total_urls": 1, "clicks": { "rust": 1 } })
    );
}

#[tokio::test]
async fn codigo_invalido_es_400() {
    let resp = app().oneshot(crear("con espacio")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
