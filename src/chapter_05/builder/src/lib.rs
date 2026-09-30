//! `#[derive(Builder)]`: genera un builder con campos requeridos, valores por
//! defecto y setters acumulativos para `Vec`.
//!
//! ```
//! use builder::Builder;
//!
//! #[derive(Debug, Builder)]
//! struct Servidor {
//!     host: String,
//!     #[builder(default = "8080")]
//!     puerto: u16,
//! }
//!
//! let s = Servidor::builder().host("localhost".into()).build().unwrap();
//! assert_eq!(s.puerto, 8080);
//! ```
//!
//! El valor de `default = "..."` se compila con el span del literal: si su tipo no
//! es el del campo, el error apunta a ese literal, no al código generado.
//!
//! ```compile_fail
//! use builder::Builder;
//!
//! #[derive(Builder)]
//! struct Servidor {
//!     #[builder(default = "\"ochenta\"")] // ❌ E0308: &str donde se espera u16
//!     puerto: u16,
//! }
//! ```

// El crate proc-macro solo puede exportar macros: este crate es la cara pública
// y el lugar para añadir tipos auxiliares (un BuilderError, traits…).
pub use builder_macro::Builder;
