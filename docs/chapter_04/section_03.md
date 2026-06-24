# WebAssembly: Rust en el navegador

La Semana 15 lleva Rust al navegador sin JavaScript como intermediario lógico.
WebAssembly (Wasm) es un formato binario portable que todos los navegadores modernos
ejecutan casi a velocidad nativa. Rust es el lenguaje con mejor soporte para Wasm:
compila a tamaños pequeños, no tiene garbage collector que pause la ejecución, y la
cadena de herramientas está integrada en el ecosistema de Cargo.

En esta sección aprenderemos:

- La arquitectura de WebAssembly: memoria lineal, imports/exports, el modelo de tipos.
- La cadena de herramientas: `wasm-pack`, `wasm-bindgen`, `wasm-opt`.
- El atributo `#[wasm_bindgen]` en funciones, structs e impl blocks.
- Tipos en la frontera JS ↔ Rust: primitivos, `JsValue`, `js_sys`, `web_sys`.
- Cómo llamar APIs del navegador desde Rust: `Canvas`, `console`, `fetch`.
- Optimización de tamaño del `.wasm`.
- Paralelismo real en el navegador con `rayon` + `wasm-bindgen-rayon`.
- Integración con Vite (bundler moderno para vanilla JS/TS).
- El proyecto completo: **Mandelbrot Explorer**.

> 💡 **Filosofía de la Semana 15:** *Wasm no reemplaza JavaScript — lo extiende. Usas
> Rust para las partes CPU-intensivas (computación, cifrado, compresión, rendering) y
> dejas a JS la gestión del DOM y los eventos. La frontera entre ambos mundos es donde
> está el diseño.*

---

## La arquitectura de WebAssembly

```text
MODELO DE EJECUCIÓN DE WASM EN EL NAVEGADOR

JavaScript Engine (V8/SpiderMonkey/JavaScriptCore)
┌─────────────────────────────────────────────────────────────┐
│                                                              │
│  JavaScript                    WebAssembly Module           │
│  ┌──────────────────┐          ┌──────────────────────────┐ │
│  │ const result =   │  calls   │  fn calcular(x: f64, y:  │ │
│  │   calcular(x, y) │ ───────► │    f64) -> f64 { ... }   │ │
│  │                  │          │                           │ │
│  │ canvas.putImageData│◄─────── │  fn render() → Uint8Array │ │
│  └──────────────────┘  returns └──────────────────────────┘ │
│                                                              │
│  Memoria compartida:  ArrayBuffer (lineal, contigua)        │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  0x0000  │  stack Wasm  │  heap Wasm  │  ... libre   │   │
│  └──────────────────────────────────────────────────────┘   │
│  JS puede leer/escribir con Uint8Array, Float64Array...     │
└─────────────────────────────────────────────────────────────┘
```

Wasm tiene cuatro tipos de valores: `i32`, `i64`, `f32`, `f64`. Para pasar cualquier
otra cosa (strings, arrays, structs) se usan **punteros a la memoria lineal**.
`wasm-bindgen` automatiza esta serialización.

### Qué es `wasm-bindgen`

`wasm-bindgen` es un generador de código que lee las anotaciones `#[wasm_bindgen]` en
tu código Rust y genera:

1. El módulo `.wasm` con las funciones exportadas.
2. Un archivo `.js` de *glue* que serializa/deserializa los tipos al cruzar la frontera.
3. Un archivo `.d.ts` con tipos TypeScript.

```text
Tu código Rust                 wasm-pack build
──────────────────             ───────────────────────────────────
#[wasm_bindgen]        →       pkg/
pub fn calcular(n: u32) → u32    ├── mandelbrot_wasm_bg.wasm   (binario Wasm)
                                  ├── mandelbrot_wasm.js        (glue JS)
                                  ├── mandelbrot_wasm_bg.js     (interno)
                                  └── mandelbrot_wasm.d.ts      (tipos TS)
```

---

## Instalación de la cadena de herramientas

```bash
# Instalar wasm-pack (gestor de todo el ciclo: build, test, publish)
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
# o con cargo:
cargo install wasm-pack

# Añadir el target de compilación Wasm
rustup target add wasm32-unknown-unknown

# Verificar
wasm-pack --version    # wasm-pack 0.13.x
```

---

## Primer módulo Wasm: hola mundo

```bash
cargo new mandelbrot-wasm --lib
cd mandelbrot-wasm
```

`Cargo.toml`:

```toml
[package]
name    = "mandelbrot-wasm"
version = "0.1.0"
edition = "2021"

[lib]
# cdylib: biblioteca dinámica para C (lo que Wasm requiere)
# rlib: biblioteca Rust (para tests con cargo test)
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"
console_error_panic_hook = "0.1"

[dev-dependencies]
wasm-bindgen-test = "0.3"

[profile.release]
# Optimizaciones para tamaño mínimo de .wasm
opt-level   = "z"       # "z" = tamaño, "s" = balance, 3 = velocidad
lto         = true
codegen-units = 1
panic       = "abort"   # sin unwinding tables → binario más pequeño
```

`src/lib.rs` inicial:

```rust
use wasm_bindgen::prelude::*;

// Mejor manejo de panics: en vez de "RuntimeError: unreachable" en la consola,
// muestra el mensaje de panic completo y el backtrace
#[wasm_bindgen(start)]   // se ejecuta automáticamente al cargar el módulo
pub fn inicializar() {
    console_error_panic_hook::set_once();
}

// Una función simple exportada a JavaScript
#[wasm_bindgen]
pub fn saludar(nombre: &str) -> String {
    format!("¡Hola desde Rust+Wasm, {nombre}!")
}

// Funciones matemáticas puras: ideal para Wasm (sin DOM, sin IO)
#[wasm_bindgen]
pub fn sumar(a: u32, b: u32) -> u32 {
    a + b
}

#[wasm_bindgen]
pub fn raiz_cuadrada(x: f64) -> f64 {
    x.sqrt()
}
```

Compilar y ver el resultado:

```bash
# --target web: genera módulo ES para importar directamente en el navegador
wasm-pack build --target web --release

# El directorio pkg/ ahora contiene:
ls pkg/
# mandelbrot_wasm.js
# mandelbrot_wasm_bg.wasm
# mandelbrot_wasm.d.ts
# package.json
```

HTML mínimo para probar:

```html
<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body>
<script type="module">
  import init, { saludar, sumar } from './pkg/mandelbrot_wasm.js';

  await init();   // carga el .wasm y ejecuta #[wasm_bindgen(start)]

  console.log(saludar("mundo"));    // ¡Hola desde Rust+Wasm, mundo!
  console.log(sumar(3, 4));         // 7
</script>
</body>
</html>
```

---

## `#[wasm_bindgen]` en profundidad

### Structs exportadas a JavaScript

```rust
use wasm_bindgen::prelude::*;

// Struct visible en JS como una clase
#[wasm_bindgen]
pub struct Contador {
    valor: i32,
    paso:  i32,
}

#[wasm_bindgen]
impl Contador {
    // Constructor: en JS → new Contador(1)
    #[wasm_bindgen(constructor)]
    pub fn nuevo(paso: i32) -> Contador {
        Contador { valor: 0, paso }
    }

    // Getter: en JS → contador.valor
    #[wasm_bindgen(getter)]
    pub fn valor(&self) -> i32 {
        self.valor
    }

    // Setter: en JS → contador.paso = 5
    #[wasm_bindgen(setter)]
    pub fn set_paso(&mut self, paso: i32) {
        self.paso = paso;
    }

    // Método: en JS → contador.incrementar()
    pub fn incrementar(&mut self) {
        self.valor += self.paso;
    }

    pub fn resetear(&mut self) {
        self.valor = 0;
    }

    // Método estático: en JS → Contador.desde_valor(42, 1)
    pub fn desde_valor(valor: i32, paso: i32) -> Contador {
        Contador { valor, paso }
    }
}
```

En JavaScript:

```javascript
import init, { Contador } from './pkg/mandelbrot_wasm.js';
await init();

const c = new Contador(2);   // paso = 2
c.incrementar();              // valor = 2
c.incrementar();              // valor = 4
console.log(c.valor);         // 4
c.paso = 5;                   // setter
c.incrementar();              // valor = 9
c.free();                     // liberar memoria explícitamente (opcional, GC también lo hace)
```

### Renombrar exports

```rust
// El nombre en Rust puede diferir del nombre en JS
#[wasm_bindgen(js_name = "calcularHash")]
pub fn calcular_hash(datos: &[u8]) -> String {
    // ...
    hex::encode(datos)
}

// En JS: calcularHash(datos)
```

---

## Tipos en la frontera JS ↔ Rust

### Primitivos: paso directo

| Rust | JavaScript |
| :--- | :--- |
| `bool` | `boolean` |
| `i32`, `u32` | `number` |
| `i64`, `u64` | `BigInt` |
| `f32`, `f64` | `number` |
| `char` | `string` (1 carácter) |
| `String`, `&str` | `string` (copia) |
| `&[u8]`, `Vec<u8>` | `Uint8Array` (copia) |
| `Box<[u8]>` | `Uint8Array` (sin copia, toma ownership) |

### `JsValue`: cualquier valor de JavaScript

```rust
use wasm_bindgen::prelude::*;
use js_sys::Array;

#[wasm_bindgen]
pub fn procesar_array(arr: &Array) -> u32 {
    // Array es un tipo de js_sys que envuelve un Array de JS
    arr.length()
}

#[wasm_bindgen]
pub fn crear_objeto() -> JsValue {
    // JsValue puede ser cualquier cosa: null, undefined, número, string, objeto
    JsValue::from_str("resultado")
}
```

### `js_sys`: tipos estándar de JavaScript

El crate `js_sys` expone todos los objetos globales de JS como tipos Rust:

```rust
use js_sys::{Array, Date, Map, Object, Promise, Uint8Array};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn timestamp_actual() -> f64 {
    Date::now()   // Date.now() en JS → número f64
}

#[wasm_bindgen]
pub fn crear_array_numeros(n: u32) -> Array {
    let arr = Array::new();
    for i in 0..n {
        arr.push(&JsValue::from_f64(i as f64));
    }
    arr
}
```

### `web_sys`: APIs del navegador

`web_sys` es un crate gigante que expone las APIs del navegador. Cada API se activa
como una feature de Cargo para mantener el tamaño pequeño:

```toml
[dependencies]
web-sys = { version = "0.3", features = [
    "console",
    "Window",
    "Document",
    "HtmlCanvasElement",
    "CanvasRenderingContext2d",
    "ImageData",
] }
```

```rust
use web_sys::{console, window, HtmlCanvasElement, CanvasRenderingContext2d};
use wasm_bindgen::{JsCast, JsValue};

// Log a la consola del navegador
pub fn log(msg: &str) {
    console::log_1(&JsValue::from_str(msg));
}

pub fn log_fmt(args: std::fmt::Arguments) {
    console::log_1(&JsValue::from_str(&args.to_string()));
}

// Obtener el canvas del DOM
pub fn obtener_canvas(id: &str) -> Option<HtmlCanvasElement> {
    window()?
        .document()?
        .get_element_by_id(id)?
        .dyn_into::<HtmlCanvasElement>()   // downcast al tipo concreto
        .ok()
}

pub fn obtener_contexto_2d(canvas: &HtmlCanvasElement) -> Option<CanvasRenderingContext2d> {
    canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<CanvasRenderingContext2d>()
        .ok()
}
```

---

## Optimización de tamaño del `.wasm`

Un módulo Wasm grande descarga lentamente y bloquea el arranque de la página:

```bash
# Antes de optimizar: puede ser 2-5 MB
wasm-pack build --target web --release
ls -lh pkg/*.wasm

# wasm-opt: optimizador de Binaryen (incluido en wasm-pack)
# wasm-pack ya lo llama automáticamente con --release
# Para llamarlo manualmente:
wasm-opt -Oz pkg/mandelbrot_wasm_bg.wasm -o pkg/mandelbrot_wasm_bg.wasm
# -Oz: optimización agresiva de tamaño
# -O3: optimización de velocidad
```

Configuración en `Cargo.toml` para minimizar el binario:

```toml
[profile.release]
opt-level   = "z"    # tamaño mínimo (usa "3" si priorizas velocidad)
lto         = true   # Link-Time Optimization
codegen-units = 1    # máxima optimización
panic       = "abort"
strip       = true   # elimina símbolos de debug
```

Reducir el tamaño del runtime:

```toml
[dependencies]
# Allocator pequeño (~1 KB vs ~10 KB de dlmalloc por defecto)
wee_alloc = "0.4"
```

```rust
// En lib.rs:
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
```

Resultados típicos para un módulo de tamaño medio:

```text
Sin optimizar (debug):   ~8 MB
Release sin wasm-opt:    ~350 KB
Release + wasm-opt -Oz:  ~150 KB
+ wee_alloc:             ~140 KB
+ panic=abort:           ~120 KB
```

---

## Paralelismo en Wasm con Rayon

Los navegadores modernos soportan hilos mediante **Web Workers** y **SharedArrayBuffer**.
`wasm-bindgen-rayon` conecta el pool de hilos de Rayon con Workers del navegador.

### Prerrequisito: headers HTTP obligatorios

SharedArrayBuffer requiere que el servidor envíe estos headers en **cada respuesta**:

```
Cross-Origin-Opener-Policy:   same-origin
Cross-Origin-Embedder-Policy: require-corp
```

Sin estos headers, `SharedArrayBuffer` no está disponible y Rayon fallará en runtime.

```toml
# Cargo.toml con paralelismo
[dependencies]
wasm-bindgen         = "0.2"
rayon                = "1"
wasm-bindgen-rayon   = "1"
console_error_panic_hook = "0.1"
```

```rust
use wasm_bindgen::prelude::*;
use wasm_bindgen_rayon::init_thread_pool;

// OBLIGATORIO: inicializar el pool de workers antes de usar rayon
// n_workers: número de Web Workers (= núcleos lógicos del navegador)
#[wasm_bindgen]
pub async fn inicializar_workers(n_workers: usize) {
    init_thread_pool(n_workers).await;
}
```

En JavaScript, antes de usar cualquier función paralela:

```javascript
import init, { inicializar_workers, compute_frame_parallel }
  from './pkg/mandelbrot_wasm.js';

await init();

// Usar hardwareConcurrency (núcleos disponibles) o un número fijo
const workers = navigator.hardwareConcurrency || 4;
await inicializar_workers(workers);
console.log(`Pool de ${workers} workers inicializado`);

// Ahora las llamadas a Rust usarán Rayon internamente
const pixeles = compute_frame_parallel(800, 600, -2.5, 1.0, -1.25, 1.25, 256);
```

---

## Proyecto: Mandelbrot Explorer

El conjunto de Mandelbrot es ideal para demostrar Wasm:

- Cálculo puro, sin I/O, sin estado compartido — perfecto para paralelizar.
- Visualmente obvio: zoom muestra que el cálculo es correcto.
- Benchmark claro: JS vs Wasm vs Wasm+Rayon.

### Matemáticas del Mandelbrot (brevemente)

Para cada píxel `(px, py)` en el canvas, mapeado a un punto complejo `c = (cx, cy)`:

```
z₀ = 0
zₙ₊₁ = zₙ² + c

Si |zₙ| > 2 después de N iteraciones → el punto "escapa" → no está en el conjunto
Si aún no escapa después de max_iter → el punto pertenece al conjunto (negro)
El número de iteraciones hasta el escape determina el color
```

### `src/lib.rs` — el núcleo del renderer

```rust
use wasm_bindgen::prelude::*;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

// ── Inicialización ─────────────────────────────────────────────────────────

#[wasm_bindgen(start)]
pub fn inicializar() {
    console_error_panic_hook::set_once();
}

#[cfg(feature = "parallel")]
#[wasm_bindgen]
pub async fn inicializar_workers(n: usize) {
    wasm_bindgen_rayon::init_thread_pool(n).await;
}

// ── Parámetros del viewport ────────────────────────────────────────────────

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Viewport {
    pub x_min:    f64,
    pub x_max:    f64,
    pub y_min:    f64,
    pub y_max:    f64,
    pub max_iter: u32,
}

#[wasm_bindgen]
impl Viewport {
    #[wasm_bindgen(constructor)]
    pub fn nuevo(x_min: f64, x_max: f64, y_min: f64, y_max: f64, max_iter: u32) -> Self {
        Self { x_min, x_max, y_min, y_max, max_iter }
    }

    pub fn por_defecto() -> Self {
        Self { x_min: -2.5, x_max: 1.0, y_min: -1.25, y_max: 1.25, max_iter: 256 }
    }

    /// Hacer zoom centrado en (cx, cy) con factor de escala
    pub fn zoom(&self, cx: f64, cy: f64, factor: f64) -> Viewport {
        let w = (self.x_max - self.x_min) * factor;
        let h = (self.y_max - self.y_min) * factor;
        Viewport {
            x_min: cx - w / 2.0,
            x_max: cx + w / 2.0,
            y_min: cy - h / 2.0,
            y_max: cy + h / 2.0,
            max_iter: (self.max_iter as f64 * 1.1).min(2048.0) as u32,
        }
    }

    /// Convertir coordenadas de canvas (px, py) a plano complejo
    pub fn pixel_a_complejo(&self, px: u32, py: u32, width: u32, height: u32) -> (f64, f64) {
        let cx = self.x_min + (px as f64 / width  as f64) * (self.x_max - self.x_min);
        let cy = self.y_min + (py as f64 / height as f64) * (self.y_max - self.y_min);
        (cx, cy)
    }
}

// ── Núcleo del cálculo (código puro, sin wasm_bindgen) ────────────────────

fn iteraciones_mandelbrot(cx: f64, cy: f64, max_iter: u32) -> u32 {
    let mut zx = 0.0_f64;
    let mut zy = 0.0_f64;
    let mut iter = 0u32;

    while iter < max_iter && zx * zx + zy * zy <= 4.0 {
        let tmp = zx * zx - zy * zy + cx;
        zy = 2.0 * zx * zy + cy;
        zx = tmp;
        iter += 1;
    }
    iter
}

/// Mapear iteraciones a color RGBA usando una paleta suave
fn iter_a_rgba(iter: u32, max_iter: u32) -> [u8; 4] {
    if iter == max_iter {
        return [0, 0, 0, 255];  // negro: punto en el conjunto
    }

    // Paleta "ultra fractal" — ciclos de colores suaves
    let t = iter as f64 / max_iter as f64;
    let t2 = t * t;
    let t3 = t2 * t;

    let r = (9.0  * (1.0 - t) * t3         * 255.0) as u8;
    let g = (15.0 * (1.0 - t) * (1.0 - t) * t2 * 255.0) as u8;
    let b = (8.5  * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * 255.0) as u8;

    [r, g, b, 255]
}

// ── Render en un único hilo ────────────────────────────────────────────────

/// Calcula un frame del Mandelbrot y devuelve los píxeles como Vec<u8> (RGBA).
/// En JS: compute_frame devuelve un Uint8Array.
#[wasm_bindgen]
pub fn compute_frame(width: u32, height: u32, vp: &Viewport) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for py in 0..height {
        for px in 0..width {
            let (cx, cy) = vp.pixel_a_complejo(px, py, width, height);
            let iter = iteraciones_mandelbrot(cx, cy, vp.max_iter);
            let rgba = iter_a_rgba(iter, vp.max_iter);
            let idx = ((py * width + px) * 4) as usize;
            pixels[idx..idx + 4].copy_from_slice(&rgba);
        }
    }

    pixels
}

// ── Render paralelo (solo con feature "parallel") ─────────────────────────

/// Versión paralela con Rayon. Requiere inicializar_workers() antes de llamar.
#[cfg(feature = "parallel")]
#[wasm_bindgen]
pub fn compute_frame_parallel(width: u32, height: u32, vp: &Viewport) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    // Procesar en paralelo por filas; cada fila es independiente
    pixels
        .par_chunks_exact_mut((width * 4) as usize)
        .enumerate()
        .for_each(|(py, fila)| {
            for px in 0..width {
                let (cx, cy) = vp.pixel_a_complejo(px, py as u32, width, height);
                let iter = iteraciones_mandelbrot(cx, cy, vp.max_iter);
                let rgba = iter_a_rgba(iter, vp.max_iter);
                let idx = (px * 4) as usize;
                fila[idx..idx + 4].copy_from_slice(&rgba);
            }
        });

    pixels
}

// ── Tests (se ejecutan con: wasm-pack test --headless --firefox) ──────────

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    // Configurar tests para ejecutar en el navegador
    wasm_bindgen_test_configure!(run_in_browser);

    #[test]
    fn mandelbrot_origen_no_escapa() {
        // (0,0) nunca escapa: z = 0² + 0 = 0 siempre
        assert_eq!(iteraciones_mandelbrot(0.0, 0.0, 100), 100);
    }

    #[test]
    fn mandelbrot_punto_exterior_escapa_rapido() {
        // (3,0) está muy fuera del conjunto: escapa en pocas iteraciones
        let iter = iteraciones_mandelbrot(3.0, 0.0, 1000);
        assert!(iter < 5, "debería escapar rápido, got {iter}");
    }

    #[test]
    fn punto_2_escapa_exactamente() {
        // (2,0): |z₀| = 0, z₁ = 4 → |z₁| = 4 > 2, escapa en iter 1
        let iter = iteraciones_mandelbrot(2.0, 0.0, 100);
        assert_eq!(iter, 1);
    }

    #[wasm_bindgen_test]
    fn frame_tiene_tamano_correcto() {
        let vp = Viewport::por_defecto();
        let pixeles = compute_frame(100, 100, &vp);
        assert_eq!(pixeles.len(), 100 * 100 * 4);
    }

    #[wasm_bindgen_test]
    fn frame_tiene_canal_alpha_lleno() {
        let vp = Viewport::por_defecto();
        let pixeles = compute_frame(10, 10, &vp);
        // Cada cuarto byte (canal alpha) debe ser 255
        for i in (3..pixeles.len()).step_by(4) {
            assert_eq!(pixeles[i], 255, "alpha en posición {i}");
        }
    }

    #[wasm_bindgen_test]
    fn zoom_reduce_viewport() {
        let vp = Viewport::por_defecto();
        let zp = vp.zoom(0.0, 0.0, 0.5);
        let ancho_orig = vp.x_max - vp.x_min;
        let ancho_zoom = zp.x_max - zp.x_min;
        assert!((ancho_zoom - ancho_orig * 0.5).abs() < 1e-10);
    }
}
```

### `Cargo.toml` completo

```toml
[package]
name    = "mandelbrot-wasm"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[features]
default  = []
parallel = ["rayon", "wasm-bindgen-rayon"]

[dependencies]
wasm-bindgen             = "0.2"
console_error_panic_hook = "0.1"
js-sys                   = "0.3"
web-sys = { version = "0.3", features = [
    "console",
    "Window",
    "Performance",
] }

# Solo activos con la feature "parallel"
rayon              = { version = "1", optional = true }
wasm-bindgen-rayon = { version = "1", optional = true }

[dev-dependencies]
wasm-bindgen-test = "0.3"

[profile.release]
opt-level     = "z"
lto           = true
codegen-units = 1
panic         = "abort"
strip         = true
```

Comandos de build:

```bash
# Build single-thread (predeterminado)
wasm-pack build --target web --release

# Build con paralelismo Rayon
wasm-pack build --target web --release -- --features parallel

# Tests en navegador headless (requiere Firefox o Chrome instalado)
wasm-pack test --headless --firefox
# o:
wasm-pack test --headless --chrome
```

---

## Frontend con Vite

Vite es el bundler más ergonómico para integrar Wasm:

```bash
# Crear proyecto frontend
npm create vite@latest mandelbrot-web -- --template vanilla
cd mandelbrot-web
npm install
```

`vite.config.js`:

```javascript
import { defineConfig } from 'vite';

export default defineConfig({
  // Headers COOP/COEP necesarios para SharedArrayBuffer (Rayon)
  server: {
    headers: {
      'Cross-Origin-Opener-Policy':   'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  // Plugin para servir archivos .wasm correctamente
  optimizeDeps: {
    exclude: ['mandelbrot-wasm'],
  },
});
```

`index.html`:

```html
<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="UTF-8">
  <title>Mandelbrot Explorer — Rust + Wasm</title>
  <style>
    body { margin: 0; background: #111; display: flex;
           flex-direction: column; align-items: center; color: #eee; font-family: monospace; }
    #canvas { cursor: crosshair; border: 1px solid #333; }
    #controles { padding: 12px; display: flex; gap: 12px; align-items: center; }
    button { background: #333; color: #eee; border: 1px solid #555;
             padding: 6px 14px; cursor: pointer; border-radius: 4px; }
    button:hover { background: #444; }
    #info { font-size: 12px; color: #888; min-width: 300px; }
  </style>
</head>
<body>
  <div id="controles">
    <button id="btn-reset">Reset</button>
    <button id="btn-paralelo">Modo paralelo</button>
    <label>
      Max iter: <input id="max-iter" type="range" min="64" max="2048" value="256">
      <span id="iter-val">256</span>
    </label>
    <div id="info">Cargando Wasm...</div>
  </div>
  <canvas id="canvas" width="900" height="600"></canvas>
  <script type="module" src="/src/main.js"></script>
</body>
</html>
```

`src/main.js`:

```javascript
// Importar el módulo Wasm generado por wasm-pack
// Ajusta la ruta si copiaste pkg/ dentro de mandelbrot-web/
import init, {
  inicializar_workers,
  compute_frame,
  Viewport,
} from '../mandelbrot-wasm/pkg/mandelbrot_wasm.js';

const canvas  = document.getElementById('canvas');
const ctx     = canvas.getContext('2d');
const info    = document.getElementById('info');
const iterIn  = document.getElementById('max-iter');
const iterVal = document.getElementById('iter-val');

const W = canvas.width;
const H = canvas.height;

// ── Estado de la aplicación ────────────────────────────────────────────────

let viewport      = null;   // Viewport de Rust (objeto Wasm)
let usarParalelo  = false;
let cargando      = false;

// ── Helpers ───────────────────────────────────────────────────────────────

function pintarFrame(pixeles) {
  // Pasar el Uint8Array directamente a ImageData (copia mínima)
  const imgData = new ImageData(new Uint8ClampedArray(pixeles), W, H);
  ctx.putImageData(imgData, 0, 0);
}

async function renderizar() {
  if (cargando) return;
  cargando = true;
  const t0 = performance.now();

  let pixeles;
  if (usarParalelo && typeof compute_frame_parallel !== 'undefined') {
    // Importado condicionalmente si la feature parallel está activa
    const { compute_frame_parallel } = await import(
      '../mandelbrot-wasm/pkg/mandelbrot_wasm.js'
    );
    pixeles = compute_frame_parallel(W, H, viewport);
  } else {
    pixeles = compute_frame(W, H, viewport);
  }

  pintarFrame(pixeles);
  const ms = (performance.now() - t0).toFixed(1);
  const modo = usarParalelo ? 'paralelo' : 'single-thread';
  info.textContent = `${W}×${H} | iter: ${viewport.max_iter} | ${ms}ms | ${modo}`;
  cargando = false;
}

// ── Interacción: click para hacer zoom ────────────────────────────────────

canvas.addEventListener('click', async (e) => {
  const rect  = canvas.getBoundingClientRect();
  const px    = e.clientX - rect.left;
  const py    = e.clientY - rect.top;

  // Convertir píxel a coordenadas del plano complejo
  const cx = viewport.x_min + (px / W) * (viewport.x_max - viewport.x_min);
  const cy = viewport.y_min + (py / H) * (viewport.y_max - viewport.y_min);

  // Zoom 2x centrado en el punto clicado
  const nuevoVp = viewport.zoom(cx, cy, 0.5);
  viewport.free();    // liberar el Viewport anterior
  viewport = nuevoVp;

  await renderizar();
});

// ── Controles ──────────────────────────────────────────────────────────────

document.getElementById('btn-reset').addEventListener('click', async () => {
  if (viewport) viewport.free();
  viewport = Viewport.por_defecto();
  await renderizar();
});

document.getElementById('btn-paralelo').addEventListener('click', () => {
  usarParalelo = !usarParalelo;
  document.getElementById('btn-paralelo').textContent =
    usarParalelo ? 'Modo single-thread' : 'Modo paralelo';
  renderizar();
});

iterIn.addEventListener('input', async () => {
  const v = parseInt(iterIn.value);
  iterVal.textContent = v;
  const nuevoVp = new Viewport(
    viewport.x_min, viewport.x_max,
    viewport.y_min, viewport.y_max,
    v
  );
  viewport.free();
  viewport = nuevoVp;
  await renderizar();
});

// ── Arranque ───────────────────────────────────────────────────────────────

async function arrancar() {
  info.textContent = 'Iniciando Wasm...';

  await init();   // carga el .wasm

  // Inicializar workers si la build tiene la feature parallel
  const workers = navigator.hardwareConcurrency || 4;
  if (typeof inicializar_workers !== 'undefined') {
    info.textContent = `Iniciando ${workers} workers...`;
    await inicializar_workers(workers);
  }

  viewport = Viewport.por_defecto();
  info.textContent = 'Renderizando...';
  await renderizar();
}

arrancar().catch(console.error);
```

Ejecutar el servidor de desarrollo:

```bash
# Copiar pkg/ al directorio del frontend (o usar un workspace npm)
cp -r ../mandelbrot-wasm/pkg ./pkg

cd mandelbrot-web
npm run dev
# → http://localhost:5173  (con headers COOP/COEP para SharedArrayBuffer)
```

---

## Llamar JavaScript desde Rust

Además de exportar funciones Rust a JS, puedes importar funciones JS en Rust usando
`extern` con `#[wasm_bindgen]`:

```rust
use wasm_bindgen::prelude::*;

// Importar funciones del objeto global de JS
#[wasm_bindgen]
extern "C" {
    // window.alert()
    fn alert(s: &str);

    // console.log()
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    // console.log con múltiples argumentos
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_dos(a: &str, b: &str);
}

// Importar una función definida en JS (en index.html o un módulo)
#[wasm_bindgen(module = "/src/helpers.js")]
extern "C" {
    fn actualizar_ui(ms: f64, pixeles: u32);
}
```

Macro de logging que funciona en Wasm:

```rust
macro_rules! wasm_log {
    ($($arg:tt)*) => {
        web_sys::console::log_1(&format!($($arg)*).into());
    };
}

// Uso:
// wasm_log!("iteración {}, tiempo: {:.2}ms", i, ms);
```

---

## Benchmark: Rust/Wasm vs JavaScript puro

Incluye este benchmark en la página para cuantificar la ventaja:

```javascript
// src/benchmark.js
import init, { compute_frame, Viewport } from '../pkg/mandelbrot_wasm.js';

// Implementación de referencia en JavaScript puro
function computeFrameJS(width, height, xMin, xMax, yMin, yMax, maxIter) {
  const pixels = new Uint8Array(width * height * 4);
  for (let py = 0; py < height; py++) {
    for (let px = 0; px < width; px++) {
      const cx = xMin + (px / width)  * (xMax - xMin);
      const cy = yMin + (py / height) * (yMax - yMin);

      let zx = 0, zy = 0, iter = 0;
      while (iter < maxIter && zx*zx + zy*zy <= 4) {
        const tmp = zx*zx - zy*zy + cx;
        zy = 2*zx*zy + cy;
        zx = tmp;
        iter++;
      }

      const idx = (py * width + px) * 4;
      if (iter === maxIter) {
        pixels[idx + 3] = 255;  // negro
      } else {
        const t = iter / maxIter;
        pixels[idx]     = (9  * (1-t) * t*t*t         * 255) | 0;
        pixels[idx + 1] = (15 * (1-t)*(1-t) * t*t     * 255) | 0;
        pixels[idx + 2] = (8.5* (1-t)*(1-t)*(1-t) * t * 255) | 0;
        pixels[idx + 3] = 255;
      }
    }
  }
  return pixels;
}

export async function ejecutarBenchmark(width, height) {
  await init();
  const vp = new Viewport(-2.5, 1.0, -1.25, 1.25, 512);
  const N  = 5;  // repeticiones para promediar

  // Calentar (primer run puede ser más lento por JIT)
  compute_frame(width, height, vp);
  computeFrameJS(width, height, -2.5, 1.0, -1.25, 1.25, 512);

  // Benchmark Wasm
  const t0 = performance.now();
  for (let i = 0; i < N; i++) compute_frame(width, height, vp);
  const msWasm = (performance.now() - t0) / N;

  // Benchmark JS
  const t1 = performance.now();
  for (let i = 0; i < N; i++) computeFrameJS(width, height, -2.5, 1.0, -1.25, 1.25, 512);
  const msJs = (performance.now() - t1) / N;

  vp.free();

  return {
    wasm:  msWasm.toFixed(1),
    js:    msJs.toFixed(1),
    ratio: (msJs / msWasm).toFixed(2),
  };
}
```

Resultados típicos en Chrome (900×600, 512 iter):

```
JavaScript puro:    ~180ms
Rust/Wasm (1 hilo): ~55ms   → ×3.3 más rápido
Rust/Wasm (8 hilos): ~8ms   → ×22 más rápido
```

La ventaja de Wasm sobre JS puro varía de 1.5× a 5× para código CPU-bound bien
optimizado. El factor más grande viene del paralelismo.

---

## Tests de Wasm con `wasm-bindgen-test`

```bash
# Tests que se ejecutan en el navegador headless
wasm-pack test --headless --firefox -- --features parallel

# Tests que se ejecutan en Node.js (sin DOM disponible)
wasm-pack test --node
```

```rust
// src/lib.rs — los tests ya están integrados arriba
// Para tests más avanzados que necesitan el DOM:

#[cfg(test)]
mod tests_navegador {
    use wasm_bindgen_test::*;
    use web_sys::window;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn performance_disponible() {
        // Verificar que la API de Performance está disponible en el navegador
        let perf = window().unwrap().performance().unwrap();
        let t = perf.now();
        assert!(t >= 0.0);
    }
}
```

---

## Distribución: publicar en GitHub Pages

```bash
# Build optimizado final
wasm-pack build --target web --release

# Copiar pkg/ al directorio del frontend
cp -r pkg mandelbrot-web/public/pkg

# Build del frontend
cd mandelbrot-web
npm run build    # genera mandelbrot-web/dist/

# Desplegar en GitHub Pages
# (con gh-pages u otro método)
```

`vite.config.js` para GitHub Pages:

```javascript
export default defineConfig({
  base: '/nombre-del-repo/',   // ajustar al nombre del repositorio
  server: {
    headers: {
      'Cross-Origin-Opener-Policy':   'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  preview: {
    headers: {
      'Cross-Origin-Opener-Policy':   'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
});
```

---

## ✅ Checklist de la Semana 15

- [ ] El toolchain está instalado: `wasm-pack`, `wasm32-unknown-unknown` target.
- [ ] `console_error_panic_hook::set_once()` está en `#[wasm_bindgen(start)]` — los
  panics muestran el mensaje completo en la consola del navegador.
- [ ] `#[wasm_bindgen]` en funciones simples (`saludar`, `sumar`), y en struct +
  `impl` block (`Viewport` con constructor, getter, setter, métodos).
- [ ] El canvas del Mandelbrot renderiza correctamente: el conjunto es negro, los
  puntos exteriores tienen color, y el zoom funciona.
- [ ] `compute_frame` devuelve `Vec<u8>` y JS lo convierte a `ImageData` con
  `new Uint8ClampedArray(pixeles)` (copia mínima).
- [ ] Los headers `COOP`/`COEP` están configurados en Vite para que
  `SharedArrayBuffer` esté disponible.
- [ ] La versión paralela (`compute_frame_parallel` con `--features parallel`) usa
  `rayon::par_chunks_exact_mut` y `inicializar_workers` se llama antes del primer uso.
- [ ] El benchmark muestra la ventaja de Wasm vs JS puro en el mismo navegador.
- [ ] `wasm-pack build --release` genera un `.wasm` inferior a 200 KB.
- [ ] `wasm-pack test --headless --firefox` pasa los 5 tests de la biblioteca.
- [ ] El proyecto está desplegado y accesible desde un browser real
  (`npm run dev` o GitHub Pages).

> **Siguiente paso:** Semana 16 — [Parsing y procesamiento de texto: nom, pest, regex](section_04.md).
