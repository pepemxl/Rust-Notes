# Parsing y procesamiento de texto: nom, pest, regex

La Semana 16 cierra el Mes 4 con una habilidad transversal: transformar texto
arbitrario en datos estructurados. Logs de servidores, archivos de configuración,
protocolos de red, lenguajes de dominio específico — todo empieza como bytes y debe
convertirse en tipos Rust antes de poder procesarse.

En esta sección aprenderemos:

- Cómo elegir la herramienta correcta según el problema: regex, nom o pest.
- El modelo mental del zero-copy parsing y por qué importa al procesar GBs de logs.
- El tipo `IResult<I, O, E>` de nom y cómo componer combinadores.
- Los combinadores esenciales: `tag`, `take_while1`, `alt`, tuplas de parsers, `many0`, `map_res`.
- Parsing de formatos reales: logs nginx en formato Combined Log.
- Gramáticas PEG con `pest`: sintaxis, reglas silenciosas, error reporting.
- `regex` con compilación única (`OnceLock`) y capturas nombradas.
- `aho-corasick` para búsqueda simultánea de múltiples patrones.
- Parsing incremental line-by-line con memoria O(1).
- Manejo de encodings no-UTF-8 con `encoding_rs`.
- El proyecto completo: **`logparser`** — CLI streaming multi-formato.
- Benchmarks con `criterion` comparando nom vs regex.

!!! quote "Filosofía de la Semana 16"

    *Un parser no es solo "leer texto" — es establecer
    un contrato entre el formato externo y los tipos internos. `nom` hace ese contrato
    composable y verificable en compilación. Un buen parser falla de forma descriptiva
    en el byte 47 en vez de devolver datos corruptos en silencio.*

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
use nom::{IResult, Parser};

// IResult<Input, Output, Error> = Result<(resto_del_input, salida), error>
//
// Ok((resto, valor)) → el parser consumió algo y producjo `valor`
//                      `resto` es lo que queda por parsear
// Err(...)           → el parser falló

fn parser_ejemplo(input: &str) -> IResult<&str, &str> {
    nom::bytes::complete::tag("hola").parse(input)
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
    sequence::{delimited, pair, preceded, separated_pair, terminated},
    IResult, Parser,
};

// ── tag: coincidencia exacta ─────────────────────────────────────────────
fn parsear_get(input: &str) -> IResult<&str, &str> {
    tag("GET").parse(input)
}

// ── digit1: uno o más dígitos ────────────────────────────────────────────
fn parsear_numero(input: &str) -> IResult<&str, u16> {
    map_res(digit1, |s: &str| s.parse::<u16>()).parse(input)
}

// ── take_while1: consumir mientras se cumpla la condición ────────────────
fn parsear_ip(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '.').parse(input)
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
    )).parse(input)
}

// ── separated_pair: dos parsers con un separador en medio ────────────────
fn parsear_par_clave_valor(input: &str) -> IResult<&str, (&str, &str)> {
    // Parsea: "clave=valor"
    separated_pair(alpha1, char('='), alphanumeric1).parse(input)
}

// ── delimited: contenido entre delimitadores ─────────────────────────────
fn parsear_entre_comillas(input: &str) -> IResult<&str, &str> {
    delimited(char('"'), take_until("\""), char('"')).parse(input)
}

// ── many0: cero o más repeticiones ──────────────────────────────────────
fn parsear_lista_numeros(input: &str) -> IResult<&str, Vec<u16>> {
    separated_list0(char(','), parsear_numero).parse(input)
    // Parsea: "80,443,8080" → vec![80, 443, 8080]
}

// ── opt: opcional (devuelve Option) ─────────────────────────────────────
fn parsear_puerto(input: &str) -> IResult<&str, Option<u16>> {
    opt(preceded(char(':'), parsear_numero)).parse(input)
    // Parsea: ":8080" → Some(8080), "" → None
}

// ── preceded / terminated: ignorar contexto ─────────────────────────────
fn parsear_valor_header(input: &str) -> IResult<&str, &str> {
    // Parsea: "Content-Type: application/json" → "application/json"
    preceded(
        (take_until(":"), tag(": ")),
        take_while1(|c: char| c != '\n'),
    ).parse(input)
}
```

### Reporte de errores: `VerboseError`

```rust
use nom::{bytes::complete::take_while1, error::context, IResult, Parser};
use nom_language::error::{convert_error, VerboseError};

type PResult<'a, O> = IResult<&'a str, O, VerboseError<&'a str>>;

fn parsear_ip_verbose(input: &str) -> PResult<&str> {
    context(
        "dirección IP",   // mensaje de contexto en caso de error
        take_while1(|c: char| c.is_ascii_digit() || c == '.'),
    ).parse(input)
}

fn formatear_error(input: &str, err: VerboseError<&str>) -> String {
    convert_error(input, err)
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
    sequence::{delimited, preceded, terminated},
    IResult, Parser,
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
    take_while1(|c: char| c.is_ascii_digit() || c == '.' || c == ':').parse(input)
}

fn campo_opcional(input: &str) -> IResult<&str, Option<&str>> {
    // "-" representa un campo vacío en los logs de nginx
    alt((
        map(tag("-"), |_| None),
        map(take_while1(|c: char| c != ' ' && c != '\n'), Some),
    )).parse(input)
}

fn timestamp(input: &str) -> IResult<&str, &str> {
    delimited(char('['), take_until("]"), char(']')).parse(input)
}

fn peticion(input: &str) -> IResult<&str, (&str, &str, &str)> {
    // Parsea: "GET /ruta HTTP/1.1"
    delimited(
        char('"'),
        (
            terminated(
                take_while1(|c: char| c.is_ascii_alphabetic()),  // método
                char(' '),
            ),
            terminated(
                take_while1(|c: char| c != ' '),  // ruta
                char(' '),
            ),
            take_until("\""),  // protocolo
        ),
        char('"'),
    ).parse(input)
}

fn codigo_estado(input: &str) -> IResult<&str, u16> {
    map_res(digit1, |s: &str| s.parse::<u16>()).parse(input)
}

fn bytes_respuesta(input: &str) -> IResult<&str, Option<u64>> {
    alt((
        map(tag("-"), |_| None),
        map(map_res(digit1, |s: &str| s.parse::<u64>()), Some),
    )).parse(input)
}

fn campo_citado_opcional(input: &str) -> IResult<&str, Option<&str>> {
    opt(delimited(char('"'), take_until("\""), char('"'))).parse(input)
}

// ── Parser principal ─────────────────────────────────────────────────────

pub fn parsear_nginx(input: &str) -> IResult<&str, EntradaNginx<'_>> {
    let (input, ip_addr)   = terminated(ip, space1).parse(input)?;
    let (input, _ident)    = terminated(campo_opcional, space1).parse(input)?;
    let (input, usuario)   = terminated(campo_opcional, space1).parse(input)?;
    let (input, ts)        = terminated(timestamp, space1).parse(input)?;
    let (input, (met, ruta, proto)) = terminated(peticion, space1).parse(input)?;
    let (input, estado)    = terminated(codigo_estado, space1).parse(input)?;
    let (input, bytes)     = terminated(bytes_respuesta, space1).parse(input)?;
    let (input, referer)   = terminated(campo_citado_opcional, space1).parse(input)?;
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
--8<-- "src/chapter_04/config_pest/src/grammar/config.pest"
```

### Código Rust para usar el parser pest

Código completo, con tests del ejemplo válido y del mensaje de error, en
[`config_pest`](https://github.com/pepemxl/Rust-Notes/tree/master/src/chapter_04/config_pest):

```rust
--8<-- "src/chapter_04/config_pest/src/lib.rs"
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
//   = expected valor
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
    // En un raw string `\"` NO escapa la comilla: usamos r#"..."# para incluir `"`.
    let re = Regex::new(r#"https?://[^\s<>"]+"#).unwrap();
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
--8<-- "src/chapter_04/logparser/Cargo.toml"
```

### `src/lib.rs` y `src/parser/mod.rs`

Los módulos se exportan desde una librería para que `tests/` y `benches/` (que son
crates aparte) puedan usarlos; `main.rs` también los importa desde ahí.

```rust
--8<-- "src/chapter_04/logparser/src/lib.rs"
```

```rust
--8<-- "src/chapter_04/logparser/src/parser/mod.rs"
```

### `src/parser/nginx.rs` — el parser completo

```rust
--8<-- "src/chapter_04/logparser/src/parser/nginx.rs"
```

### `src/filter.rs` — DSL simple de filtros

```rust
--8<-- "src/chapter_04/logparser/src/filter.rs"
```

### `src/aggregate.rs` — conteos y estadísticas

```rust
--8<-- "src/chapter_04/logparser/src/aggregate.rs"
```

### `src/output.rs` — formatters de salida

```rust
--8<-- "src/chapter_04/logparser/src/output.rs"
```

### `src/main.rs` — CLI con clap

```rust
--8<-- "src/chapter_04/logparser/src/main.rs"
```

---

## Benchmarks con `criterion`

`benches/parser.rs`:

```rust
--8<-- "src/chapter_04/logparser/benches/parser.rs"
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
--8<-- "src/chapter_04/logparser/tests/parser_test.rs"
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
estados=(200 404 500)
for i in $(seq 1 20); do
  echo "10.0.0.$i - - [01/Jan/2024:00:00:00 +0000] \"GET /api/v1/items/$i HTTP/1.1\" ${estados[$((i % 3))]} $((i*100)) \"-\" \"curl/7.68\""
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
- [ ] Las regex se compilan una sola vez con `LazyLock<Regex>` / `OnceLock<Regex>` de
  `std` — nunca dentro de un bucle (y sin `lazy_static!`, que ya no hace falta).
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

!!! success "Fin del Mes 4"

    Has construido una CLI profesional, un wrapper FFI seguro, una
    app Wasm interactiva y un parser de logs de alto rendimiento.

    **Siguiente paso:** Mes 5 — [Arquitectura, patrones y rendimiento](../chapter_05/section_00.md).
