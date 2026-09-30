use crate::almacen::AlmacenUrls;
use std::sync::Arc;

/// Estado compartido entre todos los handlers
pub struct EstadoApp<S: AlmacenUrls> {
    pub almacen: Arc<S>,
    pub base_url: String,
}

impl<S: AlmacenUrls> EstadoApp<S> {
    pub fn nuevo(almacen: S, base_url: String) -> Arc<Self> {
        Arc::new(Self {
            almacen: Arc::new(almacen),
            base_url,
        })
    }
}
