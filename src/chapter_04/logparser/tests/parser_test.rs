use logparser::filter::Filtro;
use logparser::parser::nginx::parsear_linea;

const LINEA: &str = r#"192.168.1.100 - alice [01/Jan/2024:10:30:00 +0000] "POST /api/login HTTP/1.1" 401 256 "https://example.com/" "Mozilla/5.0""#;
const LINEA_SIN_REFERER: &str =
    r#"10.0.0.1 - - [01/Jan/2024:00:00:00 +0000] "GET /health HTTP/1.1" 200 12 "-" "-""#;

#[test]
fn parsea_linea_completa() {
    let (_, entrada) = parsear_linea(LINEA).unwrap();
    assert_eq!(entrada.ip, "192.168.1.100");
    assert_eq!(entrada.usuario, Some("alice".to_string()));
    assert_eq!(entrada.metodo, "POST");
    assert_eq!(entrada.ruta, "/api/login");
    assert_eq!(entrada.protocolo, "HTTP/1.1");
    assert_eq!(entrada.estado, 401);
    assert_eq!(entrada.bytes, Some(256));
    assert!(entrada.referer.is_some());
    assert!(entrada.agente.is_some());
}

#[test]
fn parsea_campos_opcionales_guion() {
    let (_, entrada) = parsear_linea(LINEA_SIN_REFERER).unwrap();
    assert_eq!(entrada.usuario, None);
    assert_eq!(entrada.bytes, Some(12));
    assert_eq!(entrada.referer, None);
    assert_eq!(entrada.agente, None);
}

#[test]
fn filtro_estado_minimo() {
    let (_, entrada) = parsear_linea(LINEA).unwrap();
    let filtro = Filtro::parsear("estado:>=400").unwrap();
    assert!(filtro.coincide(&entrada)); // 401 >= 400

    let filtro2 = Filtro::parsear("estado:>=500").unwrap();
    assert!(!filtro2.coincide(&entrada)); // 401 < 500
}

#[test]
fn filtro_ruta_contiene() {
    let (_, entrada) = parsear_linea(LINEA).unwrap();
    assert!(Filtro::parsear("ruta:/api").unwrap().coincide(&entrada));
    assert!(!Filtro::parsear("ruta:/static").unwrap().coincide(&entrada));
}

#[test]
fn filtro_campo_desconocido_falla() {
    assert!(Filtro::parsear("ignorado:valor").is_err());
}

#[test]
fn parsear_linea_malformada_falla() {
    assert!(parsear_linea("esto no es un log nginx").is_err());
}

#[test]
fn parsear_linea_vacia_falla() {
    assert!(parsear_linea("").is_err());
}
