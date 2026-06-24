# Parsing y procesamiento de texto: nom, pest, regex

La Semana 16 cierra el Mes 4 con una habilidad transversal: transformar texto
arbitrario en datos estructurados. Logs de servidores, archivos de configuración,
protocolos de red, lenguajes de dominio específico — todo empieza como bytes y debe
convertirse en tipos Rust antes de poder procesarse.

En esta sección aprenderemos:

- Cómo elegir la herramienta correcta según el problema: regex, nom o pest.
- El modelo mental del zero-copy parsing y por qué importa al procesar GBs de logs.
- El tipo `IResult<I, O, E>` de nom y cómo componer combinadores.
- Los combinadores esenciales: `tag`, `take_while1`, `alt`, `tuple`, `many0`, `map_res`.
- Parsing de formatos reales: logs nginx en formato Combined Log.
- Gramáticas PEG con `pest`: sintaxis, reglas silenciosas, error reporting.
- `regex` con compilación única (`OnceLock`) y capturas nombradas.
- `aho-corasick` para búsqueda simultánea de múltiples patrones.
- Parsing incremental line-by-line con memoria O(1).
- Manejo de encodings no-UTF-8 con `encoding_rs`.
- El proyecto completo: **`logparser`** — CLI streaming multi-formato.
- Benchmarks con `criterion` comparando nom vs regex.

> 💡 **Filosofía de la Semana 16:** *Un parser no es solo "leer texto" — es establecer
> un contrato entre el formato externo y los tipos internos. `nom` hace ese contrato
> composable y verificable en compilación. Un buen parser falla de forma descriptiva
> en el byte 47 en vez de devolver datos corruptos en silencio.*

---

## Elegir la herramienta correcta

```text
ÁRBOL DE DECISIÓN: ¿QUÉ HERRAMIENTA USAR?

¿El patrón es simple y el usuario lo escribe?
    ├── Sí → regex
    │        (búsqueda, extracción, validación de un patrón)
    │
    └── No → ¿El formato tiene una gramática formal documentada?
                 ├── Sí → pest (PEG)
                 │        (gramática separada del código, error reporting automático)
                 │        mejor para: DSLs, lenguajes de configuración, protocolos
                 │
                 └── No → nom (combinadores)
                           (código Rust puro, zero-copy, streaming, máximo rendimiento)
                           mejor para: formatos binarios, logs de alto volumen,
                                       protocolos de red, parsers embebidos

¿Necesitas buscar varios patrones simultáneamente en texto largo?
    → aho-corasick (sin importar los anteriores)
```

---

## El modelo mental: zero-copy parsing

Un parser toma una entrada y devuelve la parte consumida y el resto:

```text
Input:   "192.168.1.1 - frank [10/Oct/2000] \"GET /api\" 200"
          ↑
          parser de IP
          
Consume: "192.168.1.1"
Resto:   " - frank [10/Oct/2000] \"GET /api\" 200"
Salida:  IpAddr::from_str("192.168.1.1")

→ el "resto" se pasa al siguiente parser en la cadena
→ los resultados son &str slices del input original (no copias)
```

Zero-copy significa que el parser devuelve **referencias al input original** en lugar
de copias. Para un archivo de logs de 10 GB procesado línea a línea, esto elimina
millones de allocations.

---

## `nom`: combinadores de parsers

### El tipo central: `IResult`

```rust
use nom::IResult;

// IResult<Input, Output, Error> = Result<(resto_del_input, salida), error>
//
// Ok((resto, valor)) → el parser consumió algo y producjo `valor`
//                      `resto` es lo que queda por parsear
// Err(...)           → el parser falló

fn parser_ejemplo(input: &str) -> IResult<&str, &str> {
    nom::bytes::complete::tag("hola")(input)
    // Si input = "hola mundo", devuelve Ok((" mundo", "hola"))
    // Si input = "adios mundo", devuelve Err(...)
}
```

### Combinadores básicos

```rust
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{
        alpha1, alphanumeric1, char, digit1, multispace0, space0, space1,
    },
    combinator::{map, map_res, opt, recognize, value},
    multi::{many0, many1, separated_list0},
    sequence::{delimited, pair, preceded, separated_pair, terminated, tuple},
    IResult,
};

// ── tag: coincidencia exacta ─────────────────────────────────────────────
fn parsear_get(input: &str) -> IResult<&str, &str> {
    tag("GET")(input)
}

// ── digit1: uno o más dígitos ────────────────────────────────────────────
fn parsear_numero(input: &str) -> IResult<&str, u16> {
    map_res(digit1, |s: &str| s.parse::<u16>())(input)
}

// ── take_while1: consumir mientras se cumpla la condición ────────────────
fn parsear_ip(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '.')(input)
}

// ── alt: intentar alternativas en orden ─────────────────────────────────
fn parsear_metodo(input: &str) -> IResult<&str, &str> {
    alt((
        tag("GET"),
        tag("POST"),
        tag("PUT"),
        tag("DELETE"),
        tag("PATCH"),
        tag("HEAD"),
        tag("OPTIONS"),
    ))(input)
}

// ── tuple: secuencia de parsers ──────────────────────────────────────────
fn parsear_par_clave_valor(input: &str) -> IResult<&str, (&str, &str)> {
    // Parsea: "clave=valor"
    separated_pair(alpha1, char('='), alphanumeric1)(input)
}

// ── delimited: contenido entre delimitadores ─────────────────────────────
fn parsear_entre_comillas(input: &str) -> IResult<&str, &str> {
    delimited(char('"'), take_until("\""), char('"'))(input)
}

// ── many0: cero o más repeticiones ──────────────────────────────────────
fn parsear_lista_numeros(input: &str) -> IResult<&str, Vec<u16>> {
    separated_list0(char(','), parsear_numero)(input)
    // Parsea: "80,443,8080" → vec![80, 443, 8080]
}

// ── opt: opcional (devuelve Option) ─────────────────────────────────────
fn parsear_puerto(input: &str) -> IResult<&str, Option<u16>> {
    opt(preceded(char(':'), parsear_numero))(input)
    // Parsea: ":8080" → Some(8080), "" → None
}

// ── preceded / terminated: ignorar contexto ─────────────────────────────
fn parsear_valor_header(input: &str) -> IResult<&str, &str> {
    // Parsea: "Content-Type: application/json" → "application/json"
    preceded(
        tuple((take_until(":"), tag(": "))),
        take_while1(|c: char| c != '\n'),
    )(input)
}
```

### Reporte de errores: `VerboseError`

```rust
use nom::error::{context, VerboseError};

type PResult<'a, O> = IResult<&'a str, O, VerboseError<&'a str>>;

fn parsear_ip_verbose(input: &str) -> PResult<&str> {
    context(
        "dirección IP",   // mensaje de contexto en caso de error
        take_while1(|c: char| c.is_ascii_digit() || c == '.'),
    )(input)
}

fn formatear_error(input: &str, err: VerboseError<&str>) -> String {
    nom::error::convert_error(input, err)
    // Genera un mensaje legible con la posición exacta del error:
    // 0: en "dirección IP", en la línea "xyz...", en la columna 5
}
```

---

## Parsing de logs nginx con `nom`

El formato "Combined Log Format" de nginx:

```
127.0.0.1 - frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326 "http://referer.com/" "Mozilla/5.0..."
```

```rust
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, space1},
    combinator::{map, map_res, opt},
    sequence::{delimited, preceded, terminated, tuple},
    IResult,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EntradaNginx<'a> {
    pub ip:           &'a str,
    pub usuario:      Option<&'a str>,
    pub timestamp:    &'a str,
    pub metodo:       &'a str,
    pub ruta:         &'a str,
    pub protocolo:    &'a str,
    pub estado:       u16,
    pub bytes:        Option<u64>,
    pub referer:      Option<&'a str>,
    pub agente:       Option<&'a str>,
}

// ── Parsers de componentes ───────────────────────────────────────────────

fn ip(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '.' || c == ':')(input)
}

fn campo_opcional(input: &str) -> IResult<&str, Option<&str>> {
    // "-" representa un campo vacío en los logs de nginx
    alt((
        map(tag("-"), |_| None),
        map(take_while1(|c: char| c != ' ' && c != '\n'), Some),
    ))(input)
}

fn timestamp(input: &str) -> IResult<&str, &str> {
    delimited(char('['), take_until("]"), char(']'))(input)
}

fn peticion(input: &str) -> IResult<&str, (&str, &str, &str)> {
    // Parsea: "GET /ruta HTTP/1.1"
    delimited(
        char('"'),
        tuple((
            terminated(
                take_while1(|c: char| c.is_ascii_alphabetic()),  // método
                char(' '),
            ),
            terminated(
                take_while1(|c: char| c != ' '),  // ruta
                char(' '),
            ),
            take_until("\""),  // protocolo
        )),
        char('"'),
    )(input)
}

fn codigo_estado(input: &str) -> IResult<&str, u16> {
    map_res(digit1, |s: &str| s.parse::<u16>())(input)
}

fn bytes_respuesta(input: &str) -> IResult<&str, Option<u64>> {
    alt((
        map(tag("-"), |_| None),
        map(map_res(digit1, |s: &str| s.parse::<u64>()), Some),
    ))(input)
}

fn campo_citado_opcional(input: &str) -> IResult<&str, Option<&str>> {
    opt(delimited(char('"'), take_until("\""), char('"')))(input)
}

// ── Parser principal ─────────────────────────────────────────────────────

pub fn parsear_nginx(input: &str) -> IResult<&str, EntradaNginx<'_>> {
    let (input, ip_addr)   = terminated(ip, space1)(input)?;
    let (input, _ident)    = terminated(campo_opcional, space1)(input)?;
    let (input, usuario)   = terminated(campo_opcional, space1)(input)?;
    let (input, ts)        = terminated(timestamp, space1)(input)?;
    let (input, (met, ruta, proto)) = terminated(peticion, space1)(input)?;
    let (input, estado)    = terminated(codigo_estado, space1)(input)?;
    let (input, bytes)     = terminated(bytes_respuesta, space1)(input)?;
    let (input, referer)   = terminated(campo_citado_opcional, space1)(input)?;
    let (input, agente)    = campo_citado_opcional(input)?;

    Ok((input, EntradaNginx {
        ip:        ip_addr,
        usuario,
        timestamp: ts,
        metodo:    met,
        ruta,
        protocolo: proto,
        estado,
        bytes,
        referer,
        agente,
    }))
}
```

---

## `pest`: gramáticas PEG

`pest` define la gramática en un archivo `.pest` separado del código Rust. La gramática
es más legible y los errores de parsing incluyen la posición exacta y la regla que falló.

### `grammar/config.pest`

```pest
// Reglas de un formato de configuración sencillo:
// [seccion]
// clave = "valor"
// clave = 42

// WHITESPACE se aplica entre tokens automáticamente (regla especial de pest)
WHITESPACE = _{ " " | "\t" }   // _ = regla silenciosa: no aparece en el árbol

// Comentarios (silenciosos)
COMMENT = _{ "#" ~ (!"\n" ~ ANY)* }

// Literales primitivos
numero    =  { ASCII_DIGIT+ }
booleano  =  { "true" | "false" }
cadena    =  { "\"" ~ (!("\"") ~ ANY)* ~ "\"" }
valor     =  { cadena | numero | booleano }

// Identificadores: letras, dígitos y guiones bajos
ident     =  { (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }

// Par clave = valor
par       =  { ident ~ "=" ~ valor }

// Sección: [nombre]
encabezado = { "[" ~ ident ~ "]" }

// Una sección completa
seccion   =  { encabezado ~ NEWLINE+ ~ (par ~ NEWLINE*)* }

// El archivo completo
archivo   =  { SOI ~ (seccion | NEWLINE)* ~ EOI }
```

### Código Rust para usar el parser pest

```rust
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;

#[derive(Parser)]
#[grammar = "grammar/config.pest"]
pub struct ConfigParser;

#[derive(Debug)]
pub struct Config {
    pub secciones: HashMap<String, HashMap<String, String>>,
}

pub fn parsear_config(input: &str) -> Result<Config, pest::error::Error<Rule>> {
    let archivo = ConfigParser::parse(Rule::archivo, input)?
        .next()
        .unwrap();

    let mut config = Config { secciones: HashMap::new() };
    let mut seccion_actual = String::new();

    for item in archivo.into_inner() {
        match item.as_rule() {
            Rule::seccion => {
                let mut inner = item.into_inner();
                // primer hijo: encabezado
                let enc = inner.next().unwrap();
                seccion_actual = enc.into_inner().next().unwrap().as_str().to_string();
                config.secciones.entry(seccion_actual.clone()).or_default();

                // resto: pares clave=valor
                for par in inner {
                    if par.as_rule() == Rule::par {
                        let mut kv = par.into_inner();
                        let clave  = kv.next().unwrap().as_str().to_string();
                        let val_nodo = kv.next().unwrap().into_inner().next().unwrap();
                        let valor = match val_nodo.as_rule() {
                            Rule::cadena => {
                                let s = val_nodo.as_str();
                                s[1..s.len()-1].to_string()   // quitar comillas
                            }
                            _ => val_nodo.as_str().to_string(),
                        };
                        config.secciones
                            .get_mut(&seccion_actual)
                            .unwrap()
                            .insert(clave, valor);
                    }
                }
            }
            Rule::EOI | Rule::NEWLINE => {}
            _ => {}
        }
    }

    Ok(config)
}
```

### Ventaja de pest: errores automáticos

```rust
let texto_malo = "[seccion]\nclave = @invalido";
match parsear_config(texto_malo) {
    Err(e) => println!("{e}"),
    Ok(_)  => {}
}
// Salida de pest:
//  --> 2:9
//   |
// 2 | clave = @invalido
//   |         ^---
//   |
//   = expected cadena, numero, or booleano
```

---

## `regex`: extracción rápida con patrones

Para patrones simples donde no vale la pena un parser completo, `regex` es la
herramienta adecuada. La compilación del patrón es costosa — siempre se hace una vez:

```rust
use regex::Regex;
use std::sync::OnceLock;

// OnceLock: compilación diferida, hilo-segura, una sola vez
static RE_IP: OnceLock<Regex> = OnceLock::new();
static RE_EMAIL: OnceLock<Regex> = OnceLock::new();
static RE_FECHA: OnceLock<Regex> = OnceLock::new();

fn re_ip() -> &'static Regex {
    RE_IP.get_or_init(|| Regex::new(r"\b(\d{1,3}\.){3}\d{1,3}\b").unwrap())
}

fn re_email() -> &'static Regex {
    RE_EMAIL.get_or_init(|| {
        Regex::new(r"\b[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}\b").unwrap()
    })
}

fn re_fecha() -> &'static Regex {
    RE_FECHA.get_or_init(|| {
        Regex::new(r"(?P<año>\d{4})-(?P<mes>\d{2})-(?P<dia>\d{2})").unwrap()
    })
}
```

### Capturas nombradas y `captures_iter`

```rust
use regex::Regex;

pub fn extraer_fechas(texto: &str) -> Vec<(String, String, String)> {
    let re = Regex::new(
        r"(?P<año>\d{4})-(?P<mes>0[1-9]|1[0-2])-(?P<dia>[0-2]\d|3[01])"
    ).unwrap();

    re.captures_iter(texto)
        .map(|cap| {
            let año = cap["año"].to_string();
            let mes = cap["mes"].to_string();
            let dia = cap["dia"].to_string();
            (año, mes, dia)
        })
        .collect()
}

pub fn extraer_urls(texto: &str) -> Vec<&str> {
    let re = Regex::new(r"https?://[^\s<>\"]+").unwrap();
    re.find_iter(texto).map(|m| m.as_str()).collect()
}

// Reemplazo con grupo de captura
pub fn anonimizar_ips(texto: &str) -> String {
    let re = Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.\d{1,3}\b").unwrap();
    re.replace_all(texto, "$1.$2.$3.xxx").to_string()
}
```

---

## `aho-corasick`: búsqueda simultánea de N patrones

Buscar un patrón en texto es O(n). Buscar N patrones en secuencia es O(n·N). Aho-Corasick
busca N patrones en O(n + m + z) donde m=longitud total de patrones, z=coincidencias:

```rust
use aho_corasick::AhoCorasick;

pub fn detectar_amenazas(texto: &str) -> Vec<(usize, &'static str)> {
    let patrones = &[
        "' OR 1=1",
        "UNION SELECT",
        "<script>",
        "javascript:",
        "../../../",
        "%2e%2e%2f",
    ];

    let nombres = &[
        "SQL Injection",
        "SQL Union",
        "XSS Script",
        "XSS Javascript",
        "Path Traversal",
        "Path Traversal (encoded)",
    ];

    let ac = AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(patrones)
        .unwrap();

    ac.find_iter(texto)
        .map(|m| (m.start(), nombres[m.pattern().as_usize()]))
        .collect()
}

// Reemplazar múltiples patrones en una pasada
pub fn censurar_palabras(texto: &str, censuradas: &[&str]) -> String {
    let reemplazos: Vec<String> = censuradas
        .iter()
        .map(|p| "*".repeat(p.len()))
        .collect();
    let refs: Vec<&str> = reemplazos.iter().map(|s| s.as_str()).collect();

    AhoCorasick::new(censuradas)
        .unwrap()
        .replace_all(texto, &refs)
}
```

---

## `encoding_rs`: texto no-UTF-8

Logs viejos, archivos de Windows, datos de sistemas heredados suelen venir en
encodings distintos a UTF-8:

```rust
use encoding_rs::{WINDOWS_1252, SHIFT_JIS, Encoding};
use std::borrow::Cow;

/// Decodificar bytes de encoding desconocido a String UTF-8
pub fn decodificar_auto(bytes: &[u8]) -> (String, &'static Encoding, bool) {
    // Detectar BOM (Byte Order Mark) si hay uno
    let (encoding, bom_consumido) = if bytes.starts_with(b"\xEF\xBB\xBF") {
        (encoding_rs::UTF_8, 3)
    } else if bytes.starts_with(b"\xFF\xFE") {
        (encoding_rs::UTF_16LE, 2)
    } else if bytes.starts_with(b"\xFE\xFF") {
        (encoding_rs::UTF_16BE, 2)
    } else {
        (WINDOWS_1252, 0)  // fallback: Latin-1 para datos europeos
    };

    let (cow, enc_usado, hubo_reemplazos) = encoding.decode(&bytes[bom_consumido..]);
    (cow.into_owned(), enc_usado, hubo_reemplazos)
}

/// Convertir archivo Windows-1252 a UTF-8
pub fn windows1252_a_utf8(bytes: &[u8]) -> Cow<str> {
    let (cow, _, _) = WINDOWS_1252.decode(bytes);
    cow
}

/// Detectar si bytes son UTF-8 válido antes de procesar
pub fn es_utf8_valido(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}
```

---

## Parsing incremental: línea a línea con memoria O(1)

Para archivos de logs de varios GB, nunca los cargues completos en memoria.
`BufReader::lines()` entrega una línea a la vez:

```rust
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn procesar_log_streaming<F>(ruta: &Path, mut procesar: F) -> std::io::Result<u64>
where
    F: FnMut(&str),
{
    let archivo = File::open(ruta)?;
    let lector  = BufReader::with_capacity(256 * 1024, archivo);
    let mut lineas = 0u64;

    for linea in lector.lines() {
        let linea = linea?;
        if !linea.is_empty() && !linea.starts_with('#') {
            procesar(&linea);
            lineas += 1;
        }
    }

    Ok(lineas)
}

// Uso:
// let mut errores = 0u64;
// procesar_log_streaming(ruta, |linea| {
//     match parsear_nginx(linea) {
//         Ok((_, entrada)) => {
//             if entrada.estado >= 400 { errores += 1; }
//         }
//         Err(_) => eprintln!("línea no parseada: {linea}"),
//     }
// })?;
```

---

## Proyecto: `logparser` CLI

### Estructura y dependencias

```bash
cargo new logparser --bin
```

`Cargo.toml`:

```toml
[package]
name    = "logparser"
version = "0.1.0"
edition = "2021"

[dependencies]
nom         = "7"
regex       = "1"
aho-corasick = "1"
encoding_rs = "0.8"
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
clap        = { version = "4", features = ["derive"] }
owo-colors  = "4"
anyhow      = "1"
thiserror   = "2"
tabled      = "0.17"

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name    = "parser"
harness = false
```

### `src/parser/nginx.rs` — el parser completo

```rust
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, space1},
    combinator::{map, map_res, opt},
    sequence::{delimited, terminated},
    IResult,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EntradaNginx {
    pub ip:        String,
    pub usuario:   Option<String>,
    pub timestamp: String,
    pub metodo:    String,
    pub ruta:      String,
    pub protocolo: String,
    pub estado:    u16,
    pub bytes:     Option<u64>,
    pub referer:   Option<String>,
    pub agente:    Option<String>,
}

fn ip_str(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '.' || c == ':')(input)
}

fn campo_o_guion(input: &str) -> IResult<&str, Option<&str>> {
    alt((
        map(tag("-"), |_| None),
        map(take_while1(|c: char| c != ' ' && c != '\n'), Some),
    ))(input)
}

fn timestamp_entre_corchetes(input: &str) -> IResult<&str, &str> {
    delimited(char('['), take_until("]"), char(']'))(input)
}

fn linea_de_peticion(input: &str) -> IResult<&str, (&str, &str, &str)> {
    let (input, _)    = char('"')(input)?;
    let (input, met)  = terminated(take_while1(|c: char| c.is_ascii_uppercase()), char(' '))(input)?;
    let (input, ruta) = terminated(take_while1(|c: char| c != ' '), char(' '))(input)?;
    let (input, proto)= take_until("\"")(input)?;
    let (input, _)    = char('"')(input)?;
    Ok((input, (met, ruta, proto)))
}

fn numero_o_guion_u64(input: &str) -> IResult<&str, Option<u64>> {
    alt((
        map(tag("-"), |_| None),
        map(map_res(digit1, |s: &str| s.parse::<u64>()), Some),
    ))(input)
}

fn campo_citado(input: &str) -> IResult<&str, Option<&str>> {
    opt(delimited(char('"'), take_until("\""), char('"')))(input)
}

pub fn parsear_linea(input: &str) -> IResult<&str, EntradaNginx> {
    let (i, ip)      = terminated(ip_str, space1)(input)?;
    let (i, _)       = terminated(campo_o_guion, space1)(i)?;      // ident (siempre -)
    let (i, usuario) = terminated(campo_o_guion, space1)(i)?;
    let (i, ts)      = terminated(timestamp_entre_corchetes, space1)(i)?;
    let (i, (met, ruta, proto)) = terminated(linea_de_peticion, space1)(i)?;
    let (i, estado)  = terminated(
        map_res(digit1, |s: &str| s.parse::<u16>()),
        space1,
    )(i)?;
    let (i, bytes)   = terminated(numero_o_guion_u64, space1)(i)?;
    let (i, referer) = terminated(campo_citado, space1)(i)?;
    let (i, agente)  = campo_citado(i)?;

    Ok((i, EntradaNginx {
        ip:        ip.to_string(),
        usuario:   usuario.map(|s| s.to_string()),
        timestamp: ts.to_string(),
        metodo:    met.to_string(),
        ruta:      ruta.to_string(),
        protocolo: proto.to_string(),
        estado,
        bytes,
        referer:   referer.map(|s| s.to_string()),
        agente:    agente.map(|s| s.to_string()),
    }))
}
```

### `src/filter.rs` — DSL simple de filtros

```rust
use crate::parser::nginx::EntradaNginx;
use anyhow::{bail, Result};

#[derive(Debug, Clone)]
pub enum Filtro {
    EstadoMin(u16),         // estado:>=400
    EstadoMax(u16),         // estado:<=299
    EstadoExacto(u16),      // estado:200
    Metodo(String),         // metodo:GET
    RutaContiene(String),   // ruta:/api
    IpExacta(String),       // ip:192.168.1.1
    Todo,                   // sin filtro
}

impl Filtro {
    /// Parsea una expresión de filtro como "estado:>=400" o "metodo:POST"
    pub fn parsear(expr: &str) -> Result<Self> {
        if expr.is_empty() { return Ok(Filtro::Todo); }

        let (campo, valor) = expr.split_once(':')
            .ok_or_else(|| anyhow::anyhow!("formato: campo:valor, got '{expr}'"))?;

        Ok(match campo {
            "estado" if valor.starts_with(">=") => {
                Filtro::EstadoMin(valor[2..].parse()?)
            }
            "estado" if valor.starts_with("<=") => {
                Filtro::EstadoMax(valor[2..].parse()?)
            }
            "estado" => Filtro::EstadoExacto(valor.parse()?),
            "metodo" => Filtro::Metodo(valor.to_ascii_uppercase()),
            "ruta"   => Filtro::RutaContiene(valor.to_string()),
            "ip"     => Filtro::IpExacta(valor.to_string()),
            c => bail!("campo desconocido: '{c}' (usa: estado, metodo, ruta, ip)"),
        })
    }

    pub fn coincide(&self, entrada: &EntradaNginx) -> bool {
        match self {
            Filtro::Todo                 => true,
            Filtro::EstadoMin(min)       => entrada.estado >= *min,
            Filtro::EstadoMax(max)       => entrada.estado <= *max,
            Filtro::EstadoExacto(e)      => entrada.estado == *e,
            Filtro::Metodo(m)            => entrada.metodo == *m,
            Filtro::RutaContiene(s)      => entrada.ruta.contains(s.as_str()),
            Filtro::IpExacta(ip)         => entrada.ip == *ip,
        }
    }
}

/// Combinar múltiples filtros con AND implícito
pub fn aplicar_filtros(entrada: &EntradaNginx, filtros: &[Filtro]) -> bool {
    filtros.iter().all(|f| f.coincide(entrada))
}
```

### `src/aggregate.rs` — conteos y estadísticas

```rust
use crate::parser::nginx::EntradaNginx;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Estadisticas {
    pub total:          u64,
    pub errores_4xx:    u64,
    pub errores_5xx:    u64,
    pub bytes_total:    u64,
    pub por_estado:     HashMap<u16, u64>,
    pub por_metodo:     HashMap<String, u64>,
    pub top_rutas:      HashMap<String, u64>,
    pub top_ips:        HashMap<String, u64>,
}

impl Estadisticas {
    pub fn registrar(&mut self, e: &EntradaNginx) {
        self.total += 1;
        if e.estado >= 400 && e.estado < 500 { self.errores_4xx += 1; }
        if e.estado >= 500                   { self.errores_5xx += 1; }
        self.bytes_total += e.bytes.unwrap_or(0);

        *self.por_estado.entry(e.estado).or_default()       += 1;
        *self.por_metodo.entry(e.metodo.clone()).or_default()+= 1;
        *self.top_rutas.entry(e.ruta.clone()).or_default()  += 1;
        *self.top_ips.entry(e.ip.clone()).or_default()      += 1;
    }

    pub fn top_n<'a>(mapa: &'a HashMap<String, u64>, n: usize) -> Vec<(&'a str, u64)> {
        let mut pares: Vec<_> = mapa.iter().map(|(k, &v)| (k.as_str(), v)).collect();
        pares.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        pares.truncate(n);
        pares
    }
}
```

### `src/output.rs` — formatters de salida

```rust
use crate::parser::nginx::EntradaNginx;
use crate::aggregate::Estadisticas;
use owo_colors::OwoColorize;
use tabled::{Table, Tabled};
use serde_json;

pub enum Formato { JsonLines, Tabla, Resumen }

pub fn imprimir_entrada(entrada: &EntradaNginx, fmt: &Formato) {
    match fmt {
        Formato::JsonLines => println!("{}", serde_json::to_string(entrada).unwrap()),
        Formato::Tabla     => {}  // se acumulan y se imprimen al final
        Formato::Resumen   => {}  // ídem
    }
}

#[derive(Tabled)]
struct FilaEstado<'a> {
    #[tabled(rename = "Estado")]
    estado: u16,
    #[tabled(rename = "Peticiones")]
    total:  u64,
    #[tabled(rename = "% del total")]
    porcentaje: String,
    #[tabled(rename = "Tipo")]
    tipo: &'a str,
}

pub fn imprimir_resumen(stats: &Estadisticas) {
    println!("\n{}", "=== Resumen ===".cyan().bold());
    println!("Total peticiones: {}", stats.total.to_string().yellow());
    println!("Errores 4xx:      {} ({:.1}%)",
        stats.errores_4xx,
        100.0 * stats.errores_4xx as f64 / stats.total.max(1) as f64
    );
    println!("Errores 5xx:      {} ({:.1}%)",
        stats.errores_5xx,
        100.0 * stats.errores_5xx as f64 / stats.total.max(1) as f64
    );
    println!("Bytes transferidos: {} MB",
        stats.bytes_total / 1_048_576
    );

    // Tabla de estados
    let mut filas: Vec<FilaEstado> = stats.por_estado.iter().map(|(&estado, &cnt)| {
        let tipo = match estado {
            200..=299 => "2xx OK",
            300..=399 => "3xx Redirect",
            400..=499 => "4xx Client Error",
            _         => "5xx Server Error",
        };
        FilaEstado {
            estado,
            total: cnt,
            porcentaje: format!("{:.1}%", 100.0 * cnt as f64 / stats.total.max(1) as f64),
            tipo,
        }
    }).collect();
    filas.sort_unstable_by_key(|f| f.estado);

    println!("\n{}", Table::new(&filas));

    // Top 5 rutas
    println!("\n{}", "Top 5 rutas:".cyan());
    for (ruta, cnt) in Estadisticas::top_n(&stats.top_rutas, 5) {
        println!("  {:6}  {}", cnt.to_string().yellow(), ruta);
    }

    // Top 5 IPs
    println!("\n{}", "Top 5 IPs:".cyan());
    for (ip, cnt) in Estadisticas::top_n(&stats.top_ips, 5) {
        println!("  {:6}  {}", cnt.to_string().yellow(), ip);
    }
}
```

### `src/main.rs` — CLI con clap

```rust
mod aggregate;
mod filter;
mod output;
mod parser;

use aggregate::Estadisticas;
use filter::{aplicar_filtros, Filtro};
use output::{Formato, imprimir_entrada, imprimir_resumen};
use parser::nginx::parsear_linea;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, IsTerminal, Read},
    path::PathBuf,
};

#[derive(Parser)]
#[command(name = "logparser", about = "Parser streaming de logs de servidor")]
struct Cli {
    /// Archivos de log a procesar (stdin si se omite)
    #[arg(value_name = "ARCHIVO")]
    archivos: Vec<PathBuf>,

    /// Filtros (ej: "estado:>=400", "metodo:GET", "ruta:/api")
    #[arg(short = 'f', long = "filtro", value_name = "EXPR")]
    filtros: Vec<String>,

    /// Formato de salida
    #[arg(short, long, value_enum, default_value_t = FmtArg::Resumen)]
    output: FmtArg,

    /// Solo mostrar errores (>=400)
    #[arg(long)]
    errores: bool,
}

#[derive(ValueEnum, Clone)]
enum FmtArg {
    Json,
    Tabla,
    Resumen,
}

impl From<FmtArg> for Formato {
    fn from(v: FmtArg) -> Self {
        match v {
            FmtArg::Json    => Formato::JsonLines,
            FmtArg::Tabla   => Formato::Tabla,
            FmtArg::Resumen => Formato::Resumen,
        }
    }
}

fn procesar_lector<R: BufRead>(
    lector: R,
    filtros: &[Filtro],
    fmt: &Formato,
    stats: &mut Estadisticas,
    parseadas: &mut u64,
    fallidas:  &mut u64,
) {
    for linea_res in lector.lines() {
        let linea = match linea_res {
            Ok(l)  => l,
            Err(e) => { eprintln!("error leyendo: {e}"); continue; }
        };
        if linea.is_empty() || linea.starts_with('#') { continue; }

        match parsear_linea(&linea) {
            Ok((_, entrada)) => {
                *parseadas += 1;
                if aplicar_filtros(&entrada, filtros) {
                    stats.registrar(&entrada);
                    imprimir_entrada(&entrada, fmt);
                }
            }
            Err(_) => {
                *fallidas += 1;
                if *fallidas <= 5 {
                    eprintln!("línea no parseada: {}", &linea[..linea.len().min(80)]);
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut filtros: Vec<Filtro> = cli.filtros.iter()
        .map(|expr| Filtro::parsear(expr))
        .collect::<Result<_>>()?;

    if cli.errores {
        filtros.push(Filtro::EstadoMin(400));
    }

    let fmt: Formato = cli.output.into();
    let mut stats    = Estadisticas::default();
    let mut parseadas = 0u64;
    let mut fallidas  = 0u64;

    if cli.archivos.is_empty() {
        // Leer de stdin
        if io::stdin().is_terminal() {
            eprintln!("Esperando stdin (Ctrl+D para terminar)...");
        }
        let lector = BufReader::new(io::stdin().lock());
        procesar_lector(lector, &filtros, &fmt, &mut stats, &mut parseadas, &mut fallidas);
    } else {
        for ruta in &cli.archivos {
            let archivo = File::open(ruta)
                .map_err(|e| anyhow::anyhow!("{}: {e}", ruta.display()))?;
            let lector = BufReader::with_capacity(256 * 1024, archivo);
            procesar_lector(lector, &filtros, &fmt, &mut stats, &mut parseadas, &mut fallidas);
        }
    }

    if !matches!(fmt, Formato::JsonLines) {
        imprimir_resumen(&stats);
    }

    if fallidas > 0 {
        eprintln!("\nAdvertencia: {fallidas} líneas no parseadas");
    }
    eprintln!("Procesadas: {parseadas} líneas");

    Ok(())
}
```

---

## Benchmarks con `criterion`

`benches/parser.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use regex::Regex;

const LINEA_NGINX: &str = r#"192.168.1.1 - frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326 "http://www.example.com/start.html" "Mozilla/4.08 [en] (Win98; I ;Nav)""#;

fn bench_parsers(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("nginx_line_parser");

    // Benchmark 1: nom parser
    grupo.bench_function("nom", |b| {
        b.iter(|| {
            logparser::parser::nginx::parsear_linea(black_box(LINEA_NGINX)).unwrap()
        })
    });

    // Benchmark 2: regex equivalente (solo extrae estado y ruta)
    let re = Regex::new(
        r#"^(\S+) \S+ \S+ \[[^\]]+\] "(\S+) ([^"]+) [^"]+" (\d+)"#
    ).unwrap();

    grupo.bench_function("regex", |b| {
        b.iter(|| {
            re.captures(black_box(LINEA_NGINX)).unwrap()
        })
    });

    grupo.finish();
}

fn bench_volumen(c: &mut Criterion) {
    // Generar 1000 líneas de log
    let lineas: Vec<String> = (0..1000)
        .map(|i| format!(
            r#"10.0.0.{} - - [01/Jan/2024:00:00:00 +0000] "GET /api/v1/users/{} HTTP/1.1" {} {} "-" "curl/7.68""#,
            i % 255,
            i,
            if i % 10 == 0 { 500 } else { 200 },
            i * 100
        ))
        .collect();

    c.bench_function("nom_1000_lineas", |b| {
        b.iter(|| {
            let mut ok = 0u32;
            for linea in &lineas {
                if logparser::parser::nginx::parsear_linea(black_box(linea)).is_ok() {
                    ok += 1;
                }
            }
            ok
        })
    });
}

criterion_group!(benches, bench_parsers, bench_volumen);
criterion_main!(benches);
```

Ejecutar benchmarks:

```bash
cargo bench
# Abre target/criterion/report/index.html para ver gráficas
```

Resultados típicos:

```
nginx_line_parser/nom       time: [1.2 µs 1.3 µs 1.4 µs]
nginx_line_parser/regex     time: [3.8 µs 3.9 µs 4.1 µs]

nom es ~3x más rápido que regex para parsear una línea completa
(regex no extrae todos los campos, por eso la comparación es parcial)

1000 líneas con nom:        time: [1.3 ms 1.4 ms 1.5 ms]
→ ~700,000 líneas/segundo → ~700 MB/s en logs de ~1 KB por línea
```

---

## Tests

```rust
// tests/parser_test.rs
use logparser::parser::nginx::parsear_linea;
use logparser::filter::Filtro;

const LINEA: &str = r#"192.168.1.100 - alice [01/Jan/2024:10:30:00 +0000] "POST /api/login HTTP/1.1" 401 256 "https://example.com/" "Mozilla/5.0""#;
const LINEA_SIN_REFERER: &str = r#"10.0.0.1 - - [01/Jan/2024:00:00:00 +0000] "GET /health HTTP/1.1" 200 12 "-" "-""#;

#[test]
fn parsea_linea_completa() {
    let (_, entrada) = parsear_linea(LINEA).unwrap();
    assert_eq!(entrada.ip,       "192.168.1.100");
    assert_eq!(entrada.usuario,  Some("alice".to_string()));
    assert_eq!(entrada.metodo,   "POST");
    assert_eq!(entrada.ruta,     "/api/login");
    assert_eq!(entrada.protocolo,"HTTP/1.1");
    assert_eq!(entrada.estado,   401);
    assert_eq!(entrada.bytes,    Some(256));
    assert!(entrada.referer.is_some());
    assert!(entrada.agente.is_some());
}

#[test]
fn parsea_campos_opcionales_guion() {
    let (_, entrada) = parsear_linea(LINEA_SIN_REFERER).unwrap();
    assert_eq!(entrada.usuario, None);
    assert_eq!(entrada.bytes,   Some(12));
    assert_eq!(entrada.referer, None);
    assert_eq!(entrada.agente,  None);
}

#[test]
fn filtro_estado_minimo() {
    let (_, entrada) = parsear_linea(LINEA).unwrap();
    let filtro = Filtro::parsear("estado:>=400").unwrap();
    assert!(filtro.coincide(&entrada));   // 401 >= 400

    let filtro2 = Filtro::parsear("estado:>=500").unwrap();
    assert!(!filtro2.coincide(&entrada)); // 401 < 500
}

#[test]
fn filtro_ruta_contiene() {
    let (_, entrada) = parsear_linea(LINEA).unwrap();
    assert!(Filtro::parsear("ruta:/api").unwrap().coincide(&entrada));
    assert!(!Filtro::parsear("ruta:/static").unwrap().coincide(&entrada));
}

#[test]
fn filtro_campo_desconocido_falla() {
    assert!(Filtro::parsear("ignorado:valor").is_err());
}

#[test]
fn parsear_linea_malformada_falla() {
    assert!(parsear_linea("esto no es un log nginx").is_err());
}

#[test]
fn parsear_linea_vacia_falla() {
    assert!(parsear_linea("").is_err());
}
```

---

## Demostración con datos reales

```bash
# Instalar la herramienta
cargo install --path .

# Parsear un archivo de log
logparser /var/log/nginx/access.log

# Solo errores, en JSON
logparser --errores --output json /var/log/nginx/access.log | jq '.estado'

# Filtrar por ruta y estado
logparser -f "ruta:/api" -f "estado:>=400" access.log

# Desde stdin (pipe con otro comando)
cat access.log | grep "POST" | logparser

# Múltiples archivos (todos los logs del mes)
logparser /var/log/nginx/access.log.{1,2,3,4,5}

# Probar con datos de prueba generados
for i in $(seq 1 20); do
  echo "10.0.0.$i - - [01/Jan/2024:00:00:00 +0000] \"GET /api/v1/items/$i HTTP/1.1\" $((200 + (i % 3)*200)) $((i*100)) \"-\" \"curl/7.68\""
done | logparser
```

---

## ✅ Checklist de la Semana 16

- [ ] Elijo la herramienta correcta según el problema: regex (patrón simple del
  usuario), nom (formato estructurado de alto volumen), pest (gramática formal con
  buen error reporting).
- [ ] Entiendo `IResult<I, O, E>`: `Ok((resto, valor))` cuando el parser tiene éxito,
  `Err(...)` cuando falla.
- [ ] Uso `nom::error::context` y `VerboseError` para obtener mensajes de error
  descriptivos con posición en el input.
- [ ] El parser de nginx con nom devuelve referencias `&str` al input original
  (zero-copy) cuando el lifetime lo permite.
- [ ] Las regex se compilan una sola vez con `OnceLock<Regex>` — nunca dentro de un
  bucle ni en `lazy_static!` si hay `OnceLock` disponible.
- [ ] `AhoCorasick` busca N patrones simultáneamente en una sola pasada O(n+m+z) —
  no N llamadas a `contains` en secuencia.
- [ ] `BufReader::lines()` procesa archivos de cualquier tamaño con memoria O(1) —
  el archivo nunca se carga completo.
- [ ] `logparser` acepta stdin y archivos como entrada, filtra con el DSL de filtros,
  acumula estadísticas y produce salida en JSON Lines o tabla.
- [ ] Los benchmarks muestran que nom es más rápido que regex para parsear líneas
  completas de nginx.
- [ ] `cargo test --test parser_test` pasa los 7 tests.
- [ ] `cargo bench` produce un informe HTML en `target/criterion/`.

> **Fin del Mes 4.** Has construido una CLI profesional, un wrapper FFI seguro, una
> app Wasm interactiva y un parser de logs de alto rendimiento.
>
> **Siguiente paso:** Mes 5 — [Arquitectura, patrones y rendimiento](../chapter_05/section_00.md).
