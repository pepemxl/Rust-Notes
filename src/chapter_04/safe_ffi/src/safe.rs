//! API Rust completamente segura sobre la biblioteca C `buffer`.
//!
//! Invariante de seguridad: `ptr` siempre es no nulo y apunta a un `BufHandle`
//! válido, creado por `buf_crear` y aún no liberado. Se mantiene automáticamente
//! por el constructor (`Buffer::nuevo`) y `impl Drop`.

use std::ptr::NonNull;

use crate::{
    error::{ErrorBuffer, verificar_codigo},
    raw,
};

/// Buffer de bytes con cifrado XOR, gestionado por la biblioteca C.
///
/// # Garantías
/// - La memoria se libera y se borra al hacer `drop`.
/// - Todas las operaciones verifican bounds antes de tocar memoria C.
/// - Imposible crear un `Buffer` con puntero nulo (usa `NonNull`).
pub struct Buffer {
    /// Invariante: siempre válido, no nulo, creado por buf_crear, no liberado.
    ptr: NonNull<raw::BufHandle>,
}

// SAFETY: BufHandle no tiene referencias a datos de hilos; la exclusión
// mutua es responsabilidad del llamador (como con Vec<T>).
unsafe impl Send for Buffer {}
unsafe impl Sync for Buffer {}

impl Buffer {
    /// Crea un nuevo buffer de `longitud` bytes, inicializado a cero.
    pub fn nuevo(longitud: usize) -> Result<Self, ErrorBuffer> {
        // SAFETY: buf_crear devuelve NULL en fallo o un puntero válido.
        let ptr = unsafe { raw::buf_crear(longitud) };

        NonNull::new(ptr)
            .map(|p| Buffer { ptr: p })
            .ok_or(ErrorBuffer::AsignacionFallida)
    }

    /// Longitud del buffer en bytes.
    pub fn len(&self) -> usize {
        // SAFETY: ptr es válido por el invariante del tipo.
        unsafe { raw::buf_len(self.ptr.as_ptr()) }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Escribe `datos` en posición `offset`.
    ///
    /// # Errors
    /// Devuelve `FueraDeRango` si `offset + datos.len() > self.len()`.
    pub fn escribir(&mut self, offset: usize, datos: &[u8]) -> Result<(), ErrorBuffer> {
        let rc =
            unsafe { raw::buf_escribir(self.ptr.as_ptr(), offset, datos.as_ptr(), datos.len()) };
        verificar_codigo(rc, offset, datos.len(), self.len())
    }

    /// Lee `n` bytes desde posición `offset`.
    ///
    /// # Errors
    /// Devuelve `FueraDeRango` si `offset + n > self.len()`.
    pub fn leer(&self, offset: usize, n: usize) -> Result<Vec<u8>, ErrorBuffer> {
        let mut dest = vec![0u8; n];
        let rc = unsafe { raw::buf_leer(self.ptr.as_ptr(), offset, dest.as_mut_ptr(), n) };
        verificar_codigo(rc, offset, n, self.len())?;
        Ok(dest)
    }

    /// Aplica XOR con `clave` a todo el buffer.
    /// Llamar dos veces con la misma clave restaura el original (cifrado simétrico).
    pub fn aplicar_xor(&mut self, clave: u8) -> Result<(), ErrorBuffer> {
        let rc = unsafe { raw::buf_xor(self.ptr.as_ptr(), clave) };
        verificar_codigo(rc, 0, 0, self.len())
    }

    /// Borra todos los bytes del buffer (pone a cero).
    pub fn limpiar(&mut self) -> Result<(), ErrorBuffer> {
        let rc = unsafe { raw::buf_limpiar(self.ptr.as_ptr()) };
        verificar_codigo(rc, 0, 0, self.len())
    }

    /// Devuelve una copia de todos los bytes del buffer.
    pub fn contenido(&self) -> Result<Vec<u8>, ErrorBuffer> {
        self.leer(0, self.len())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: ptr es válido (invariante del tipo) y esta es la única
        // vez que se libera (Drop se llama exactamente una vez por objeto).
        unsafe { raw::buf_liberar(self.ptr.as_ptr()) }
        // buf_liberar internamente borra la memoria con memset antes de free.
    }
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Buffer({} bytes)", self.len())
    }
}
