//! Bindings crudos de la biblioteca C `buffer`.
//! Código generado (en proyecto real: include!(concat!(env!("OUT_DIR"), "/bindings.rs")))

use std::os::raw::{c_int, c_uchar};

/// Handle opaco — nunca instanciar directamente
#[repr(C)]
pub struct BufHandle {
    _privado: [u8; 0], // campo de tamaño cero: hace la struct no-instanciable
}

pub const BUF_OK: c_int = 0;
pub const BUF_ERR_NULL: c_int = -1;
pub const BUF_ERR_RANGE: c_int = -2;
pub const BUF_ERR_ALLOC: c_int = -3;

unsafe extern "C" {
    pub fn buf_crear(len: usize) -> *mut BufHandle;
    pub fn buf_liberar(b: *mut BufHandle);
    pub fn buf_len(b: *const BufHandle) -> usize;
    pub fn buf_escribir(b: *mut BufHandle, offset: usize, data: *const c_uchar, n: usize) -> c_int;
    pub fn buf_leer(b: *const BufHandle, offset: usize, dest: *mut c_uchar, n: usize) -> c_int;
    pub fn buf_xor(b: *mut BufHandle, clave: c_uchar) -> c_int;
    pub fn buf_limpiar(b: *mut BufHandle) -> c_int;
}
