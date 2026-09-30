use dashmap::DashMap;
use std::sync::Arc;

use crate::error::ErrorApp;
use crate::models::{CodigoCorto, EntradaUrl};

/// Trait de almacenamiento. Permite intercambiar implementaciones (en-memoria, PG...).
pub trait AlmacenUrls: Send + Sync + 'static {
    fn guardar(&self, entrada: EntradaUrl) -> Result<(), ErrorApp>;
    fn buscar(&self, codigo: &CodigoCorto) -> Option<EntradaUrl>;
    fn incrementar_clics(&self, codigo: &CodigoCorto) -> Option<u64>;
    fn listar_todo(&self) -> Vec<EntradaUrl>;
}

/// Implementación en memoria con DashMap (sin Mutex, lock-striped)
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
    fn guardar(&self, entrada: EntradaUrl) -> Result<(), ErrorApp> {
        self.mapa.insert(entrada.codigo.0.clone(), entrada);
        Ok(())
    }

    fn buscar(&self, codigo: &CodigoCorto) -> Option<EntradaUrl> {
        self.mapa.get(&codigo.0).map(|r| r.clone())
    }

    fn incrementar_clics(&self, codigo: &CodigoCorto) -> Option<u64> {
        self.mapa.get_mut(&codigo.0).map(|mut r| {
            r.clics += 1;
            r.clics
        })
    }

    fn listar_todo(&self) -> Vec<EntradaUrl> {
        self.mapa.iter().map(|r| r.clone()).collect()
    }
}
