use sqlx::PgPool;
use url_shortener::almacen::{AlmacenPostgres, AlmacenUrls};
use url_shortener::models::EntradaUrl;

// #[sqlx::test] crea una base de datos nueva para cada test, le aplica las
// migraciones y la borra al terminar. Necesita DATABASE_URL al ejecutarse, por eso
// está marcado #[ignore]: `DATABASE_URL=postgres://... cargo test -- --ignored`
#[sqlx::test(migrations = "./migrations")]
#[ignore = "necesita DATABASE_URL (PostgreSQL local)"]
async fn test_con_sqlx_test(pool: PgPool) {
    let almacen = AlmacenPostgres::nuevo(pool);
    let entrada = EntradaUrl::nueva("https://ferris.rs".into());
    let codigo = entrada.codigo.clone();

    almacen.guardar(entrada).await.unwrap();

    let encontrada = almacen.buscar(&codigo).await.unwrap();
    assert_eq!(encontrada.url_orig, "https://ferris.rs");
}
