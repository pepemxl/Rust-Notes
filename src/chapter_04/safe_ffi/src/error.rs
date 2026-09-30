use thiserror::Error;

#[derive(Debug, Error)]
pub enum ErrorBuffer {
    #[error("asignación de memoria fallida en C")]
    AsignacionFallida,

    #[error("offset {offset} + longitud {longitud} supera el tamaño del buffer ({tam})")]
    FueraDeRango {
        offset: usize,
        longitud: usize,
        tam: usize,
    },

    #[error("puntero nulo inesperado (error interno)")]
    PunteroNulo,

    #[error("código de error C desconocido: {0}")]
    CodigoDesconocido(i32),
}

pub(crate) fn verificar_codigo(
    rc: i32,
    offset: usize,
    n: usize,
    tam: usize,
) -> Result<(), ErrorBuffer> {
    use crate::raw::*;
    match rc {
        r if r == BUF_OK => Ok(()),
        r if r == BUF_ERR_NULL => Err(ErrorBuffer::PunteroNulo),
        r if r == BUF_ERR_RANGE => Err(ErrorBuffer::FueraDeRango {
            offset,
            longitud: n,
            tam,
        }),
        r if r == BUF_ERR_ALLOC => Err(ErrorBuffer::AsignacionFallida),
        r => Err(ErrorBuffer::CodigoDesconocido(r)),
    }
}
