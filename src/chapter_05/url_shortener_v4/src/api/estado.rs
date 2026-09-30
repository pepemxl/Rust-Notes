use crate::actor::contador::ContadorSharded;
use crate::store::memoria::AlmacenUrls;
use std::sync::Arc;

/// Estado compartido de Axum. El almacén es `Arc<dyn AlmacenUrls>` (DI dinámico):
/// los handlers no son genéricos y en los tests se puede inyectar otro almacén.
#[derive(Clone)]
pub struct AppState {
    pub almacen: Arc<dyn AlmacenUrls>,
    pub contador: ContadorSharded,
    pub base_url: String,
}
