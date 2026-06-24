# Profiling y optimización: ciencia sobre intuición

La Semana 19 establece la metodología que separa una optimización de una suposición:
medir primero, entender después, optimizar en el sitio correcto y verificar que el
cambio funciona. Un programa Rust bien escrito ya es rápido; uno bien *perfilado*
es óptimo.

En esta sección aprenderemos:

- **Metodología MUOV**: Medir → Entender → Optimizar → Verificar. Sin flamegraph
  previo, cualquier optimización es ruleta.
- **`criterion`**: microbenchmarks estadísticos con `black_box`, `Throughput`,
  grupos de comparación y baseline persistente.
- **`cargo flamegraph`** + **`perf`**: localizar hot paths a nivel de función y
  línea. Identificar los "platos anchos" que dominan el tiempo de CPU.
- **`heaptrack`** / **`dhat`**: medir allocaciones, peak memory y vida de objetos.
  Una allocation por iteración en un loop caliente destruye el rendimiento.
- **`cargo bloat`** + **`cargo llvm-lines`**: tamaño de binario y monomorphization
  bloat. ¿Cuántas versiones de `Vec<T>` generaste?
- **Técnicas**: `ahash`, `SmallVec`, `Cow`, String Interning con `lasso`,
  `#[inline]`/`#[cold]`, SIMD con `wide`, output buffering.
- **Proyecto**: Log Parser v2 — tres optimizaciones documentadas con baseline y
  verificación. `OPTIMIZATION_LOG.md` como artefacto obligatorio.

> *"Premature optimization is the root of all evil — but that doesn't mean
> you shouldn't optimize at all. It means you should optimize the right thing,
> at the right time, with data to justify it."*
> — Donald Knuth (y la segunda mitad que todos olvidan)

---

## Metodología MUOV

```text
┌───────────────────────────────────────────────────────────────────────────┐
│                        CICLO DE OPTIMIZACIÓN                              │
│                                                                           │
│   1. MEDIR          2. ENTENDER         3. OPTIMIZAR    4. VERIFICAR      │
│   ─────────         ───────────         ────────────    ──────────────    │
│   criterion         flamegraph          cambio          criterion diff     │
│   baseline          heaptrack           mínimal         cargo test         │
│   (wall time,       cargo bloat         feature flag    cargo miri test    │
│    throughput,      perf stat           (reversible)    (si unsafe)        │
│    allocs)          perf annotate                                          │
│                                                                            │
│   ↑ Si el speedup es < 1.2x o los tests fallan → REVERTIR y re-entender  │
│                                                                            │
│   LEY DE AMDAHL: Si el 10% del código tarda el 90% del tiempo,           │
│   optimizar el 90% restante da < 10% de speedup total.                    │
│   Enfócate SIEMPRE en el cuello de botella real.                          │
└───────────────────────────────────────────────────────────────────────────┘
```

---

## 1. `criterion`: microbenchmarks estadísticos

`criterion` ejecuta cada benchmark múltiples veces, aplica análisis estadístico y
compara con una baseline guardada. Detecta regresiones de rendimiento al igual que
un test detecta regresiones de corrección.

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name    = "mi_bench"
harness = false   # desactiva el harness por defecto de Rust
```

### Anatomía de un benchmark bien escrito

```rust
use criterion::{
    black_box, criterion_group, criterion_main,
    BenchmarkId, Criterion, Throughput,
};

// ── Regla 1: black_box ───────────────────────────────────────────────────
// El compilador puede eliminar cómputo "sin efectos" si no usa el resultado.
// black_box() fuerza al compilador a tratarlo como observable.
fn suma_naive(v: &[u64]) -> u64 {
    v.iter().sum()
}

fn bench_suma(c: &mut Criterion) {
    let datos: Vec<u64> = (0..10_000).collect();

    // MAL: el compilador puede evaluar esto en tiempo de compilación
    // c.bench_function("suma", |b| b.iter(|| suma_naive(&datos)));

    // BIEN: black_box opacifica tanto la entrada como la salida
    c.bench_function("suma", |b| {
        b.iter(|| black_box(suma_naive(black_box(&datos))))
    });
}

// ── Regla 2: Throughput para comparar implementaciones ───────────────────
fn bench_throughput(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("parsers");
    
    for tamaño in [1_000usize, 10_000, 100_000, 1_000_000] {
        let datos = vec![b'x'; tamaño];
        
        // Throughput::Bytes: criterion calcula MB/s automáticamente
        grupo.throughput(Throughput::Bytes(tamaño as u64));
        
        grupo.bench_with_input(
            BenchmarkId::new("implementacion_a", tamaño),
            &datos,
            |b, d| b.iter(|| black_box(implementacion_a(d))),
        );
        grupo.bench_with_input(
            BenchmarkId::new("implementacion_b", tamaño),
            &datos,
            |b, d| b.iter(|| black_box(implementacion_b(d))),
        );
    }
    grupo.finish();
}

fn implementacion_a(d: &[u8]) -> usize { d.len() }
fn implementacion_b(d: &[u8]) -> usize { d.iter().filter(|&&b| b == b'x').count() }

// ── Regla 3: baseline persistente ────────────────────────────────────────
// Primera ejecución guarda baseline en target/criterion/
// Ejecuciones posteriores comparan contra ella:
//
// cargo bench
// → running bench "suma"... [baseline: 12.4 µs] [new: 10.1 µs] (-18.5%) ✅
//
// Para guardar una nueva baseline después de confirmar la mejora:
// cargo bench -- --save-baseline mi-optimizacion
//
// Para comparar contra una baseline guardada:
// cargo bench -- --baseline mi-optimizacion

// ── Regla 4: sample_size y warm_up_time ─────────────────────────────────
fn bench_configurado(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("costoso");
    grupo.sample_size(10);           // menos muestras para benchmarks lentos
    grupo.warm_up_time(std::time::Duration::from_secs(1));
    grupo.measurement_time(std::time::Duration::from_secs(5));
    // ...
    grupo.finish();
}

criterion_group!(benches, bench_suma, bench_throughput, bench_configurado);
criterion_main!(benches);
```

### Interpretar la salida de criterion

```text
running bench "parsers/implementacion_a/100000"
                        time:   [42.3 µs 42.8 µs 43.4 µs]
                              ↑      ↑      ↑
                           lower   mean   upper (intervalo de confianza 95%)
                        thrpt:  [2.1745 GiB/s 2.2001 GiB/s 2.2284 GiB/s]
                        change: [-18.4% -17.2% -16.0%] (p = 0.00 < 0.05)
                        Performance has improved.

Glosario:
- "time": tiempo por iteración (lower/mean/upper del 95% CI)
- "thrpt": throughput calculado de Throughput::Bytes(N)
- "change": comparación con baseline. p < 0.05 = estadísticamente significativo.
- "Performance has improved" / "Performance has regressed"
- Outliers: criterion los detecta y avisa si distorsionan la medida
```

---

## 2. `cargo flamegraph`: localizar hot paths

Un flamegraph muestra qué funciones consumen más tiempo de CPU. El eje X es
tiempo de muestra (ancho = proporción del tiempo total), el eje Y es la cadena
de llamadas. Los "platos anchos" en la base son los hotspots reales.

```bash
# Instalar herramientas
cargo install flamegraph
# En Linux también se necesita: sudo apt install linux-perf
# (o equivalente de la distribución)

# Compilar en release CON símbolos de debug (necesarios para el flamegraph)
# Añadir a Cargo.toml:
# [profile.release]
# debug = 1   ← símbolos (no afecta rendimiento, sí el tamaño del binario)

# Generar flamegraph de un binario
cargo flamegraph --bin logparser -- /ruta/access.log

# Generar flamegraph de un benchmark específico
cargo flamegraph --bench counter_bench -- --bench

# Con perf directamente (más control)
perf record -g --call-graph dwarf -F 997 \
    ./target/release/logparser /ruta/access.log
perf script | inferno-collapse-perf | inferno-flamegraph > flame.svg

# Ver el SVG en el navegador
xdg-open flame.svg   # Linux
open flame.svg        # macOS
```

```text
EJEMPLO DE FLAMEGRAPH (representación textual):

100% ┌─────────────────────────────────────────────────────────────────┐
     │                          main                                   │
 95% ├────────────────────────────────────────────────────────────┐    │
     │                    procesar_log_streaming                  │    │
 80% ├─────────────────────────────────────┐───────────┐          │    │
     │         parsear_linea (nom)          │ agregar() │          │    │
 60% ├───────────────────┐ ┌───────────────┤           │          │    │
     │ HashMap::insert() │ │nom::tag()     │ ahash::…  │          │    │
     │                   │ │               │           │          │    │
─────┴───────────────────┴─┴───────────────┴───────────┴──────────┴────┘

INTERPRETACIÓN:
• HashMap::insert() ocupa ~20% del tiempo total → candidato #1
• nom::tag() ocupa ~15% → candidato #2
• agregar() ocupa ~15% → candidato #3
• El resto (main, procesar_log_streaming) es solo overhead de coordinación

→ Optimizar HashMap primero (mayor impacto potencial)
→ NO tocar output o config (< 5% del tiempo)
```

---

## 3. `heaptrack` / `dhat`: profiling de allocaciones

Una allocación de heap no es gratis: llama al allocator, puede causar contención
en programas multi-thread y destruye la localidad de cache.

```bash
# Instalar heaptrack (Linux)
sudo apt install heaptrack heaptrack-gui   # Ubuntu/Debian
# o compilar desde: https://github.com/KDE/heaptrack

# Perfilar allocaciones
heaptrack ./target/release/logparser /ruta/access.log

# Ver resultados en GUI
heaptrack_gui heaptrack.logparser.*.gz

# Ver resultados en texto
heaptrack_print heaptrack.logparser.*.gz | head -50
```

```text
SALIDA TÍPICA DE heaptrack_print:

PEAK MEMORY CONSUMPTION: 128.5 MB
TOTAL ALLOCATIONS:       4,821,033
TOTAL TEMPORARY:         4,820,891 (99.9% temporales — SEÑAL DE ALERTA)

Top allocations by count:
  alloc_count  bytes    location
  2,400,000    57.6 MB  logparser::parser::nginx::parsear_linea:382
                        → String::from() para cada campo ← HOTSPOT
  1,200,000    28.8 MB  logparser::aggregate::Estadisticas::registrar:95
                        → HashMap::insert() con String::clone() ← HOTSPOT
    400,000     9.6 MB  logparser::output::imprimir_entrada:45
                        → serde_json::to_string() por línea ← HOTSPOT

DIAGNÓSTICO:
• 2.4 M allocaciones de String por línea parseada → usar Cow<str>
• 1.2 M clone de String en agregación → usar String Interning (lasso)
• 400 K serde_json::to_string → batch + write! a buffer
```

```bash
# dhat (parte de Valgrind): más lento pero más detallado
valgrind --tool=dhat ./target/release/logparser /ruta/access.log
# Abrir dhat-viewer: https://nnethercote.github.io/dh_view/dh_view.html
```

---

## 4. `cargo bloat` y `cargo llvm-lines`: code bloat

La monomorphization en Rust genera una copia del código por cada combinación de
tipos concretos. Demasiadas instancias de `HashMap<K, V>` inflamen el binario:

```bash
# Instalar
cargo install cargo-bloat cargo-llvm-lines

# Ver las funciones más grandes del binario
cargo bloat --release -n 20
# FUNCTION                              FILE SIZE  TEXT SIZE  CRATE
# logparser::parser::nginx::parsear_linea  45.2 KiB   45.2 KiB  logparser
# <Vec<T> as IntoIterator>::into_iter      22.1 KiB   22.1 KiB  core
# HashMap<K,V,S>::insert                   18.4 KiB   18.4 KiB  std
# ...

# Ver qué crates contribuyen más al tamaño
cargo bloat --release --crates
# CRATE        FILE SIZE  TEXT SIZE
# logparser      234.1 KiB  234.1 KiB
# nom            102.3 KiB  102.3 KiB
# std             87.4 KiB   87.4 KiB
# serde_json      65.2 KiB   65.2 KiB

# Ver cuántas instancias de cada función genérica se generaron
cargo llvm-lines --release | head -20
# Lines  Copies  Function name
# 18432       3  core::ptr::drop_in_place<logparser::...>
#  9216       6  alloc::vec::Vec<T,A>::push
#  4608       4  alloc::collections::hash_map::HashMap<K,V,S>::insert
```

```text
SEÑALES DE ALERTA EN cargo bloat:
• Una función genérica con muchas "Copies" → considerar type erasure (dyn Trait)
• Crate "std" > 200 KiB → muchos tipos concretos distintos (Vec<T>, HashMap<K,V>)
• Binario > 10 MB en release con strip = false → activa strip = true o LTO

[profile.release]
lto          = true      # Link-Time Optimization: elimina código muerto cross-crate
codegen-units = 1        # Un solo CGU: más optimizaciones (+ lento de compilar)
strip        = "symbols" # Elimina símbolos: binario más pequeño
opt-level    = 3         # Nivel de optimización máximo (por defecto en release)
```

---

## 5. Técnicas de optimización

### 5.1 `ahash`: HashMap más rápido

El hasher por defecto de Rust (`SipHash-1-3`) es seguro contra HashDoS pero lento.
`ahash` usa instrucciones AES-NI del hardware:

```rust
use ahash::AHasher;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

// Alias para no repetir el tipo completo
type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<AHasher>>;
type FastSet<K>    = std::collections::HashSet<K, BuildHasherDefault<AHasher>>;

fn usar_fastmap() -> FastMap<String, u64> {
    let mut mapa: FastMap<String, u64> = FastMap::default();
    mapa.insert("visitas".to_string(), 0);
    mapa
}

// También disponible con ahash::HashMap (wrapper directo):
use ahash::HashMap as AHashMap;
let mut mapa: AHashMap<String, u64> = AHashMap::new();

// ⚠️ NO uses ahash para claves controladas por atacantes externos
//    (no es seguro contra HashDoS). Usa solo en mapas internos.
```

```text
BENCHMARK: HashMap::insert() + lookup — 1M operaciones

hasher          insert (ns/op)   lookup (ns/op)   vs SipHash
─────────────────────────────────────────────────────────────
SipHash-1-3           187              142           1.0x
ahash                  73               58           2.6x
foldhash               68               54           2.8x
fxhash                 62               48           3.0x  (no DoS safe)
```

### 5.2 `SmallVec` y `ArrayVec`: evitar allocaciones para colecciones pequeñas

```rust
use smallvec::SmallVec;
use arrayvec::ArrayVec;

// SmallVec<[T; N]>: hasta N elementos en stack, luego heap
// Ideal cuando la mayoría de los casos tienen <= N elementos
fn parsear_headers(input: &str) -> SmallVec<[&str; 8]> {
    // 8 headers en stack → cero allocaciones en el caso común
    // Si hay > 8 headers → spill automático a heap
    let mut headers: SmallVec<[&str; 8]> = SmallVec::new();
    for part in input.split('\n') {
        if !part.is_empty() {
            headers.push(part);
        }
    }
    headers
}

// ArrayVec<T, N>: capacidad fija, NUNCA alloca
// Panics si push() excede N (usa try_push para Result)
fn cinco_mas_frecuentes(conteos: &[(String, u64)]) -> ArrayVec<&str, 5> {
    let mut top: ArrayVec<&str, 5> = ArrayVec::new();
    let mut ordenados = conteos.to_vec();
    ordenados.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    for (k, _) in ordenados.iter().take(5) {
        let _ = top.try_push(k.as_str()); // silencioso si ya hay 5
    }
    top
}

// ¿Cuándo usar cuál?
// SmallVec: cuando N es un caso común pero no el único
// ArrayVec: cuando N es un límite duro (protocolo, UI, API)
// Vec: cuando el tamaño es impredecible o grande
```

### 5.3 `Cow`: clonar solo cuando es necesario

```rust
use std::borrow::Cow;

// SIN Cow: siempre alloca String nueva
fn normalizar_path_costoso(path: &str) -> String {
    if path.starts_with("/api/v1/") {
        path.replace("/api/v1/", "/api/v2/")  // alloca siempre
    } else {
        path.to_string()  // alloca aunque no haya cambio
    }
}

// CON Cow: alloca solo si hay transformación
fn normalizar_path(path: &str) -> Cow<str> {
    if path.starts_with("/api/v1/") {
        Cow::Owned(path.replace("/api/v1/", "/api/v2/"))  // alloca solo aquí
    } else {
        Cow::Borrowed(path)  // cero allocaciones en el caso común (>90%)
    }
}

// En el parser de logs: campo que PODRÍA necesitar decode pero normalmente no
fn parsear_campo_url<'a>(raw: &'a str) -> Cow<'a, str> {
    if raw.contains('%') {
        // Hay percent-encoding: necesitamos decode → String nueva
        Cow::Owned(percent_decode(raw))
    } else {
        // Sin encoding: devolvemos referencia al input original (cero copia)
        Cow::Borrowed(raw)
    }
}

fn percent_decode(s: &str) -> String {
    // simplificado
    s.replace("%20", " ").replace("%2F", "/")
}
```

### 5.4 String Interning con `lasso`

Si el mismo string aparece miles de veces (rutas, IPs, métodos HTTP), internarlo
significa guardarlo una sola vez y usar un `u32` como identificador:

```rust
use lasso::{Rodeo, Spur};
use std::sync::Arc;
use parking_lot::RwLock;

// Interner de un solo hilo
fn interning_local() {
    let mut interner = Rodeo::default();

    let get   = interner.get_or_intern("GET");    // Spur (u32 interno)
    let post  = interner.get_or_intern("POST");
    let get2  = interner.get_or_intern("GET");    // misma Spur que `get`

    assert_eq!(get, get2);                         // mismo ID
    assert_ne!(get, post);

    println!("{}", interner.resolve(&get));        // "GET"
}

// ThreadedRodeo: interner multi-hilo (Arc interno)
use lasso::ThreadedRodeo;

fn interner_global() {
    let interner = Arc::new(ThreadedRodeo::default());

    let handles: Vec<_> = (0..4).map(|_| {
        let i = Arc::clone(&interner);
        std::thread::spawn(move || {
            let metodos = ["GET", "POST", "PUT", "DELETE"];
            metodos.map(|m| i.get_or_intern(m))
        })
    }).collect();

    for h in handles { h.join().unwrap(); }

    // Todos los hilos obtuvieron los mismos Spurs para los mismos strings
    let get_a = interner.get_or_intern("GET");
    let get_b = interner.get_or_intern("GET");
    assert_eq!(get_a, get_b);
}

// Uso en agregación: FastMap<Spur, u64> vs HashMap<String, u64>
// Spur es u32 (4 bytes, Copy, Hash O(1)) vs String (24+ bytes, Clone, Hash O(n))
use ahash::AHashMap;
type ConteoRutas = AHashMap<Spur, u64>;
```

### 5.5 `#[inline]`, `#[cold]` y `#[likely]`

```rust
// #[inline]: sugiere al compilador que inline la función
// Úsalo en funciones pequeñas y calientes llamadas desde genéricos
#[inline]
pub fn es_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

// #[inline(always)]: fuerza el inline (ignora heurísticas)
// Solo cuando el benchmark demuestra que inline marca diferencia
#[inline(always)]
fn avanzar_hasta_espacio(input: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < input.len() && !es_whitespace(input[i]) { i += 1; }
    i
}

// #[cold]: el path de error raramente se ejecuta; el compilador lo mueve
// a una sección fría del binario (mejora la localidad de cache del hot path)
#[cold]
#[inline(never)]
fn reportar_error_parse(linea: &str, pos: usize) -> String {
    format!("error al parsear en columna {pos}: {:?}", &linea[..pos.min(40)])
}

// #[cold] en variantes de error de enums:
pub enum ResultadoParse<T> {
    Ok(T),
    #[cold]        // esta variante raramente ocurre en producción
    Error(String),
}

// core::hint::likely / unlikely (nightly):
// En stable, usa ramas con #[cold] en el camino unlikely como workaround
fn procesar_byte(b: u8) -> u8 {
    if b == 0 {
        // Caso raro: marcar la función de manejo como #[cold]
        manejar_nul(b)
    } else {
        b + 1  // caso común: sin overhead de hint
    }
}

#[cold]
fn manejar_nul(_: u8) -> u8 { 0 }
```

### 5.6 SIMD con `wide`: procesar N elementos a la vez

```rust
// [dependencies]
// wide = "0.7"

use wide::{f32x8, u32x8, CmpLt};

/// Calcular iteraciones de Mandelbrot para 8 píxeles simultáneamente.
/// En lugar de el bucle escalar de la Semana 15, procesamos 8 puntos
/// complejos en paralelo con instrucciones AVX2 (256-bit SIMD).
pub fn mandelbrot_simd_x8(
    cx: f32x8,   // 8 coordenadas X del plano complejo
    cy: f32x8,   // 8 coordenadas Y del plano complejo
    max_iter: u32,
) -> [u32; 8] {
    let mut zx    = f32x8::ZERO;
    let mut zy    = f32x8::ZERO;
    let mut iters = u32x8::ZERO;
    let cuatro    = f32x8::splat(4.0);
    let uno       = u32x8::splat(1);

    for _ in 0..max_iter {
        let zx2 = zx * zx;
        let zy2 = zy * zy;
        // Máscara: lanes donde |z|² < 4 (el punto sigue "dentro")
        let dentro: u32x8 = (zx2 + zy2).cmp_lt(cuatro).into();
        if dentro == u32x8::ZERO { break; }  // todos escaparon
        iters += dentro & uno;
        let nuevo_zy = f32x8::splat(2.0) * zx * zy + cy;
        zx = zx2 - zy2 + cx;
        zy = nuevo_zy;
    }

    iters.into()
}

/// Versión escalar equivalente (para comparar en benchmark)
pub fn mandelbrot_escalar(cx: f32, cy: f32, max_iter: u32) -> u32 {
    let (mut zx, mut zy) = (0.0f32, 0.0);
    for i in 0..max_iter {
        let (zx2, zy2) = (zx * zx, zy * zy);
        if zx2 + zy2 > 4.0 { return i; }
        let nuevo_zy = 2.0 * zx * zy + cy;
        zx = zx2 - zy2 + cx;
        zy = nuevo_zy;
    }
    max_iter
}

// Speedup típico: 4x-7x en x86-64 con AVX2
// cargo rustc --release -- -C target-cpu=native   ← activa AVX2 si disponible
```

### 5.7 Output buffering: evitar syscalls por línea

```rust
use std::io::{self, BufWriter, Write};

// MAL: una syscall write() por línea (costoso con millones de líneas)
fn escribir_mal(lineas: &[String]) -> io::Result<()> {
    let stdout = io::stdout();
    for linea in lineas {
        writeln!(stdout.lock(), "{linea}")?;  // lock + write + unlock por iteración
    }
    Ok(())
}

// BIEN: buffer en memoria, una sola syscall al final (o cada N bytes)
fn escribir_bien(lineas: &[String]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(256 * 1024, stdout.lock());
    for linea in lineas {
        writeln!(out, "{linea}")?;  // escribe al buffer en memoria
    }
    out.flush()?;  // una sola syscall (o pocas si el buffer se llenó)
    Ok(())
}

// Para JSON Lines: evitar serde_json::to_string() (alloca String temporal)
// Usa serde_json::to_writer() directamente al buffer
use serde::Serialize;

fn escribir_json_lines<T: Serialize>(items: &[T]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = BufWriter::with_capacity(256 * 1024, stdout.lock());
    for item in items {
        serde_json::to_writer(&mut out, item)?;  // sin String temporal
        out.write_all(b"\n")?;
    }
    out.flush()
}
```

---

## Proyecto: Log Parser v2 — tres optimizaciones documentadas

Tomamos el `logparser` de la Semana 16 y aplicamos la metodología MUOV
para obtener speedup medible y documentado.

### Estructura

```
logparser_v2/
├── Cargo.toml
├── src/
│   ├── lib.rs          ← implementaciones base y optimizadas
│   ├── parser.rs       ← parser nginx (Cow en campos)
│   ├── aggregate.rs    ← ahash + lasso
│   └── output.rs       ← BufWriter
└── benches/
    └── optimizaciones.rs
```

### `Cargo.toml`

```toml
[package]
name    = "logparser-v2"
version = "0.1.0"
edition = "2021"

[dependencies]
nom        = "7"
ahash      = "0.8"
lasso      = { version = "0.6", features = ["multi-threaded"] }
smallvec   = { version = "1", features = ["union"] }
serde      = { version = "1", features = ["derive"] }
serde_json = "1"
wide       = "0.7"

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name    = "optimizaciones"
harness = false

[profile.release]
debug         = 1   # conservar símbolos para flamegraph
lto           = true
codegen-units = 1
```

### `src/parser.rs` — optimización 1: Cow en campos frecuentes

```rust
use nom::{
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{char, digit1, space1},
    combinator::{map_res, opt},
    sequence::{delimited, terminated},
    IResult,
};
use std::borrow::Cow;

// ANTES (Semana 16): todos los campos son String (always allocate)
#[derive(Debug)]
pub struct EntradaAntes {
    pub ip:      String,      // alloca siempre
    pub metodo:  String,      // alloca siempre
    pub ruta:    String,      // alloca siempre
    pub estado:  u16,
    pub bytes:   Option<u64>,
}

// DESPUÉS: campos de alta frecuencia como &str (zero-copy del input)
// Solo alloamos si el campo necesita transformación (raro en logs nginx)
#[derive(Debug)]
pub struct EntradaDespues<'a> {
    pub ip:      &'a str,     // referencia al input: cero alloc
    pub metodo:  &'a str,     // cero alloc
    pub ruta:    Cow<'a, str>,// cero alloc si sin %-encoding; alloca si lo hay
    pub estado:  u16,
    pub bytes:   Option<u64>,
}

fn campo_ip(i: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_digit() || c == '.' || c == ':')(i)
}

fn campo_metodo(i: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_ascii_uppercase())(i)
}

fn campo_ruta(i: &str) -> IResult<&str, Cow<str>> {
    let (i, ruta) = take_while1(|c: char| c != ' ')(i)?;
    // Solo decodificamos si hay caracteres codificados
    if ruta.contains('%') {
        Ok((i, Cow::Owned(decodificar_url(ruta))))
    } else {
        Ok((i, Cow::Borrowed(ruta)))
    }
}

fn decodificar_url(s: &str) -> String {
    // Simplificado: en producción usar percent-encoding crate
    s.replace("%20", " ").replace("%2F", "/").replace("%3F", "?")
}

pub fn parsear_linea_optimizado(input: &str) -> IResult<&str, EntradaDespues> {
    let (i, ip)     = terminated(campo_ip, space1)(input)?;
    let (i, _)      = terminated(take_while1(|c: char| c != ' '), space1)(i)?;
    let (i, _)      = terminated(take_while1(|c: char| c != ' '), space1)(i)?;
    let (i, _)      = terminated(delimited(char('['), take_until("]"), char(']')), space1)(i)?;
    let (i, _)      = char('"')(i)?;
    let (i, metodo) = terminated(campo_metodo, char(' '))(i)?;
    let (i, ruta)   = terminated(campo_ruta, char(' '))(i)?;
    let (i, _)      = take_until("\"")(i)?;
    let (i, _)      = terminated(char('"'), space1)(i)?;
    let (i, estado) = terminated(map_res(digit1, |s: &str| s.parse::<u16>()), space1)(i)?;
    let (i, bytes)  = opt(map_res(digit1, |s: &str| s.parse::<u64>()))(i)?;

    Ok((i, EntradaDespues { ip, metodo, ruta, estado, bytes }))
}

// ANTES: parsear_linea devuelve EntradaAntes con 3 String allocations/línea
// DESPUÉS: parsear_linea_optimizado devuelve EntradaDespues con ~0 allocs/línea
// (solo aloca si la ruta tiene %-encoding, caso < 5% en logs típicos)
```

### `src/aggregate.rs` — optimización 2: ahash + lasso

```rust
use ahash::AHashMap;
use lasso::{Rodeo, Spur};
use smallvec::SmallVec;

// ANTES: HashMap<String, u64> con SipHash
pub struct EstadisticasAntes {
    pub por_ruta:   std::collections::HashMap<String, u64>,
    pub por_ip:     std::collections::HashMap<String, u64>,
    pub por_estado: std::collections::HashMap<u16, u64>,
}

// DESPUÉS: AHashMap<Spur, u64> con lasso para dedup de strings
pub struct EstadisticasDespues {
    // Spurs son u32: 4 bytes en vez de 24+ de String, hash O(1) en vez de O(n)
    pub por_ruta:      AHashMap<Spur, u64>,
    pub por_ip:        AHashMap<Spur, u64>,
    pub por_estado:    AHashMap<u16, u64>,
    pub interner:      Rodeo,          // dueño de los strings únicos
    // SmallVec para listas de top-N: cero alloc si <= 10 elementos
    pub top_rutas:     SmallVec<[(Spur, u64); 10]>,
}

impl EstadisticasDespues {
    pub fn nueva() -> Self {
        EstadisticasDespues {
            por_ruta:   AHashMap::new(),
            por_ip:     AHashMap::new(),
            por_estado: AHashMap::new(),
            interner:   Rodeo::default(),
            top_rutas:  SmallVec::new(),
        }
    }

    pub fn registrar(&mut self, ip: &str, ruta: &str, estado: u16) {
        // intern() devuelve el Spur existente o crea uno nuevo
        let spur_ip   = self.interner.get_or_intern(ip);
        let spur_ruta = self.interner.get_or_intern(ruta);

        *self.por_ip.entry(spur_ip).or_default()        += 1;
        *self.por_ruta.entry(spur_ruta).or_default()    += 1;
        *self.por_estado.entry(estado).or_default()     += 1;
    }

    pub fn top_rutas(&self, n: usize) -> Vec<(&str, u64)> {
        let mut v: Vec<_> = self.por_ruta.iter()
            .map(|(&spur, &cnt)| (self.interner.resolve(&spur), cnt))
            .collect();
        v.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        v.truncate(n);
        v
    }
}

// MEJORA ESPERADA:
// - ahash vs SipHash: 2.5x más rápido en lookup/insert
// - Spur (4B) vs String (24+B clone): -80% allocaciones en agregación
// - SmallVec top_rutas: cero alloc para logs con <= 10 rutas distintas
```

### `src/output.rs` — optimización 3: output buffering

```rust
use serde::Serialize;
use serde_json;
use std::io::{self, BufWriter, Write};

// ANTES: una syscall por línea
pub fn escribir_json_antes<T: Serialize>(items: &[T]) -> io::Result<()> {
    for item in items {
        // to_string alloca String temporal + println! hace lock/write/unlock
        println!("{}", serde_json::to_string(item).unwrap());
    }
    Ok(())
}

// DESPUÉS: buffer de 256 KB, pocas syscalls
pub fn escribir_json_despues<T: Serialize>(items: &[T]) -> io::Result<()> {
    let stdout = io::stdout();
    // BufWriter: acumula writes en buffer, syscall cada 256 KB (no por línea)
    let mut out = BufWriter::with_capacity(256 * 1024, stdout.lock());
    for item in items {
        // to_writer: sin String temporal, escribe directo al BufWriter
        serde_json::to_writer(&mut out, item)?;
        out.write_all(b"\n")?;
    }
    out.flush()  // asegurar que el buffer se vacíe al final
}

// ANTES (1M líneas): ~1M syscalls write() ← muy costoso
// DESPUÉS (1M líneas): ~16 syscalls write() ← insignificante

// Para CSV: write! directo al buffer, sin serde overhead
pub fn escribir_csv<F>(
    estadisticas: &[(String, u64)],
    mut out: impl Write,
) -> io::Result<()> {
    out.write_all(b"ruta,peticiones\n")?;
    for (ruta, cnt) in estadisticas {
        write!(out, "{ruta},{cnt}\n")?;
    }
    Ok(())
}
```

### `benches/optimizaciones.rs` — comparar antes vs después

```rust
use criterion::{
    black_box, criterion_group, criterion_main,
    BenchmarkId, Criterion, Throughput,
};
use logparser_v2::{aggregate::*, output::*, parser::*};
use std::io::sink;

// ── Generar datos de prueba ───────────────────────────────────────────────

fn generar_lineas(n: usize) -> Vec<String> {
    (0..n).map(|i| format!(
        r#"10.0.0.{ip} - - [01/Jan/2024:00:00:{ss:02} +0000] "GET /api/v1/items/{i} HTTP/1.1" {status} {bytes} "-" "curl/7.68""#,
        ip     = i % 255,
        ss     = i % 60,
        status = if i % 10 == 0 { 500 } else { 200 },
        bytes  = 100 + (i % 900),
        i      = i,
    )).collect()
}

// ── Benchmark 1: parser (Cow vs String) ──────────────────────────────────

fn bench_parser(c: &mut Criterion) {
    let lineas = generar_lineas(10_000);
    let bytes_total: u64 = lineas.iter().map(|l| l.len() as u64).sum();

    let mut grupo = c.benchmark_group("parser");
    grupo.throughput(Throughput::Bytes(bytes_total));

    grupo.bench_function("antes/String-alloc", |b| {
        b.iter(|| {
            lineas.iter().filter_map(|l| {
                // simula el parser original que crea Strings
                let parts: Vec<&str> = l.splitn(10, ' ').collect();
                if parts.len() < 7 { return None; }
                Some(black_box((
                    parts[0].to_string(),  // ip: alloca
                    parts[5].to_string(),  // metodo: alloca
                    parts[6].to_string(),  // ruta: alloca
                )))
            }).count()
        })
    });

    grupo.bench_function("despues/Cow-zero-copy", |b| {
        b.iter(|| {
            lineas.iter().filter_map(|l| {
                parsear_linea_optimizado(black_box(l)).ok()
                    .map(|(_, e)| black_box((e.ip, e.metodo, e.ruta)))
            }).count()
        })
    });

    grupo.finish();
}

// ── Benchmark 2: agregación (SipHash/String vs ahash/Spur) ───────────────

fn bench_agregacion(c: &mut Criterion) {
    let lineas = generar_lineas(100_000);
    let n = lineas.len() as u64;

    let mut grupo = c.benchmark_group("agregacion");
    grupo.throughput(Throughput::Elements(n));

    grupo.bench_function("antes/SipHash-String", |b| {
        b.iter(|| {
            let mut stats = EstadisticasAntes {
                por_ruta:   std::collections::HashMap::new(),
                por_ip:     std::collections::HashMap::new(),
                por_estado: std::collections::HashMap::new(),
            };
            for linea in &lineas {
                let parts: Vec<&str> = linea.splitn(10, ' ').collect();
                if parts.len() >= 8 {
                    let ip    = parts[0].to_string();
                    let ruta  = parts[6].to_string();
                    let estado: u16 = parts[8].parse().unwrap_or(0);
                    *stats.por_ip.entry(ip).or_default()       += 1;
                    *stats.por_ruta.entry(ruta).or_default()   += 1;
                    *stats.por_estado.entry(estado).or_default()+= 1;
                }
            }
            black_box(stats.por_ruta.len())
        })
    });

    grupo.bench_function("despues/ahash-Spur", |b| {
        b.iter(|| {
            let mut stats = EstadisticasDespues::nueva();
            for linea in &lineas {
                if let Ok((_, e)) = parsear_linea_optimizado(black_box(linea)) {
                    stats.registrar(e.ip, e.ruta.as_ref(), e.estado);
                }
            }
            black_box(stats.por_ruta.len())
        })
    });

    grupo.finish();
}

// ── Benchmark 3: output (println! vs BufWriter) ──────────────────────────

fn bench_output(c: &mut Criterion) {
    let n = 100_000usize;
    let lineas = generar_lineas(n);

    // Parseamos una vez, comparamos solo el output
    let entradas: Vec<_> = lineas.iter()
        .filter_map(|l| parsear_linea_optimizado(l).ok().map(|(_, e)| {
            serde_json::json!({ "ip": e.ip, "estado": e.estado })
        }))
        .collect();

    let mut grupo = c.benchmark_group("output");
    grupo.throughput(Throughput::Elements(n as u64));

    grupo.bench_function("antes/to_string-por-linea", |b| {
        b.iter(|| {
            // Simular el overhead de to_string por línea (sin la syscall real)
            let mut total_bytes = 0usize;
            for e in &entradas {
                let s = black_box(serde_json::to_string(e).unwrap());
                total_bytes += s.len();
            }
            black_box(total_bytes)
        })
    });

    grupo.bench_function("despues/to_writer-buffered", |b| {
        b.iter(|| {
            // Simular to_writer a sink (sin syscall)
            let mut out = std::io::BufWriter::with_capacity(256 * 1024, sink());
            for e in &entradas {
                let _ = serde_json::to_writer(black_box(&mut out), e);
                let _ = out.write_all(b"\n");
            }
            black_box(out.flush())
        })
    });

    grupo.finish();
}

criterion_group!(benches, bench_parser, bench_agregacion, bench_output);
criterion_main!(benches);
```

---

## Plantilla `OPTIMIZATION_LOG.md`

Este documento es el artefacto obligatorio de la semana. Cada optimización
requiere una entrada:

```markdown
# OPTIMIZATION_LOG — logparser v2

## Metodología
Ciclo: Baseline → Flamegraph → Hipótesis → Implementación → Criterion → Regresión

---

## OPT-01: ahash + lasso en agregación

### Baseline (cargo bench -- agregacion/antes)
```
time: [14.2 ms 14.5 ms 14.8 ms]
thrpt: [6.3 Mops/s 6.5 Mops/s 6.7 Mops/s]
```

### Flamegraph antes
- `HashMap::insert`: 28% del tiempo total (identificado con cargo flamegraph)
- `siphasher::sip128::hash`: 11% del tiempo total

### Hipótesis
SipHash-1-3 calcula un hash criptográficamente seguro para cada key.
Para un mapa interno sin exposición a input externo, ahash (AES-NI) es
suficiente y 2.5x más rápido en x86-64.

### Implementación
- `HashMap<String, u64>` → `AHashMap<Spur, u64>`
- `String::clone()` en insert → `Rodeo::get_or_intern()` devuelve Spur (u32)
- Diff: 47 líneas modificadas, 0 unsafe, feature flags: ninguno

### Resultado (cargo bench -- agregacion/despues)
```
time: [5.8 ms 5.9 ms 6.1 ms]
thrpt: [15.4 Mops/s 16.0 Mops/s 16.5 Mops/s]
change: [-59.7% -59.0% -58.4%] (p = 0.00 < 0.05) ← 2.5x speedup
```

### Regresión
- cargo test --all: ✅ 0 failures
- cargo miri test: ✅ (no unsafe en este cambio)
- heaptrack antes: 1.2M allocs en agregación
- heaptrack después: 0.1M allocs en agregación (-92%)

---

## OPT-02: Cow<str> en parser (zero-copy)

### Baseline
- heaptrack: 2.4M String allocs/run (una por campo ip/método/ruta)

### Hipótesis
El 95% de las rutas en los logs no tienen %-encoding. Devolver &str
directamente elimina la allocación en ese caso. Cow<str> cubre el 5%
que sí necesita decodificación.

### Resultado
- Parser throughput: 1.8 → 3.1 GB/s (+72%)
- Allocaciones: -85% (2.4M → 0.36M)

---

## OPT-03: BufWriter en output

### Baseline
- strace revela: 1,000,000 llamadas write() por 1M líneas de log

### Resultado
- BufWriter(256KB): ~60 llamadas write() por 1M líneas
- Output throughput: 120 MB/s → 890 MB/s (+7x)
- to_writer vs to_string: -45% tiempo de serialización (sin String temporal)
```

---

## Resumen de resultados esperados

```text
BENCHMARK FINAL: logparser v2 vs v1 (100K líneas nginx, 1 hilo)

Fase          │ v1 (Semana 16)    │ v2 (Semana 19)    │ Speedup
──────────────┼───────────────────┼───────────────────┼────────
Parsing       │ 380 MB/s          │ 980 MB/s          │ 2.6x
Agregación    │  6.5 Mops/s       │ 16.0 Mops/s       │ 2.5x
Output JSON   │ 120 MB/s          │ 890 MB/s          │ 7.4x
Allocaciones  │  4.8 M/run        │  0.4 M/run        │ -92%
Peak memory   │ 128.5 MB          │ 18.2 MB           │ -86%

Pipeline completo (parse + aggregate + output):
  v1: 14.8 ms / 100K líneas  →  ~6.8 M líneas/s
  v2:  5.1 ms / 100K líneas  →  ~19.6 M líneas/s  (2.9x end-to-end)
```

---

## Tests de regresión

```rust
// tests/regresion.rs

#[test]
fn parser_cow_equivalente_a_string() {
    let linea = r#"192.168.1.1 - - [01/Jan/2024:00:00:00 +0000] "GET /api/items HTTP/1.1" 200 1024 "-" "curl/7""#;

    let (_, opt) = logparser_v2::parser::parsear_linea_optimizado(linea).unwrap();
    assert_eq!(opt.ip,     "192.168.1.1");
    assert_eq!(opt.metodo, "GET");
    assert_eq!(opt.ruta.as_ref(), "/api/items");
    assert_eq!(opt.estado, 200);
    assert_eq!(opt.bytes,  Some(1024));
}

#[test]
fn cow_borrowed_en_ruta_sin_encoding() {
    use std::borrow::Cow;
    let linea = r#"1.2.3.4 - - [01/Jan/2024:00:00:00 +0000] "GET /sin/encoding HTTP/1.1" 200 0 "-" "-""#;
    let (_, opt) = logparser_v2::parser::parsear_linea_optimizado(linea).unwrap();
    // Sin %-encoding → Borrowed (cero alloc)
    assert!(matches!(opt.ruta, Cow::Borrowed(_)));
}

#[test]
fn cow_owned_con_percent_encoding() {
    use std::borrow::Cow;
    let linea = r#"1.2.3.4 - - [01/Jan/2024:00:00:00 +0000] "GET /con%20encoding HTTP/1.1" 200 0 "-" "-""#;
    let (_, opt) = logparser_v2::parser::parsear_linea_optimizado(linea).unwrap();
    // Con %-encoding → Owned (decodificado)
    assert!(matches!(opt.ruta, Cow::Owned(_)));
    assert_eq!(opt.ruta.as_ref(), "/con encoding");
}

#[test]
fn estadisticas_correctas() {
    let mut stats = logparser_v2::aggregate::EstadisticasDespues::nueva();
    stats.registrar("1.1.1.1", "/api", 200);
    stats.registrar("1.1.1.1", "/api", 404);
    stats.registrar("2.2.2.2", "/home", 200);

    let top = stats.top_rutas(5);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].0, "/api");    // más frecuente
    assert_eq!(top[0].1, 2);
    assert_eq!(top[1].0, "/home");
    assert_eq!(top[1].1, 1);
}

#[test]
fn output_json_produce_lineas_validas() {
    use serde_json::Value;
    let items = vec![
        serde_json::json!({"ip": "1.1.1.1", "estado": 200}),
        serde_json::json!({"ip": "2.2.2.2", "estado": 404}),
    ];
    let mut buf: Vec<u8> = Vec::new();
    logparser_v2::output::escribir_json_despues(&items[..], &mut buf).unwrap();

    let contenido = String::from_utf8(buf).unwrap();
    let lineas: Vec<&str> = contenido.trim().split('\n').collect();
    assert_eq!(lineas.len(), 2);

    let parsed: Value = serde_json::from_str(lineas[0]).unwrap();
    assert_eq!(parsed["ip"], "1.1.1.1");
}
```

---

## ✅ Checklist de la Semana 19

- [ ] Sigo la metodología MUOV en orden: **primero mido** con `cargo bench --
  --save-baseline antes`, luego flamegraph, luego optimizo, luego comparo con
  `cargo bench -- --baseline antes`.
- [ ] `black_box()` envuelve tanto el input como el output de cada benchmark.
  Sin él, el compilador puede eliminar el código que se mide.
- [ ] `cargo flamegraph` identifica los 3 hotspots reales antes de tocar código.
  No optimizo funciones que aparecen por debajo del 5% en el flamegraph.
- [ ] `heaptrack` muestra las allocaciones por ubicación. Reduzco las allocaciones
  de líneas calientes antes de perfilar CPU (una alloc por iteración = cache miss).
- [ ] `cargo bloat --release --crates` me dice qué crates contribuyen más al
  tamaño. Activo `lto = true` y `codegen-units = 1` en `[profile.release]`.
- [ ] `ahash` reemplaza `SipHash` en mapas internos (no expuestos a input externo).
  Verifico con criterion que el speedup es ≥ 2x en el benchmark de agregación.
- [ ] `Cow<str>` en el parser devuelve `Borrowed` (cero alloc) en el caso común
  (sin %-encoding, ≥ 90% de las líneas). Solo `Owned` cuando hay transformación.
- [ ] `SmallVec<[T; N]>` en colecciones donde N cubre el caso común (top-10 rutas,
  headers HTTP). Verifico que `size_of::<SmallVec<[u8; 16]>>()` es razonable.
- [ ] El output usa `BufWriter::with_capacity(256 * 1024)` + `to_writer` en lugar
  de `to_string` por línea. El benchmark muestra ≥ 5x mejora en output throughput.
- [ ] `OPTIMIZATION_LOG.md` tiene una entrada por cada optimización con: baseline
  criterion, flamegraph analysis, hipótesis, implementación y resultado.
- [ ] `cargo test --all-features` pasa los 5 tests de regresión tras cada
  optimización. Ninguna optimización rompe la corrección.

> **Siguiente sección:** [Semana 20 — Especialización: elige tu camino](section_04.md)
