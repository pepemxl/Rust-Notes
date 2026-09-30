use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Identificador corto de una URL (8 caracteres alfanuméricos)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CodigoCorto(pub String);

impl CodigoCorto {
    pub fn generar() -> Self {
        use std::fmt::Write;
        let id = uuid::Uuid::new_v4();
        let mut s = String::with_capacity(8);
        // Toma los primeros 6 bytes del UUID y los codifica en base62
        for byte in &id.as_bytes()[..6] {
            let c = match byte % 62 {
                n @ 0..=9 => b'0' + n,
                n @ 10..=35 => b'a' + n - 10,
                n => b'A' + n - 36,
            };
            s.push(c as char);
        }
        // Añade 2 caracteres extra del timestamp para reducir colisiones
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let _ = write!(s, "{:02}", ts % 62);
        CodigoCorto(s)
    }

    #[allow(dead_code)] // la v1 no la usa; AlmacenPostgres la necesita en la v2 (Semana 11)
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CodigoCorto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Registro de una URL acortada
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntradaUrl {
    pub codigo: CodigoCorto,
    pub url_orig: String,
    pub creada_en: u64, // Unix timestamp en segundos
    pub clics: u64,
}

impl EntradaUrl {
    pub fn nueva(url_orig: String) -> Self {
        let creada_en = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            codigo: CodigoCorto::generar(),
            url_orig,
            creada_en,
            clics: 0,
        }
    }
}

/// Cuerpo de la petición POST /shorten
#[derive(Debug, Deserialize)]
pub struct SolicitudAcortar {
    pub url: String,
}

/// Cuerpo de la respuesta al acortar
#[derive(Debug, Serialize)]
pub struct RespuestaAcortar {
    pub codigo: CodigoCorto,
    pub url_corta: String,
}

/// Estadísticas de una URL
#[derive(Debug, Serialize)]
pub struct EstadisticasUrl {
    pub codigo: CodigoCorto,
    pub url_orig: String,
    pub creada_en: u64,
    pub clics: u64,
}

impl From<EntradaUrl> for EstadisticasUrl {
    fn from(e: EntradaUrl) -> Self {
        Self {
            codigo: e.codigo,
            url_orig: e.url_orig,
            creada_en: e.creada_en,
            clics: e.clics,
        }
    }
}
