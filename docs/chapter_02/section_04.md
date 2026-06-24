# Testing, Documentación y Tooling Profesional

La Semana 8 cierra el Mes 2. Hasta ahora has escrito código que funciona; ahora
aprenderás a **demostrar que funciona** de forma sistemática, a documentarlo para que
otros (y tú mismo en seis meses) lo entiendan, y a configurar las herramientas que
mantienen la calidad a lo largo del tiempo.

En esta sección aprenderemos:

- La **pirámide de testing** de Rust: unit, integration, doc y benchmarks.
- **Unit tests**: acceso a código privado, aserciones, tests de panic, tests que
  devuelven `Result`.
- **Integration tests**: crates separadas, helpers compartidos.
- **Doc tests**: ejemplos en documentación que el compilador verifica.
- **Benchmarks** con `criterion`: metodología estadística y `black_box`.
- Configuración avanzada de **Clippy** y **rustfmt**.
- El proyecto integrador del Mes 2.

> 💡 **Filosofía de la Semana 8:** *Un test que no falla cuando el código es incorrecto
> no sirve de nada. Un test que falla cuando el código es correcto molesta. El objetivo
> es la especificidad: cada test verifica exactamente una cosa, con el mínimo de
> infraestructura posible.*

---

## La pirámide de testing

```text
        ▲
       /▲\        Benchmarks  — benches/*.rs  — cargo bench
      / ▲ \       (lentos, estadísticos, comparativos)
     /  ▲  \
    /   ▲   \     Doc tests   — /// ```rust    — cargo test --doc
   /    ▲    \    (compilan los ejemplos de documentación)
  /     ▲     \
 /      ▲      \  Integration — tests/*.rs     — cargo test
/       ▲       \ (solo API pública, crate separada)
────────▲────────
        ▲          Unit tests  — src/**/*.rs   — cargo test
                  (acceso privado, rápidos, numerosos)
```

| Nivel | Ubicación | Acceso | Velocidad | Cuántos |
| :--- | :--- | :--- | :--- | :--- |
| **Unit** | `src/` dentro de `#[cfg(test)]` | Privado y público | Muy rápido | Muchos |
| **Integration** | `tests/*.rs` | Solo público | Rápido | Pocos (contratos) |
| **Doc** | `///` en el código fuente | Solo público | Medio | Uno por función |
| **Benchmark** | `benches/*.rs` | Solo público | Lento | Solo regresiones |

---

## Unit tests

### Estructura básica

```rust
// src/matematicas.rs

pub fn sumar(a: i32, b: i32) -> i32 { a + b }
pub fn dividir(a: f64, b: f64) -> f64 { a / b }

fn doble_interno(x: i32) -> i32 { x * 2 }  // función privada

#[cfg(test)]        // este bloque solo existe cuando se compila para tests
mod tests {
    use super::*;   // importa todo del módulo padre, INCLUDING privados

    #[test]
    fn suma_positivos() {
        assert_eq!(sumar(2, 3), 5);
    }

    #[test]
    fn suma_negativos() {
        assert_eq!(sumar(-1, -1), -2);
    }

    #[test]
    fn test_privado() {
        // ✅ Los unit tests pueden llamar funciones privadas
        assert_eq!(doble_interno(5), 10);
    }
}
```

Ejecutar:

```bash
cargo test                          # todos los tests
cargo test suma                     # tests cuyo nombre contiene "suma"
cargo test tests::suma_positivos    # test exacto por ruta
cargo test -- --nocapture           # mostrar println! dentro de tests
cargo test -- --test-threads=1      # sin paralelismo (para tests con estado global)
```

### Macros de aserción

```rust
#[test]
fn demo_aserciones() {
    // Igualdad / desigualdad — muestran ambos valores al fallar
    assert_eq!(2 + 2, 4);
    assert_ne!(2 + 2, 5);

    // Condición booleana
    assert!(4 > 3);
    assert!(!false);

    // Con mensaje personalizado (formato como println!)
    let x = 7;
    assert!(x % 2 != 0, "se esperaba que {x} fuera impar");
    assert_eq!(x, 7, "x debería ser 7, pero es {x}");

    // Comparación aproximada para flotantes
    let pi = std::f64::consts::PI;
    assert!((pi - 3.14159).abs() < 0.001);
}
```

### Tests que deben hacer panic

```rust
fn dividir_enteros(a: i32, b: i32) -> i32 {
    if b == 0 { panic!("división por cero"); }
    a / b
}

#[cfg(test)]
mod tests {
    use super::*;

    // #[should_panic] sin expected: cualquier panic pasa
    #[test]
    #[should_panic]
    fn falla_con_divisor_cero_basic() {
        dividir_enteros(10, 0);
    }

    // expected: el mensaje del panic debe CONTENER el string dado
    #[test]
    #[should_panic(expected = "división por cero")]
    fn falla_con_divisor_cero_mensaje() {
        dividir_enteros(5, 0);
    }

    // ❗ Si el código NO hace panic, el test FALLA (aunque haya expected)
    // ❗ Si el mensaje no contiene el substring, también falla
}
```

### Tests que devuelven `Result`

Un test puede devolver `Result<(), E>`. Si devuelve `Err`, el test falla con el
mensaje de error formateado. Esto permite usar `?` dentro del test:

```rust
use std::num::ParseIntError;

fn parsear(s: &str) -> Result<i32, ParseIntError> {
    s.trim().parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parseo_valido() -> Result<(), ParseIntError> {
        let n = parsear("42")?;   // ? dentro del test: falla con error si Err
        assert_eq!(n, 42);
        Ok(())
    }

    #[test]
    fn parseo_invalido_es_err() {
        assert!(parsear("abc").is_err());
    }
}
```

### Organizar tests en submódulos

```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod suma {
        use super::*;
        #[test] fn positivos() { assert_eq!(sumar(1, 2), 3); }
        #[test] fn cero()      { assert_eq!(sumar(0, 0), 0); }
        #[test] fn negativos() { assert_eq!(sumar(-1, -2), -3); }
    }

    mod division {
        use super::*;
        #[test] fn exacta()   { assert_eq!(sumar(10, 5), 15); }
        // cargo test tests::division::exacta — ruta completa
    }
}
```

### Ignorar tests con `#[ignore]`

```rust
#[test]
#[ignore = "requiere conexión a base de datos externa"]
fn test_bd_real() {
    // ...
}
```

```bash
cargo test                  # omite los #[ignore]
cargo test -- --ignored     # solo los #[ignore]
cargo test -- --include-ignored  # todos
```

---

## Integration tests

### Estructura de archivos

```
mi_libreria/
├── Cargo.toml
├── src/
│   └── lib.rs          ← código de la librería
└── tests/
    ├── comun/
    │   └── mod.rs      ← helpers compartidos (NO es un test en sí)
    ├── api_basica.rs   ← test de integración 1
    └── casos_borde.rs  ← test de integración 2
```

Cada archivo en `tests/` es una **crate separada** que linkea la librería. Solo puede
acceder a su **API pública**.

### `tests/comun/mod.rs`: helpers compartidos

```rust
// tests/comun/mod.rs
// Convención: usar mod.rs dentro de un directorio para que Cargo
// no lo compile como test independiente (si fuera tests/comun.rs sí lo haría)

use std::path::PathBuf;
use std::fs;

pub struct TestDir {
    pub ruta: PathBuf,
}

impl TestDir {
    pub fn nueva() -> Self {
        let ruta = std::env::temp_dir().join(format!("test_{}", uuid_simple()));
        fs::create_dir_all(&ruta).expect("no se pudo crear dir de test");
        Self { ruta }
    }

    pub fn archivo(&self, nombre: &str) -> PathBuf {
        self.ruta.join(nombre)
    }

    pub fn escribir(&self, nombre: &str, contenido: &str) -> PathBuf {
        let ruta = self.archivo(nombre);
        fs::write(&ruta, contenido).expect("no se pudo escribir archivo de test");
        ruta
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.ruta); // limpia al terminar el test
    }
}

fn uuid_simple() -> String {
    // Identificador único simple sin dependencias externas
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
    format!("{t:x}")
}
```

### `tests/api_basica.rs`: test de integración

```rust
// tests/api_basica.rs
mod comun;   // importa tests/comun/mod.rs

use mi_libreria::Calculadora;   // solo API pública

#[test]
fn suma_desde_api_publica() {
    let calc = Calculadora::nueva();
    assert_eq!(calc.sumar(3, 4), 7);
}

#[test]
fn historial_se_acumula() {
    let mut calc = Calculadora::nueva();
    calc.sumar(1, 2);
    calc.sumar(3, 4);
    assert_eq!(calc.historial().len(), 2);
}

#[test]
fn cargar_desde_archivo() {
    let dir = comun::TestDir::nueva();
    let archivo = dir.escribir("ops.txt", "sumar 1 2\nsumar 3 4\n");

    let mut calc = Calculadora::nueva();
    calc.ejecutar_archivo(&archivo).expect("falló la carga");
    assert_eq!(calc.historial().len(), 2);
}
```

### Diferencias clave entre unit e integration

```rust
// UNIT TEST: puede acceder a internals
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prueba_funcion_privada() {
        assert_eq!(helper_interno(5), 10);  // ✅ acceso privado
    }
}

// INTEGRATION TEST (tests/mi_test.rs):
use mi_libreria::FuncionPublica;
// helper_interno(5)  // ❌ no compila: es privada, crate separada
```

---

## Doc tests

Los doc tests convierten los ejemplos de la documentación en tests ejecutables.
Garantizan que lo que documentas realmente funciona.

### Estructura de documentación

```rust
/// Calcula la raíz cuadrada entera de `n`.
///
/// Devuelve el mayor entero `k` tal que `k² ≤ n`.
///
/// # Examples
///
/// ```
/// use mi_libreria::sqrt_entera;
///
/// assert_eq!(sqrt_entera(9), 3);
/// assert_eq!(sqrt_entera(10), 3);   // 3² = 9 ≤ 10 < 16 = 4²
/// assert_eq!(sqrt_entera(0), 0);
/// ```
///
/// # Panics
///
/// No hace panic para ningún valor de `u64`.
///
/// # Examples (more)
///
/// ```
/// # use mi_libreria::sqrt_entera;  // línea con # se ejecuta pero no aparece en docs
/// let grande: u64 = 1_000_000;
/// assert_eq!(sqrt_entera(grande), 1000);
/// ```
pub fn sqrt_entera(n: u64) -> u64 {
    (n as f64).sqrt() as u64
}
```

### Modificadores de bloques de código

````markdown
```rust
// Normal: compila Y ejecuta. El que ves en la documentación.
assert_eq!(2 + 2, 4);
```

```rust,no_run
// Compila pero NO ejecuta (útil para ejemplos que necesitan red, archivos, etc.)
let _conn = conectar_a_bd("postgres://usuario:pass@localhost/db");
```

```rust,ignore
// No compila ni ejecuta (documenta sintaxis conceptual, APIs de otras crates, etc.)
// código de ejemplo que no está pensado para ejecutarse
mi_crate::funcion_que_no_existe();
```

```rust,should_panic
// El bloque debe hacer panic para que el test pase
let v: Vec<i32> = vec![];
let _ = v[99];   // index out of bounds → panic esperado
```

```rust,compile_fail
// El bloque NO debe compilar (útil para documentar errores del compilador)
let x: i32 = "esto no es un número";
```
````

### El prefijo `#` para ocultar líneas

Las líneas que empiezan con `# ` (almohadilla + espacio) se ejecutan pero no aparecen
en la documentación renderizada:

```rust
/// ```
/// # // Setup que el lector no necesita ver
/// # use mi_libreria::Cache;
/// # let mut cache = Cache::nuevo(std::time::Duration::from_secs(60));
/// cache.insertar("clave", 42);
/// assert_eq!(cache.obtener(&"clave"), Some(&42));
/// ```
```

### Ejecutar doc tests

```bash
cargo test --doc                    # solo doc tests
cargo test --doc -- sqrt_entera     # doc tests de una función específica
cargo doc --open                    # ver la documentación renderizada
```

### Secciones estándar de documentación

```rust
/// Breve descripción en una línea.
///
/// Descripción más larga si es necesaria. Puede tener varios párrafos.
/// Soporta **negrita**, *cursiva*, `código` y [enlaces](https://rust-lang.org).
///
/// # Examples
///
/// Código que el lector puede copiar y ejecutar.
///
/// # Panics
///
/// Cuándo y por qué puede hacer panic (aunque la firma no lo indique).
///
/// # Errors
///
/// Si la función devuelve `Result`, describe cada variante de error.
///
/// # Safety
///
/// Solo para funciones `unsafe`: qué invariantes debe garantizar el caller.
pub fn mi_funcion() {}

/// Documenta un módulo completo.
///
/// Aparece en la página del módulo en `cargo doc`.
pub mod mi_modulo {}

//! Documentación del crate/módulo actual (con `!`, no `///`).
//! Aparece en la página raíz de `cargo doc`.
```

---

## Benchmarks con `criterion`

`criterion` es el estándar de facto para benchmarks en Rust. Usa metodología
estadística (warmup, múltiples muestras, detección de outliers) para dar resultados
fiables.

### Setup

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "mi_benchmark"
harness = false          # criterion usa su propio harness, no el de cargo test
```

### Estructura básica

```rust
// benches/mi_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mi_libreria::{busqueda_lineal, busqueda_binaria};

// Un benchmark simple
fn bench_suma(c: &mut Criterion) {
    c.bench_function("suma_simple", |b| {
        b.iter(|| {
            // black_box: impide que el compilador optimice el resultado
            // como "dead code" o lo precalcule en compile-time
            black_box(black_box(2) + black_box(3))
        })
    });
}

// Comparar dos implementaciones con el mismo input
fn bench_busqueda(c: &mut Criterion) {
    let datos: Vec<i32> = (0..1000).collect();
    let objetivo = black_box(750);

    let mut grupo = c.benchmark_group("busqueda");

    grupo.bench_function("lineal", |b| {
        b.iter(|| busqueda_lineal(&datos, objetivo))
    });

    grupo.bench_function("binaria", |b| {
        b.iter(|| busqueda_binaria(&datos, objetivo))
    });

    grupo.finish();
}

// Benchmarks parametrizados: mismo benchmark con distintos tamaños
fn bench_ordenar(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("ordenar");

    for tamaño in [100, 1_000, 10_000].iter() {
        grupo.bench_with_input(
            BenchmarkId::from_parameter(tamaño),
            tamaño,
            |b, &n| {
                b.iter_batched(
                    || (0..n).rev().collect::<Vec<i32>>(),  // setup: crea datos frescos
                    |mut v| { v.sort(); black_box(v) },      // medición: ordena
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    grupo.finish();
}

criterion_group!(benches, bench_suma, bench_busqueda, bench_ordenar);
criterion_main!(benches);
```

### Ejecutar benchmarks

```bash
cargo bench                             # todos los benchmarks
cargo bench -- busqueda                 # solo grupos que contienen "busqueda"
cargo bench -- --save-baseline antes    # guarda una baseline para comparar
# (tras un cambio):
cargo bench -- --baseline antes         # compara contra la baseline guardada
```

### Interpretar resultados

```
busqueda/lineal         time:   [12.345 µs 12.401 µs 12.458 µs]
                        change: [-5.2341% -4.8901% -4.5231%] (p = 0.00 < 0.05)
                        Performance has improved.

busqueda/binaria        time:   [0.9823 µs 0.9871 µs 0.9921 µs]
```

- Los tres valores son el intervalo de confianza (bajo, estimado, alto).
- `change` solo aparece cuando hay una baseline de comparación.
- `p < 0.05`: el cambio es estadísticamente significativo.

### `black_box`: por qué es imprescindible

```rust
// ❌ MAL: el compilador puede precalcular 2+3=5 en compile-time
// El benchmark mide prácticamente cero
b.iter(|| 2 + 3);

// ✅ BIEN: black_box impide optimizaciones que anulan la medición
b.iter(|| black_box(2) + black_box(3));

// ✅ También: si el resultado no se usa, el compilador lo elimina
b.iter(|| {
    let v: Vec<i32> = (0..100).collect();
    black_box(v)    // fuerza al compilador a "materializar" el Vec
});
```

---

## Configuración avanzada de Clippy

Clippy tiene más de 700 lints categorizados. Puedes configurarlos por proyecto.

### `Cargo.toml` (recomendado para proyectos)

```toml
[workspace.lints.clippy]
# Convertir en error — nunca en producción
unwrap_used        = "deny"     # usa ? o match en su lugar
expect_used        = "deny"     # usa ? con contexto descriptivo
panic              = "deny"     # maneja el error explícitamente
indexing_slicing   = "warn"     # prefiere .get(i) sobre v[i]

# Calidad de código
cognitive_complexity  = "warn"  # funciones demasiado complejas
large_enum_variant    = "warn"  # variantes muy grandes → Box
type_complexity       = "warn"  # tipos muy anidados → type alias
too_many_arguments    = "warn"  # más de 7 parámetros → struct

# Style
clone_on_ref_ptr   = "warn"     # Rc::clone(&x) no x.clone() en Rc/Arc
missing_errors_doc = "warn"     # falta sección # Errors en funciones que devuelven Result
missing_panics_doc = "warn"     # falta sección # Panics
must_use_candidate = "warn"     # devuelves valor que probablemente se deba usar

# Permitir explícitamente
print_stdout       = "allow"    # println! está bien en CLIs
```

### `clippy.toml`: ajustes numéricos

```toml
# clippy.toml en la raíz del proyecto
too-many-arguments-threshold = 6    # warn si > 6 parámetros (default: 7)
type-complexity-threshold = 250     # complejidad máxima de tipos
cognitive-complexity-threshold = 15 # complejidad cognitiva máxima
```

### Ejecutar con lints como errores (para CI)

```bash
cargo clippy --all-targets --all-features -- -D warnings
#            └─ tests, benches, ejemplos   └─ features opcionales
#                                                       └─ warnings = errors
```

### Anotaciones en código para suprimir lints puntuales

```rust
#[allow(clippy::too_many_arguments)]
fn funcion_legacy(a: i32, b: i32, c: i32, d: i32, e: i32, f: i32, g: i32, h: i32) {
    // ...
}

fn procesar(v: &[i32]) {
    #[allow(clippy::indexing_slicing)]
    let primero = v[0];   // sabemos que v no está vacío porque...
    // ...
}
```

---

## Configuración de `rustfmt`

`rustfmt` formatea el código automáticamente. Configura en `rustfmt.toml`:

```toml
# rustfmt.toml
edition = "2021"
max_width = 100            # ancho máximo de línea (default: 100)
tab_spaces = 4             # espacios de indentación
hard_tabs = false          # usa espacios, no tabs
newline_style = "Unix"     # LF, no CRLF

# Imports
imports_granularity = "Crate"       # agrupa todos los imports del mismo crate
group_imports = "StdExternalCrate"  # orden: std → external → crate

# Estilo
use_small_heuristics = "Default"    # heurística para listas cortas en una línea
trailing_comma = "Vertical"         # coma final en listas multilínea
```

### Uso en flujo de trabajo

```bash
cargo fmt                    # formatea todo el proyecto (sobreescribe)
cargo fmt --check            # verifica sin sobreescribir (para CI)
cargo fmt -- --emit=stdout   # imprime resultado sin sobreescribir
```

---

## Proyecto integrador: librería `estadisticas`

Este proyecto aplica todas las herramientas de la semana: unit tests, integration
tests, doc tests, benchmarks, Clippy y rustfmt.

### Setup

```bash
cargo new estadisticas --lib
cd estadisticas
# Agregar a Cargo.toml:
# [dev-dependencies]
# criterion = { version = "0.5", features = ["html_reports"] }
#
# [[bench]]
# name = "bench_estadisticas"
# harness = false
```

### `src/lib.rs`

```rust
//! # Estadísticas
//!
//! Librería de funciones estadísticas básicas con garantías de rendimiento O(n).

#![deny(missing_docs)]   // error si falta documentación en items públicos

/// Calcula la media aritmética de una lista de números.
///
/// # Examples
///
/// ```
/// use estadisticas::media;
///
/// assert_eq!(media(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some(3.0));
/// assert_eq!(media(&[]), None);
/// ```
///
/// # Returns
///
/// `None` si la lista está vacía, `Some(media)` en caso contrario.
pub fn media(datos: &[f64]) -> Option<f64> {
    if datos.is_empty() {
        return None;
    }
    Some(datos.iter().sum::<f64>() / datos.len() as f64)
}

/// Calcula la mediana de una lista de números.
///
/// # Examples
///
/// ```
/// use estadisticas::mediana;
///
/// assert_eq!(mediana(&[3.0, 1.0, 2.0]), Some(2.0));
/// assert_eq!(mediana(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
/// assert_eq!(mediana(&[]), None);
/// ```
pub fn mediana(datos: &[f64]) -> Option<f64> {
    if datos.is_empty() {
        return None;
    }
    let mut ordenados = datos.to_vec();
    ordenados.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = ordenados.len();
    if n % 2 == 0 {
        Some((ordenados[n / 2 - 1] + ordenados[n / 2]) / 2.0)
    } else {
        Some(ordenados[n / 2])
    }
}

/// Calcula la moda de una lista de enteros.
///
/// Si hay múltiples modas, devuelve la más pequeña.
///
/// # Examples
///
/// ```
/// use estadisticas::moda;
///
/// assert_eq!(moda(&[1, 2, 2, 3, 3, 3]), Some(3));
/// assert_eq!(moda(&[1, 2, 3]), Some(1));  // todas iguales: la menor
/// assert_eq!(moda(&[]), None);
/// ```
pub fn moda(datos: &[i64]) -> Option<i64> {
    use std::collections::HashMap;
    if datos.is_empty() {
        return None;
    }
    let mut conteo: HashMap<i64, usize> = HashMap::new();
    for &x in datos {
        *conteo.entry(x).or_insert(0) += 1;
    }
    conteo
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(val, _)| val)
}

/// Calcula la desviación estándar de una lista de números.
///
/// # Examples
///
/// ```
/// use estadisticas::desviacion_std;
///
/// let datos = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
/// let ds = desviacion_std(&datos).unwrap();
/// assert!((ds - 2.0).abs() < 1e-10);
/// ```
///
/// # Returns
///
/// `None` si la lista tiene menos de 2 elementos.
pub fn desviacion_std(datos: &[f64]) -> Option<f64> {
    if datos.len() < 2 {
        return None;
    }
    let m = media(datos)?;
    let varianza = datos.iter().map(|&x| (x - m).powi(2)).sum::<f64>() / (datos.len() - 1) as f64;
    Some(varianza.sqrt())
}

/// Devuelve el rango (máximo − mínimo) de una lista.
///
/// # Examples
///
/// ```
/// use estadisticas::rango;
///
/// assert_eq!(rango(&[1.0, 5.0, 3.0, 2.0]), Some(4.0));
/// assert_eq!(rango(&[7.0]), Some(0.0));
/// assert_eq!(rango(&[]), None);
/// ```
pub fn rango(datos: &[f64]) -> Option<f64> {
    let min = datos.iter().cloned().reduce(f64::min)?;
    let max = datos.iter().cloned().reduce(f64::max)?;
    Some(max - min)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    mod tests_media {
        use super::*;

        #[test]
        fn lista_vacia() {
            assert_eq!(media(&[]), None);
        }

        #[test]
        fn un_elemento() {
            assert_eq!(media(&[42.0]), Some(42.0));
        }

        #[test]
        fn valores_iguales() {
            assert_eq!(media(&[5.0, 5.0, 5.0]), Some(5.0));
        }

        #[test]
        fn valores_mixtos() {
            assert_eq!(media(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some(3.0));
        }

        #[test]
        fn valores_negativos() {
            assert_eq!(media(&[-1.0, 0.0, 1.0]), Some(0.0));
        }
    }

    mod tests_mediana {
        use super::*;

        #[test]
        fn longitud_impar() {
            assert_eq!(mediana(&[3.0, 1.0, 2.0]), Some(2.0));
        }

        #[test]
        fn longitud_par() {
            assert_eq!(mediana(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
        }

        #[test]
        fn no_requiere_orden_previo() {
            assert_eq!(mediana(&[5.0, 1.0, 3.0]), Some(3.0));
        }

        #[test]
        fn lista_vacia() {
            assert_eq!(mediana(&[]), None);
        }
    }

    mod tests_moda {
        use super::*;

        #[test]
        fn moda_clara() {
            assert_eq!(moda(&[1, 2, 2, 3]), Some(2));
        }

        #[test]
        fn empate_devuelve_menor() {
            // 1 y 3 aparecen 2 veces cada uno → devuelve el menor
            assert_eq!(moda(&[1, 1, 3, 3]), Some(1));
        }

        #[test]
        fn todos_distintos_devuelve_menor() {
            assert_eq!(moda(&[5, 3, 1, 4, 2]), Some(1));
        }

        #[test]
        fn lista_vacia() {
            assert_eq!(moda(&[]), None);
        }
    }

    mod tests_desviacion {
        use super::*;

        #[test]
        fn ejemplo_conocido() {
            let datos = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
            let ds = desviacion_std(&datos).unwrap();
            assert!((ds - 2.0).abs() < 1e-10, "esperado ≈ 2.0, obtenido {ds}");
        }

        #[test]
        fn un_elemento_devuelve_none() {
            assert_eq!(desviacion_std(&[5.0]), None);
        }

        #[test]
        fn valores_identicos() {
            let ds = desviacion_std(&[3.0, 3.0, 3.0]).unwrap();
            assert_eq!(ds, 0.0);
        }
    }
}
```

### `tests/integracion.rs`

```rust
// tests/integracion.rs — crate separada, solo API pública
use estadisticas::{desviacion_std, media, mediana, moda, rango};

#[test]
fn pipeline_completo() {
    let datos = vec![4.0, 8.0, 6.0, 5.0, 3.0, 2.0, 8.0, 9.0, 2.0, 5.0];
    let datos_int: Vec<i64> = datos.iter().map(|&x| x as i64).collect();

    assert_eq!(media(&datos), Some(5.2));
    assert_eq!(mediana(&datos), Some(5.0));
    assert_eq!(moda(&datos_int), Some(2));  // 2 y 8 y 5 aparecen 2 veces; la menor es 2
    assert!(rango(&datos).is_some());

    let ds = desviacion_std(&datos).unwrap();
    assert!(ds > 0.0, "desviación debe ser positiva para datos no uniformes");
}

#[test]
fn resiliencia_con_un_solo_dato() {
    assert!(media(&[42.0]).is_some());
    assert!(mediana(&[42.0]).is_some());
    assert_eq!(moda(&[42]), Some(42));
    assert_eq!(rango(&[42.0]), Some(0.0));
    assert_eq!(desviacion_std(&[42.0]), None); // necesita ≥ 2
}
```

### `benches/bench_estadisticas.rs`

```rust
// benches/bench_estadisticas.rs
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use estadisticas::{media, mediana};

fn bench_media(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("media");
    for n in [100_usize, 10_000, 1_000_000] {
        let datos: Vec<f64> = (0..n).map(|i| i as f64).collect();
        grupo.bench_with_input(BenchmarkId::from_parameter(n), &datos, |b, d| {
            b.iter(|| media(black_box(d)))
        });
    }
    grupo.finish();
}

fn bench_mediana(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("mediana");
    for n in [100_usize, 10_000, 100_000] {
        let datos: Vec<f64> = (0..n).map(|i| i as f64).collect();
        grupo.bench_with_input(BenchmarkId::from_parameter(n), &datos, |b, d| {
            // iter_batched: clona los datos antes de cada iteración
            // porque mediana ordena in-place un Vec propio
            b.iter_batched(
                || d.clone(),
                |datos| black_box(mediana(black_box(&datos))),
                criterion::BatchSize::SmallInput,
            )
        });
    }
    grupo.finish();
}

criterion_group!(benches, bench_media, bench_mediana);
criterion_main!(benches);
```

### Ejecutar todo

```bash
cargo test                          # unit + integration + doc tests
cargo test --doc                    # solo doc tests
cargo bench                         # benchmarks (no modifica tests)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo doc --open                    # ver documentación generada
```

---

## Pipeline de CI/CD

Un archivo `.github/workflows/ci.yml` completo para el proyecto:

```yaml
name: CI

on:
  push:
    branches: [main, master]
  pull_request:

jobs:
  test:
    name: Test suite
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Instalar Rust estable
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache de compilación
        uses: Swatinem/rust-cache@v2

      - name: Verificar formato
        run: cargo fmt --all -- --check

      - name: Clippy (warnings como errores)
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Tests unitarios e integración
        run: cargo test --all-features

      - name: Doc tests
        run: cargo test --doc --all-features

      - name: Verificar que la documentación compila
        run: cargo doc --no-deps --all-features
        env:
          RUSTDOCFLAGS: "-D warnings"   # error si hay warnings en docs

  audit:
    name: Auditoría de seguridad
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v1
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
```

---

## ✅ Checklist de la Semana 8

- [ ] Distingo los cuatro niveles de la pirámide de testing y cuándo usar cada uno.
- [ ] Escribo unit tests con `#[cfg(test)]` y accedo a funciones privadas con `use super::*`.
- [ ] Uso `assert_eq!`, `assert_ne!`, `assert!` con mensajes descriptivos.
- [ ] Escribo tests con `#[should_panic(expected = "...")]` para paths de error.
- [ ] Escribo tests que devuelven `Result<(), E>` usando `?` interno.
- [ ] Creo integration tests en `tests/*.rs` que solo usan la API pública.
- [ ] Uso `tests/comun/mod.rs` para helpers compartidos entre tests de integración.
- [ ] Documento funciones con `///`, secciones `# Examples`, `# Errors`, `# Panics`.
- [ ] Los doc tests pasan con `cargo test --doc`.
- [ ] Uso `no_run` e `ignore` correctamente en doc tests que no pueden ejecutarse solos.
- [ ] Configuro `criterion` en `Cargo.toml` y escribo al menos un benchmark con `black_box`.
- [ ] Entiendo por qué `black_box` es necesario y uso `iter_batched` cuando hay setup.
- [ ] Configuro Clippy con `deny` en `unwrap_used` y `expect_used` en el proyecto.
- [ ] El CI pasa: `fmt --check`, `clippy -D warnings`, `cargo test`, `cargo test --doc`.
- [ ] El proyecto `estadisticas` compila limpio y todos los tests pasan.

---

## ✅ Checklist Final Mes 2

- [ ] **Semana 5:** Generics, trait bounds, `where`, `impl Trait` en arg/retorno, lifetimes en funciones y structs, `'static`. Ejercicio `Cache<K,V,T>`.
- [ ] **Semana 6:** `dyn Trait` vs dispatch estático, `From`/`TryFrom`/`AsRef`, `Deref`/`DerefMut`, `Drop`, operadores. Ejercicio `ByteBuffer`.
- [ ] **Semana 7:** `Box`, `Rc`/`Arc`, `RefCell`/`Cell`/`Mutex`/`RwLock`, `Weak`. Ejercicio árbol con `Rc<RefCell<Nodo>>`.
- [ ] **Semana 8:** Pirámide de testing, doc tests, benchmarks `criterion`, Clippy avanzado, rustfmt. Proyecto `estadisticas`.

> **¡Mes 2 completado!** El siguiente paso es el **Mes 3**:
> [Async Rust, Futures, Tokio, Axum y SQLx](../chapter_03/section_00.md).
