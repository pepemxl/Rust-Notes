use dashmap::DashMap;
use sqlx::PgPool;
use std::future::Future;
use std::sync::Arc;

use crate::{
    error::ErrorApp,
    models::{CodigoCorto, EntradaUrl, FilaUrl},
};

/// Trait de almacenamiento. Cambio respecto a la Semana 10: los métodos ahora son
/// async, porque hablar con PostgreSQL es I/O. `-> impl Future + Send` permite que
/// los handlers genéricos de Axum (que exigen futures `Send`) acepten cualquier `S`.
pub trait AlmacenUrls: Send + Sync + 'static {
    fn guardar(&self, entrada: EntradaUrl) -> impl Future<Output = Result<(), ErrorApp>> + Send;
    fn buscar(&self, codigo: &CodigoCorto) -> impl Future<Output = Option<EntradaUrl>> + Send;
    fn incrementar_clics(&self, codigo: &CodigoCorto) -> impl Future<Output = Option<u64>> + Send;
    fn listar_todo(&self) -> impl Future<Output = Vec<EntradaUrl>> + Send;
}

/// La implementación en memoria sigue siendo útil para tests sin base de datos.
/// Solo cambia la firma: `async fn`, aunque por dentro no espere nada.
#[derive(Clone, Default)]
pub struct AlmacenMemoria {
    mapa: Arc<DashMap<String, EntradaUrl>>,
}

impl AlmacenMemoria {
    pub fn nuevo() -> Self {
        Self::default()
    }
}

impl AlmacenUrls for AlmacenMemoria {
    async fn guardar(&self, entrada: EntradaUrl) -> Result<(), ErrorApp> {
        self.mapa.insert(entrada.codigo.0.clone(), entrada);
        Ok(())
    }

    async fn buscar(&self, codigo: &CodigoCorto) -> Option<EntradaUrl> {
        self.mapa.get(&codigo.0).map(|r| r.clone())
    }

    async fn incrementar_clics(&self, codigo: &CodigoCorto) -> Option<u64> {
        self.mapa.get_mut(&codigo.0).map(|mut r| {
            r.clics += 1;
            r.clics
        })
    }

    async fn listar_todo(&self) -> Vec<EntradaUrl> {
        self.mapa.iter().map(|r| r.clone()).collect()
    }
}

/// Implementación con PostgreSQL + SQLx
#[derive(Clone)]
pub struct AlmacenPostgres {
    pool: PgPool,
}

impl AlmacenPostgres {
    pub fn nuevo(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl AlmacenUrls for AlmacenPostgres {
    async fn guardar(&self, entrada: EntradaUrl) -> Result<(), ErrorApp> {
        sqlx::query!(
            r#"
            INSERT INTO urls (code, target_url, created_at, expires_at, clicks)
            VALUES ($1, $2, $3, $4, 0)
            ON CONFLICT (code) DO NOTHING
            "#,
            entrada.codigo.as_str(),
            entrada.url_orig,
            entrada.creada_en,
            entrada.expira_en,
        )
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorApp::Almacenamiento)?;
        Ok(())
    }

    async fn buscar(&self, codigo: &CodigoCorto) -> Option<EntradaUrl> {
        sqlx::query_as!(
            FilaUrl,
            "SELECT code, target_url, created_at, expires_at, clicks
             FROM urls WHERE code = $1",
            codigo.as_str()
        )
        .fetch_optional(&self.pool)
        .await
        .ok()?
        .map(EntradaUrl::from)
    }

    async fn incrementar_clics(&self, codigo: &CodigoCorto) -> Option<u64> {
        sqlx::query_scalar!(
            "UPDATE urls SET clicks = clicks + 1 WHERE code = $1 RETURNING clicks",
            codigo.as_str()
        )
        .fetch_optional(&self.pool)
        .await
        .ok()?
        .map(|n| n as u64)
    }

    async fn listar_todo(&self) -> Vec<EntradaUrl> {
        sqlx::query_as!(
            FilaUrl,
            "SELECT code, target_url, created_at, expires_at, clicks
             FROM urls ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(EntradaUrl::from)
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mismo trait, sin PostgreSQL: los handlers no notan la diferencia.
    #[tokio::test]
    async fn memoria_cuenta_clics() {
        let almacen = AlmacenMemoria::nuevo();
        let entrada = EntradaUrl::nueva("https://www.rust-lang.org".into());
        let codigo = entrada.codigo.clone();
        almacen.guardar(entrada).await.unwrap();

        assert_eq!(almacen.incrementar_clics(&codigo).await, Some(1));
        assert_eq!(almacen.buscar(&codigo).await.unwrap().clics, 1);
    }
}
