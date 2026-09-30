use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)] // le dice a SQLx que es solo un String
pub struct CodigoCorto(pub String);

impl CodigoCorto {
    pub fn generar() -> Self {
        let id = uuid::Uuid::new_v4();
        let bytes = id.as_bytes();
        let mut s = String::with_capacity(8);
        for &b in &bytes[..6] {
            s.push(match b % 62 {
                n @ 0..=9 => (b'0' + n) as char,
                n @ 10..=35 => (b'a' + n - 10) as char,
                n => (b'A' + n - 36) as char,
            });
        }
        let nano = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        s.push_str(&format!("{:02}", nano % 62));
        CodigoCorto(s)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CodigoCorto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Fila de la tabla urls — usada con sqlx::FromRow
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FilaUrl {
    pub code: String,
    pub target_url: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub clicks: i64,
}

/// Modelo de dominio
#[derive(Debug, Clone)]
pub struct EntradaUrl {
    pub codigo: CodigoCorto,
    pub url_orig: String,
    pub creada_en: DateTime<Utc>,
    pub expira_en: Option<DateTime<Utc>>,
    pub clics: u64,
}

impl EntradaUrl {
    /// URL nueva, sin expiración y sin clics; `expira_en` se ajusta después si hace falta.
    pub fn nueva(url_orig: String) -> Self {
        Self {
            codigo: CodigoCorto::generar(),
            url_orig,
            creada_en: Utc::now(),
            expira_en: None,
            clics: 0,
        }
    }
}

impl From<FilaUrl> for EntradaUrl {
    fn from(f: FilaUrl) -> Self {
        Self {
            codigo: CodigoCorto(f.code),
            url_orig: f.target_url,
            creada_en: f.created_at,
            expira_en: f.expires_at,
            clics: f.clicks as u64,
        }
    }
}

/// Cuerpo de la petición POST /shorten
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")] // en el JSON: {"url": "...", "expiraEnSegundos": 3600}
pub struct SolicitudAcortar {
    pub url: String,

    // Expiración opcional: si el campo no viene en el JSON, queda en None
    #[serde(default)]
    pub expira_en_segundos: Option<u64>,
}

/// Respuesta de POST /shorten — Serde completo para API pública
#[derive(Debug, Serialize)]
pub struct RespuestaAcortar {
    pub codigo: CodigoCorto,
    pub url_corta: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expira_en: Option<i64>, // Unix timestamp, None → no expira
}

/// Estadísticas devueltas por GET /{codigo}/stats
#[derive(Debug, Serialize)]
pub struct EstadisticasUrl {
    pub codigo: CodigoCorto,
    pub url_orig: String,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub creada_en: DateTime<Utc>,

    #[serde(
        with = "chrono::serde::ts_seconds_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub expira_en: Option<DateTime<Utc>>,

    pub clics: u64,
}

impl From<EntradaUrl> for EstadisticasUrl {
    fn from(e: EntradaUrl) -> Self {
        Self {
            codigo: e.codigo,
            url_orig: e.url_orig,
            creada_en: e.creada_en,
            expira_en: e.expira_en,
            clics: e.clics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solicitud_usa_camel_case() {
        let s: SolicitudAcortar =
            serde_json::from_str(r#"{"url": "https://ferris.rs", "expiraEnSegundos": 60}"#)
                .unwrap();
        assert_eq!(s.expira_en_segundos, Some(60));

        // Sin el campo opcional → None gracias a #[serde(default)]
        let s: SolicitudAcortar = serde_json::from_str(r#"{"url": "https://ferris.rs"}"#).unwrap();
        assert_eq!(s.expira_en_segundos, None);
    }
}
