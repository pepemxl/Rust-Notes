use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, space1},
    combinator::{map, map_res, opt},
    sequence::{delimited, terminated},
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EntradaNginx {
    pub ip: String,
    pub usuario: Option<String>,
    pub timestamp: String,
    pub metodo: String,
    pub ruta: String,
    pub protocolo: String,
    pub estado: u16,
    pub bytes: Option<u64>,
    pub referer: Option<String>,
    pub agente: Option<String>,
}

fn ip_str(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '.' || c == ':').parse(input)
}

fn campo_o_guion(input: &str) -> IResult<&str, Option<&str>> {
    alt((
        map(tag("-"), |_| None),
        map(take_while1(|c: char| c != ' ' && c != '\n'), Some),
    ))
    .parse(input)
}

fn timestamp_entre_corchetes(input: &str) -> IResult<&str, &str> {
    delimited(char('['), take_until("]"), char(']')).parse(input)
}

fn linea_de_peticion(input: &str) -> IResult<&str, (&str, &str, &str)> {
    let (input, _) = char('"').parse(input)?;
    let (input, met) =
        terminated(take_while1(|c: char| c.is_ascii_uppercase()), char(' ')).parse(input)?;
    let (input, ruta) = terminated(take_while1(|c: char| c != ' '), char(' ')).parse(input)?;
    let (input, proto) = take_until("\"").parse(input)?;
    let (input, _) = char('"').parse(input)?;
    Ok((input, (met, ruta, proto)))
}

fn numero_o_guion_u64(input: &str) -> IResult<&str, Option<u64>> {
    alt((
        map(tag("-"), |_| None),
        map(map_res(digit1, |s: &str| s.parse::<u64>()), Some),
    ))
    .parse(input)
}

fn campo_citado(input: &str) -> IResult<&str, Option<&str>> {
    // nginx escribe "-" (entre comillas) cuando no hay referer o user agent
    let (resto, campo) = opt(delimited(char('"'), take_until("\""), char('"'))).parse(input)?;
    Ok((resto, campo.filter(|s| *s != "-")))
}

pub fn parsear_linea(input: &str) -> IResult<&str, EntradaNginx> {
    let (i, ip) = terminated(ip_str, space1).parse(input)?;
    let (i, _) = terminated(campo_o_guion, space1).parse(i)?; // ident (siempre -)
    let (i, usuario) = terminated(campo_o_guion, space1).parse(i)?;
    let (i, ts) = terminated(timestamp_entre_corchetes, space1).parse(i)?;
    let (i, (met, ruta, proto)) = terminated(linea_de_peticion, space1).parse(i)?;
    let (i, estado) = terminated(map_res(digit1, |s: &str| s.parse::<u16>()), space1).parse(i)?;
    let (i, bytes) = terminated(numero_o_guion_u64, space1).parse(i)?;
    let (i, referer) = terminated(campo_citado, space1).parse(i)?;
    let (i, agente) = campo_citado(i)?;

    Ok((
        i,
        EntradaNginx {
            ip: ip.to_string(),
            usuario: usuario.map(|s| s.to_string()),
            timestamp: ts.to_string(),
            metodo: met.to_string(),
            ruta: ruta.to_string(),
            protocolo: proto.to_string(),
            estado,
            bytes,
            referer: referer.map(|s| s.to_string()),
            agente: agente.map(|s| s.to_string()),
        },
    ))
}
