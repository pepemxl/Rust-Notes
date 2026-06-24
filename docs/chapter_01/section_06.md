# Módulos, Colecciones, String, Errores y Traits

La Semana 4 cierra el Mes 1. Aquí aprendes a **organizar código real**: cómo dividirlo
en módulos, qué colecciones usar según el problema, cómo no confundir `String` con `&str`,
cómo propagar errores de forma profesional, y qué traits del estándar conviene conocer de
memoria.

En esta sección aprenderemos:

- El **sistema de módulos**: `mod`, `pub`, `use`, `pub use` y rutas `crate::`.
- Las cuatro **colecciones estándar** más importantes y cuándo usar cada una.
- La diferencia definitiva entre `String` y `&str`, y cómo diseñar APIs correctas.
- **Error handling avanzado**: tipos de error propios, el trait `From`, `Box<dyn Error>`.
- Los **traits estándar** derivables e implementados manualmente.

> 💡 **Filosofía de la Semana 4:** *El código que escala no es el que hace más cosas;
> es el que mantiene cada cosa en su lugar. Los módulos son la frontera entre "funciona"
> y "puedo tocar esto sin romper el resto".*

---

## Sistema de módulos

### ¿Qué es un módulo?

Un módulo es un **espacio de nombres** que agrupa definiciones relacionadas y controla su
visibilidad. Por defecto todo en Rust es **privado**; solo lo que marcas con `pub` es
accesible desde fuera del módulo.

### Módulos en línea

La forma más simple: declarar el módulo directamente en el mismo archivo.

```rust
mod matematicas {
    pub fn suma(a: i32, b: i32) -> i32 { a + b }

    fn privada() -> i32 { 42 }  // no accesible desde fuera

    pub mod avanzado {
        pub fn potencia(base: i32, exp: u32) -> i32 {
            base.pow(exp)
        }
    }
}

fn main() {
    println!("{}", matematicas::suma(3, 4));                   // 7
    println!("{}", matematicas::avanzado::potencia(2, 8));     // 256
    // matematicas::privada();  // ❌ NO COMPILA: función privada
}
```

### Módulos en archivos separados

En proyectos reales cada módulo vive en su propio archivo. Con la **edición 2018** de Rust
(la vigente), la convención es:

```
src/
├── main.rs          ← crate root; declara módulos con `mod nombre;`
├── modelos.rs       ← contenido del módulo `modelos`
├── almacen.rs       ← contenido del módulo `almacen`
└── cli/             ← módulo `cli` como directorio
    ├── mod.rs       ← declara submódulos del directorio
    ├── args.rs
    └── comandos.rs
```

**Regla fundamental:** `mod nombre;` en el padre es una *declaración*, no una inclusión.
Le dice al compilador "busca el código de este módulo en `nombre.rs` o en
`nombre/mod.rs`". El archivo del módulo **no** escribe `mod nombre;` dentro de sí mismo.

```rust
// src/main.rs
mod modelos;      // busca src/modelos.rs
mod almacen;      // busca src/almacen.rs
mod cli;          // busca src/cli/mod.rs

use modelos::Tarea;
use almacen::Store;

fn main() { /* ... */ }
```

```rust
// src/modelos.rs
#[derive(Debug, Clone)]
pub struct Tarea {
    pub id: u64,
    pub descripcion: String,
}
```

```rust
// src/almacen.rs
use crate::modelos::Tarea;   // ruta absoluta desde la raíz del crate

pub struct Store {
    tareas: Vec<Tarea>,      // campo privado
}
```

```rust
// src/cli/mod.rs
pub mod args;       // busca src/cli/args.rs
pub mod comandos;   // busca src/cli/comandos.rs
```

### Rutas: `crate::`, `super::`, `self::`

| Prefijo | Significa |
| :--- | :--- |
| `crate::` | Raíz del crate actual (absoluto) |
| `super::` | Módulo padre (relativo hacia arriba) |
| `self::` | El módulo actual (relativo, raramente necesario) |

```rust
// src/cli/comandos.rs
use crate::modelos::Tarea;          // desde la raíz
use super::args::Argumentos;        // desde cli/ (módulo padre)
```

### `use` y `pub use`

`use` trae un nombre al scope local para no tener que escribir la ruta completa cada vez.
`pub use` re-exporta el nombre hacia afuera, creando una **fachada pública**:

```rust
// src/lib.rs de una librería
pub use crate::modelos::Tarea;      // los usuarios del crate escriben: mi_crate::Tarea
pub use crate::almacen::Store;      // en lugar de: mi_crate::almacen::Store

// Atajos útiles
use std::collections::{HashMap, HashSet};   // import múltiple con {}
use std::io::{self, Write};                 // self = el módulo io en sí
```

### Visibilidad granular

```rust
pub struct Config {
    pub nombre: String,         // accesible desde cualquier lugar
    pub(crate) nivel: u32,      // solo dentro del mismo crate
    secreto: String,            // privado al módulo
}
```

---

## Colecciones estándar

### `Vec<T>` — lista dinámica

La colección más usada en Rust. Crece en el heap, acceso O(1) por índice.

```rust
fn main() {
    // Creación
    let mut v: Vec<i32> = Vec::new();
    let v2 = vec![1, 2, 3];        // macro de conveniencia

    // Modificación
    v.push(10);
    v.push(20);
    v.push(30);
    let ultimo = v.pop();           // Option<i32>: Some(30)

    // Acceso seguro vs inseguro
    let segundo = v.get(1);         // Option<&i32>: Some(&20)
    let directo = v[0];             // i32: 10 (panic si fuera de rango)

    // Iteración (las tres formas)
    for x in &v { println!("{x}"); }           // itera referencias
    for x in &mut v { *x *= 2; }               // itera referencias mutables
    for x in v.clone() { println!("{x}"); }    // itera valores (consume)

    // Métodos útiles
    v.sort();
    v.dedup();                      // elimina duplicados consecutivos (requiere sort previo)
    v.retain(|&x| x > 5);          // conserva solo los que cumplen el predicado
    let suma: i32 = v.iter().sum();
    println!("suma: {suma}");
}
```

**Cuándo usar `Vec<T>`**: siempre que necesites una lista ordenada de tamaño variable.
Es la colección por defecto.

### `HashMap<K, V>` — mapa clave-valor

Lookup O(1) amortizado. Las claves deben implementar `Eq` y `Hash`.

```rust
use std::collections::HashMap;

fn main() {
    let mut mapa: HashMap<String, i32> = HashMap::new();

    // Insertar
    mapa.insert(String::from("uno"), 1);
    mapa.insert(String::from("dos"), 2);

    // Acceso
    let val = mapa.get("uno");              // Option<&i32>
    println!("{:?}", val);                  // Some(1)

    // Iterar
    for (clave, valor) in &mapa {
        println!("{clave}: {valor}");
    }

    // Patrón entry API — la forma idiomática de insertar-o-actualizar
    // sin hacer dos búsquedas en la tabla hash:
    mapa.entry(String::from("tres")).or_insert(3);       // solo inserta si no existe
    mapa.entry(String::from("uno")).or_insert(99);       // ya existe -> no hace nada
    println!("{:?}", mapa["uno"]);                       // 1 (no cambió)

    // Caso típico: contar ocurrencias
    let texto = "hola mundo hola rust hola";
    let mut conteo: HashMap<&str, u32> = HashMap::new();
    for palabra in texto.split_whitespace() {
        *conteo.entry(palabra).or_insert(0) += 1;
    }
    println!("{:?}", conteo["hola"]);       // 3
}
```

**Cuándo usar `HashMap<K, V>`**: lookups frecuentes por clave, cachés, agrupaciones.

> ⚠️ `HashMap` **no** garantiza orden de iteración. Si necesitas iterar en orden de
> inserción o de clave, usa `BTreeMap` o una crate como `indexmap`.

### `HashSet<T>` — conjunto sin duplicados

Implementado sobre `HashMap<T, ()>`. Pertenencia O(1).

```rust
use std::collections::HashSet;

fn main() {
    let mut set: HashSet<i32> = HashSet::new();
    set.insert(1);
    set.insert(2);
    set.insert(2);   // ignorado, ya existe
    set.insert(3);
    println!("{}", set.contains(&2));   // true
    println!("{}", set.len());          // 3

    let a: HashSet<i32> = [1, 2, 3].into_iter().collect();
    let b: HashSet<i32> = [2, 3, 4].into_iter().collect();

    let union: HashSet<_>        = a.union(&b).collect();
    let interseccion: HashSet<_> = a.intersection(&b).collect();
    let diferencia: HashSet<_>   = a.difference(&b).collect();

    println!("∪ {:?}", union);          // {1, 2, 3, 4}
    println!("∩ {:?}", interseccion);   // {2, 3}
    println!("∖ {:?}", diferencia);     // {1}
}
```

**Cuándo usar `HashSet<T>`**: eliminar duplicados, pertenencia rápida, operaciones de
conjuntos.

### `BTreeMap<K, V>` y `BTreeSet<T>` — colecciones ordenadas

Como `HashMap`/`HashSet` pero **ordenadas por clave** (árbol B). Las claves deben
implementar `Ord`. Iteración siempre en orden ascendente.

```rust
use std::collections::BTreeMap;

fn main() {
    let mut mapa = BTreeMap::new();
    mapa.insert("zebra", 26);
    mapa.insert("avion", 1);
    mapa.insert("mango", 13);

    for (k, v) in &mapa {
        println!("{k}: {v}");   // avion:1, mango:13, zebra:26 (orden alfabético)
    }

    // Rangos de claves
    use std::ops::Bound::Included;
    for (k, v) in mapa.range("a"..="m") {
        println!("{k}: {v}");   // avion:1, mango:13
    }
}
```

**Cuándo usar `BTreeMap`**: cuando necesitas ordenación o consultas por rango.
Más lento que `HashMap` (O(log n) vs O(1)), pero predecible.

### Resumen de colecciones

| Colección | Lookup | Inserción | Orden | Claves requieren |
| :--- | :--- | :--- | :--- | :--- |
| `Vec<T>` | O(n) / O(1) por índice | O(1) amort. | Inserción | — |
| `HashMap<K,V>` | O(1) amort. | O(1) amort. | Ninguno | `Eq + Hash` |
| `HashSet<T>` | O(1) amort. | O(1) amort. | Ninguno | `Eq + Hash` |
| `BTreeMap<K,V>` | O(log n) | O(log n) | Clave asc. | `Ord` |
| `BTreeSet<T>` | O(log n) | O(log n) | Valor asc. | `Ord` |

---

## `String` vs `&str`: la guía definitiva

Esta es la confusión más común al llegar a Rust. La tabla completa:

| Tipo | ¿Tiene ownership? | ¿Mutable? | ¿Dónde vive? | Caso de uso |
| :--- | :--- | :--- | :--- | :--- |
| `String` | **Sí** | Sí (si es `mut`) | Heap | Construir/modificar texto en runtime |
| `&str` | No (vista) | No | Stack (fat ptr) + Heap o `.rodata` | Leer texto sin copiarlo |
| `&String` | No (referencia) | No | Stack (puntero a String) | Casi nunca en firmas de función |
| `&mut String` | No (ref mutable) | **Sí** | Stack | Modificar String sin moverla |

### Conversiones

```rust
fn main() {
    // &str -> String (varias formas equivalentes)
    let s1: String = "hola".to_string();
    let s2: String = String::from("hola");
    let s3: String = "hola".to_owned();

    // String -> &str
    let r1: &str = &s1;            // deref coercion: &String -> &str
    let r2: &str = s1.as_str();    // explícito
    let r3: &str = &s1[..];        // slice de toda la String

    // Construir Strings
    let mut construida = String::new();
    construida.push('H');           // añade un char
    construida.push_str("ola");     // añade un &str
    construida += " mundo";         // operador += acepta &str

    let concatenada = format!("{} {}", s1, s2);   // sin consumir ninguna
    println!("{concatenada}");
}
```

### Reglas de diseño de APIs

```rust
// ❌ MAL: obliga al caller a tener exactamente una &String
fn saludar_mal(nombre: &String) {
    println!("Hola, {nombre}");
}

// ✅ BIEN: acepta &str, &String (por deref), literales, String temporales
fn saludar(nombre: &str) {
    println!("Hola, {nombre}");
}

// ✅ AÚN MEJOR para APIs genéricas: impl AsRef<str>
fn saludar_generico(nombre: impl AsRef<str>) {
    println!("Hola, {}", nombre.as_ref());
    // acepta: &str, String, &String, Box<str>, Cow<str>, PathBuf...
}

fn main() {
    let owned = String::from("Ana");
    let literal = "Bea";

    saludar(&owned);            // &String coacciona a &str
    saludar(literal);           // &str directo
    saludar_generico(&owned);
    saludar_generico(literal);
    saludar_generico(String::from("Carlos")); // String temporal
}
```

**Regla de oro para parámetros de texto:**

- Entrada que solo lees: usa `&str`.
- Entrada que necesitas guardar (en un struct, p.ej.): usa `String` (toma ownership).
- Retorno de texto construido: devuelve `String`.
- Retorno de una vista a texto existente: devuelve `&str` (con lifetime implícito).

---

## Error handling avanzado

### El problema de los tipos de error mezclados

Cuando una función usa `?` con errores de distinto tipo, el compilador protesta porque
no sabe cómo convertir uno en otro:

```rust
fn leer_numero(ruta: &str) -> Result<i32, ???> {
    let contenido = std::fs::read_to_string(ruta)?;  // io::Error
    let n: i32 = contenido.trim().parse()?;           // ParseIntError
    Ok(n)
}
```

Hay tres estrategias, de menos a más rigurosa:

### Estrategia 1: `Box<dyn Error>` (prototipado rápido)

```rust
use std::error::Error;

fn leer_numero(ruta: &str) -> Result<i32, Box<dyn Error>> {
    let contenido = std::fs::read_to_string(ruta)?;  // io::Error -> Box<dyn Error>
    let n: i32 = contenido.trim().parse()?;           // ParseIntError -> Box<dyn Error>
    Ok(n)
}

fn main() -> Result<(), Box<dyn Error>> {
    let n = leer_numero("numero.txt")?;
    println!("leído: {n}");
    Ok(())
}
```

`Box<dyn Error>` acepta cualquier tipo que implemente `Error`. Es cómodo pero **borra el
tipo** del error: quien llama no puede hacer `match` sobre el error específico. Úsalo en
`main`, en tests o en código exploratorio.

### Estrategia 2: tipo de error propio con `From`

Define un enum que represente todos los errores posibles de tu módulo e implementa
`From<OtroError>` para que `?` los convierta automáticamente:

```rust
use std::fmt;
use std::num::ParseIntError;

// 1. Definir el enum de errores
#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Parseo(ParseIntError),
    NumeroNegativo(i32),
}

// 2. Implementar Display (obligatorio para implementar Error)
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "error de I/O: {e}"),
            AppError::Parseo(e) => write!(f, "error de parseo: {e}"),
            AppError::NumeroNegativo(n) => write!(f, "el número {n} no puede ser negativo"),
        }
    }
}

// 3. Implementar el trait Error (puede ser vacío si los campos ya implementan Error)
impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Io(e) => Some(e),
            AppError::Parseo(e) => Some(e),
            AppError::NumeroNegativo(_) => None,
        }
    }
}

// 4. Implementar From<T> para cada tipo de error fuente
//    Esto es lo que permite que `?` convierta automáticamente
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e)
    }
}

impl From<ParseIntError> for AppError {
    fn from(e: ParseIntError) -> Self {
        AppError::Parseo(e)
    }
}

// 5. Usar el tipo de error en funciones
fn leer_numero_positivo(ruta: &str) -> Result<u32, AppError> {
    let contenido = std::fs::read_to_string(ruta)?;   // io::Error -> AppError::Io
    let n: i32 = contenido.trim().parse()?;            // ParseIntError -> AppError::Parseo
    if n < 0 {
        return Err(AppError::NumeroNegativo(n));        // error de dominio
    }
    Ok(n as u32)
}

fn main() -> Result<(), AppError> {
    match leer_numero_positivo("num.txt") {
        Ok(n) => println!("número: {n}"),
        Err(AppError::Io(e)) => eprintln!("archivo no encontrado: {e}"),
        Err(AppError::Parseo(e)) => eprintln!("contenido inválido: {e}"),
        Err(AppError::NumeroNegativo(n)) => eprintln!("era negativo: {n}"),
    }
    Ok(())
}
```

### Estrategia 3: crate `thiserror` (producción)

`thiserror` genera automáticamente el código de `Display`, `Error` y `From` con macros:

```rust
// Cargo.toml: thiserror = "1"
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("error de I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("error de parseo: {0}")]
    Parseo(#[from] std::num::ParseIntError),

    #[error("el número {0} no puede ser negativo")]
    NumeroNegativo(i32),
}
```

Esto es **equivalente** a las 30+ líneas manuales de la estrategia 2. Úsalo en código
de producción.

### Cuándo usar cada estrategia

| Estrategia | Cuándo |
| :--- | :--- |
| `Box<dyn Error>` | `main`, scripts, prototipos, cuando no importa el tipo exacto |
| Tipo propio manual | Aprendizaje, sin dependencias externas, librerías pequeñas |
| `thiserror` | Librerías y aplicaciones en producción |
| `anyhow` (crate) | Aplicaciones donde solo importa el mensaje, no el tipo |

---

## Traits estándar

Un **trait** define un comportamiento que un tipo puede implementar. Aquí los más
importantes que encontrarás en el día a día:

### Traits derivables con `#[derive]`

#### `Debug`

```rust
#[derive(Debug)]
struct Punto { x: f64, y: f64 }

fn main() {
    let p = Punto { x: 1.0, y: 2.0 };
    println!("{:?}", p);    // Punto { x: 1.0, y: 2.0 }
    println!("{:#?}", p);   // pretty-print multilínea
    dbg!(&p);               // imprime a stderr con archivo y línea: [src/main.rs:8] &p = Punto { ... }
}
```

Derívalo **siempre** en tus tipos. No hay razón para no hacerlo.

#### `Clone` y `Copy`

```rust
#[derive(Debug, Clone)]         // Clone: duplicación explícita con .clone()
struct Config { nombre: String, nivel: u32 }

#[derive(Debug, Clone, Copy)]   // Copy: duplicación implícita (bitwise)
struct Punto2D { x: f32, y: f32 }  // solo si TODOS los campos son Copy

fn main() {
    let c1 = Config { nombre: String::from("app"), nivel: 3 };
    let c2 = c1.clone();    // copia explícita: c1 sigue válido

    let p1 = Punto2D { x: 1.0, y: 2.0 };
    let p2 = p1;            // copia implícita: p1 sigue válido (Copy)
    println!("{p1:?} y {p2:?}");
}
```

#### `PartialEq`, `Eq`

```rust
#[derive(Debug, Clone, PartialEq)]  // habilita ==  y !=
struct Version { mayor: u32, menor: u32 }

fn main() {
    let v1 = Version { mayor: 1, menor: 0 };
    let v2 = Version { mayor: 1, menor: 0 };
    println!("{}", v1 == v2);   // true
}
```

`Eq` (sin campos) indica *equivalencia total* (a == a siempre). Se requiere para claves
de `HashMap`. Los `f32`/`f64` implementan `PartialEq` pero no `Eq` (porque `NaN != NaN`).

#### `PartialOrd`, `Ord`

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Version { mayor: u32, menor: u32, parche: u32 }
// El orden lexicográfico por defecto compara campo a campo de izquierda a derecha

fn main() {
    let mut versiones = vec![
        Version { mayor: 1, menor: 2, parche: 0 },
        Version { mayor: 0, menor: 9, parche: 5 },
        Version { mayor: 1, menor: 0, parche: 3 },
    ];
    versiones.sort();
    println!("{:?}", versiones);  // [0.9.5, 1.0.3, 1.2.0]
}
```

#### `Hash`

Necesario para que el tipo sea clave de `HashMap` o elemento de `HashSet`. Se deriva
junto con `Eq`:

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Coordenada { x: i32, y: i32 }

fn main() {
    let mut mapa: HashMap<Coordenada, &str> = HashMap::new();
    mapa.insert(Coordenada { x: 0, y: 0 }, "origen");
    mapa.insert(Coordenada { x: 1, y: 0 }, "derecha");
    println!("{:?}", mapa[&Coordenada { x: 0, y: 0 }]);  // "origen"
}
```

#### `Default`

Proporciona un valor "vacío" o "cero" para el tipo:

```rust
#[derive(Debug, Default)]
struct Opciones {
    verbose: bool,      // default: false
    limite: u32,        // default: 0
    prefijo: String,    // default: ""
}

fn main() {
    let opts = Opciones::default();
    println!("{opts:?}");   // Opciones { verbose: false, limite: 0, prefijo: "" }

    // Struct update syntax + Default
    let custom = Opciones { verbose: true, ..Opciones::default() };
    println!("{custom:?}");
}
```

### Traits implementados manualmente

#### `Display`: formato para el usuario final

`Debug` es para desarrolladores; `Display` es para el usuario. No es derivable porque
el formato exacto es una decisión de diseño:

```rust
use std::fmt;

struct Color { r: u8, g: u8, b: u8 }

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

fn main() {
    let rojo = Color { r: 255, g: 0, b: 0 };
    println!("{rojo}");     // #FF0000
    println!("{rojo:?}");   // ❌ falta #[derive(Debug)]  —  añádelo también
}
```

#### `From` / `Into`

`From<T>` convierte de `T` al tipo actual. Implementar `From<T> for U` te da `Into<U>
for T` gratis:

```rust
struct Metros(f64);
struct Centimetros(f64);

impl From<Metros> for Centimetros {
    fn from(m: Metros) -> Self {
        Centimetros(m.0 * 100.0)
    }
}

fn main() {
    let m = Metros(1.5);
    let cm: Centimetros = m.into();     // Into<Centimetros> derivado automáticamente
    println!("{} cm", cm.0);            // 150
}
```

`From`/`Into` también es el mecanismo que usa `?` para convertir errores:
`impl From<io::Error> for AppError` hace que `io::Error` se convierta automáticamente
cuando usas `?` en una función que devuelve `Result<_, AppError>`.

### Tabla resumen de traits

| Trait | Derivable | Habilita | Notas |
| :--- | :--- | :--- | :--- |
| `Debug` | Sí | `{:?}`, `{:#?}`, `dbg!()` | Derívalo siempre |
| `Display` | **No** | `{}`, `to_string()` | Implementa manual para output de usuario |
| `Clone` | Sí (si campos lo son) | `.clone()` | Deep copy explícita |
| `Copy` | Sí (si campos lo son) | Copia implícita | Solo tipos 100% stack |
| `PartialEq` | Sí | `==`, `!=` | Necesita `Eq` para HashMap keys |
| `Eq` | Sí (marker) | Garantía de equivalencia total | Junto con `PartialEq` |
| `PartialOrd` | Sí | `<`, `>`, `<=`, `>=` | Necesita `PartialEq` |
| `Ord` | Sí (si campos lo son) | `.sort()`, `BTreeMap` keys | Necesita `Eq + PartialOrd` |
| `Hash` | Sí (si campos lo son) | `HashMap`/`HashSet` keys | Necesita `Eq` |
| `Default` | Sí (si campos lo son) | `Default::default()` | Campos usan su propio default |
| `From<T>` | **No** | `.into()`, `?` conversión | Impl manual por par de tipos |
| `Into<T>` | Automático vía `From` | `.into()` | No implementar directamente |

---

## Mini-proyecto: Todo List v2 (modular + persistencia)

Este proyecto une todo lo de la semana: módulos reales, colecciones, `String`/`&str`,
error handling con tipo propio, y traits estándar. La versión anterior vivía solo en
memoria; esta persiste en un archivo JSON.

### Setup

```bash
cargo new todo_cli
cd todo_cli
cargo add serde --features derive
cargo add serde_json
```

### Estructura de archivos

```
todo_cli/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── models.rs
    ├── error.rs
    ├── storage.rs
    └── commands.rs
```

### `src/error.rs`

```rust
use std::fmt;

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Json(serde_json::Error),
    TareaNoEncontrada(u64),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(e) => write!(f, "error de I/O: {e}"),
            AppError::Json(e) => write!(f, "error JSON: {e}"),
            AppError::TareaNoEncontrada(id) => write!(f, "tarea #{id} no encontrada"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self { AppError::Io(e) }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self { AppError::Json(e) }
}
```

### `src/models.rs`

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Estado {
    Pendiente,
    Terminado,
}

impl Estado {
    pub fn icono(&self) -> &str {
        match self {
            Estado::Pendiente => "[ ]",
            Estado::Terminado => "[x]",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tarea {
    pub id: u64,
    pub descripcion: String,
    pub estado: Estado,
}

impl Tarea {
    pub fn nueva(id: u64, descripcion: String) -> Self {
        Self { id, descripcion, estado: Estado::Pendiente }
    }
}
```

### `src/storage.rs`

```rust
use std::path::PathBuf;
use crate::{error::AppError, models::Tarea};

fn ruta_archivo() -> PathBuf {
    let mut ruta = std::env::current_dir().unwrap_or_default();
    ruta.push("tareas.json");
    ruta
}

pub fn cargar() -> Result<Vec<Tarea>, AppError> {
    let ruta = ruta_archivo();
    if !ruta.exists() {
        return Ok(Vec::new());
    }
    let contenido = std::fs::read_to_string(&ruta)?;
    let tareas = serde_json::from_str(&contenido)?;
    Ok(tareas)
}

pub fn guardar(tareas: &[Tarea]) -> Result<(), AppError> {
    let contenido = serde_json::to_string_pretty(tareas)?;
    std::fs::write(ruta_archivo(), contenido)?;
    Ok(())
}
```

### `src/commands.rs`

```rust
use crate::{error::AppError, models::{Estado, Tarea}, storage};

pub fn agregar(descripcion: &str) -> Result<(), AppError> {
    let mut tareas = storage::cargar()?;
    let id = tareas.iter().map(|t| t.id).max().unwrap_or(0) + 1;
    tareas.push(Tarea::nueva(id, descripcion.to_string()));
    storage::guardar(&tareas)?;
    println!("Tarea #{id} creada.");
    Ok(())
}

pub fn listar() -> Result<(), AppError> {
    let tareas = storage::cargar()?;
    if tareas.is_empty() {
        println!("Sin tareas.");
        return Ok(());
    }
    for t in &tareas {
        println!("  {} [{}] {}", t.estado.icono(), t.id, t.descripcion);
    }
    Ok(())
}

pub fn completar(id: u64) -> Result<(), AppError> {
    let mut tareas = storage::cargar()?;
    let tarea = tareas
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or(AppError::TareaNoEncontrada(id))?;
    tarea.estado = Estado::Terminado;
    storage::guardar(&tareas)?;
    println!("Tarea #{id} completada.");
    Ok(())
}

pub fn eliminar(id: u64) -> Result<(), AppError> {
    let mut tareas = storage::cargar()?;
    let pos = tareas
        .iter()
        .position(|t| t.id == id)
        .ok_or(AppError::TareaNoEncontrada(id))?;
    let desc = tareas.remove(pos).descripcion;
    storage::guardar(&tareas)?;
    println!("Eliminada: '{desc}'.");
    Ok(())
}
```

### `src/main.rs`

```rust
mod commands;
mod error;
mod models;
mod storage;

use error::AppError;

fn main() -> Result<(), AppError> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        [cmd] if cmd == "list" || cmd == "ls" => commands::listar()?,
        [cmd, desc] if cmd == "add" => commands::agregar(desc)?,
        [cmd, id] if cmd == "done" => {
            let id: u64 = id.parse().map_err(|_| {
                AppError::Io(std::io::Error::other("ID inválido"))
            })?;
            commands::completar(id)?;
        }
        [cmd, id] if cmd == "del" || cmd == "rm" => {
            let id: u64 = id.parse().map_err(|_| {
                AppError::Io(std::io::Error::other("ID inválido"))
            })?;
            commands::eliminar(id)?;
        }
        _ => {
            eprintln!("Uso:");
            eprintln!("  todo add \"descripción\"");
            eprintln!("  todo list");
            eprintln!("  todo done <id>");
            eprintln!("  todo del <id>");
        }
    }

    Ok(())
}
```

### Prueba manual

```bash
cargo run -- add "comprar leche"
cargo run -- add "llamar al médico"
cargo run -- list
cargo run -- done 1
cargo run -- list
cargo run -- del 2
cargo run -- list
cargo clippy    # debe mostrar 0 warnings
```

---

## ✅ Checklist de la Semana 4

- [ ] Organizo código en módulos separados (`mod nombre;`, archivos `.rs`).
- [ ] Controlo visibilidad con `pub`, `pub(crate)` y privado por defecto.
- [ ] Uso `use crate::`, `use super::` correctamente. Sé qué hace `pub use`.
- [ ] Elijo entre `Vec`, `HashMap`, `HashSet`, `BTreeMap` según el caso.
- [ ] Uso el patrón entry API (`entry().or_insert()`) en `HashMap`.
- [ ] Mis parámetros de texto son `&str`, no `&String`. Sé cuándo devolver `String` vs `&str`.
- [ ] Defino un `enum AppError` con `From` para cada error fuente; `?` convierte solo.
- [ ] Implemento `Display` manualmente para mis tipos de error.
- [ ] Derivo el conjunto correcto de traits (`Debug`, `Clone`, `PartialEq`, `Hash`, `Default`…).
- [ ] El proyecto `todo_cli` compila con `cargo clippy` limpio, persiste en JSON y los comandos funcionan.

> **¡Mes 1 completado!** El siguiente paso es el **Mes 2**:
> [Genéricos, Traits avanzados, Lifetimes, Smart Pointers](../chapter_02/section_00.md).
