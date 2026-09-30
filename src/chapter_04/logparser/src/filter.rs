use crate::parser::nginx::EntradaNginx;
use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub enum Filtro {
    EstadoMin(u16),       // estado:>=400
    EstadoMax(u16),       // estado:<=299
    EstadoExacto(u16),    // estado:200
    Metodo(String),       // metodo:GET
    RutaContiene(String), // ruta:/api
    IpExacta(String),     // ip:192.168.1.1
    Todo,                 // sin filtro
}

impl Filtro {
    /// Parsea una expresión de filtro como "estado:>=400" o "metodo:POST"
    pub fn parsear(expr: &str) -> Result<Self> {
        if expr.is_empty() {
            return Ok(Filtro::Todo);
        }

        let (campo, valor) = expr
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("formato: campo:valor, got '{expr}'"))?;

        Ok(match campo {
            "estado" if valor.starts_with(">=") => Filtro::EstadoMin(valor[2..].parse()?),
            "estado" if valor.starts_with("<=") => Filtro::EstadoMax(valor[2..].parse()?),
            "estado" => Filtro::EstadoExacto(valor.parse()?),
            "metodo" => Filtro::Metodo(valor.to_ascii_uppercase()),
            "ruta" => Filtro::RutaContiene(valor.to_string()),
            "ip" => Filtro::IpExacta(valor.to_string()),
            c => bail!("campo desconocido: '{c}' (usa: estado, metodo, ruta, ip)"),
        })
    }

    pub fn coincide(&self, entrada: &EntradaNginx) -> bool {
        match self {
            Filtro::Todo => true,
            Filtro::EstadoMin(min) => entrada.estado >= *min,
            Filtro::EstadoMax(max) => entrada.estado <= *max,
            Filtro::EstadoExacto(e) => entrada.estado == *e,
            Filtro::Metodo(m) => entrada.metodo == *m,
            Filtro::RutaContiene(s) => entrada.ruta.contains(s.as_str()),
            Filtro::IpExacta(ip) => entrada.ip == *ip,
        }
    }
}

/// Combinar múltiples filtros con AND implícito
pub fn aplicar_filtros(entrada: &EntradaNginx, filtros: &[Filtro]) -> bool {
    filtros.iter().all(|f| f.coincide(entrada))
}
