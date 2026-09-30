use builder::Builder;

#[derive(Debug, PartialEq, Builder)]
struct Configuracion {
    // Campo requerido: build() falla si no se llama a .host()
    host: String,

    // Campo opcional con valor por defecto literal
    #[builder(default = "8080")]
    puerto: u16,

    // Campo opcional que usa Default::default() del tipo
    #[builder(default)]
    reintentos: u32, // → 0

    // Vec acumulativo: se llama .cabecera() varias veces
    #[builder(each = "cabecera")]
    cabeceras: Vec<String>,
}

#[test]
fn construccion_completa() {
    let cfg = Configuracion::builder()
        .host("api.ejemplo.com".to_string())
        .puerto(9090)
        .reintentos(3)
        .cabecera("X-Request-ID: abc".to_string())
        .cabecera("Accept: application/json".to_string())
        .build()
        .unwrap();

    assert_eq!(cfg.host, "api.ejemplo.com");
    assert_eq!(cfg.puerto, 9090);
    assert_eq!(cfg.reintentos, 3);
    assert_eq!(cfg.cabeceras.len(), 2);
}

#[test]
fn valores_por_defecto() {
    let cfg = Configuracion::builder()
        .host("localhost".to_string())
        .build()
        .unwrap();

    assert_eq!(cfg.puerto, 8080); // default = "8080"
    assert_eq!(cfg.reintentos, 0); // Default::default()
    assert!(cfg.cabeceras.is_empty()); // Vec vacío
}

#[test]
fn campo_requerido_faltante() {
    let error = Configuracion::builder().build().unwrap_err();
    assert_eq!(error, "falta el campo requerido `host`");
}

#[test]
fn each_acumula_elementos() {
    let cfg = Configuracion::builder()
        .host("x.com".to_string())
        .cabecera("A: 1".to_string())
        .cabecera("B: 2".to_string())
        .cabecera("C: 3".to_string())
        .build()
        .unwrap();

    assert_eq!(cfg.cabeceras, vec!["A: 1", "B: 2", "C: 3"]);
}

// ── Genéricos: el builder hereda los parámetros y los bounds ───────────────

#[derive(Debug, Builder)]
struct Paginado<T: Clone> {
    datos: Vec<T>,
    #[builder(default = "1")]
    pagina: usize,
    #[builder(default = "20")]
    por_pagina: usize,
}

#[test]
fn builder_con_genericos() {
    let p = Paginado::<String>::builder()
        .datos(vec!["a".to_string(), "b".to_string()])
        .pagina(3)
        .build()
        .unwrap();

    assert_eq!(p.datos.len(), 2);
    assert_eq!(p.pagina, 3);
    assert_eq!(p.por_pagina, 20); // default

    let p = Paginado::<u8>::builder()
        .datos(vec![])
        .por_pagina(50)
        .build()
        .unwrap();
    assert_eq!((p.pagina, p.por_pagina), (1, 50));
}
