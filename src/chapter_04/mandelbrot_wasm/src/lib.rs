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
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub max_iter: u32,
}

#[wasm_bindgen]
impl Viewport {
    #[wasm_bindgen(constructor)]
    pub fn nuevo(x_min: f64, x_max: f64, y_min: f64, y_max: f64, max_iter: u32) -> Self {
        Self {
            x_min,
            x_max,
            y_min,
            y_max,
            max_iter,
        }
    }

    pub fn por_defecto() -> Self {
        Self {
            x_min: -2.5,
            x_max: 1.0,
            y_min: -1.25,
            y_max: 1.25,
            max_iter: 256,
        }
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
}

// Métodos solo para Rust: sin #[wasm_bindgen], porque wasm-bindgen no puede exportar
// tuplas a JavaScript (`(f64, f64): IntoWasmAbi` no se cumple). Si JS lo necesitara,
// devolveríamos un struct exportado o un `Vec<f64>`.
impl Viewport {
    /// Convertir coordenadas de canvas (px, py) a plano complejo
    pub fn pixel_a_complejo(&self, px: u32, py: u32, width: u32, height: u32) -> (f64, f64) {
        let cx = self.x_min + (px as f64 / width as f64) * (self.x_max - self.x_min);
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
        return [0, 0, 0, 255]; // negro: punto en el conjunto
    }

    // Paleta "ultra fractal" — ciclos de colores suaves
    let t = iter as f64 / max_iter as f64;
    let t2 = t * t;
    let t3 = t2 * t;

    let r = (9.0 * (1.0 - t) * t3 * 255.0) as u8;
    let g = (15.0 * (1.0 - t) * (1.0 - t) * t2 * 255.0) as u8;
    let b = (8.5 * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * 255.0) as u8;

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

// ── Tests ─────────────────────────────────────────────────────────────────
// #[test]              → en el host, con `cargo test` (código Rust puro)
// #[wasm_bindgen_test] → compilados a Wasm, con `wasm-pack test --node`
// Ninguno necesita el DOM; los que sí lo necesitan están en tests/navegador.rs.

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

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
        // c = 2: z₁ = 0² + 2 = 2 → |z₁|² = 4, justo en el límite (≤ 4), sigue.
        //        z₂ = 2² + 2 = 6 → |z₂|² = 36 > 4, escapa: 2 iteraciones.
        let iter = iteraciones_mandelbrot(2.0, 0.0, 100);
        assert_eq!(iter, 2);
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
