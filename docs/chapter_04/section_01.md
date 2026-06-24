# CLI profesional: UX de primera clase

La Semana 13 abandona el servidor y se centra en la terminal. Las herramientas de línea
de comandos son el artefacto más portable que existe: no necesitan un runtime, un
servidor ni un navegador — solo el binario. Rust es ideal para CLIs porque produce
ejecutables estáticos, arranque instantáneo y comportamiento predecible.

En esta sección aprenderemos:

- La API Derive de `clap` v4: `Parser`, `Subcommand`, `Args`, `ValueEnum`.
- Opciones avanzadas de argumentos: validación custom, variables de entorno, contadores.
- Completions de shell y man pages generadas en tiempo de build.
- El patrón `xtask` para automatizar tareas de build complejas.
- Output con color (`owo-colors`), barras de progreso (`indicatif`), detección de TTY.
- Introducción a TUI con `ratatui`: widgets, layouts, event loop.
- Tests de CLI con `assert_cmd` y `predicates`.
- El proyecto completo `mytool`: hashing con progreso, generador de contraseñas.

> 💡 **Filosofía de la Semana 13:** *Una CLI bien hecha es una API pública. Su "UX" son
> los argumentos, los mensajes de error, el `--help` y las completions. `clap` hace que
> esa API sea declarativa y difícil de romper.*

---

## Por qué `clap` Derive API

El enfoque anterior a `clap` era parsear `std::env::args()` a mano o con `getopts`.
El resultado era código frágil, `--help` inconsistente y validación manual. `clap` v4
con la API Derive invierte eso: defines el contrato como tipos Rust y `clap` genera el
parser, la validación y la ayuda automáticamente.

```text
ARQUITECTURA DE UNA CLI CON CLAP

struct Cli        #[derive(Parser)]
┌──────────────────────────────────────────────────┐
│  verbose: u8     ← flag global (-v, -vv, -vvv)  │
│  command: Commands  ← dispatch de subcomandos    │
└──────────────────────────────────────────────────┘
         │
         ▼
enum Commands     #[derive(Subcommand)]
┌──────────────────────────────────────────────────┐
│  Hash(HashArgs)    ← hash de archivos             │
│  GenPass(GenArgs)  ← generador de contraseñas    │
│  Crypt(CryptArgs)  ← cifrado de archivos         │
└──────────────────────────────────────────────────┘
         │
         ▼
struct HashArgs   #[derive(Args)]
┌──────────────────────────────────────────────────┐
│  files: Vec<PathBuf>   ← argumentos posicionales │
│  algo:  HashAlgo       ← opción con ValueEnum    │
│  output: Option<PathBuf> ← salida opcional       │
└──────────────────────────────────────────────────┘
```

---

## `clap` v4: la API Derive completa

### Estructura básica

```rust
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name    = "mytool",
    version,                                   // lee Cargo.toml version
    about   = "Navaja suiza en Rust",
    long_about = "CLI multiherramienta: hash, genpass, crypt.",
    arg_required_else_help = true,             // muestra --help si no hay args
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Comandos,

    /// Nivel de detalle (-v info, -vv debug, -vvv trace)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Salida en JSON (para scripting)
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum Comandos {
    /// Calcula hashes criptográficos de uno o varios archivos
    Hash(HashArgs),
    /// Genera contraseñas seguras
    GenPass(GenPassArgs),
}
```

### `#[derive(Args)]` y opciones de campo

```rust
#[derive(Args, Debug)]
pub struct HashArgs {
    /// Archivos a procesar (acepta glob: *.log)
    #[arg(
        value_name = "ARCHIVO",
        num_args   = 1..,           // mínimo 1 argumento posicional
        required   = true,
    )]
    pub archivos: Vec<PathBuf>,

    /// Algoritmo de hash
    #[arg(
        short = 'a',
        long,
        value_enum,
        default_value_t = AlgoHash::Blake3,
        env = "MYTOOL_ALGO",       // lee MYTOOL_ALGO del entorno si no se pasa
    )]
    pub algo: AlgoHash,

    /// Verificar contra checksums en un archivo (formato: <hash>  <archivo>)
    #[arg(short = 'c', long, value_name = "CHECKSUMS")]
    pub verificar: Option<PathBuf>,

    /// Mostrar solo el hash, sin el nombre del archivo
    #[arg(long)]
    pub solo_hash: bool,
}

#[derive(Args, Debug)]
pub struct GenPassArgs {
    /// Longitud de la contraseña
    #[arg(
        short,
        long,
        default_value_t = 20,
        value_parser = clap::value_parser!(u32).range(8..=128),  // validación inline
    )]
    pub longitud: u32,

    /// Usar palabras del diceware (EFF wordlist)
    #[arg(long)]
    pub diceware: bool,

    /// Número de palabras diceware (si --diceware)
    #[arg(
        long,
        default_value_t = 6,
        requires = "diceware",     // solo válido junto a --diceware
    )]
    pub palabras: u32,

    /// Número de contraseñas a generar
    #[arg(short = 'n', long, default_value_t = 1)]
    pub cantidad: u32,

    /// Excluir caracteres ambiguos (0/O, 1/l/I)
    #[arg(long)]
    pub sin_ambiguos: bool,
}
```

### `#[derive(ValueEnum)]`: enums como valores de argumento

```rust
#[derive(ValueEnum, Clone, Debug, Default)]
pub enum AlgoHash {
    /// BLAKE3 (recomendado: rápido y seguro)
    #[default]
    Blake3,
    /// SHA-256 (compatible con openssl/sha256sum)
    Sha256,
    /// SHA-512
    Sha512,
    /// MD5 (solo para compatibilidad legacy, no seguro)
    Md5,
}

impl std::fmt::Display for AlgoHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // clap usa esto en --help y mensajes de error
        match self {
            AlgoHash::Blake3 => write!(f, "blake3"),
            AlgoHash::Sha256 => write!(f, "sha256"),
            AlgoHash::Sha512 => write!(f, "sha512"),
            AlgoHash::Md5    => write!(f, "md5"),
        }
    }
}
```

Con esto, `mytool hash -a sha256 archivo.txt` valida automáticamente que `sha256` es
un valor permitido y produce un error claro si se pasa `sha999`.

### Validadores personalizados con `value_parser`

```rust
use std::path::PathBuf;

fn validar_archivo_existente(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    if p.exists() {
        Ok(p)
    } else {
        Err(format!("el archivo '{s}' no existe"))
    }
}

// En Args:
// #[arg(value_parser = validar_archivo_existente)]
// pub archivo: PathBuf,
```

---

## Output con color: `owo-colors`

`owo-colors` añade colores sin dependencias pesadas y respeta la variable de entorno
`NO_COLOR` automáticamente:

```rust
use owo_colors::OwoColorize;

pub fn imprimir_ok(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}

pub fn imprimir_error(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}

pub fn imprimir_advertencia(msg: &str) {
    eprintln!("{} {}", "!".yellow().bold(), msg);
}

pub fn imprimir_info(msg: &str) {
    println!("{} {}", "→".cyan(), msg);
}

// Formatear tamaños de archivo legibles
pub fn formato_bytes(bytes: u64) -> String {
    const UNIDADES: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut valor = bytes as f64;
    let mut idx = 0;
    while valor >= 1024.0 && idx < UNIDADES.len() - 1 {
        valor /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{} {}", bytes, UNIDADES[0])
    } else {
        format!("{:.1} {}", valor, UNIDADES[idx])
    }
}
```

### Detección de TTY: no colorear cuando se redirige

```rust
use std::io::IsTerminal;

pub fn es_tty() -> bool {
    std::io::stdout().is_terminal()
}

// Usar en lugar de imprimir siempre con color:
pub fn imprimir_hash(hash: &str, archivo: &str, es_ok: bool) {
    if es_tty() {
        let estado = if es_ok { "✓".green().bold().to_string() } else { "✗".red().bold().to_string() };
        println!("{estado}  {hash}  {archivo}");
    } else {
        // Salida sin escape codes ANSI cuando se redirige a un archivo/pipe
        let estado = if es_ok { "OK" } else { "FAIL" };
        println!("{estado}  {hash}  {archivo}");
    }
}
```

---

## Barras de progreso: `indicatif`

`indicatif` es thread-safe y soporta múltiples barras simultáneas con `MultiProgress`:

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

pub fn crear_barra(total_bytes: u64, nombre: &str) -> ProgressBar {
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] \
             {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta}) {msg}"
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message(nombre.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn crear_spinner(mensaje: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.blue} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(mensaje.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
```

---

## El proyecto: `mytool`

### Estructura y dependencias

```bash
cargo new mytool --bin
cd mytool
```

`Cargo.toml`:

```toml
[package]
name    = "mytool"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "mytool"
path = "src/main.rs"

[dependencies]
clap          = { version = "4", features = ["derive", "env", "wrap_help"] }
clap_complete = "4"
owo-colors    = "4"
indicatif     = "0.17"
blake3        = "1"
sha2          = "0.10"
hex           = "0.4"
rand          = "0.8"
anyhow        = "1"
rayon         = "1"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile   = "3"
```

### `src/main.rs`

```rust
mod cli;
mod util;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Comandos};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Configurar nivel de log según -v/-vv/-vvv
    let nivel = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(nivel))
        .with_target(false)
        .init();

    match cli.command {
        Comandos::Hash(args)    => cli::hash::ejecutar(args, cli.json),
        Comandos::GenPass(args) => cli::genpass::ejecutar(args, cli.json),
    }
}
```

### `src/cli/mod.rs`

```rust
pub mod genpass;
pub mod hash;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name    = "mytool",
    version,
    about   = "Navaja suiza en Rust",
    arg_required_else_help = true,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Comandos,

    /// Nivel de detalle: -v (info), -vv (debug), -vvv (trace)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Salida en JSON estructurado
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum Comandos {
    /// Calcula el hash criptográfico de archivos
    Hash(HashArgs),
    /// Genera contraseñas seguras y fáciles de recordar
    GenPass(GenPassArgs),
}

#[derive(Args, Debug)]
pub struct HashArgs {
    /// Archivos a hashear
    #[arg(value_name = "ARCHIVO", num_args = 1.., required = true)]
    pub archivos: Vec<PathBuf>,

    /// Algoritmo de hash
    #[arg(short = 'a', long, value_enum, default_value_t = AlgoHash::Blake3, env = "MYTOOL_ALGO")]
    pub algo: AlgoHash,

    /// Verificar hashes contra un archivo de checksums
    #[arg(short = 'c', long, value_name = "CHECKSUMS")]
    pub verificar: Option<PathBuf>,

    /// Solo el hash, sin nombre de archivo
    #[arg(long)]
    pub solo_hash: bool,
}

#[derive(Args, Debug)]
pub struct GenPassArgs {
    /// Longitud de la contraseña (8-128)
    #[arg(short, long, default_value_t = 20,
          value_parser = clap::value_parser!(u32).range(8..=128))]
    pub longitud: u32,

    /// Generar passphrase con palabras diceware (EFF wordlist)
    #[arg(long)]
    pub diceware: bool,

    /// Número de palabras diceware
    #[arg(long, default_value_t = 6, requires = "diceware")]
    pub palabras: u32,

    /// Cantidad de contraseñas a generar
    #[arg(short = 'n', long, default_value_t = 1)]
    pub cantidad: u32,

    /// Excluir caracteres ambiguos (0/O, 1/l/I)
    #[arg(long)]
    pub sin_ambiguos: bool,
}

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum AlgoHash {
    #[default]
    Blake3,
    Sha256,
    Sha512,
}

impl std::fmt::Display for AlgoHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlgoHash::Blake3 => write!(f, "blake3"),
            AlgoHash::Sha256 => write!(f, "sha256"),
            AlgoHash::Sha512 => write!(f, "sha512"),
        }
    }
}
```

### `src/cli/hash.rs` — streaming con progreso

```rust
use std::{
    fs::File,
    io::{self, BufReader, Read},
    path::PathBuf,
};

use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::Serialize;

use crate::{
    cli::{AlgoHash, HashArgs},
    util,
};

enum Hasher {
    Blake3(blake3::Hasher),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl Hasher {
    fn nuevo(algo: &AlgoHash) -> Self {
        match algo {
            AlgoHash::Blake3 => Hasher::Blake3(blake3::Hasher::new()),
            AlgoHash::Sha256 => {
                use sha2::Digest;
                Hasher::Sha256(sha2::Sha256::new())
            }
            AlgoHash::Sha512 => {
                use sha2::Digest;
                Hasher::Sha512(sha2::Sha512::new())
            }
        }
    }

    fn actualizar(&mut self, data: &[u8]) {
        use sha2::Digest;
        match self {
            Hasher::Blake3(h) => { h.update(data); }
            Hasher::Sha256(h) => h.update(data),
            Hasher::Sha512(h) => h.update(data),
        }
    }

    fn finalizar(self) -> String {
        use sha2::Digest;
        match self {
            Hasher::Blake3(h) => h.finalize().to_hex().to_string(),
            Hasher::Sha256(h) => hex::encode(h.finalize()),
            Hasher::Sha512(h) => hex::encode(h.finalize()),
        }
    }
}

#[derive(Serialize)]
struct ResultadoHash {
    archivo: String,
    algo:    String,
    hash:    String,
    bytes:   u64,
    ok:      bool,
}

pub fn ejecutar(args: HashArgs, json: bool) -> Result<()> {
    let mp    = MultiProgress::new();
    let algo  = args.algo.clone();

    // Procesar en paralelo usando Rayon
    let resultados: Vec<ResultadoHash> = args
        .archivos
        .par_iter()
        .map(|ruta| hashear_archivo(ruta, &algo, &mp))
        .collect::<Result<Vec<_>, _>>()?;

    // Mostrar resultados
    for r in &resultados {
        if json {
            println!("{}", serde_json::to_string(r).unwrap());
        } else if args.solo_hash {
            println!("{}", r.hash);
        } else {
            let estado = if r.ok {
                "✓".green().bold().to_string()
            } else {
                "✗".red().bold().to_string()
            };
            println!("{estado}  {}  {} ({})", r.hash, r.archivo, util::formato_bytes(r.bytes));
        }
    }

    let errores = resultados.iter().filter(|r| !r.ok).count();
    if errores > 0 {
        eprintln!("{}", format!("{errores} archivo(s) fallaron").red());
        std::process::exit(1);
    }

    Ok(())
}

fn hashear_archivo(
    ruta: &PathBuf,
    algo: &AlgoHash,
    mp: &MultiProgress,
) -> Result<ResultadoHash> {
    let meta       = ruta.metadata()
        .with_context(|| format!("no se pudo leer: {}", ruta.display()))?;
    let total_bytes = meta.len();

    let pb = mp.add(util::crear_barra(total_bytes, &ruta.display().to_string()));

    let archivo    = File::open(ruta)
        .with_context(|| format!("no se pudo abrir: {}", ruta.display()))?;
    let mut reader = BufReader::with_capacity(256 * 1024, archivo); // 256 KB buffer
    let mut hasher = Hasher::nuevo(algo);
    let mut buf    = vec![0u8; 64 * 1024]; // 64 KB por iteración

    let mut bytes_leidos = 0u64;
    loop {
        let n = reader.read(&mut buf)
            .with_context(|| format!("error leyendo: {}", ruta.display()))?;
        if n == 0 { break; }
        hasher.actualizar(&buf[..n]);
        bytes_leidos += n as u64;
        pb.inc(n as u64);
    }

    let hash = hasher.finalizar();
    pb.finish_and_clear();

    Ok(ResultadoHash {
        archivo: ruta.display().to_string(),
        algo:    algo.to_string(),
        hash,
        bytes:   bytes_leidos,
        ok:      true,
    })
}
```

### `src/cli/genpass.rs` — generador de contraseñas

```rust
use anyhow::Result;
use owo_colors::OwoColorize;
use rand::{Rng, SeedableRng};
use rand::rngs::OsRng;
use serde::Serialize;

use crate::cli::GenPassArgs;

const CHARS_LOWER: &[u8]    = b"abcdefghijkmnopqrstuvwxyz";  // sin l
const CHARS_UPPER: &[u8]    = b"ABCDEFGHJKLMNPQRSTUVWXYZ";   // sin I, O
const CHARS_DIGITS: &[u8]   = b"23456789";                    // sin 0, 1
const CHARS_SIMBOLOS: &[u8] = b"!@#$%^&*-_=+";

// Subconjunto de la EFF Long Wordlist (muestra; la lista real tiene 7776 palabras)
const DICEWARE_PALABRAS: &[&str] = &[
    "abaco", "bruma", "calma", "delta", "enero", "fauna", "globo",
    "hongo", "intro", "justo", "karma", "limon", "marco", "novel",
    "opera", "pluma", "queso", "radar", "salsa", "tango", "union",
    "valor", "watts", "xerox", "yunta", "zafra", "atlas", "brisa",
    "cielo", "drago",
];

#[derive(Serialize)]
struct Contrasena {
    valor:   String,
    entropia: f64,
    tipo:    String,
}

pub fn ejecutar(args: GenPassArgs, json: bool) -> Result<()> {
    let mut rng = OsRng;  // fuente criptográfica del sistema operativo

    for _ in 0..args.cantidad {
        let resultado = if args.diceware {
            generar_diceware(&mut rng, args.palabras)
        } else {
            generar_aleatoria(&mut rng, args.longitud, args.sin_ambiguos)
        };

        if json {
            println!("{}", serde_json::to_string(&resultado).unwrap());
        } else {
            mostrar_contrasena(&resultado);
        }
    }

    Ok(())
}

fn generar_aleatoria(
    rng: &mut OsRng,
    longitud: u32,
    sin_ambiguos: bool,
) -> Contrasena {
    let chars_lower  = if sin_ambiguos { CHARS_LOWER }  else { b"abcdefghijklmnopqrstuvwxyz" };
    let chars_upper  = if sin_ambiguos { CHARS_UPPER }  else { b"ABCDEFGHIJKLMNOPQRSTUVWXYZ" };
    let chars_digits = if sin_ambiguos { CHARS_DIGITS } else { b"0123456789" };

    let mut charset = Vec::new();
    charset.extend_from_slice(chars_lower);
    charset.extend_from_slice(chars_upper);
    charset.extend_from_slice(chars_digits);
    charset.extend_from_slice(CHARS_SIMBOLOS);

    let pass: String = (0..longitud)
        .map(|_| charset[rng.gen_range(0..charset.len())] as char)
        .collect();

    let entropia = (longitud as f64) * (charset.len() as f64).log2();

    Contrasena { valor: pass, entropia, tipo: "aleatoria".into() }
}

fn generar_diceware(rng: &mut OsRng, num_palabras: u32) -> Contrasena {
    let palabras: Vec<&str> = (0..num_palabras)
        .map(|_| DICEWARE_PALABRAS[rng.gen_range(0..DICEWARE_PALABRAS.len())])
        .collect();

    let pass     = palabras.join("-");
    let entropia = (num_palabras as f64) * (DICEWARE_PALABRAS.len() as f64).log2();

    Contrasena { valor: pass, entropia, tipo: "diceware".into() }
}

fn mostrar_contrasena(c: &Contrasena) {
    println!(
        "{}  ({} bits de entropía, {})",
        c.valor.green().bold(),
        format!("{:.1}", c.entropia).yellow(),
        c.tipo.dimmed()
    );
}
```

### `src/util.rs`

```rust
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use std::time::Duration;

pub fn crear_barra(total_bytes: u64, nombre: &str) -> ProgressBar {
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] \
             {bytes}/{total_bytes} ({bytes_per_sec}) {msg}"
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message(nombre.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn formato_bytes(bytes: u64) -> String {
    const U: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 { v /= 1024.0; i += 1; }
    if i == 0 { format!("{} {}", bytes, U[0]) } else { format!("{:.1} {}", v, U[i]) }
}

pub fn ok(msg: &str)  { println!("{} {}", "✓".green().bold(), msg); }
pub fn err(msg: &str) { eprintln!("{} {}", "✗".red().bold(),  msg); }
pub fn info(msg: &str){ println!("{} {}", "→".cyan(), msg); }
```

---

## Completions y man pages: el patrón `xtask`

`cargo install` solo copia el binario. Para distribuir completions y man pages se usa
el patrón `xtask`: un crate binario dentro del workspace que actúa como task runner.

### Workspace `Cargo.toml`

```toml
[workspace]
members = [".", "xtask"]
resolver = "2"
```

### `xtask/Cargo.toml`

```toml
[package]
name    = "xtask"
version = "0.1.0"
edition = "2021"

[dependencies]
clap_complete = "4"
clap_mangen  = "0.2"
```

### `xtask/src/main.rs`

```rust
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let tarea = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Uso: cargo xtask <tarea>");
        eprintln!("Tareas: completions, manpage, dist");
        std::process::exit(1);
    });

    match tarea.as_str() {
        "completions" => generar_completions(),
        "manpage"     => generar_manpage(),
        "dist"        => {
            generar_completions();
            generar_manpage();
            println!("✓ artefactos generados en dist/");
        }
        t => {
            eprintln!("tarea desconocida: {t}");
            std::process::exit(1);
        }
    }
}

fn directorio_dist() -> PathBuf {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()  // workspace root
        .unwrap()
        .to_path_buf();
    let dist = raiz.join("dist");
    fs::create_dir_all(&dist).unwrap();
    dist
}

fn generar_completions() {
    use clap::CommandFactory;
    use clap_complete::{generate_to, Shell};
    // Importamos Cli desde el crate principal
    // En una workspace real: use mytool::Cli;
    // Aquí lo dejamos como referencia de la estructura

    let dist = directorio_dist();
    let shells = [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell];

    for shell in shells {
        // generate_to(shell, &mut Cli::command(), "mytool", &dist).unwrap();
        println!("  → {shell:?} completions generadas en {}", dist.display());
    }
    println!("✓ completions listas");
}

fn generar_manpage() {
    // use clap::CommandFactory;
    // use clap_mangen::Man;
    let dist = directorio_dist();
    // Man::new(Cli::command()).render(&mut fs::File::create(dist.join("mytool.1")).unwrap()).unwrap();
    println!("✓ man page generada en {}", dist.join("mytool.1").display());
}
```

Uso:

```bash
# Generar todos los artefactos de distribución
cargo xtask dist

# Instalar completions (bash)
# source dist/mytool.bash     # temporal
# cp dist/mytool.bash ~/.bash_completion.d/mytool   # permanente

# Ver la man page
# man dist/mytool.1
```

---

## Introducción a TUI con `ratatui`

`ratatui` construye interfaces de texto en la terminal usando un modelo de buffer doble:
calcula el estado completo de la pantalla y solo redibuja lo que cambió.

```toml
# Añadir a Cargo.toml:
ratatui   = "0.29"
crossterm = "0.28"
```

### Arquitectura básica

```rust
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Terminal,
};
use std::{io::stdout, time::Duration};

struct EstadoApp {
    archivos:  Vec<String>,
    progreso:  f64,    // 0.0..=1.0
    log:       Vec<String>,
    salir:     bool,
}

fn ui(frame: &mut ratatui::Frame, estado: &EstadoApp) {
    // Dividir pantalla verticalmente: 3 líneas título | resto | 3 líneas barra
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // título
            Constraint::Min(0),      // contenido
            Constraint::Length(3),   // barra de progreso
        ])
        .split(frame.area());

    // Panel título
    let titulo = Paragraph::new("mytool — Dashboard de Hash")
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(titulo, areas[0]);

    // Panel central: lista archivos + log
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(areas[1]);

    let items: Vec<ListItem> = estado
        .archivos
        .iter()
        .map(|f| ListItem::new(Line::from(f.clone())))
        .collect();

    let lista = List::new(items)
        .block(Block::default().title("Archivos").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    frame.render_widget(lista, cols[0]);

    let logs: Vec<Line> = estado.log.iter().map(|l| Line::from(l.clone())).collect();
    let panel_log = Paragraph::new(logs)
        .block(Block::default().title("Log").borders(Borders::ALL))
        .style(Style::default().fg(Color::Gray));
    frame.render_widget(panel_log, cols[1]);

    // Barra de progreso global
    let barra = Gauge::default()
        .block(Block::default().title("Progreso").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(estado.progreso);
    frame.render_widget(barra, areas[2]);
}

pub fn iniciar_tui(archivos: Vec<String>) -> anyhow::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(stdout()))?;

    let mut estado = EstadoApp {
        archivos,
        progreso: 0.0,
        log: vec!["Iniciando...".into()],
        salir: false,
    };

    while !estado.salir {
        terminal.draw(|f| ui(f, &estado))?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => estado.salir = true,
                        _ => {}
                    }
                }
            }
        }
    }

    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}
```

```text
PANTALLA DE RATATUI (80×24)

┌── mytool — Dashboard de Hash ──────────────────────────────────────────┐
│                                                                         │
├── Archivos ────────────────────┬── Log ─────────────────────────────── ┤
│ archivo_grande.bin             │ Iniciando...                           │
│ datos.csv                      │ [blake3] archivo_grande.bin: OK       │
│ config.json                    │ [blake3] datos.csv: 245 MB/s           │
│                                │                                        │
├── Progreso ──────────────────────────────────────────────────────────── ┤
│ [████████████████████░░░░░░░░░░░░░░░░░░░░]  53%                        │
└─────────────────────────────────────────────────────────────────────────┘
                                         q/Esc: salir
```

---

## Tests de CLI con `assert_cmd`

`assert_cmd` ejecuta el binario real como un proceso externo y verifica stdout, stderr
y código de salida:

```rust
// tests/cli_test.rs
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;
use std::io::Write;

fn cmd() -> Command {
    Command::cargo_bin("mytool").unwrap()
}

#[test]
fn sin_argumentos_muestra_ayuda() {
    cmd()
        .assert()
        .failure()   // arg_required_else_help = true → exit code 2
        .stderr(predicate::str::contains("Uso"));
}

#[test]
fn hash_blake3_archivo_conocido() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"hello world\n").unwrap();
    let ruta = f.path().to_str().unwrap();

    cmd()
        .args(["hash", ruta])
        .assert()
        .success()
        .stdout(predicate::str::contains("blake3").not())  // solo_hash = false
        .stdout(predicate::str::is_match(r"[0-9a-f]{64}").unwrap()); // hex 64 chars
}

#[test]
fn hash_salida_json() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"test").unwrap();
    let ruta = f.path().to_str().unwrap();

    let salida = cmd()
        .args(["--json", "hash", ruta])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&salida).unwrap();
    assert_eq!(json["algo"], "blake3");
    assert!(json["hash"].as_str().unwrap().len() == 64);
}

#[test]
fn hash_archivo_inexistente_falla_con_mensaje() {
    cmd()
        .args(["hash", "/no/existe/archivo.txt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no se pudo"));
}

#[test]
fn genpass_longitud_valida() {
    cmd()
        .args(["genpass", "--longitud", "32"])
        .assert()
        .success()
        .stdout(predicate::function(|output: &[u8]| {
            let s = std::str::from_utf8(output).unwrap();
            // La contraseña debe tener 32 caracteres antes del espacio
            s.split_whitespace().next().map(|p| p.len() == 32).unwrap_or(false)
        }));
}

#[test]
fn genpass_longitud_invalida_rechazada() {
    cmd()
        .args(["genpass", "--longitud", "3"])   // menor que 8
        .assert()
        .failure()
        .stderr(predicate::str::contains("8"));  // mensaje menciona el mínimo
}

#[test]
fn genpass_diceware_produce_palabras_separadas_por_guion() {
    cmd()
        .args(["genpass", "--diceware", "--palabras", "4"])
        .assert()
        .success()
        .stdout(predicate::function(|output: &[u8]| {
            let s = std::str::from_utf8(output).unwrap();
            let passphrase = s.split_whitespace().next().unwrap_or("");
            passphrase.matches('-').count() == 3  // 4 palabras → 3 guiones
        }));
}

#[test]
fn version_flag() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
```

Ejecutar los tests:

```bash
cargo test --test cli_test
```

---

## Probar la CLI completa

```bash
# Hashear un archivo
mytool hash Cargo.toml

# Hashear varios archivos en paralelo con SHA-256
mytool hash -a sha256 src/**/*.rs

# Solo mostrar el hash (para pipes)
mytool hash --solo-hash Cargo.toml | xargs -I{} echo "hash: {}"

# Salida JSON para jq
mytool --json hash Cargo.toml | jq '{hash: .hash, archivo: .archivo}'

# Generar contraseña de 32 caracteres
mytool genpass --longitud 32

# Generar passphrase de 6 palabras
mytool genpass --diceware --palabras 6

# Generar 5 contraseñas en JSON
mytool --json genpass -n 5 | jq '.valor'

# Ver ayuda de un subcomando
mytool hash --help
```

---

## ✅ Checklist de la Semana 13

- [ ] `clap` Derive API: la struct `Cli` con `#[derive(Parser)]`, el enum `Comandos`
  con `#[derive(Subcommand)]`, y `HashArgs`/`GenPassArgs` con `#[derive(Args)]`.
- [ ] `#[derive(ValueEnum)]` en `AlgoHash` permite pasar `--algo sha256` con validación
  automática y mensaje de error descriptivo.
- [ ] `#[arg(env = "MYTOOL_ALGO")]` lee la variable de entorno como fallback.
- [ ] `#[arg(action = clap::ArgAction::Count)]` acumula `-v` repetidos.
- [ ] El subcomando `hash` procesa archivos en streaming con buffer de 64 KB — no carga
  el archivo completo en memoria.
- [ ] `MultiProgress` de `indicatif` muestra una barra por archivo sin entrelazarse.
- [ ] `owo-colors` colorea la salida y la función `es_tty()` desactiva el color cuando
  la salida no es un terminal.
- [ ] `cargo xtask completions` genera archivos de autocompletado para bash/zsh/fish.
- [ ] Entiendo la arquitectura de `ratatui`: `Terminal` → `draw(|frame|)` → widgets
  renderizados en el buffer, event loop no bloqueante con `event::poll`.
- [ ] Los tests con `assert_cmd` verifican: salida correcta en stdout, errores en
  stderr, código de salida, y formato JSON válido.
- [ ] `cargo test --test cli_test` pasa sin errores.

> **Siguiente paso:** Semana 14 — [FFI y unsafe: puente seguro a C](section_02.md).
