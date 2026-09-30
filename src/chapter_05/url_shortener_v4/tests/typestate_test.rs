use url_shortener_v4::domain::url_entry::{Active, Draft, EntradaUrl, Expired, UrlCode, UrlEntry};

#[test]
fn ciclo_de_vida_completo() {
    let code = UrlCode::parse("mi-url").unwrap();
    let draft = UrlEntry::<Draft>::nueva(code, "https://ejemplo.com");

    // Draft → Active
    let mut activa = draft.publicar();
    assert_eq!(activa.clicks, 0);
    assert_eq!(activa.url_destino(), "https://ejemplo.com");

    // Active: registrar clicks
    activa.registrar_click();
    activa.registrar_click();
    activa.registrar_click();
    assert_eq!(activa.clicks, 3);

    // Active → Expired: los clicks se conservan
    let expirada = activa.expirar();
    assert_eq!(expirada.clicks_finales(), 3);
}

#[test]
fn tamanos_iguales_por_phantomdata() {
    use std::mem::size_of;
    assert_eq!(size_of::<UrlEntry<Draft>>(), size_of::<UrlEntry<Active>>());
    assert_eq!(
        size_of::<UrlEntry<Active>>(),
        size_of::<UrlEntry<Expired>>()
    );
}

#[test]
fn tipo_borrado_solo_cuenta_clicks_activos() {
    let code = UrlCode::parse("abc").unwrap();
    let mut e = EntradaUrl::nueva_activa(code, "https://ejemplo.com");

    assert!(e.registrar_click()); // activa: cuenta
    assert_eq!(e.clicks(), 1);

    let mut expirada = e.expirar();
    assert!(expirada.url_destino_activa().is_none());
    assert!(!expirada.registrar_click()); // expirada: no cuenta
    assert_eq!(expirada.clicks(), 1);
}

#[test]
fn url_code_valida_caracteres() {
    assert!(UrlCode::parse("hola-mundo_123").is_ok());
    assert!(UrlCode::parse("").is_err());
    assert!(UrlCode::parse("url con espacio").is_err());
    assert!(UrlCode::parse("url/slash").is_err());
    assert!(UrlCode::parse("a".repeat(33)).is_err());
}
