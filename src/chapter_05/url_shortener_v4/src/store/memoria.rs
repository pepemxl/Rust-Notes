use crate::domain::url_entry::{EntradaUrl, UrlCode};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum ErrorAlmacen {
    #[error("el código '{0}' ya existe")]
    Duplicado(UrlCode),
}

// `async-trait` sigue siendo necesario aquí porque usamos `Arc<dyn AlmacenUrls>`:
// los `async fn` nativos en traits (Rust 1.75+) aún no son dyn-compatibles.
// Si solo usaras generics, bastaría con `async fn` nativo sin el atributo.
#[async_trait]
pub trait AlmacenUrls: Send + Sync {
    async fn guardar(&self, url: EntradaUrl) -> Result<(), ErrorAlmacen>;
    async fn obtener(&self, code: &UrlCode) -> Option<EntradaUrl>;
    /// Registra un click si la URL existe y está activa; devuelve su destino.
    async fn registrar_click(&self, code: &UrlCode) -> Option<String>;
    /// Pasa la URL a `Expired`; devuelve `false` si no existía o no estaba activa.
    async fn expirar(&self, code: &UrlCode) -> bool;
    async fn total_urls(&self) -> usize;
}

#[derive(Clone, Default)]
pub struct AlmacenMemoria {
    mapa: Arc<DashMap<String, EntradaUrl>>,
}

#[async_trait]
impl AlmacenUrls for AlmacenMemoria {
    async fn guardar(&self, url: EntradaUrl) -> Result<(), ErrorAlmacen> {
        // entry() bloquea el shard: comprobar e insertar es una sola operación
        match self.mapa.entry(url.codigo().to_string()) {
            dashmap::Entry::Occupied(_) => Err(ErrorAlmacen::Duplicado(url.codigo().clone())),
            dashmap::Entry::Vacant(hueco) => {
                hueco.insert(url);
                Ok(())
            }
        }
    }

    async fn obtener(&self, code: &UrlCode) -> Option<EntradaUrl> {
        // Clonamos: una referencia dentro de DashMap mantendría el shard bloqueado
        self.mapa.get(code.as_str()).map(|r| r.value().clone())
    }

    async fn registrar_click(&self, code: &UrlCode) -> Option<String> {
        let mut entrada = self.mapa.get_mut(code.as_str())?;
        // Click y lectura del destino bajo el mismo guard: nadie puede expirarla en medio
        if entrada.registrar_click() {
            entrada.url_destino_activa().map(str::to_owned)
        } else {
            None
        }
    }

    async fn expirar(&self, code: &UrlCode) -> bool {
        let mut estaba_activa = false;
        // alter() toma el valor por move: justo lo que pide expirar(self)
        self.mapa.alter(code.as_str(), |_, entrada| {
            estaba_activa = entrada.url_destino_activa().is_some();
            entrada.expirar()
        });
        estaba_activa
    }

    async fn total_urls(&self) -> usize {
        self.mapa.len()
    }
}
