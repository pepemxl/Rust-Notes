use nom::{
    IResult, Parser,
    bytes::complete::{take_until, take_while1},
    character::complete::{char, digit1, space1},
    combinator::{map_res, opt},
    sequence::{delimited, terminated},
};
use serde::Serialize;
use std::borrow::Cow;

// ANTES (Semana 16): cada campo de texto es un String → 3 allocaciones por línea
#[derive(Debug, PartialEq, Serialize)]
pub struct EntradaAntes {
    pub ip: String,
    pub metodo: String,
    pub ruta: String,
    pub estado: u16,
    pub bytes: Option<u64>,
}

// DESPUÉS: los campos apuntan al input (zero-copy). Solo la ruta puede necesitar
// una copia, y solo cuando trae %-encoding que hay que decodificar.
#[derive(Debug, PartialEq, Serialize)]
pub struct EntradaDespues<'a> {
    pub ip: &'a str,
    pub metodo: &'a str,
    pub ruta: Cow<'a, str>,
    pub estado: u16,
    pub bytes: Option<u64>,
}

/// Los campos crudos de una línea, todos prestados del input.
struct Campos<'a> {
    ip: &'a str,
    metodo: &'a str,
    ruta: &'a str,
    estado: u16,
    bytes: Option<u64>,
}

fn hasta_espacio(i: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c != ' ').parse(i)
}

// Formato combined de nginx:
// IP - usuario [fecha] "MÉTODO RUTA PROTOCOLO" ESTADO BYTES "referer" "user-agent"
fn campos(input: &str) -> IResult<&str, Campos<'_>> {
    let (i, ip) = terminated(hasta_espacio, space1).parse(input)?;
    let (i, _) = terminated(hasta_espacio, space1).parse(i)?; // ident (siempre "-")
    let (i, _) = terminated(hasta_espacio, space1).parse(i)?; // usuario
    let (i, _) = terminated(delimited(char('['), take_until("]"), char(']')), space1).parse(i)?;
    let (i, _) = char('"').parse(i)?;
    let (i, metodo) =
        terminated(take_while1(|c: char| c.is_ascii_uppercase()), char(' ')).parse(i)?;
    let (i, ruta) = terminated(hasta_espacio, char(' ')).parse(i)?;
    let (i, _) = terminated(take_until("\""), char('"')).parse(i)?; // protocolo
    let (i, _) = space1.parse(i)?;
    let (i, estado) = terminated(map_res(digit1, str::parse::<u16>), space1).parse(i)?;
    let (i, bytes) = opt(map_res(digit1, str::parse::<u64>)).parse(i)?;

    Ok((
        i,
        Campos {
            ip,
            metodo,
            ruta,
            estado,
            bytes,
        },
    ))
}

pub fn parsear_linea_antes(input: &str) -> IResult<&str, EntradaAntes> {
    let (resto, c) = campos(input)?;
    let entrada = EntradaAntes {
        ip: c.ip.to_string(),
        metodo: c.metodo.to_string(),
        ruta: decodificar_url(c.ruta).into_owned(), // copia aunque no haya nada que decodificar
        estado: c.estado,
        bytes: c.bytes,
    };
    Ok((resto, entrada))
}

pub fn parsear_linea(input: &str) -> IResult<&str, EntradaDespues<'_>> {
    let (resto, c) = campos(input)?;
    let entrada = EntradaDespues {
        ip: c.ip,
        metodo: c.metodo,
        ruta: decodificar_url(c.ruta),
        estado: c.estado,
        bytes: c.bytes,
    };
    Ok((resto, entrada))
}

/// Decodifica `%XX`. Sin `%` devuelve el mismo `&str` (Borrowed, cero allocaciones).
pub fn decodificar_url(s: &str) -> Cow<'_, str> {
    if !s.contains('%') {
        return Cow::Borrowed(s);
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Solo "%" + dos dígitos hex; from_str_radix a secas aceptaría "+1"
            let hex = bytes
                .get(i + 1..i + 3)
                .filter(|h| h.iter().all(u8::is_ascii_hexdigit));
            if let Some(h) = hex {
                let h = std::str::from_utf8(h).expect("dígitos hex ASCII");
                out.push(u8::from_str_radix(h, 16).expect("dígitos hex validados"));
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]); // cualquier otro byte, incluido un % suelto, se copia tal cual
        i += 1;
    }
    // from_utf8 reutiliza el buffer; solo si hay bytes inválidos copiamos con lossy
    let texto = String::from_utf8(out)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    Cow::Owned(texto)
}
