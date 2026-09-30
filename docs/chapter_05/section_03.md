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

!!! quote ""

    *"Premature optimization is the root of all evil — but that doesn't mean
    you shouldn't optimize at all. It means you should optimize the right thing,
    at the right time, with data to justify it."*
    — Donald Knuth (y la segunda mitad que todos olvidan)

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

Salida real de `cargo bench -- parser/despues` del proyecto de esta semana,
ejecutado por segunda vez:

```text
parser/despues          time:   [13.411 ms 13.507 ms 13.668 ms]
                        thrpt:  [696.34 MiB/s 704.64 MiB/s 709.71 MiB/s]
                 change:
                        time:   [-10.041% -8.7968% -7.4036%] (p = 0.00 < 0.05)
                        thrpt:  [+7.9956% +9.6453% +11.162%]
                        Performance has improved.
Found 4 outliers among 100 measurements (4.00%)
  2 (2.00%) high mild
  2 (2.00%) high severe
```

- **`time`**: tiempo por iteración como intervalo de confianza del 95 %
  (límite inferior, estimación, límite superior).
- **`thrpt`**: el mismo dato como throughput, porque el grupo declaró
  `Throughput::Bytes`.
- **`change`**: comparación con la ejecución anterior guardada en
  `target/criterion/`. `p < 0.05` significa que la diferencia es estadísticamente
  significativa.
- **Outliers**: muestras anómalas detectadas y clasificadas (leves o severas).

!!! warning "Significativo no es lo mismo que real"

    Entre esa ejecución y la anterior **no cambió ni una línea de código**. El -8.8 %
    viene de la máquina: frecuencia de la CPU, temperatura, otros procesos. Por eso un
    cambio menor al ~10 % en un portátil no demuestra nada; repite la medición, fija la
    carga de la máquina y compara contra una baseline con nombre
    (`--save-baseline` / `--baseline`) medida en las mismas condiciones.

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

El informe ordena las ubicaciones por número de allocaciones. Lo que hay que buscar
es la proporción de allocaciones **temporales** (se liberan casi de inmediato) y las
líneas que allocan **una vez por elemento procesado**: un `String::from` por campo
parseado, un `clone()` de clave al insertar en un `HashMap`, un
`serde_json::to_string` por línea de salida. En el proyecto de esta semana las
medimos con un test en lugar de heaptrack, y salen exactamente esas tres.

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

// También disponible como AHashMap (wrapper con ::new(); `ahash::HashMap` es solo un
// alias de std::collections::HashMap con otro hasher y no tiene ::new()):
use ahash::AHashMap;
let mut mapa: AHashMap<String, u64> = AHashMap::new();

// ⚠️ NO uses ahash para claves controladas por atacantes externos
//    (no es seguro contra HashDoS). Usa solo en mapas internos.
```

Cuánto se gana depende del tipo de clave y del tamaño del mapa: mídelo en tu caso.
En la agregación del proyecto de esta semana, `ahash` + interning aparece medido
contra `HashMap<String, _>` con SipHash.

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
    // Ordenamos referencias, no copias: los &str del resultado apuntan a `conteos`
    // (con `conteos.to_vec()` apuntarían a un Vec local y no compilaría).
    let mut ordenados: Vec<&(String, u64)> = conteos.iter().collect();
    ordenados.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    ordenados.iter().take(5).map(|(k, _)| k.as_str()).collect() // take(5): nunca excede N
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

### 5.5 `#[inline]`, `#[cold]` y pistas de rama

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

// core::hint::likely / unlikely existen solo en nightly (#![feature(likely_unlikely)]).
// En stable, el truco equivalente: llamar a una función #[cold] en la rama rara
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
// bytemuck = "1"

use bytemuck::cast;
use wide::{f32x8, u32x8, CmpLe};

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
        // Máscara: lanes donde |z|² ≤ 4 (el punto sigue "dentro"; escapa si |z| > 2,
        // igual que la versión escalar. Con `<` los puntos con |z|² = 4 exacto
        // escaparían una iteración antes)
        // cmp_le devuelve una máscara f32x8 (todos los bits a 1 donde se cumple);
        // cast la reinterpreta como u32x8 sin convertir valores
        let dentro: u32x8 = cast((zx2 + zy2).cmp_le(cuatro));
        if dentro == u32x8::ZERO { break; }  // todos escaparon
        iters += dentro & uno;
        let nuevo_zy = f32x8::splat(2.0) * zx * zy + cy;
        zx = zx2 - zy2 + cx;
        zy = nuevo_zy;
    }

    iters.to_array()
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

Tomamos el `logparser` de la Semana 16 y aplicamos la metodología MUOV a sus tres
fases: parsear, agregar y escribir. Cada optimización **convive con su versión
"antes"** en el mismo crate: así criterion puede compararlas lado a lado y los tests
pueden comprobar que ambas dan exactamente el mismo resultado. El código completo
está en
[`logparser_v2`](https://github.com/pepemxl/Rust-Notes/tree/master/src/chapter_05/logparser_v2).

### Estructura

```text
logparser_v2/
├── Cargo.toml
├── src/
│   ├── lib.rs          ← módulos + generador de líneas de prueba
│   ├── main.rs         ← CLI: stdin → JSON Lines (con --antes usa la versión lenta)
│   ├── parser.rs       ← OPT-01: Cow / &str en lugar de String
│   ├── aggregate.rs    ← OPT-02: ahash + lasso
│   └── output.rs       ← OPT-03: BufWriter + to_writer
├── benches/
│   └── optimizaciones.rs
└── tests/
    ├── regresion.rs    ← antes y después producen lo mismo
    └── allocaciones.rs ← cuenta allocaciones con un allocator propio
```

### `Cargo.toml`

```toml
--8<-- "src/chapter_05/logparser_v2/Cargo.toml"
```

### `src/lib.rs`

Los datos de prueba importan tanto como el código: con una ruta distinta por línea,
el interning no ahorraría nada. El generador imita un log real, con pocas rutas y
pocas IPs que se repiten miles de veces.

```rust
--8<-- "src/chapter_05/logparser_v2/src/lib.rs"
```

### `src/parser.rs` — OPT-01: `Cow` y `&str` en lugar de `String`

Las dos versiones comparten el mismo parser `nom` (`campos`); solo cambia qué hacen
con lo que extrae. `parsear_linea_antes` copia cada campo a un `String`;
`parsear_linea` devuelve referencias al input y solo copia la ruta cuando hay
`%XX` que decodificar.

```rust
--8<-- "src/chapter_05/logparser_v2/src/parser.rs"
```

### `src/aggregate.rs` — OPT-02: `ahash` + `lasso`

```rust
--8<-- "src/chapter_05/logparser_v2/src/aggregate.rs"
```

### `src/output.rs` — OPT-03: `BufWriter` + `to_writer`

Ambas funciones reciben cualquier `impl Write`: en producción, `stdout`; en los tests,
un `Vec<u8>`; en el benchmark, un archivo temporal real.

```rust
--8<-- "src/chapter_05/logparser_v2/src/output.rs"
```

### `src/main.rs`

```rust
--8<-- "src/chapter_05/logparser_v2/src/main.rs"
```

### `benches/optimizaciones.rs`

Cada grupo mide **una** optimización aislada: la agregación recibe entradas ya
parseadas y la salida escribe entradas ya construidas. Si midiéramos todo el
pipeline junto, no sabríamos qué cambio aportó qué.

```rust
--8<-- "src/chapter_05/logparser_v2/benches/optimizaciones.rs"
```

---

## Tests de regresión

Una optimización que cambia el resultado es un bug. Estos tests comparan la versión
antes y después con las mismas entradas:

```rust
--8<-- "src/chapter_05/logparser_v2/tests/regresion.rs"
```

### Contar allocaciones sin heaptrack

`heaptrack` responde "¿cuántas allocaciones hace esto?", pero no corre en CI ni en
todos los sistemas. Un `#[global_allocator]` que cuenta llamadas responde lo mismo
desde un test, y además lo **congela**: si una refactorización vuelve a allocar por
línea, el test falla.

```rust
--8<-- "src/chapter_05/logparser_v2/tests/allocaciones.rs"
```

```bash
cargo test -p logparser_v2 --test allocaciones -- --nocapture
```

```text
allocaciones para 100000 líneas:
  parser      antes  300000   después   12500
  agregación  antes  200014   después      27
  output      antes  100000   después       1
```

Las cifras se explican línea a línea: el parser "antes" crea 3 `String` por línea;
el "después" solo alloca para las 12 500 rutas con `%20` (1 de cada 8). La agregación
"antes" copia IP y ruta en cada llamada (2 por línea); con `lasso`, cada string se
guarda una vez y lo que queda son las pocas allocaciones del interner y de los mapas
al crecer. En la salida, `to_string` crea un `String` por línea; `to_writer` escribe
directo al único buffer del `BufWriter`.

Este test ya se ganó el sueldo mientras escribíamos el proyecto: la primera versión
de `decodificar_url` terminaba con `String::from_utf8_lossy(&out).into_owned()`, que
copia el buffer en vez de reutilizarlo, y el test reportó 312 500 allocaciones en
lugar de las 300 000 esperadas.

### Contar syscalls con `strace`

```bash
cargo build --release -p logparser_v2
strace -f -c -e trace=write target/release/logparser_v2 --antes < access.log > /dev/null
strace -f -c -e trace=write target/release/logparser_v2         < access.log > /dev/null
```

Con un `access.log` de 100 000 líneas (10 MB):

```text
# --antes
% time     seconds  usecs/call     calls    errors syscall
------ ----------- ----------- --------- --------- ----------------
100.00    1.135667          11    100001           write

# después
% time     seconds  usecs/call     calls    errors syscall
------ ----------- ----------- --------- --------- ----------------
100.00    0.000515           7        66           write
```

`stdout` en Rust es line-buffered: sin `BufWriter`, cada `writeln!` es una syscall
(100 000, más la del resumen en stderr). Con el buffer de 256 KiB, 66.

---

## Plantilla `OPTIMIZATION_LOG.md`

Este documento es el artefacto obligatorio de la semana: una entrada por
optimización, con números **medidos**, no estimados. Así queda con los resultados de
este proyecto:

````markdown
# OPTIMIZATION_LOG — logparser v2

Máquina: AMD Ryzen 7 7735HS (8 núcleos / 16 hilos), Linux (WSL2), Rust 1.96, perfil bench.
Datos: 100 000 líneas nginx sintéticas (9.6 MiB), 250 IPs, 40 rutas.

## OPT-01: `&str` + `Cow<str>` en el parser

### Hipótesis
El parser copia IP, método y ruta a tres `String` por línea, aunque solo 1 de cada 8
rutas necesita decodificarse.

### Resultado (`cargo bench -- parser`)
parser/antes     time: [18.832 ms 18.910 ms 18.996 ms]  thrpt: 503 MiB/s
parser/despues   time: [14.688 ms 14.810 ms 14.975 ms]  thrpt: 643 MiB/s
Allocaciones: 300 000 → 12 500.

### Conclusión
1.28x. Casi 24x menos allocaciones, pero el tiempo lo domina el parseo con nom,
no el allocator: quitar allocaciones ayuda menos de lo que sugiere su número.

## OPT-02: `ahash` + interning con `lasso` en la agregación

### Resultado (`cargo bench -- agregacion`)
agregacion/antes    time: [7.6068 ms 7.6346 ms 7.6677 ms]  thrpt: 13.1 Melem/s
agregacion/despues  time: [6.1098 ms 6.1377 ms 6.1673 ms]  thrpt: 16.3 Melem/s
Allocaciones: 200 014 → 27.

### Conclusión
1.24x. Con claves cortas (IPs y rutas de ~15 bytes), hashear el texto una vez por
línea sigue siendo el costo principal: el interner también lo hashea para buscar
el Spur. La ganancia real es de memoria: cada string se guarda una sola vez.

## OPT-03: `BufWriter` + `to_writer` en la salida

### Resultado (`cargo bench -- output`, escribiendo a un archivo real)
output/antes     time: [168.11 ms 169.19 ms 170.52 ms]  thrpt: 591 Kelem/s
output/despues   time: [10.781 ms 10.884 ms 10.996 ms]  thrpt: 9.19 Melem/s
strace -c: 100 001 → 66 llamadas a write().

### Conclusión
15.5x. Una syscall cuesta microsegundos; una allocation, decenas de nanosegundos.
La optimización con más impacto fue la más simple.

## Regresión
cargo test -p logparser_v2: 7 tests OK (antes y después producen la misma salida).
````

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
- [ ] `ahash` reemplaza `SipHash` en mapas internos (no expuestos a input externo), y
  mido con criterion cuánto gana **en mi caso** (en el proyecto: 1.24x junto con el
  interning).
- [ ] `Cow<str>` en el parser devuelve `Borrowed` (cero alloc) en el caso común
  (sin %-encoding, ≥ 90% de las líneas). Solo `Owned` cuando hay transformación.
- [ ] `SmallVec<[T; N]>` en colecciones donde N cubre el caso común (top-10 rutas,
  headers HTTP). Verifico que `size_of::<SmallVec<[u8; 16]>>()` es razonable.
- [ ] El output usa `BufWriter::with_capacity(256 * 1024)` + `to_writer` en lugar
  de `to_string` por línea. El benchmark lo confirma (en el proyecto: 15.5x) y
  `strace -c` muestra la caída en llamadas a `write()`.
- [ ] `OPTIMIZATION_LOG.md` tiene una entrada por cada optimización con: baseline
  criterion, flamegraph analysis, hipótesis, implementación y resultado.
- [ ] `cargo test` pasa los 6 tests de regresión y el de allocaciones tras cada
  optimización. Ninguna optimización cambia el resultado.

!!! abstract "Siguiente sección"

    [Semana 20 — Especialización: elige tu camino](section_04.md)
