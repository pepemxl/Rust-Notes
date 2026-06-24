# 🦀 MES 4: SISTEMAS, CLI AVANZADO Y WASM — Guía Detallada
> **Filosofía del Mes:** *"Rust brilla donde otros lenguajes tiemblan: en la frontera con el hardware (FFI), en la terminal (CLI UX), en el navegador (Wasm) y procesando datos masivos (Parsing). Aquí es donde 'Zero-Cost Abstractions' se vuelve tangible."*
> **Meta:** Entregar 4 artefactos profesionales: **CLI instalable**, **Wrapper FFI seguro**, **App Wasm interactiva**, **Parser streaming de alto rendimiento**.

---

## 📅 SEMANA 13: CLI PROFESIONAL — UX DE PRIMERA CLASE
**Objetivo:** Dejar de escribir `std::env::args().nth(1)...`. Dominar `clap` Derive API, completions, color, progreso y TUI básica.

### 🎯 Conceptos Clave

#### 1. `clap` v4+ Derive API (Estándar actual)
```rust
use clap::{Parser, Subcommand, Args, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "mytool", version, about = "Navaja suiza en Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true, help = "Verbosity level")]
    verbose: u8,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Hash files with progress bar
    Hash(HashArgs),
    /// Generate secure passwords
    GenPass(GenPassArgs),
    /// Encrypt/Decrypt files (age/rage compatible)
    Crypt(CryptArgs),
}

#[derive(Args, Debug)]
struct HashArgs {
    #[arg(value_name = "FILES", num_args = 1.., help = "Files to hash")]
    files: Vec<PathBuf>,
    
    #[arg(short, long, value_enum, default_value_t = HashAlgo::Blake3)]
    algo: HashAlgo,
}

#[derive(ValueEnum, Clone, Debug)]
enum HashAlgo { Blake3, Sha256, Md5 }
```
*   **`#[arg(...)]`**: `short`, `long`, `default_value`, `default_value_if`, `env` (lee `MYTOOL_ALGO`), `value_parser` (validación custom), `num_args` (0.., 1.., N), `action` (count, set_true, append).
*   **`#[command(subcommand)]`**: Dispatch automático.

#### 2. Completions & Man Pages (Build-time Generation)
**`build.rs` o `Cargo.toml` `[profile.release]` + `cargo xtask` pattern.**
```rust
// build.rs (simple) o mejor: xtask crate para lógica compleja
use clap::CommandFactory;
use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut cmd = Cli::command();
    
    // Generar completions
    for shell in [clap_complete::Shell::Bash, clap_complete::Shell::Zsh, clap_complete::Shell::Fish, clap_complete::Shell::PowerShell] {
        clap_complete::generate(shell, &mut cmd, "mytool", &mut std::fs::File::create(out_dir.join(format!("mytool.{}", shell))).unwrap());
    }
    // Man page (requiere clap_mangen feature)
    // clap_mangen::Man::new(cmd).render(&mut File::create(out_dir.join("mytool.1"))).unwrap();
    
    // Copiar a directorio de instalación (requiere logic post-install o xtask)
}
```
*   **Patrón `xtask`**: Crate binario interno (`xtask/`) para tareas de build complejas (generar docs, completions, firmar, empaquetar `.deb`/`.rpm`).

#### 3. Coloreo y Output Bonito
*   **`owo-colors` / `anstyle`**: `println!("{}", "Error".red().bold());` (Compatible con `tracing`).
*   **`indicatif`**: Progress bars **multi-thread safe**, templates custom, `MultiProgress` para hilos paralelos.
*   **`console`**: Detección TTY, ancho terminal, `style()`.

#### 4. TUI Intro: `ratatui` (Fork de `tui-rs`)
*   Arquitectura: **Terminal** (Backend: `crossterm`/`termion`) -> **Frame** (Buffer) -> **Widgets** (`Block`, `List`, `Table`, `Chart`, `Paragraph`, `Tabs`, `Gauge`).
*   **Layout**: `Layout::default().direction(Direction::Vertical).constraints([...]).split(area)`.
*   **Event Loop**: `loop { terminal.draw(|f| ui(f, &app))?; handle_events(&mut app)?; }`.

### 🛠️ Proyecto: **`mytool` — CLI Multiusos Publicable**

#### Estructura
```text
mytool/
├── Cargo.toml
├── xtask/                 # Build automation (opcional pero recomendado)
│   ├── Cargo.toml
│   └── src/main.rs        # Genera completions, man pages, checksums
├── src/
│   ├── main.rs            # Entry point, clap setup
│   ├── cli/               # Modulos por subcomando
│   │   ├── mod.rs
│   │   ├── hash.rs        # indicatif + blake3/sha2 streaming
│   │   ├── genpass.rs     # entropy, wordlist (eff/diceware)
│   │   └── crypt.rs       # age encryption (lib: rage/age)
│   ├── ui/                # TUI opcional (dashboard hash)
│   │   └── dashboard.rs   # ratatui: lista archivos, progreso, logs
│   └── util.rs            # Colores, formato bytes, errores
└── tests/
    └── cli_test.rs        # assert_cli / snapbox testing
```

#### Implementación Core: `hash.rs` (Streaming + Progress)
```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::{BufReader, Read};
use blake3; // or sha2::Sha256

pub fn hash_files(files: Vec<PathBuf>, algo: HashAlgo) -> Result<()> {
    let mp = MultiProgress::new();
    let style = ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}")?;
    
    // Paralelismo con rayon (opcional) o secuencial con MultiProgress
    for file in files {
        let pb = mp.add(ProgressBar::new(file.metadata()?.len()));
        pb.set_style(style.clone());
        pb.set_message(file.display().to_string());
        
        let mut hasher = match algo { HashAlgo::Blake3 => Hasher::Blake3(blake3::Hasher::new()), ... };
        let mut reader = BufReader::new(File::open(&file)?);
        let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
        
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 { break; }
            hasher.update(&buffer[..n]);
            pb.inc(n as u64);
        }
        pb.finish_with_message(format!("{}  {}", hasher.finalize(), file.display()));
    }
    Ok(())
}
```

#### Tests CLI (`tests/cli_test.rs` con `assert_cmd`)
```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_hash_single_file() {
    let mut cmd = Command::cargo_bin("mytool").unwrap();
    cmd.args(["hash", "Cargo.toml"])
       .assert()
       .success()
       .stdout(predicate::str::contains("blake3"));
}
```

#### Entregable: `cargo install --git <url> mytool` funciona. Completions generadas. `mytool --help` bonito.

---

## 📅 SEMANA 14: FFI & `unsafe` — PUENTE SEGURO A C
**Objetivo:** Llamar C desde Rust y exponer Rust a C **sin UB (Undefined Behavior)**. Encapsular `unsafe` en APIs 100% Safe.

### 🎯 Conceptos Clave

#### 1. FFI Basics: `extern "C"`, `#[no_mangle]`, `repr(C)`
```rust
// Rust -> C (Exportar)
#[no_mangle] // Nombre simbólico exacto "add"
pub extern "C" fn add(a: i32, b: i32) -> i32 { a + b }

// Structs compartidos (Layout garantizado)
#[repr(C)] // Orden de campos = C, sin padding sorpresa
pub struct Point { pub x: f64, pub y: f64 }

// Callbacks (Function Pointers)
type Callback = extern "C" fn(*mut c_void, i32);
```

#### 2. Tipos de Datos FFI (`std::ffi`, `libc`)
| Rust | C | Uso |
| :--- | :--- | :--- |
| `c_int`, `c_long`, `c_char` | `int`, `long`, `char` | Primitivos (`libc` crate). |
| `CString` / `CStr` | `char*` (NUL-terminated) | **Strings**: `CString::new("hi").unwrap().as_ptr()`. **NUNCA** `String::as_ptr()` (no NUL term). |
| `*mut T` / `*const T` | `T*` / `const T*` | Punteros raw. |
| `Option<NonNull<T>>` | `T*` (nullable) | **Mejor que `*mut T`**: `NonNull` covarianza + optimización `Option` (null pointer optimization). |

#### 3. `bindgen` — Generación Automática de Bindings
**`build.rs`**:
```rust
// build.rs
fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    
    let bindings = bindgen::Builder::default()
        .header("wrapper.h") // Header C con includes
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .allowlist_function("sodium_.*") // Solo lo que necesitamos
        .allowlist_type("crypto_.*")
        .derive_default(true)
        .derive_debug(true)
        .generate()
        .expect("bindgen failed");
    
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out_path.join("bindings.rs")).unwrap();
    
    // Compilar librería C si es necesario (cc crate)
    cc::Build::new().file("src/libsodium_stub.c").compile("mylib");
}
```
*   **`wrapper.h`**: `#include <sodium.h>` (o tu header).

#### 4. Reglas de Oro `unsafe` (Rustonomicon)
1.  **No dereferenciar punteros nulos/inválidos.**
2.  **No crear referencias inválidas** (`&*ptr` requiere `ptr` alineado, válido, initialized, no aliasing mutable).
3.  **No romper aliasing** (`&mut` único, `&` shared frozen).
4.  **No memory leaks** (safe code no debe leakear, pero `unsafe` puede). `Box::leak` es safe pero leak.
5.  **`Send`/`Sync` correctos** en structs con punteros raw.

#### 5. Patrón "Safe Wrapper"
```rust
// ffi/raw.rs (bindings generados + manuales)
pub mod raw { /* bindgen output */ }

// ffi/wrapper.rs (API SEGURA)
use crate::ffi::raw;
use std::ptr::NonNull;

pub struct CryptoBox { 
    // Invariante: ptr siempre válido, apunta a memoria inicializada por libsodium
    ptr: NonNull<raw::crypto_box_state>, 
}

impl CryptoBox {
    pub fn new(key: &[u8; 32]) -> Result<Self, CryptoError> {
        // 1. Validar inputs (Rust side)
        // 2. Alloc memoria (Rust allocator o C allocator)
        let mut state = std::mem::MaybeUninit::uninit();
        // 3. Llamada UNSAFE encapsulada
        let res = unsafe { raw::crypto_box_init(state.as_mut_ptr(), key.as_ptr()) };
        // 4. Chequear errores C -> Result
        if res != 0 { return Err(CryptoError::InitFailed); }
        // 5. Construir Safe Struct
        Ok(Self { ptr: NonNull::new(state.as_mut_ptr()).unwrap() })
    }
    
    pub fn encrypt(&mut self, nonce: &[u8; 24], msg: &[u8]) -> Vec<u8> {
        // Lógica segura, llama a raw::crypto_box_easy
    }
}

impl Drop for CryptoBox {
    fn drop(&mut self) {
        // Limpieza C (zeroize keys)
        unsafe { raw::crypto_box_cleanup(self.ptr.as_ptr()) };
    }
}
```

### 🛠️ Ejercicio: **Wrapper Seguro para `libsodium` (crypto_secretbox)**
1.  **`build.rs`**: `bindgen` + `cc` compila `libsodium` (o usa `pkg-config` + system lib).
2.  **`src/sodium.rs`**: Módulo `raw` (bindings), módulo `safe` (`SecretBox`, `Nonce`, `Key`).
3.  **API**: `SecretBox::encrypt(key: &Key, nonce: &Nonce, msg: &[u8]) -> Vec<u8>`.
4.  **Tests**: Vectores de prueba oficiales (RFC 8439 / libsodium test suite). Test `wrong key fails`, `wrong nonce fails`, `large message`.
5.  **Docs**: `/// # Safety` en internals. `/// # Panics` si precondiciones rotas.

---

## 📅 SEMANA 15: WEBASSEMBLY (WASM) — RUST EN EL NAVEGADOR
**Objetivo:** Compilar Rust a Wasm, interactuar con JS/DOM, paralelismo con `rayon`/`wasm-bindgen-rayon`, integrar en frontend real.

### 🎯 Conceptos Clave

#### 1. Toolchain: `wasm-pack` + `wasm-bindgen`
*   **`wasm-pack build --target web`**: Genera `.wasm` + `.js` glue + `.d.ts` (Typescript).
*   **`wasm-bindgen`**: Puente Rust <-> JS.
    *   `#[wasm_bindgen]` en `fn`, `struct`, `impl`, `extern "C"`.
    *   Tipos soportados nativamente: `js_sys` (`Array`, `Object`, `Promise`, `Function`, `Date`, `RegExp`, `Map`, `Set`, `Uint8Array`, `Float64Array`...), `web_sys` (APIs DOM: `Document`, `Canvas`, `WebGL`, `Fetch`, `localStorage`).
    *   **Ownership**: `JsValue` (GC'd por JS). `Box<[u8]>` -> `Uint8Array` (copia o `take` ownership).

#### 2. Memoria y Performance
*   **`wee_alloc` / `lol_alloc`**: Allocators pequeños (~1KB) vs `dlmalloc` default. `#[global_allocator] static ALLOC: wee_alloc::WeeAlloc = ...`.
*   **`#[wasm_bindgen(js_name = "...")]`**: Renombrar exports.
*   **`#[wasm_bindgen(getter, setter)]`**: Props en structs exportados.
*   **Paralelismo**: **`wasm-bindgen-rayon`** (usa Web Workers). Requiere `SharedArrayBuffer` -> Headers `COOP`/`COEP` en servidor HTTP.
    ```rust
    // Cargo.toml: rayon = "1.10", wasm-bindgen-rayon = "1.0"
    use rayon::prelude::*;
    
    #[wasm_bindgen]
    pub fn render_mandelbrot(width: u32, height: u32, ...) -> Vec<u8> {
        (0..height).into_par_iter() // ¡Paralelo en Wasm!
            .workers!
            .flat_map(|y| { ... }) 
            .collect()
    }
    ```

#### 3. Integración Frontend
*   **Vanilla JS/TS + Vite**: `import init, { render_mandelbrot } from './pkg'; await init(); const pixels = render_mandelbrot(...); ctx.putImageData(...)`.
*   **Leptos / Yew / Dioxus (Fullstack/WASM UI)**: Componentes Reactivos en Rust. **Leptos** (Signals, Fine-grained reactivity, SSR). **Yew** (Virtual DOM, Hooks).
*   **Panic Hook**: `console_error_panic_hook::set_once();` -> Panics en Console DevTools.

### 🛠️ Proyecto: **"Mandelbrot Explorer" (Wasm + Leptos/Vanilla)**

#### Rust Core (`pkg/mandelbrot-core`)
```rust
// lib.rs
use wasm_bindgen::prelude::*;
use rayon::prelude::*;

#[wasm_bindgen]
pub struct Params { pub x: f64, pub y: f64, pub scale: f64, pub max_iter: u32 }

#[wasm_bindgen]
pub fn compute_frame(width: u32, height: u32, params: Params) -> Vec<u8> {
    let buf_size = (width * height * 4) as usize;
    let mut pixels = vec![0u8; buf_size];
    
    // Paralelismo por filas (Rayon -> Web Workers)
    pixels.par_chunks_exact_mut(width as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                // Math Mandelbrot...
                let iter = mandelbrot_iter(x as f64, y as f64, width, height, &params);
                let color = color_map(iter, params.max_iter);
                pixel.copy_from_slice(&color);
            }
        });
    pixels
}
```

#### Frontend (Leptos Example - `app/`)
```rust
// app/src/app.rs
use leptos::*;
use mandelbrot_core::{compute_frame, Params};
use web_sys::{CanvasRenderingContext2d, ImageData};

#[component]
pub fn App() -> impl IntoView {
    let (params, set_params) = create_signal(Params::default());
    let canvas_ref = create_node_ref::<html::Canvas>();
    
    // Effect: Re-render on params change
    create_effect(move |_| {
        let p = params.get();
        if let Some(canvas) = canvas_ref.get() {
            let ctx = canvas.get_context("2d").unwrap().unwrap().dyn_into::<CanvasRenderingContext2d>().unwrap();
            let pixels = compute_frame(canvas.width(), canvas.height(), p);
            let img_data = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&pixels), canvas.width(), canvas.height()).unwrap();
            ctx.put_image_data(&img_data, 0.0, 0.0).unwrap();
        }
    });
    
    view! {
        <div class="controls">
            <input type="range" prop:value=move || params.get().scale ... />
            <button on:click=move |_| set_params.update(|p| p.scale *= 1.1)>"Zoom In"</button>
        </div>
        <canvas node_ref=canvas_ref width="800" height="600"></canvas>
    }
}
```
*   **Benchmark:** `console.time("wasm")` vs `console.time("js")` para mismo algoritmo. Wasm + Rayon (workers) suele ganar 2x-4x en CPU bound.

---

## 📅 SEMANA 16: PARSING & TEXT PROCESSING — NOM, PEST, REGEX
**Objetivo:** Parsers robustos, mantenibles y rápidos. Elegir la herramienta correcta: *Combinators (Nom)* vs *PEG (Pest)* vs *Regex*.

### 🎯 Conceptos Clave

#### 1. `nom` (Parser Combinators — Streaming, Zero-Copy)
*   **Filosofía:** Funciones `I -> IResult<I, O, E>`. Composición: `alt`, `tuple`, `many0`, `map`, `flat_map`.
*   **Streaming:** `complete` vs `streaming` (incomplete data). `nom::error::VerboseError` para debug.
*   **Zero-Copy:** Devuelve `&[u8]` / `&str` slices del input original.
```rust
use nom::{bytes::complete::{tag, take_while1}, character::complete::{digit1, space0}, combinator::map_res, sequence::delimited, IResult};

fn parse_number(input: &str) -> IResult<&str, u64> {
    map_res(delimited(space0, digit1, space0), |s: &str| s.parse::<u64>())(input)
}
// Composición: tuple((parse_number, parse_number)) -> (u64, u64)
```

#### 2. `pest` (PEG — Grammar en archivo separado)
*   **Grammar (`grammar.pest`):**
    ```pest
    number = { ASCII_DIGIT+ }
    pair = { number ~ "," ~ number }
    list = { pair ~ ("," ~ pair)* }
    WHITESPACE = _{ " " | "\t" | "\n" | "\r" } // Silent rules _
    ```
*   **Rust:** `#[derive(Parser)] #[grammar = "grammar.pest"] struct MyParser;`.
*   **Ventaja:** Gramática legible, separada del código, error reporting automático excelente. **Más lento que `nom` optimizado a mano**, pero desarrollo más rápido.

#### 3. `regex` / `aho-corasick` (Búsqueda/Extracción Simple)
*   **`regex`**: `Regex::new(r"(\d{4}-\d{2}-\d{2})")?`. `captures_iter`. **Compilación costosa** -> `lazy_static!` / `once_cell`.
*   **`aho-corasick`**: Búsqueda **múltiples patrones** simultánea (O(n + m + z)). Ideal para "buscar 10k IPs en log de 1GB". `AhoCorasick::new(patterns)`.

#### 4. `encoding_rs` (Non-UTF8)
*   `encoding_rs::WINDOWS_1252.decode(bytes)` -> `Cow<str>`. Manejo `BOM`, `replacement` chars.

#### 5. Streaming / Incremental Parsing
*   **`nom`**: `many_till`, `length_data`, `streaming` combinators. Maneja `Incomplete` -> pide más data.
*   **`pest`**: No streaming nativo fácil (requiere input completo).
*   **Line-by-line**: `BufRead::lines()` + parser por línea (JSON Lines, Nginx, Syslog). **Backpressure natural**.

### 🛠️ Mini-Proyecto: **`logparser` — CLI Streaming Parser**

#### Specs
1.  **Input:** Stdin o Files (glob support). Encoding auto-detect (`encoding_rs`).
2.  **Formatos:** `nginx` (combined), `json` (structured), `syslog` (RFC 5424/3164), `custom` (regex usuario).
3.  **Pipeline:** Parse -> Filter (query DSL simple: `status >= 400`, `path contains "/api"`) -> Aggregate (count by IP, status, latency p99) -> Output (JSON Lines, CSV, Table `tabled` crate).
4.  **Streaming:** Constante memoria O(1) (procesa línea a línea, no carga archivo).

#### Implementación Núcleo (`src/parser/nginx.rs` con `nom`)
```rust
use nom::{branch::alt, bytes::complete::{tag, take_until, take_while1}, character::complete::{digit1, char}, combinator::{map_res, opt}, sequence::{tuple, preceded, terminated}, IResult};

#[derive(Debug, Serialize)]
pub struct NginxLog {
    pub ip: String, pub time: String, pub method: String, pub path: String,
    pub status: u16, pub body_bytes: u64, pub referer: String, pub ua: String,
}

pub fn parse_nginx(input: &str) -> IResult<&str, NginxLog> {
    let (input, ip) = take_until(" ")(input)?; // IP
    let (input, _) = tag(" - ")(input)?; // ident + auth
    let (input, time) = delimited(tag("["), take_until("]"), tag("] "))(input)?;
    let (input, request) = delimited(tag("\""), take_until("\""), tag("\" "))(input)?;
    let (input, status) = map_res(digit1, |s: &str| s.parse::<u16>())(input)?;
    let (input, _) = tag(" ")(input)?;
    let (input, body_bytes) = map_res(alt((tag("-"), digit1)), |s| if s=="-" { Ok(0) } else { s.parse() })(input)?;
    let (input, _) = tag(" ")(input)?;
    let (input, referer) = delimited(tag("\""), take_until("\""), tag("\" "))(input)?;
    let (input, ua) = take_until("\n")(input)?;
    
    let (method, path) = parse_request(request).unwrap_or(("".into(), "".into()));
    
    Ok((input, NginxLog { ip: ip.into(), time: time.into(), method, path, status, body_bytes, referer: referer.into(), ua: ua.into() }))
}
```

#### Benchmarking (`benches/log_parser.rs` + `criterion`)
```rust
fn bench_nginx_parsing(c: &mut Criterion) {
    let log_line = "192.168.1.1 - - [12/Jan/2024:10:00:00 +0000] \"GET /api/users HTTP/1.1\" 200 1234 \"-\" \"curl/7.68.0\"";
    let mut parser = NginxParser::new();
    
    c.bench_function("nom_nginx_line", |b| b.iter(|| parser.parse(black_box(log_line))));
    // Comparar vs regex manual, vs pest
}
```
*   **Meta:** > 50 MB/s parsing single-thread. `nom` suele ganar a `regex`/`pest` en throughput puro.

---

## 📚 RESUMEN RECURSOS MES 4

| Semana | Lectura Oficial / Docs | Video / Blog Profundo | Práctica Clave |
| :--- | :--- | :--- | :--- |
| **13** | **Clap Derive Tutorial** (docs.rs/clap) <br> **Ratatui Tutorial** (ratatui.rs) | *Let's Get Rusty: Clap v4* | **`mytool` CLI** (Subcommands, `indicatif`, Completions, Man pages, `xtask`) |
| **14** | **The Book Cap 19** (FFI) <br> **Rust FFI Omnibus** (Michael Bryan) <br> **Rustonomicon (FFI/Unsafe)** | **Jon Gjengset: "Crust of Rust: Unsafe / FFI"** | **Wrapper `libsodium`/`sqlite`** (`bindgen`, `build.rs`, Safe API, Tests vectores oficiales) |
| **15** | **Rust Wasm Book** (rustwasm.github.io/book) <br> **Leptos Book** / **Yew Tutorial** | *Faster Web Apps with Rust & Wasm (Lin Clark)* | **Mandelbrot Explorer** (Wasm + Rayon Workers + Leptos/Vanilla JS, Canvas, Bench vs JS) |
| **16** | **Nom Tutorial** (github.com/Geal/nom) <br> **Pest Book** (pest.rs/book) <br> **Regex/aho-corasick docs** | *Parsing Logs at Scale (RustConf talks)* | **`logparser` CLI** (Nom/Pest, Streaming, Multi-format, Filter/Aggregate, Bench vs `grep`/`jq`) |

---

## ⚠️ PROBLEMAS COMUNES MES 4 (Y SOLUCIONES)

| Área | Trampa | Síntoma | Solución |
| :--- | :--- | :--- | :--- |
| **CLI** | `clap` no genera completions en `cargo install` | Usuario instala pero `mytool <TAB>` no funciona. | Usar **`xtask`** o `cargo-dist` / `cargo-generate` para empaquetar completions/man pages en release assets (GitHub Releases) o paquetes `.deb`/`.rpm`. `cargo install` solo copia binario. |
| **CLI/TUI** | `ratatui` parpadea / input bloqueado | `terminal.draw()` lento o `crossterm::event::read()` bloquea loop. | **Non-blocking event poll**: `event::poll(Duration::from_millis(16))?`. `terminal.draw()` solo si `app.state_changed` o timer. Usar `crossterm::event::EventStream` (async). |
| **FFI** | **UB Silencioso** (Alignment, Layout, Drop) | Crashes aleatorios, corrupción memoria, solo en Release. | **`#[repr(C)]` SIEMPRE**. `std::mem::align_of::<T>()`. **No `Drop` en tipos `#[repr(C)]` pasados a C** (C no llama Drop). Usa `Box::into_raw` / `from_raw` para ownership transfer. `bindgen` `--no-layout-tests` desactiva checks -> **NO LO HAGAS**. |
| **FFI** | Strings: `String` vs `CString` | "Invalid UTF-8" en Rust / Truncado en C (NUL byte medio). | **Rust->C**: `CString::new(rust_str).unwrap().into_raw()` (caller `free` con `libc::free` o wrapper `free_string`). **C->Rust**: `unsafe { CStr::from_ptr(c_ptr).to_string_lossy().into_owned() }`. |
| **Wasm** | `wasm-bindgen-rayon` no paraleliza / panica | Un solo hilo usado / "Thread spawn failed". | **Headers HTTP Obligatorios**: `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`. Servir con `vite`/`wasm-pack` plugin o config manual nginx/apache. `rayon::ThreadPoolBuilder` custom stack size. |
| **Wasm** | Tamaño `.wasm` enorme ( > 1MB ) | Carga lenta, "Application too large". | `wasm-opt -Oz` (Binaryen). `lto = true`, `opt-level = "z"` (o `"s"`), `codegen-units = 1`, `strip = true` en `Cargo.toml` `[profile.release]`. `wee_alloc`. Evitar `std` grande (`panic = "abort"`). |
| **Parsing (Nom)** | Backtracking exponencial / Stack Overflow | Parser cuelga en input malicioso/grande. | **Evita `alt` ambiguo** sin `peek`. Usa `complete` combinators (fallan rápido si input incompleto) vs `streaming`. `nom::branch::alt` ordena alternativas por especificidad. |
| **Parsing (General)** | Manejo Errores Pobre | "Parse error at byte 4096" (inútil). | **`nom::error::VerboseError`** o **`pest`** (mejores errores). Añade contexto: `context("parse nginx line", parse_nginx)`. En CLI: muestra línea + columna + snippet. |

---

## 🧩 MATERIAL COMPLEMENTARIO: Laboratorio de Código Comentado

> Las secciones **1–5 compilan y corren con `rustc 1.81` (edición 2021) usando SOLO `std`** — sin `clap`, `nom`, `bindgen` ni `wasm-bindgen`. Cubren el núcleo verificable del mes: **FFI/`unsafe` seguro** y **parsing zero-copy a mano**. Las secciones 6–7 (clap / wasm-bindgen) requieren crates externas y se muestran como referencia idiomática.

### 1️⃣ Exportar a C: `extern "C"`, `#[no_mangle]`, `#[repr(C)]`

```rust
#[repr(C)] // layout idéntico a C: campos en orden, sin reordenado del compilador
struct Punto { x: f64, y: f64 }

#[no_mangle] // el símbolo se llama exactamente "suma" (sin name-mangling de Rust)
pub extern "C" fn suma(a: i32, b: i32) -> i32 { a + b }
```

> `#[repr(C)]` es **obligatorio** en cualquier struct que cruce la frontera FFI: sin él, Rust puede reordenar campos y el layout no coincidiría con el de C → UB.

### 2️⃣ Strings entre mundos: `CString` (owned) y `CStr` (prestado)

```rust
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

fn rust_a_c_y_vuelta(s: &str) -> String {
    let c = CString::new(s).expect("sin NUL interior"); // String -> char* (NUL-terminated)
    let ptr: *const c_char = c.as_ptr();
    // unsafe encapsulado: `ptr` es válido mientras `c` siga viva.
    let de_vuelta = unsafe { CStr::from_ptr(ptr) };
    de_vuelta.to_string_lossy().into_owned()            // char* -> String
}
// rust_a_c_y_vuelta("hola FFI") == "hola FFI"
```

> Nunca pases `String::as_ptr()` a C: un `String` **no** termina en `\0`. Usa siempre `CString`. Y al recibir de C, `CStr::from_ptr` es `unsafe` porque confías en que el puntero es válido y NUL-terminado.

### 3️⃣ Transferir ownership como en C: `Box::into_raw` / `Box::from_raw`

```rust
fn ownership_transfer() -> i32 {
    let b = Box::new(99);
    let raw: *mut i32 = Box::into_raw(b);           // ownership "sale" hacia C
    // ... C guarda/usa el puntero ...
    let recuperado = unsafe { Box::from_raw(raw) }; // de vuelta a Rust
    *recuperado                                     // si NO recuperas: memory leak
}
```

> `into_raw` **renuncia** a la gestión automática; tú (o C) eres responsable de devolverlo con `from_raw` exactamente una vez. Es el mecanismo canónico para pasar objetos Rust por un puntero opaco a C.

### 4️⃣ El patrón clave del mes: wrapper **seguro** sobre `unsafe` + `Drop`

```rust
struct Buffer { ptr: *mut u8, len: usize }

impl Buffer {
    fn new(len: usize) -> Self {
        let boxed = vec![0u8; len].into_boxed_slice();
        let ptr = Box::into_raw(boxed) as *mut u8; // tomamos ownership crudo
        Buffer { ptr, len }
    }

    // API pública 100% segura: la precondición se valida ANTES del unsafe.
    fn set(&mut self, i: usize, val: u8) {
        assert!(i < self.len, "fuera de rango");
        unsafe { *self.ptr.add(i) = val; }
    }
    fn get(&self, i: usize) -> u8 {
        assert!(i < self.len, "fuera de rango");
        unsafe { *self.ptr.add(i) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // Reconstruimos el Box y dejamos que Rust libere la memoria. Sin leak.
        unsafe {
            let slice = std::ptr::slice_from_raw_parts_mut(self.ptr, self.len);
            let _ = Box::from_raw(slice);
        }
    }
}
```

> Esta es la **idea central de la Semana 14**: el `unsafe` queda *encapsulado* y la API expuesta es imposible de usar mal (bounds checked + `Drop` libera el recurso). Cada `into_raw` tiene su `from_raw` ⇒ ni doble-free ni leak.

### 5️⃣ Parser combinator a mano (el modelo de `nom`), **zero-copy**

```rust
// Igual que nom: una función toma el input y devuelve (resto, salida).
type PResult<'a, O> = Result<(&'a str, O), &'a str>;

/// Consume el prefijo `t` si está. Devuelve (resto, prefijo).
fn etiqueta<'a>(t: &str, input: &'a str) -> PResult<'a, &'a str> {
    match input.strip_prefix(t) {
        Some(resto) => Ok((resto, &input[..t.len()])),
        None => Err(input),
    }
}

/// Toma todo hasta `c` (sin incluirlo). El resultado es un SLICE del input: no copia.
fn tomar_hasta<'a>(c: char, input: &'a str) -> PResult<'a, &'a str> {
    match input.find(c) {
        Some(i) => Ok((&input[i + c.len_utf8()..], &input[..i])),
        None => Err(input),
    }
}

fn main() {
    assert_eq!(etiqueta("GET ", "GET /api"), Ok(("/api", "GET ")));
    // Extrae la IP de una línea de log sin asignar memoria nueva:
    assert_eq!(tomar_hasta(' ', "127.0.0.1 200"), Ok(("200", "127.0.0.1")));
    println!("OK");
}
```

> Los resultados (`&str`) son **vistas dentro del input original** (zero-copy), exactamente la propiedad que hace a `nom` rápido. `nom` añade combinadores (`alt`, `tuple`, `many0`, `map_res`) sobre este mismo patrón `I -> IResult<I, O>`.

### 6️⃣ `clap` Derive: parseo de argumentos declarativo *(requiere `clap`)*

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mytool", version, about = "Navaja suiza en Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, action = clap::ArgAction::Count)] // -v, -vv, -vvv
    verbose: u8,
}

#[derive(Subcommand)]
enum Commands {
    /// Hashea archivos
    Hash { files: Vec<std::path::PathBuf> },
}

// fn main() {
//     let cli = Cli::parse();   // valida, genera --help/--version, sale con error claro
//     match cli.command { Commands::Hash { files } => { /* ... */ } }
// }
```

### 7️⃣ `wasm-bindgen`: exportar Rust al navegador *(requiere `wasm-bindgen`)*

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn saludar(nombre: &str) -> String {
    format!("Hola desde Rust+Wasm, {nombre}!")
}

// JS:  import init, { saludar } from "./pkg";
//      await init(); saludar("mundo");
```

> `#[wasm_bindgen]` genera el *glue* JS/TS automáticamente. El mismo principio del § 5 (devolver slices/datos sin copiar) aplica para mover `Uint8Array`/`ImageData` entre Rust y el Canvas con el mínimo de copias.

---

## ✅ CHECKLIST FINAL MES 4 (Definition of Done)

### 1. CLI Profesional (`mytool`)
- [ ] `cargo install --path .` instala binario + **completions** (bash/zsh/fish) + **man page** (`man mytool`).
- [ ] Subcomandos: `hash` (streaming, `indicatif` multi-progress), `genpass` (entropía segura, diceware), `crypt` (age/rage).
- [ ] **Tests:** `assert_cmd` / `snapbox` para output CLI (stdout/stderr/exit code).
- [ ] **UX:** Colores (`owo-colors`), `--verbose`/`-v`/`-vv`, `--json` output para scripting, `--help` exhaustivo.
- [ ] **TUI (Bonus):** `mytool hash --tui` lanza `ratatui` dashboard interactivo.

### 2. FFI Wrapper (`safe-sodium` / `safe-sqlite`)
- [ ] `build.rs` con `bindgen` + `cc`/`pkg-config` **funciona en Linux/macOS/Windows (CI)**.
- [ ] **API 100% Safe Rust:** `SecretBox::encrypt(&[u8]) -> Result<Vec<u8>, Error>`. **Cero `unsafe` en API pública**.
- [   ] **Internals documentados:** `/// # Safety` explicando invariantes (valid ptr, initialized, alignment, lifetimes).
- [ ] **Tests:** Vectores de prueba oficiales (RFC / lib docs). Fuzzing (`cargo fuzz`) en boundary parsing.
- [ ] **No Leaks:** `Drop` impl limpia recursos C (`free`, `cleanup`, `close`). `Miri` (`cargo miri test`) **pasa sin errores**.

### 3. Wasm App (`mandelbrot-explorer`)
- [ ] **Rust Core:** `compute_frame` usando `rayon` + `wasm-bindgen-rayon` (paralelo real en Workers).
- [ ] **Frontend:** Leptos / Yew / Vanilla TS + Vite. Canvas rendering fluido (60fps zoom/pan).
- [ ] **Interop:** `JsValue` / `web_sys` para UI, `Uint8Array` / `ImageData` para pixels (zero-copy ideal).
- [ ] **Build:** `wasm-pack build --target web --release --out-dir pkg`. `wasm-opt -Oz` integrado en pipeline.
- [ ] **Deploy:** GitHub Pages / Netlify / Vercel funcionando. Headers `COOP/COEP` configurados para Workers.

### 4. Parser (`logparser`)
- [ ] **Formatos:** Nginx (Nom), JSON (serde_json streaming `Deserializer`), Syslog (Pest/Nom), Custom (Regex).
- [ ] **Streaming:** Memoria constante O(1) procesando logs de **10GB+** sin OOM.
- [ ] **Query DSL:** Filtro simple (`status:>=400 method:GET`) + Agregaciones (Top K IPs, Percentiles latencia `tdigest`/`hdrhistogram`).
- [ ] **Output:** JSON Lines, CSV, Tabla (`tabled` crate con colores).
- [ ] **Bench:** `criterion` reporta **> 50 MB/s** (Nom) / **> 100 MB/s** (Aho-Corasick multi-pattern). README con gráficos vs `grep`/`jq`/`awk`.

---

### 🚀 PRÓXIMO PASO: MES 5
> **ARQUITECTURA, PATRONES Y RENDIMIENTO (Senior Level)**
> *Design Patterns en Rust (Typestate, Actor, Builder), Concurrencia Lock-Free (Atomics, Crossbeam), Profiling (`perf`, `flamegraph`, `criterion`), Optimización real (SIMD, Allocators, Cache-friendly).*

*Ya tocas el metal, la terminal, el navegador y los logs. Ahora diseñas sistemas que escalan.* 🦀⚙️📊