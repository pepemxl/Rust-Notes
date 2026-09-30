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

!!! quote "Filosofía de la Semana 13"

    *Una CLI bien hecha es una API pública. Su "UX" son
    los argumentos, los mensajes de error, el `--help` y las completions. `clap` hace que
    esa API sea declarativa y difícil de romper.*

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
use owo_colors::OwoColorize;
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
--8<-- "src/chapter_04/mytool/Cargo.toml"
```

(`rust-version.workspace` y `publish` vienen del workspace de estas notas; en tu
proyecto puedes omitirlos.)

### `src/lib.rs` y `src/main.rs`

Los módulos viven en una **librería** (`lib.rs`) y `main.rs` solo la usa. Así otros
crates del workspace, como `xtask` más abajo, pueden importar `mytool::cli::Cli` para
generar completions y man pages a partir de la misma definición de clap.

```rust
--8<-- "src/chapter_04/mytool/src/lib.rs"
```

```rust
--8<-- "src/chapter_04/mytool/src/main.rs"
```

### `src/cli/mod.rs`

```rust
--8<-- "src/chapter_04/mytool/src/cli/mod.rs"
```

### `src/cli/hash.rs` — streaming con progreso

```rust
--8<-- "src/chapter_04/mytool/src/cli/hash.rs"
```

### `src/cli/genpass.rs` — generador de contraseñas

```rust
--8<-- "src/chapter_04/mytool/src/cli/genpass.rs"
```

!!! warning "La lista de palabras es una muestra"

    Con 30 palabras, cada palabra aporta solo log2(30) ≈ 4,9 bits: una passphrase de 6
    palabras tiene ~29 bits, demasiado poco. Con la
    [EFF Long Wordlist](https://www.eff.org/dice) completa (7776 palabras) cada palabra
    aporta 12,9 bits y 6 palabras dan ~77 bits. En un proyecto real, incluye la lista
    completa con `include_str!("eff_large_wordlist.txt")`.

### `src/util.rs`

```rust
--8<-- "src/chapter_04/mytool/src/util.rs"
```

---

## Completions y man pages: el patrón `xtask`

`cargo install` solo copia el binario. Para distribuir completions y man pages se usa
el patrón `xtask`: un crate binario dentro del workspace que actúa como task runner.

### Workspace `Cargo.toml`

```toml
[workspace]
members = [".", "xtask"]
resolver = "3"
```

### `xtask/Cargo.toml`

```toml
--8<-- "src/chapter_04/xtask/Cargo.toml"
```

### `xtask/src/main.rs`

```rust
--8<-- "src/chapter_04/xtask/src/main.rs"
```

### `.cargo/config.toml`

`cargo xtask` no es un comando de Cargo: es un **alias** que se define en la raíz del
workspace. Sin él, el equivalente es `cargo run -p xtask -- dist`.

```toml
[alias]
xtask = "run --package xtask --"
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
--8<-- "src/chapter_04/mytool/tests/cli_test.rs"
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

!!! abstract "Siguiente paso"

    Semana 14 — [FFI y unsafe: puente seguro a C](section_02.md).
