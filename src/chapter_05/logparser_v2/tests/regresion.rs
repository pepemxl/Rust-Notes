use std::borrow::Cow;

use logparser_v2::aggregate::{EstadisticasAntes, EstadisticasDespues};
use logparser_v2::lineas_de_prueba;
use logparser_v2::output::{escribir_json_antes, escribir_json_despues};
use logparser_v2::parser::{decodificar_url, parsear_linea, parsear_linea_antes};

#[test]
fn parser_extrae_todos_los_campos() {
    let linea = r#"192.168.1.1 - - [01/Jan/2024:00:00:00 +0000] "GET /api/items HTTP/1.1" 200 1024 "-" "curl/7""#;

    let (_, e) = parsear_linea(linea).unwrap();
    assert_eq!(e.ip, "192.168.1.1");
    assert_eq!(e.metodo, "GET");
    assert_eq!(e.ruta, "/api/items");
    assert_eq!(e.estado, 200);
    assert_eq!(e.bytes, Some(1024));
}

#[test]
fn cow_borrowed_sin_encoding_owned_con_encoding() {
    assert!(matches!(decodificar_url("/sin/encoding"), Cow::Borrowed(_)));

    let decodificada = decodificar_url("/con%20encoding%2Fy%3F");
    assert!(matches!(decodificada, Cow::Owned(_)));
    assert_eq!(decodificada, "/con encoding/y?");

    // Un % que no va seguido de dos dígitos hex se conserva
    assert_eq!(decodificar_url("/100%"), "/100%");
    assert_eq!(decodificar_url("/%zz"), "/%zz");
    assert_eq!(decodificar_url("/%+1"), "/%+1");
}

// La regla de oro de una optimización: el resultado no cambia.
#[test]
fn parser_antes_y_despues_coinciden() {
    for linea in lineas_de_prueba(1_000) {
        let (_, antes) = parsear_linea_antes(&linea).unwrap();
        let (_, despues) = parsear_linea(&linea).unwrap();
        assert_eq!(antes.ip, despues.ip);
        assert_eq!(antes.metodo, despues.metodo);
        assert_eq!(antes.ruta, despues.ruta);
        assert_eq!(antes.estado, despues.estado);
        assert_eq!(antes.bytes, despues.bytes);
    }
}

#[test]
fn agregacion_antes_y_despues_coinciden() {
    let lineas = lineas_de_prueba(10_000);
    let mut antes = EstadisticasAntes::default();
    let mut despues = EstadisticasDespues::default();

    for linea in &lineas {
        let (_, e) = parsear_linea(linea).unwrap();
        antes.registrar(e.ip.to_string(), e.ruta.to_string(), e.estado);
        despues.registrar(e.ip, &e.ruta, e.estado);
    }

    assert_eq!(antes.top_rutas(10), despues.top_rutas(10));
    assert_eq!(antes.por_ip.len(), 250);
    assert_eq!(despues.por_ip.len(), 250);
    assert_eq!(antes.por_estado, despues.por_estado.into_iter().collect());
}

#[test]
fn top_rutas_ordena_por_conteo() {
    let mut stats = EstadisticasDespues::default();
    stats.registrar("1.1.1.1", "/api", 200);
    stats.registrar("1.1.1.1", "/api", 404);
    stats.registrar("2.2.2.2", "/home", 200);

    assert_eq!(stats.top_rutas(5), vec![("/api", 2), ("/home", 1)]);
}

#[test]
fn output_antes_y_despues_producen_los_mismos_bytes() {
    let lineas = lineas_de_prueba(500);
    let entradas: Vec<_> = lineas.iter().map(|l| parsear_linea(l).unwrap().1).collect();

    let mut antes = Vec::new();
    let mut despues = Vec::new();
    escribir_json_antes(&entradas, &mut antes).unwrap();
    escribir_json_despues(&entradas, &mut despues).unwrap();

    assert_eq!(antes, despues);
    let primera: serde_json::Value =
        serde_json::from_slice(antes.split(|&b| b == b'\n').next().unwrap()).unwrap();
    assert_eq!(primera["ruta"], "/buscar?q=rust async 0");
}
