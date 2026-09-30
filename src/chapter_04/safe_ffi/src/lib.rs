//! Wrapper seguro sobre la biblioteca C de `csrc/`.
//!
//! `raw` (las declaraciones `unsafe extern "C"`) es **privado**: quien use esta crate
//! solo ve `Buffer` y `ErrorBuffer`, y no puede llamar a C directamente.
mod raw;

pub mod error;
pub mod safe;

pub use error::ErrorBuffer;
pub use safe::Buffer;
