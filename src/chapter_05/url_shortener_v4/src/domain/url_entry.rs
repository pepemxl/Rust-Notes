use std::fmt;
use std::marker::PhantomData;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Newtype: un código de URL siempre válido ──────────────────────────────
// El campo es privado: la única forma de obtener un UrlCode es parse() o
// nueva_aleatoria(), así que tener uno ES la prueba de que es válido.

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UrlCode(String);

impl UrlCode {
    pub fn nueva_aleatoria() -> Self {
        // Suficiente para el ejemplo: si dos peticiones caen en el mismo
        // nanosegundo, el almacén rechaza el duplicado con 409.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("reloj antes de 1970")
            .as_nanos() as u64;
        UrlCode(format!("{nanos:x}"))
    }

    pub fn parse(s: impl Into<String>) -> Result<Self, String> {
        let s = s.into();
        if !s.is_empty()
            && s.len() <= 32
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            Ok(UrlCode(s))
        } else {
            Err(format!("código de URL inválido: '{s}'"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UrlCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Marcadores de estado (zero-sized types) ───────────────────────────────
// Debug y Clone hacen falta porque #[derive] en UrlEntry<S> exige S: Debug + Clone.

#[derive(Debug, Clone)]
pub struct Draft;
#[derive(Debug, Clone)]
pub struct Active;
#[derive(Debug, Clone)]
pub struct Expired;

// ── Typestate: el estado vive en el tipo ──────────────────────────────────

/// Una URL en el estado `S`. Cada estado expone solo sus métodos válidos:
///
/// ```compile_fail
/// use url_shortener_v4::domain::url_entry::{UrlCode, UrlEntry};
///
/// let mut draft = UrlEntry::nueva(UrlCode::parse("abc").unwrap(), "https://ejemplo.com");
/// draft.registrar_click(); // ❌ E0599: no existe en UrlEntry<Draft>
/// ```
///
/// ```compile_fail
/// use url_shortener_v4::domain::url_entry::{UrlCode, UrlEntry};
///
/// let draft = UrlEntry::nueva(UrlCode::parse("abc").unwrap(), "https://ejemplo.com");
/// let _activa = draft.publicar();
/// let _otra = draft.publicar(); // ❌ E0382: publicar() consumió `draft`
/// ```
#[derive(Debug, Clone)]
pub struct UrlEntry<S> {
    pub code: UrlCode,
    pub target: String,
    pub clicks: u64,
    _estado: PhantomData<S>,
}

impl UrlEntry<Draft> {
    pub fn nueva(code: UrlCode, target: impl Into<String>) -> Self {
        UrlEntry {
            code,
            target: target.into(),
            clicks: 0,
            _estado: PhantomData,
        }
    }

    /// Consume el Draft (ya no se puede usar) y devuelve la URL activa.
    pub fn publicar(self) -> UrlEntry<Active> {
        UrlEntry {
            code: self.code,
            target: self.target,
            clicks: 0,
            _estado: PhantomData,
        }
    }
}

impl UrlEntry<Active> {
    pub fn registrar_click(&mut self) {
        self.clicks += 1;
    }

    pub fn url_destino(&self) -> &str {
        &self.target
    }

    pub fn expirar(self) -> UrlEntry<Expired> {
        UrlEntry {
            code: self.code,
            target: self.target,
            clicks: self.clicks,
            _estado: PhantomData,
        }
    }
}

impl UrlEntry<Expired> {
    pub fn clicks_finales(&self) -> u64 {
        self.clicks
    }
}

// Disponible en todos los estados
impl<S> UrlEntry<S> {
    pub fn codigo(&self) -> &UrlCode {
        &self.code
    }
}

// ── Tipo borrado para almacenamiento ──────────────────────────────────────
// Un mapa no puede guardar UrlEntry<?> de estados distintos: el enum los reúne.
// Las transiciones siguen pasando por los métodos del Typestate.

#[derive(Debug, Clone)]
pub enum EntradaUrl {
    Draft(UrlEntry<Draft>),
    Active(UrlEntry<Active>),
    Expired(UrlEntry<Expired>),
}

impl EntradaUrl {
    pub fn nueva_activa(code: UrlCode, target: impl Into<String>) -> Self {
        EntradaUrl::Active(UrlEntry::nueva(code, target).publicar())
    }

    pub fn url_destino_activa(&self) -> Option<&str> {
        match self {
            EntradaUrl::Active(u) => Some(u.url_destino()),
            _ => None,
        }
    }

    /// Registra el click si la URL está activa; devuelve si lo hizo.
    pub fn registrar_click(&mut self) -> bool {
        match self {
            EntradaUrl::Active(u) => {
                u.registrar_click();
                true
            }
            _ => false,
        }
    }

    pub fn expirar(self) -> Self {
        match self {
            EntradaUrl::Active(u) => EntradaUrl::Expired(u.expirar()),
            otro => otro,
        }
    }

    pub fn codigo(&self) -> &UrlCode {
        match self {
            EntradaUrl::Draft(u) => u.codigo(),
            EntradaUrl::Active(u) => u.codigo(),
            EntradaUrl::Expired(u) => u.codigo(),
        }
    }

    pub fn clicks(&self) -> u64 {
        match self {
            EntradaUrl::Draft(u) => u.clicks,
            EntradaUrl::Active(u) => u.clicks,
            EntradaUrl::Expired(u) => u.clicks,
        }
    }
}
