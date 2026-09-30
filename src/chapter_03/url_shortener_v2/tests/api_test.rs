use sqlx::PgPool;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;

// Fixture: crea un PgPool apuntando a un PG efímero
async fn pool_para_test() -> (PgPool, impl Drop) {
    let contenedor = Postgres::default()
        .with_tag("16-alpine")
        .start()
        .await
        .expect("Docker disponible");

    let puerto = contenedor.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{}/postgres", puerto);

    let pool = PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    (pool, contenedor) // contenedor vive mientras el test corre
}

#[tokio::test]
async fn ciclo_completo_crear_y_buscar() {
    let (pool, _contenedor) = pool_para_test().await;

    // Importar nuestros tipos de producción
    use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
    use url_shortener::models::EntradaUrl;

    let almacen = AlmacenPostgres::nuevo(pool);

    let entrada = EntradaUrl::nueva("https://www.rust-lang.org".into());
    let codigo = entrada.codigo.clone();

    // Guardar
    almacen.guardar(entrada).await.expect("guardar falló");

    // Buscar
    let encontrada = almacen.buscar(&codigo).await.expect("no encontrada");
    assert_eq!(encontrada.url_orig, "https://www.rust-lang.org");
    assert_eq!(encontrada.clics, 0);
}

#[tokio::test]
async fn incrementar_clics_atomico() {
    let (pool, _contenedor) = pool_para_test().await;

    use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
    use url_shortener::models::EntradaUrl;

    let almacen = AlmacenPostgres::nuevo(pool);
    let entrada = EntradaUrl::nueva("https://example.com".into());
    let codigo = entrada.codigo.clone();
    almacen.guardar(entrada).await.unwrap();

    // Incrementos concurrentes
    let a = almacen.clone();
    let b = almacen.clone();
    let c = almacen.clone();
    let cod_a = codigo.clone();
    let cod_b = codigo.clone();
    let cod_c = codigo.clone();

    tokio::join!(
        async move { a.incrementar_clics(&cod_a).await },
        async move { b.incrementar_clics(&cod_b).await },
        async move { c.incrementar_clics(&cod_c).await },
    );

    let stats = almacen.buscar(&codigo).await.unwrap();
    assert_eq!(stats.clics, 3); // UPDATE atómico de PG garantiza 3
}

#[tokio::test]
async fn url_inexistente_devuelve_none() {
    let (pool, _contenedor) = pool_para_test().await;

    use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
    use url_shortener::models::CodigoCorto;

    let almacen = AlmacenPostgres::nuevo(pool);
    let result = almacen.buscar(&CodigoCorto("NOEXISTE".into())).await;
    assert!(result.is_none());
}
