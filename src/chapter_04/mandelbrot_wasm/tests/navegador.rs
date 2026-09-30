// tests/navegador.rs — tests que necesitan el DOM (window, performance...).
// Solo corren en un navegador: wasm-pack test --headless --firefox (o --chrome).

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
