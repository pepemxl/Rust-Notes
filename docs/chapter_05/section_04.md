# Especialización: elige tu camino maestro

La Semana 20 cierra el Mes 5 con una elección deliberada. Los cuatro meses
anteriores construyeron una base amplia; ahora profundizas en un dominio
que se alinea con tu trayectoria profesional. No hay ruta equivocada — hay
una ruta **tuya**.

En esta sección presentamos las tres rutas en detalle. Elige una, impleméntala
al 100% y documenta el proceso. El artefacto de esta semana demuestra dominio,
no cobertura.

- **Ruta A — Embedded async**: `#![no_std]` + Embassy. Rust sin sistema operativo.
  Hardware real, determinismo de microsegundos, cero dependencias de libc.
- **Ruta B — Procedural Macros**: `proc-macro` crate + `syn` + `quote`. Extender
  el compilador. `#[derive(Builder)]` que genera código seguro, ergonómico y con
  errores que apuntan al código del usuario.
- **Ruta C — Monorepo Workspace**: arquitectura limpia en un workspace Cargo.
  Separación de concerns, `workspace.dependencies`, `workspace.lints`,
  `cargo hack` y `xtask` para automatización del build.

> *"A master in the art of living draws no sharp distinction between his work
> and his play, his labor and his leisure, his mind and his body, his education
> and his recreation. He hardly knows which is which. He simply pursues his
> vision of excellence through whatever he is doing and leaves others to
> determine whether he is working or playing."*
> — François-René de Chateaubriand

---

## Árbol de decisión

```text
¿Tienes hardware embebido (RP2040, STM32) o quieres trabajar en sistemas?
    └── SÍ → RUTA A: Embedded (Embassy + no_std)

¿Quieres extender el compilador de Rust y hacer APIs ergonómicas?
    └── SÍ → RUTA B: Procedural Macros (#[derive(Builder)])

¿Trabajas en un equipo con múltiples crates y quieres arquitectura limpia?
    └── SÍ → RUTA C: Monorepo Workspace

¿No estás seguro? → RUTA B: Las proc macros son la elección más
    "universalmente ejecutable" sin hardware ni equipo grande.
    Se prueban con cargo test en cualquier máquina.
```

---

# RUTA A: Embedded async — `no_std` + Embassy

## Fundamentos de `no_std`

En Rust embebido no hay sistema operativo, no hay libc y por defecto no hay
asignador de heap. Solo el núcleo del lenguaje:

```text
┌───────────────────────────────────────────────────────────────────────────┐
│  CAPAS DE UNA APLICACIÓN EMBEBIDA RUST                                    │
│                                                                           │
│  Tu aplicación                                                            │
│  ─────────────                                                            │
│  Embassy (executor async + HALs)     ← reemplaza tokio                   │
│  ─────────────────────────────────────                                    │
│  PAC (Peripheral Access Crate)        ← acceso registro de HW             │
│  ─────────────────────────────────────                                    │
│  core  (sin alloc, sin std)           ← siempre disponible               │
│  alloc (heap opcional con allocator)  ← activo si necesitas Vec/Box      │
│  ─────────────────────────────────────                                    │
│  Bare metal (flash, RAM del MCU)                                          │
└───────────────────────────────────────────────────────────────────────────┘
```

```rust
// main.rs — plantilla mínima no_std
#![no_std]          // sin std library
#![no_main]         // sin fn main() convencional; el entry point lo define Embassy

// Si necesitas Vec, String, Box: activa el allocator
// extern crate alloc;
// use alloc::vec::Vec;

// Panic handler obligatorio: ¿qué hacemos si hay un panic?
use defmt_rtt as _;         // envía logs por RTT (sonda J-Link/CMSIS-DAP)
use panic_probe as _;       // panic → mensaje defmt → detiene el programa

// Entry point de Embassy
#[embassy_executor::main]
async fn main(spawner: embassy_executor::Spawner) {
    // spawner lanza otras tasks
    let peripherals = embassy_rp::init(Default::default());
    spawner.spawn(blinky(peripherals.PIN_25)).unwrap();
    spawner.spawn(sensor(peripherals.I2C0, peripherals.PIN_4, peripherals.PIN_5)).unwrap();
}
```

### `panic_handler` propio y `defmt`

```rust
// sin defmt: panic handler mínimo
#[cfg(not(test))]
#[panic_handler]
fn mi_panic(info: &core::panic::PanicInfo) -> ! {
    // Aquí no puedes hacer println! (no hay stdout)
    // Opciones: LED de error, escribir a UART, watchdog reset
    loop {
        // si tienes watchdog, no hagas loop infinito → usa cortex_m::peripheral::SCB::sys_reset()
    }
}

// Con defmt: el macro defmt::panic! envía el mensaje al host vía RTT
// y luego resetea el MCU (configurado por panic-probe)
```

### `defmt`: logging de cero coste para embebido

`defmt` es el logger estándar en el ecosistema Embassy. El dispositivo solo envía
índices de formato y argumentos; el host (tu PC) reconstruye el mensaje:

```rust
use defmt::{debug, error, info, warn};

// En el dispositivo: envía ~4 bytes (índice de string + valor)
info!("Temperatura: {} °C", temperatura);
warn!("Batería baja: {}%", nivel_bateria);
error!("Error I2C: {:?}", resultado_error);

// En el host (vía probe-rs / probe-run):
// 00:01.234 INFO  Temperatura: 23 °C
// 00:02.001 WARN  Batería baja: 12%

// debug! solo en builds debug (cero coste en release)
debug!("tick interno: {}", contador);

// defmt::assert! no añade strings al binario en release
defmt::assert_eq!(resultado, Ok(()), "La inicialización debe tener éxito");
```

## El modelo de tareas de Embassy

```text
EMBASSY: TASKS = STATE MACHINES, NO OS THREADS

  main task (spawner)
      │
      ├─ spawn(blinky_task)  ──► Task 1: parpadea LED cada 500ms
      │                              State: { pin, delay_ms }
      │                              Await: Timer::after()
      │
      ├─ spawn(sensor_task)  ──► Task 2: lee I2C cada 1s
      │                              State: { i2c, canal_tx }
      │                              Await: i2c.read().await
      │
      └─ spawn(display_task) ──► Task 3: recibe datos, refresca pantalla
                                     State: { display, canal_rx }
                                     Await: canal_rx.receive().await

  SIN THREADS DE OS: el executor de Embassy es cooperativo (no preemptive)
  → Cada .await es un punto de yield
  → Sin data races (un solo executor por núcleo)
  → Sin overhead de context switch de SO
```

## Proyecto: Weather Station

```bash
# Configurar target para RP2040 (Cortex-M0+)
rustup target add thumbv6m-none-eabi

# Herramientas de flash y debug
cargo install probe-rs-cli    # o probe-run
cargo install elf2uf2-rs      # para Pico sin probe JTAG
```

### `Cargo.toml`

```toml
[package]
name    = "weather-station"
version = "0.1.0"
edition = "2021"

[dependencies]
embassy-executor  = { version = "0.6", features = ["arch-cortex-m", "executor-thread"] }
embassy-rp        = { version = "0.2", features = ["defmt", "unstable-pac", "time-driver-alarm0"] }
embassy-time      = { version = "0.3", features = ["defmt"] }
embassy-sync      = { version = "0.6", features = ["defmt"] }

defmt             = "0.3"
defmt-rtt         = "0.4"
panic-probe       = { version = "0.3", features = ["print-defmt"] }

# Driver I2C para BMP280 (sensor de T/P)
bmp280-ehal       = "0.2"

# No_std compatible
cortex-m          = { version = "0.7", features = ["inline-asm"] }
cortex-m-rt       = "0.7"

[profile.release]
opt-level = "z"      # optimizar tamaño (típico en embebido)
debug     = 2        # conservar símbolos para defmt
lto       = true

[profile.dev]
opt-level = 1        # el depurado puro (opt=0) puede ser demasiado lento en MCU
debug     = 2
```

### `src/main.rs` — tareas Embassy

```rust
#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::{
    bind_interrupts,
    i2c::{self, I2c, InterruptHandler as I2cHandler},
    peripherals::{I2C0, PIN_4, PIN_5, PIN_25},
    gpio::{Level, Output},
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use defmt_rtt as _;
use panic_probe as _;

// ── Canal entre tareas ─────────────────────────────────────────────────────

#[derive(Format)]  // defmt Format trait (no Debug/Display de std)
struct DatosSensor {
    temperatura_c10: i32,  // temperatura * 10 (sin float: 235 = 23.5 °C)
    presion_hpa:     u32,
    humedad_pct:     u8,
}

// Canal con capacidad 4: la tarea de display puede ir más lento
static CANAL_SENSOR: Channel<ThreadModeRawMutex, DatosSensor, 4> = Channel::new();

// ── Registro de interrupciones ─────────────────────────────────────────────

bind_interrupts!(struct Irqs {
    I2C0_IRQ => I2cHandler<I2C0>;
});

// ── Entry point ────────────────────────────────────────────────────────────

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Weather Station iniciando…");
    let p = embassy_rp::init(Default::default());

    // I2C para el sensor BMP280
    let i2c = I2c::new_async(p.I2C0, p.PIN_5, p.PIN_4, Irqs, i2c::Config::default());

    spawner.spawn(tarea_sensor(i2c)).unwrap();
    spawner.spawn(tarea_blinky(p.PIN_25)).unwrap();
    // spawner.spawn(tarea_display(...)).unwrap();

    // main task ya no hace nada: las tareas se auto-mantienen
}

// ── Tarea sensor: lee BMP280 cada 1 segundo ────────────────────────────────

#[embassy_executor::task]
async fn tarea_sensor(i2c: I2c<'static, I2C0, i2c::Async>) {
    let mut sensor = bmp280_ehal::BMP280::new(i2c, bmp280_ehal::Oversampling::x4).await
        .expect("BMP280 no encontrado en bus I2C");

    loop {
        match sensor.read_fixed_point().await {
            Ok(medida) => {
                let datos = DatosSensor {
                    temperatura_c10: medida.temperature_hundredths / 10,
                    presion_hpa:     medida.pressure_pa / 100,
                    humedad_pct:     0,  // BMP280 no tiene humedad (BME280 sí)
                };
                info!("T: {}.{}°C  P: {} hPa",
                    datos.temperatura_c10 / 10,
                    datos.temperatura_c10.abs() % 10,
                    datos.presion_hpa
                );
                // Enviar al canal (sin bloquear si está lleno: drop silencioso)
                let _ = CANAL_SENSOR.try_send(datos);
            }
            Err(e) => error!("Error de lectura: {:?}", e),
        }
        Timer::after(Duration::from_secs(1)).await;
    }
}

// ── Tarea blinky: heartbeat LED ─────────────────────────────────────────────

#[embassy_executor::task]
async fn tarea_blinky(pin: PIN_25) {
    let mut led = Output::new(pin, Level::Low);
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(900)).await;
    }
}
```

### CI: compilar para thumbv6m en GitHub Actions

```yaml
# .github/workflows/embedded.yml
name: Embedded CI

on: [push, pull_request]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Instalar Rust + target embebido
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: thumbv6m-none-eabi

      - name: Compilar para RP2040
        run: cargo build --release --target thumbv6m-none-eabi

      - name: Verificar tamaño del binario
        run: |
          size target/thumbv6m-none-eabi/release/weather-station
          # Alerta si el binario supera 512 KB (flash del Pico)
          python3 -c "
          import os
          size = os.path.getsize('target/thumbv6m-none-eabi/release/weather-station')
          assert size < 512*1024, f'Binario demasiado grande: {size} bytes'
          print(f'Tamaño OK: {size // 1024} KB')
          "
```

---

# RUTA B: Procedural Macros — `#[derive(Builder)]`

## Mecánica de un crate proc-macro

```text
FLUJO DE COMPILACIÓN CON PROC MACRO

  código del usuario
  ──────────────────
  #[derive(Builder)]
  struct Config { host: String, port: u16 }
            │
            │  TokenStream (tokens del struct)
            ▼
  ┌──────────────────────────────┐
  │  builder-derive crate        │   ← proc-macro = true
  │  (se ejecuta como plugin     │      (corre en la máquina de compilación)
  │   del compilador)            │
  │                              │
  │  syn::parse → DeriveInput    │   ← parsear la estructura
  │  analizar campos + attrs     │
  │  quote! { ... }              │   ← generar nuevo código
  └──────────────────────────────┘
            │
            │  TokenStream (código generado)
            ▼
  código expandido (visible con cargo expand):
  ──────────────────────────────────────────
  struct ConfigBuilder { host: Option<String>, port: Option<u16> }
  impl ConfigBuilder { fn host(mut self, v: ...) -> Self { ... } }
  impl Config { fn builder() -> ConfigBuilder { ... } }
```

### Reglas fundamentales

```text
1. Un proc-macro crate es SOLO proc-macros: no puede exponer tipos o funciones
   normales. Necesitas un crate "envoltorio" para re-exportar.

2. proc_macro::TokenStream es opaco fuera del contexto de compilación.
   Usa proc_macro2::TokenStream para tests unitarios del macro.

3. Los errores deben apuntar al código del USUARIO, no al código generado.
   Usa Span::call_site() y syn::Error::new_spanned().

4. cargo expand (cargo install cargo-expand) es tu mejor amigo para depurar.
   cargo expand  →  muestra el código que genera el macro
```

## Workspace para proc macros

```bash
cargo new builder-macro --lib     # el proc-macro puro
cargo new builder        --lib    # re-exporta y añade helpers
mkdir builder-tests
```

**`Cargo.toml` (raíz del workspace):**

```toml
[workspace]
resolver = "2"
members  = ["builder-macro", "builder", "builder-tests"]
```

**`builder-macro/Cargo.toml`:**

```toml
[package]
name    = "builder-macro"
version = "0.1.0"
edition = "2021"

[lib]
proc-macro = true   # CLAVE: este crate es un plugin del compilador

[dependencies]
syn   = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

**`builder/Cargo.toml`:**

```toml
[package]
name    = "builder"
version = "0.1.0"
edition = "2021"

[dependencies]
builder-macro = { path = "../builder-macro" }

[dev-dependencies]
builder-tests = { path = "../builder-tests" }
```

## Implementación: `builder-macro/src/lib.rs`

```rust
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, quote_spanned};
use syn::{
    parse_macro_input, spanned::Spanned,
    Data, DeriveInput, Expr, Fields, Ident, LitStr, Type,
};

// ── Estructuras de análisis ────────────────────────────────────────────────

#[derive(Default)]
struct CampoAttr {
    default:  Option<DefaultKind>,
    each:     Option<Ident>,        // #[builder(each = "item")] para Vec
}

enum DefaultKind {
    Trait,           // #[builder(default)]       → Default::default()
    Expr(Box<Expr>), // #[builder(default = "42")]→ expr literal
}

struct CampoBuilder {
    ident:   Ident,
    ty:      Type,
    attr:    CampoAttr,
}

// ── Parseo de atributos #[builder(...)] ───────────────────────────────────

fn parsear_attrs(field: &syn::Field) -> syn::Result<CampoAttr> {
    let mut attr = CampoAttr::default();

    for a in &field.attrs {
        if !a.path().is_ident("builder") { continue; }

        a.parse_nested_meta(|meta| {
            // #[builder(default)] o #[builder(default = "expr")]
            if meta.path.is_ident("default") {
                if meta.input.peek(syn::Token![=]) {
                    let value = meta.value()?;
                    let s: LitStr = value.parse()?;
                    let expr: Expr = s.parse_with(syn::Expr::parse)
                        .map_err(|e| syn::Error::new(s.span(), e.to_string()))?;
                    attr.default = Some(DefaultKind::Expr(Box::new(expr)));
                } else {
                    attr.default = Some(DefaultKind::Trait);
                }
                return Ok(());
            }

            // #[builder(each = "método")]
            if meta.path.is_ident("each") {
                let value = meta.value()?;
                let s: LitStr = value.parse()?;
                attr.each = Some(Ident::new(&s.value(), s.span()));
                return Ok(());
            }

            Err(meta.error(
                "atributo builder desconocido; se esperaba `default` o `each`"
            ))
        })?;
    }

    Ok(attr)
}

// ── Helpers de tipo ────────────────────────────────────────────────────────

/// Devuelve el tipo interno T de Vec<T>, o None si no es Vec
fn tipo_interior_vec(ty: &Type) -> Option<&Type> {
    let Type::Path(tp) = ty else { return None; };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Vec" { return None; }
    let syn::PathArguments::AngleBracketed(ref args) = seg.arguments else { return None; };
    args.args.iter().find_map(|a| {
        if let syn::GenericArgument::Type(t) = a { Some(t) } else { None }
    })
}

// ── Entry point del proc-macro ─────────────────────────────────────────────

#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive_builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match implementar_builder(&input) {
        Ok(ts)  => ts.into(),
        Err(e)  => e.into_compile_error().into(),
    }
}

fn implementar_builder(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let nombre  = &input.ident;
    let vis     = &input.vis;
    let nombre_builder = format_ident!("{}Builder", nombre);

    // Solo soportamos structs con campos nombrados
    let Data::Struct(data_struct) = &input.data else {
        return Err(syn::Error::new_spanned(
            nombre,
            "Builder solo soporta structs con campos nombrados",
        ));
    };
    let Fields::Named(fields_named) = &data_struct.fields else {
        return Err(syn::Error::new_spanned(
            nombre,
            "Builder requiere campos nombrados (no tuple structs)",
        ));
    };

    // Parsear todos los campos con sus atributos
    let campos: Vec<CampoBuilder> = fields_named.named.iter()
        .map(|f| {
            let attr = parsear_attrs(f)?;
            Ok(CampoBuilder {
                ident: f.ident.clone().unwrap(),
                ty:    f.ty.clone(),
                attr,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    // ── Generar campos del Builder struct ────────────────────────────────
    let campos_struct: Vec<TokenStream2> = campos.iter().map(|c| {
        let ident = &c.ident;
        let ty    = &c.ty;
        if c.attr.each.is_some() {
            // Campos con `each`: el builder acumula un Vec
            quote! { #ident: #ty, }
        } else {
            // Campos normales: Option<T>
            quote! { #ident: ::core::option::Option<#ty>, }
        }
    }).collect();

    // ── Generar valor por defecto para cada campo (para Default impl) ───
    let defaults_struct: Vec<TokenStream2> = campos.iter().map(|c| {
        let ident = &c.ident;
        if c.attr.each.is_some() {
            quote! { #ident: ::std::vec::Vec::new(), }
        } else {
            quote! { #ident: ::core::option::Option::None, }
        }
    }).collect();

    // ── Generar setters ──────────────────────────────────────────────────
    let setters: Vec<TokenStream2> = campos.iter().map(|c| {
        let ident = &c.ident;
        let ty    = &c.ty;
        let span  = ident.span();

        if let Some(each_ident) = &c.attr.each {
            // each: setter que añade UN elemento (no reemplaza todo el Vec)
            let inner = tipo_interior_vec(ty).map_or_else(
                || quote_spanned!(span=> compile_error!("`each` requiere Vec<T>")),
                |t| quote! { #t },
            );
            quote_spanned! { span=>
                pub fn #each_ident(mut self, valor: #inner) -> Self {
                    self.#ident.push(valor);
                    self
                }
            }
        } else {
            // setter normal: envuelve en Some
            quote_spanned! { span=>
                pub fn #ident(mut self, valor: #ty) -> Self {
                    self.#ident = ::core::option::Option::Some(valor);
                    self
                }
            }
        }
    }).collect();

    // ── Generar cuerpo de build() ────────────────────────────────────────
    let build_campos: Vec<TokenStream2> = campos.iter().map(|c| {
        let ident = &c.ident;
        let span  = ident.span();

        if c.attr.each.is_some() {
            // Vec se mueve directamente
            quote_spanned! { span=> #ident: self.#ident, }
        } else {
            match &c.attr.default {
                Some(DefaultKind::Trait) => {
                    quote_spanned! { span=>
                        #ident: self.#ident.unwrap_or_default(),
                    }
                }
                Some(DefaultKind::Expr(expr)) => {
                    quote_spanned! { span=>
                        #ident: self.#ident.unwrap_or_else(|| #expr),
                    }
                }
                None => {
                    // Campo requerido: Err si no fue configurado
                    let msg = format!("campo `{}` es requerido", ident);
                    quote_spanned! { span=>
                        #ident: self.#ident.ok_or(#msg)?,
                    }
                }
            }
        }
    }).collect();

    // ── Código final generado ────────────────────────────────────────────
    Ok(quote! {
        // Builder struct
        #vis struct #nombre_builder {
            #(#campos_struct)*
        }

        // Default para #nombre_builder
        impl ::core::default::Default for #nombre_builder {
            fn default() -> Self {
                Self {
                    #(#defaults_struct)*
                }
            }
        }

        // Métodos del Builder
        impl #nombre_builder {
            #(#setters)*

            pub fn build(self) -> ::core::result::Result<#nombre, ::std::string::String> {
                ::core::result::Result::Ok(#nombre {
                    #(#build_campos)*
                })
            }
        }

        // Método estático builder() en la struct original
        impl #nombre {
            pub fn builder() -> #nombre_builder {
                #nombre_builder::default()
            }
        }
    })
}
```

## Re-export y uso: `builder/src/lib.rs`

```rust
// Re-exporta el derive macro para que los usuarios solo dependan de `builder`
pub use builder_macro::Builder;

// Aquí podrías añadir tipos helpers (BuilderError, etc.)
```

## Ejemplo de uso: `builder-tests/src/lib.rs`

```rust
use builder::Builder;

#[derive(Debug, PartialEq, Builder)]
struct Configuracion {
    // Campo requerido: build() falla si no se llama a .host()
    host: String,

    // Campo opcional con valor por defecto literal
    #[builder(default = "8080")]
    puerto: u16,

    // Campo opcional que usa Default::default() del tipo
    #[builder(default)]
    reintentos: u32,  // → 0u32

    // Vec acumulativo: se llama .cabecera() varias veces
    #[builder(each = "cabecera")]
    cabeceras: Vec<String>,
}

#[test]
fn construccion_completa() {
    let cfg = Configuracion::builder()
        .host("api.ejemplo.com".to_string())
        .puerto(9090)
        .reintentos(3)
        .cabecera("X-Request-ID: abc".to_string())
        .cabecera("Accept: application/json".to_string())
        .build()
        .unwrap();

    assert_eq!(cfg.host,       "api.ejemplo.com");
    assert_eq!(cfg.puerto,     9090);
    assert_eq!(cfg.reintentos, 3);
    assert_eq!(cfg.cabeceras.len(), 2);
}

#[test]
fn valores_por_defecto() {
    let cfg = Configuracion::builder()
        .host("localhost".to_string())
        .build()
        .unwrap();

    assert_eq!(cfg.puerto,     8080);     // default = "8080"
    assert_eq!(cfg.reintentos, 0);        // Default::default()
    assert!(cfg.cabeceras.is_empty());    // Vec vacío
}

#[test]
fn campo_requerido_faltante() {
    let resultado = Configuracion::builder().build();
    assert!(resultado.is_err());
    assert!(resultado.unwrap_err().contains("host"));
}

#[test]
fn each_acumula_elementos() {
    let cfg = Configuracion::builder()
        .host("x.com".to_string())
        .cabecera("A: 1".to_string())
        .cabecera("B: 2".to_string())
        .cabecera("C: 3".to_string())
        .build()
        .unwrap();

    assert_eq!(cfg.cabeceras, vec!["A: 1", "B: 2", "C: 3"]);
}

// ── Builder funciona con tipos genéricos ──────────────────────────────────

#[derive(Debug, Builder)]
struct Paginado<T> {
    datos: Vec<T>,
    #[builder(default = "1")]
    pagina: usize,
    #[builder(default = "20")]
    por_pagina: usize,
}

#[test]
fn builder_con_genericos() {
    let p = Paginado::<String>::builder()
        .datos(vec!["a".to_string(), "b".to_string()])
        .pagina(3)
        .build()
        .unwrap();

    assert_eq!(p.pagina, 3);
    assert_eq!(p.por_pagina, 20);  // default
}
```

## Tests de errores de compilación con `trybuild`

`trybuild` verifica que cierto código **no compila** y que el mensaje de error
es exactamente el esperado:

```bash
cargo add --dev trybuild
```

**`tests/compile_fail.rs`:**

```rust
#[test]
fn compile_fail_tests() {
    // Ejecuta todos los archivos .rs en tests/ui/
    // y verifica que fallan con el mensaje en el .stderr correspondiente
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
```

**`tests/ui/campo_invalido.rs`:**

```rust
use builder::Builder;

#[derive(Builder)]
struct Malo {
    #[builder(atributo_inventado)]  // ← debe fallar
    campo: String,
}

fn main() {}
```

**`tests/ui/campo_invalido.stderr`:**

```
error: atributo builder desconocido; se esperaba `default` o `each`
 --> tests/ui/campo_invalido.rs:5:14
  |
5 |     #[builder(atributo_inventado)]
  |               ^^^^^^^^^^^^^^^^^^
```

### Debugging con `cargo expand`

```bash
# Instalar
cargo install cargo-expand

# Ver qué genera el macro para tu struct
cargo expand --bin mi-app 2>/dev/null | grep -A 50 "impl ConfiguracionBuilder"

# Salida (ejemplo para Configuracion):
impl ConfiguracionBuilder {
    pub fn host(mut self, valor: String) -> Self {
        self.host = Some(valor);
        self
    }
    pub fn puerto(mut self, valor: u16) -> Self {
        self.puerto = Some(valor);
        self
    }
    pub fn cabecera(mut self, valor: String) -> Self {
        self.cabeceras.push(valor);
        self
    }
    pub fn build(self) -> Result<Configuracion, String> {
        Ok(Configuracion {
            host: self.host.ok_or("campo `host` es requerido")?,
            puerto: self.puerto.unwrap_or_else(|| 8080),
            reintentos: self.reintentos.unwrap_or_default(),
            cabeceras: self.cabeceras,
        })
    }
}
```

---

# RUTA C: Monorepo Workspace — arquitectura limpia

## Anatomía del workspace

```text
mi-ecosistema/
├── Cargo.toml              ← raíz del workspace
├── Cargo.lock              ← UN solo lock file para todo el workspace
│
├── crates/
│   ├── core/               ← dominio puro: traits, tipos, errores
│   │   ├── Cargo.toml      ← sin I/O, sin async (o async mínimo)
│   │   └── src/lib.rs
│   │
│   ├── db/                 ← implementa traits de core con SQLx
│   │   ├── Cargo.toml      ← depende de: core, sqlx, tracing
│   │   └── src/lib.rs
│   │
│   ├── api/                ← handlers Axum, usa core + db
│   │   ├── Cargo.toml      ← depende de: core, db, axum, tower
│   │   └── src/lib.rs
│   │
│   └── cli/                ← binario clap, usa core + api (como cliente HTTP)
│       ├── Cargo.toml      ← depende de: core, clap, reqwest
│       └── src/main.rs
│
├── xtask/                  ← automatización del build (no es un crate normal)
│   ├── Cargo.toml
│   └── src/main.rs
│
└── .cargo/
    └── config.toml         ← alias: [alias] xtask = "run --package xtask --"
```

```text
GRAFO DE DEPENDENCIAS (SIN CICLOS — OBLIGATORIO):

  core  ←── db  ←── api ←── (binario api-server)
   ↑                  ↑
   └──────────────── cli (usa core directamente, llama api por HTTP)

PROHIBIDO:
  core → db    (core no sabe nada de persistencia)
  db   → api   (db no sabe nada de HTTP)
  api  → cli   (api no sabe nada de CLI)
```

## `Cargo.toml` raíz — el corazón del workspace

```toml
[workspace]
resolver = "2"   # OBLIGATORIO: feature unification correcta en Rust >= 1.51
members  = [
    "crates/core",
    "crates/db",
    "crates/api",
    "crates/cli",
    "xtask",
]

# ── Versiones centralizadas de dependencias ──────────────────────────────
# Los crates del workspace heredan con: serde.workspace = true
[workspace.dependencies]
tokio        = { version = "1",   features = ["full"] }
axum         = "0.7"
sqlx         = { version = "0.8", features = ["postgres", "runtime-tokio", "tls-rustls", "uuid", "chrono"] }
serde        = { version = "1",   features = ["derive"] }
serde_json   = "1"
thiserror    = "2"
anyhow       = "1"
tracing      = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
uuid         = { version = "1",   features = ["v4", "serde"] }
clap         = { version = "4",   features = ["derive"] }
reqwest      = { version = "0.12", features = ["json"] }

# ── Lints unificados para todo el workspace ──────────────────────────────
[workspace.lints.rust]
unsafe_code                 = "forbid"   # no unsafe sin revisión explícita
unused_imports              = "warn"
unused_variables            = "warn"

[workspace.lints.clippy]
# Estilo
pedantic                    = { level = "warn", priority = -1 }
# Excepciones comunes a pedantic
must_use_candidate          = "allow"
missing_errors_doc          = "allow"
missing_panics_doc          = "allow"
module_name_repetitions     = "allow"
# Seguridad
unwrap_used                 = "warn"    # usar ? o expect con mensaje
expect_used                 = "allow"   # OK si el mensaje explica el invariante

# ── Perfiles unificados ───────────────────────────────────────────────────
[profile.release]
lto           = "thin"  # thin LTO: buen balance velocidad/tiempo de compilación
codegen-units = 1
strip         = "symbols"
opt-level     = 3

[profile.dev]
opt-level = 1           # más rápido que 0, más rápido de compilar que 2/3

# Perfil para CI: más rápido de compilar, sin optimizaciones pesadas
[profile.ci]
inherits      = "dev"
opt-level     = 0
debug         = 0
```

## `crates/core/Cargo.toml` y `src/lib.rs`

```toml
[package]
name    = "mi-core"
version = "0.1.0"
edition = "2021"

[lints]
workspace = true   # hereda workspace.lints

[dependencies]
serde.workspace     = true   # hereda versión y features de workspace
thiserror.workspace = true
uuid.workspace      = true
async-trait = "0.1"
```

```rust
// crates/core/src/lib.rs
// Core: SOLO dominio puro. Sin I/O, sin HTTP, sin SQL.
// La única razón para cambiar este crate es un cambio en las reglas de negocio.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ── Tipos de dominio ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UrlId(Uuid);

impl UrlId {
    pub fn nueva() -> Self { UrlId(Uuid::new_v4()) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlCorta {
    pub id:     UrlId,
    pub codigo: String,
    pub target: String,
    pub clicks: u64,
}

// ── Errores de dominio ────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ErrorDominio {
    #[error("URL con código '{0}' no encontrada")]
    NoEncontrada(String),
    #[error("código de URL inválido: '{0}'")]
    CodigoInvalido(String),
    #[error("URL ya existe con código '{0}'")]
    Duplicada(String),
    #[error("error de repositorio: {0}")]
    Repositorio(String),
}

// ── Traits (interfaces) ───────────────────────────────────────────────────

// core define LA INTERFAZ; db, api etc. implementan o usan la interfaz.
// Nunca al revés.
#[async_trait]
pub trait RepositorioUrls: Send + Sync + 'static {
    async fn guardar(&self, url: &UrlCorta) -> Result<(), ErrorDominio>;
    async fn buscar_por_codigo(&self, codigo: &str) -> Result<UrlCorta, ErrorDominio>;
    async fn incrementar_clicks(&self, codigo: &str) -> Result<u64, ErrorDominio>;
    async fn listar(&self, limite: u32) -> Result<Vec<UrlCorta>, ErrorDominio>;
}

// ── Lógica de negocio pura ────────────────────────────────────────────────

pub fn validar_codigo(codigo: &str) -> Result<(), ErrorDominio> {
    if codigo.is_empty() || codigo.len() > 32 {
        return Err(ErrorDominio::CodigoInvalido(codigo.to_string()));
    }
    if !codigo.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(ErrorDominio::CodigoInvalido(codigo.to_string()));
    }
    Ok(())
}

pub struct ServicioUrls<R: RepositorioUrls> {
    repo: R,
}

impl<R: RepositorioUrls> ServicioUrls<R> {
    pub fn nuevo(repo: R) -> Self { ServicioUrls { repo } }

    pub async fn crear(&self, codigo: String, target: String) -> Result<UrlCorta, ErrorDominio> {
        validar_codigo(&codigo)?;
        let url = UrlCorta { id: UrlId::nueva(), codigo, target, clicks: 0 };
        self.repo.guardar(&url).await?;
        Ok(url)
    }

    pub async fn redirigir(&self, codigo: &str) -> Result<String, ErrorDominio> {
        let url = self.repo.buscar_por_codigo(codigo).await?;
        self.repo.incrementar_clicks(codigo).await?;
        Ok(url.target)
    }
}
```

## `crates/api/src/lib.rs` — usando los traits de core

```rust
// crates/api/Cargo.toml depende de: mi-core (path), axum, tokio, serde, tracing

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Redirect},
    Router,
    routing::{get, post},
};
use mi_core::{ErrorDominio, RepositorioUrls, ServicioUrls};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// El estado de Axum contiene el servicio (DI dinámico para testabilidad)
#[derive(Clone)]
pub struct EstadoApp {
    pub servicio: Arc<ServicioUrls<Arc<dyn RepositorioUrls>>>,
}

#[derive(Deserialize)]
pub struct CrearRequest {
    pub codigo: String,
    pub target: String,
}

#[derive(Serialize)]
pub struct CrearResponse {
    pub codigo:    String,
    pub short_url: String,
}

pub async fn crear_url(
    State(estado): State<EstadoApp>,
    Json(body): Json<CrearRequest>,
) -> impl IntoResponse {
    match estado.servicio.crear(body.codigo.clone(), body.target).await {
        Ok(url) => Json(CrearResponse {
            short_url: format!("/{}", url.codigo),
            codigo:    url.codigo,
        }).into_response(),
        Err(ErrorDominio::CodigoInvalido(c)) =>
            (StatusCode::BAD_REQUEST, c).into_response(),
        Err(ErrorDominio::Duplicada(c)) =>
            (StatusCode::CONFLICT, format!("código '{c}' ya existe")).into_response(),
        Err(e) =>
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn redirigir_url(
    State(estado): State<EstadoApp>,
    Path(codigo): Path<String>,
) -> impl IntoResponse {
    match estado.servicio.redirigir(&codigo).await {
        Ok(target)                        => Redirect::temporary(&target).into_response(),
        Err(ErrorDominio::NoEncontrada(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(e)                            =>
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub fn crear_router(estado: EstadoApp) -> Router {
    Router::new()
        .route("/url", post(crear_url))
        .route("/{codigo}", get(redirigir_url))
        .with_state(estado)
}
```

## `xtask/src/main.rs` — automatización del build

```rust
// .cargo/config.toml:
// [alias]
// xtask = "run --package xtask --"
//
// Uso: cargo xtask codegen
//      cargo xtask release --version 1.2.3
//      cargo xtask docker --push

use std::process::Command;

fn main() {
    let mut args = std::env::args().skip(1);
    let tarea = args.next().unwrap_or_else(|| "help".to_string());

    match tarea.as_str() {
        "codegen"  => codegen(),
        "release"  => release(args.next().expect("--version X.Y.Z")),
        "docker"   => docker(args.any(|a| a == "--push")),
        "lint"     => lint(),
        _          => {
            eprintln!("Tareas disponibles: codegen, release, docker, lint");
            std::process::exit(1);
        }
    }
}

fn sh(cmd: &str, args: &[&str]) {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("no se puede ejecutar {cmd}: {e}"));
    if !status.success() {
        panic!("{cmd} falló con código: {:?}", status.code());
    }
}

fn codegen() {
    println!("→ Generando tipos desde esquema OpenAPI...");
    // Ejemplo: usar utoipa para exportar OpenAPI spec
    sh("cargo", &["run", "--package", "mi-api", "--bin", "export-openapi"]);
    sh("npx", &["@openapitools/openapi-generator-cli", "generate",
        "-i", "openapi.json", "-g", "typescript-fetch", "-o", "frontend/src/api"]);
}

fn release(version: String) {
    println!("→ Preparando release v{version}...");
    sh("cargo", &["test", "--workspace"]);
    sh("cargo", &["clippy", "--workspace", "--", "-D", "warnings"]);
    // Actualizar versiones en todos los Cargo.toml...
    println!("✅ Listo para tag v{version}");
}

fn docker(push: bool) {
    println!("→ Construyendo imágenes Docker...");
    sh("docker", &["build", "-f", "docker/Dockerfile.api", "-t", "mi-api:latest", "."]);
    if push {
        sh("docker", &["push", "mi-api:latest"]);
    }
}

fn lint() {
    println!("→ Ejecutando lints...");
    sh("cargo", &["clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]);
    sh("cargo", &["fmt", "--", "--check"]);
    // cargo hack: verificar que cada combinación de features compila
    sh("cargo", &["hack", "check", "--workspace", "--each-feature", "--no-dev-deps"]);
}
```

## `cargo hack` y `cargo deny`

```bash
# cargo hack: instalar
cargo install cargo-hack

# Verificar que cada feature individual compila (sin combinaciones rotas)
cargo hack check --workspace --each-feature --no-dev-deps

# Verificar que todas las combinaciones de features compilan
cargo hack check --workspace --feature-powerset --no-dev-deps

# cargo deny: políticas de licencias, bans, fuentes de crates
cargo install cargo-deny
```

**`deny.toml`:**

```toml
[advisories]
ignore = []         # no ignorar vulnerabilidades conocidas

[licenses]
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-DFS-2016",
    "CC0-1.0",
]
# deny = ["GPL-3.0"]  # si necesitas evitar ciertas licencias

[bans]
# Prohibir crates conflictivos o inseguros
deny = [
    { name = "openssl" },   # preferimos rustls
]
# Máximo 1 versión de crates críticos (evitar duplicados)
multiple-versions = "warn"

[sources]
unknown-registry = "deny"   # solo crates.io
unknown-git      = "deny"   # sin dependencias git sin aprobación
```

```bash
# CI
cargo deny check
```

---

## Tests comunes a las tres rutas

```rust
// tests/integracion.rs — aplicable a cualquier ruta

// RUTA B: verificar que el macro genera código correcto
#[cfg(test)]
mod proc_macro_tests {
    use builder::Builder;

    #[derive(Builder, Debug, PartialEq)]
    struct Ejemplo {
        requerido: String,
        #[builder(default = "42")]
        opcional: u32,
    }

    #[test]
    fn build_exitoso() {
        let e = Ejemplo::builder()
            .requerido("hola".to_string())
            .build()
            .unwrap();
        assert_eq!(e.requerido, "hola");
        assert_eq!(e.opcional,  42);
    }

    #[test]
    fn build_falla_sin_requerido() {
        let res = Ejemplo::builder().build();
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("requerido"));
    }
}

// RUTA C: verificar que el core no tiene dependencias de I/O
#[cfg(test)]
mod arquitectura_tests {
    #[test]
    fn validar_codigo_correcto() {
        assert!(mi_core::validar_codigo("mi-url").is_ok());
        assert!(mi_core::validar_codigo("abc123").is_ok());
    }

    #[test]
    fn validar_codigo_incorrecto() {
        assert!(mi_core::validar_codigo("").is_err());
        assert!(mi_core::validar_codigo("con espacio").is_err());
        assert!(mi_core::validar_codigo(&"x".repeat(33)).is_err());
    }
}
```

---

## ✅ Checklist de la Semana 20

### Checklist común (independiente de la ruta)

- [ ] Elegí UNA ruta y la implementé al 100%. No hay implementaciones parciales
  de varias rutas.
- [ ] El proyecto compila en `cargo build --release` sin warnings.
- [ ] `cargo test --workspace` (o `cargo test` si es un solo crate) pasa todos
  los tests.
- [ ] El código no tiene `clippy` warnings: `cargo clippy -- -D warnings`.
- [ ] Documenté el artefacto con un `README.md` que explica: propósito,
  dependencias del sistema, cómo compilar y cómo ejecutar/testear.

### Ruta A: Embedded (Embassy)

- [ ] El binario compila para el target embebido sin warnings:
  `cargo build --release --target thumbv6m-none-eabi` (RP2040) o
  `--target thumbv7em-none-eabihf` (STM32).
- [ ] Hay al menos 3 Embassy tasks con responsabilidades distintas
  (sensor, display/output, blinky).
- [ ] La comunicación entre tasks usa `Channel<M, T, N>` de `embassy-sync`,
  no variables globales con `Mutex`.
- [ ] `defmt` logs en cada task con nivel adecuado (`info!`, `debug!`,
  `error!`). No `println!` ni `eprintln!`.
- [ ] El `panic_handler` cierra el dispositivo de forma segura (no loop
  infinito que activa el watchdog).
- [ ] CI compila en GitHub Actions para el target embebido.

### Ruta B: Procedural Macros

- [ ] El macro `#[derive(Builder)]` soporta: campos requeridos, `default`,
  `default = "expr"` y `each = "método"`.
- [ ] Errores del macro apuntan al código del usuario (span correcto),
  no al código generado.
- [ ] `cargo expand` muestra el código generado correctamente. Revisé la
  expansión para al menos un struct complejo.
- [ ] Tests de `trybuild` verifican que código inválido produce el error
  esperado (al menos 2 casos compile-fail).
- [ ] 5 tests de corrección pasan: construcción completa, valores por
  defecto, campo requerido faltante, `each` acumulativo, struct genérico.
- [ ] El macro está en un crate separado (`proc-macro = true`). El crate
  de usuario re-exporta el derive.

### Ruta C: Monorepo Workspace

- [ ] `[workspace]` con `resolver = "2"`. Todos los crates del equipo
  están en `members`.
- [ ] `[workspace.dependencies]` centraliza todas las versiones.
  Ningún crate duplica una versión diferente de una dependencia compartida.
- [ ] `[workspace.lints]` define las políticas de lint. `unsafe_code = "forbid"`
  a menos que el crate específicamente lo permita con `[lints] unsafe_code = "allow"`.
- [ ] El grafo de dependencias no tiene ciclos. `core` no depende de `db`,
  `db` no depende de `api`, etc.
- [ ] `cargo hack check --workspace --each-feature` pasa sin errores.
- [ ] `cargo deny check` pasa: licencias aprobadas, sin vulnerabilidades
  conocidas sin ignorar explícitamente.
- [ ] `cargo xtask lint` automatiza: `clippy`, `fmt --check`, `hack check`.
- [ ] `cargo build --release --workspace` genera los binarios finales.

> **Siguiente sección:** [Mes 6 — Proyecto Capstone](../chapter_06/section_00.md)
