# Desarrollo de video player

Para crear una aplicación que reproduzca videos utilizando **Rust** y **WebAssembly** (WASM):

### 1. **Instalar Rust y Herramientas WebAssembly**
Primero, debes instalar las herramientas necesarias para compilar código Rust a WebAssembly:

- Instala Rust si no lo tienes:  
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- Añade el objetivo WebAssembly a tu herramienta Rust:  
  ```bash
  rustup target add wasm32-unknown-unknown
  ```

- Instala **wasm-pack**, una herramienta para facilitar la creación de paquetes de WebAssembly:
  ```bash
  cargo install wasm-pack
  ```

### 2. **Crear Proyecto con Rust y WebAssembly**
Inicia un nuevo proyecto de Rust con WebAssembly:

```bash
cargo new video_player --lib
cd video_player
```

En tu archivo `Cargo.toml`, asegúrate de agregar las dependencias adecuadas:

```toml
[dependencies]
wasm-bindgen = "0.2"
```

### 3. **Uso de wasm-bindgen para Interactuar con JavaScript**
`wasm-bindgen` es una herramienta clave que permite que el código Rust se comunique con JavaScript. Utilizarás JavaScript para controlar el elemento de video del navegador y luego invocar funciones desde Rust.

En tu archivo `lib.rs` en la carpeta `src`:

```rust
use wasm_bindgen::prelude::*;
use web_sys::HtmlVideoElement;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub fn play_video(video_id: &str) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let video_element = document
        .get_element_by_id(video_id)
        .unwrap()
        .dyn_into::<HtmlVideoElement>()
        .unwrap();

    video_element.play().unwrap();
    log("Video started!");
}
```

Este código accede al elemento HTML de video por su ID y llama al método `play()` del navegador para reproducir el video.

### 4. **Crear la Interfaz de Usuario en HTML/JavaScript**
Ahora, crea una página HTML sencilla para tu aplicación de video:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Rust Video Player</title>
</head>
<body>
    <video id="video-player" width="600" controls>
        <source src="your_video.mp4" type="video/mp4">
        Your browser does not support the video tag.
    </video>
    <button id="play-btn">Play with Rust</button>

    <script type="module">
        import init, { play_video } from './pkg/video_player.js';

        async function run() {
            await init();
            document.getElementById('play-btn').addEventListener('click', () => {
                play_video("video-player");
            });
        }
        run();
    </script>
</body>
</html>
```

Este código HTML contiene un elemento `video` y un botón que, al hacer clic, invoca la función `play_video` escrita en Rust para reproducir el video.

### 5. **Construir y Ejecutar la Aplicación**
Compila tu proyecto Rust a WebAssembly usando `wasm-pack`:

```bash
wasm-pack build --target web
```

Esto generará un paquete de WebAssembly que puede ser utilizado desde tu archivo HTML.

Para ejecutar la aplicación, puedes usar un servidor local, como `http-server` en Node.js:

```bash
npx http-server
```

### 6. **Optimización y Mejoras**
- **Control más avanzado del video**: Puedes agregar más controles como pausa, detener, y cambiar la fuente del video usando métodos adicionales de `HtmlVideoElement`.
- **Interacción más rica entre Rust y JavaScript**: Usar `wasm-bindgen` para mejorar la interacción y manejar eventos complejos en la interfaz.

Este enfoque combina la eficiencia de Rust con la interoperabilidad de WebAssembly y JavaScript, lo que te permite crear aplicaciones de video interactivas en el navegador.
