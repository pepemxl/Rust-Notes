# Structs, Enums, Pattern Matching y Manejo de Errores

En las semanas anteriores aprendiste a controlar la memoria con ownership y borrowing.
Ahora aprenderás a **modelar el dominio de tu problema con tipos propios**: estructuras de
datos que hacen imposibles los estados inválidos y código que el compilador verifica
exhaustivamente.

En esta sección aprenderemos:

- Cómo definir **structs** (datos estructurados) y añadirles métodos con `impl`.
- Los **enums** de Rust: no son simples etiquetas como en C/Java, llevan datos.
- **Pattern matching** con `match`, `if let` y `while let`.
- `Option<T>` como alternativa a `null`.
- `Result<T, E>` como alternativa a las excepciones.
- Un mini-proyecto para atar todos los conceptos.

> 💡 **Filosofía de la Semana 3:** *Haz que los estados inválidos sean irrepresentables.*
> Si el compilador puede verificar que tu modelo es correcto, no necesitas escribir
> docenas de validaciones en tiempo de ejecución.

---

## Structs

Un `struct` agrupa campos relacionados bajo un único tipo con nombre.

### Struct clásico (named fields)

```rust
struct Usuario {
    nombre: String,
    email: String,
    activo: bool,
    intentos_login: u32,
}

fn main() {
    let user1 = Usuario {
        nombre: String::from("Ana"),
        email: String::from("ana@ejemplo.com"),
        activo: true,
        intentos_login: 0,
    };

    println!("{} — activo: {}", user1.nombre, user1.activo);
}
```

Para modificar un campo el **struct entero** debe ser `mut` (no se puede marcar un campo
individual como mutable):

```rust
let mut user1 = Usuario { /* ... */ };
user1.email = String::from("nueva@ejemplo.com");
```

### Field init shorthand

Cuando el nombre del parámetro o variable coincide con el nombre del campo, puedes
omitir la repetición:

```rust
fn crear_usuario(nombre: String, email: String) -> Usuario {
    Usuario {
        nombre,             // shorthand: nombre: nombre
        email,              // shorthand: email: email
        activo: true,
        intentos_login: 0,
    }
}
```

### Struct update syntax

Crea una instancia nueva a partir de otra existente, copiando los campos que no
especifiques. Los campos que no son `Copy` se **mueven**:

```rust
let user2 = Usuario {
    email: String::from("otro@ejemplo.com"),
    ..user1     // los campos restantes provienen de user1
};
// ⚠️ user1.nombre fue MOVIDO a user2 -> user1 ya no es completamente válido
// user1.activo y user1.intentos_login siguen accesibles (son Copy)
```

### Tuple structs

Structs sin nombres de campo, accedidos por posición (`.0`, `.1`, …). Útiles para dar
un nombre significativo a una tupla o para el patrón *newtype*:

```rust
struct Color(u8, u8, u8);
struct Punto(f64, f64, f64);

let negro = Color(0, 0, 0);
let origen = Punto(0.0, 0.0, 0.0);

println!("R={}, G={}, B={}", negro.0, negro.1, negro.2);
```

`Color` y `Punto` son tipos **distintos** aunque tengan la misma estructura interna.
El compilador te impide pasar un `Color` donde se espera un `Punto`.

### Unit structs

Sin campos. Útiles como tipos marcadores para traits:

```rust
struct SiempreIgual;

let a = SiempreIgual;
let b = SiempreIgual;
```

### Métodos con `impl`

Los métodos se definen en un bloque `impl`. El primer parámetro siempre es `self`
(o alguna referencia a él):

```rust
#[derive(Debug)]
struct Rectangulo {
    ancho: f64,
    alto: f64,
}

impl Rectangulo {
    // Constructor asociado (no toma self)
    fn nuevo(ancho: f64, alto: f64) -> Self {
        Self { ancho, alto }
    }

    // Método: toma &self (préstamo inmutable)
    fn area(&self) -> f64 {
        self.ancho * self.alto
    }

    fn perimetro(&self) -> f64 {
        2.0 * (self.ancho + self.alto)
    }

    fn es_cuadrado(&self) -> bool {
        self.ancho == self.alto
    }

    // Método mutante: toma &mut self
    fn escalar(&mut self, factor: f64) {
        self.ancho *= factor;
        self.alto *= factor;
    }

    // Método que consume el struct: toma self (sin &)
    fn convertir_a_cuadrado(self) -> Self {
        let lado = self.ancho.min(self.alto);
        Self::nuevo(lado, lado)
    }
}

fn main() {
    let mut r = Rectangulo::nuevo(3.0, 4.0);
    println!("área={}, perímetro={}", r.area(), r.perimetro());
    println!("¿cuadrado? {}", r.es_cuadrado());
    r.escalar(2.0);
    println!("escalado: {:?}", r);                  // Rectangulo { ancho: 6.0, alto: 8.0 }
    let c = r.convertir_a_cuadrado();               // r se mueve aquí
    println!("cuadrado resultante: {:?}", c);
}
```

| Receptor | Tipo de self | Qué puede hacer |
| :--- | :--- | :--- |
| `&self` | Referencia inmutable | Solo leer campos |
| `&mut self` | Referencia mutable | Leer y modificar campos |
| `self` | Ownership completo | Consumir el struct (move) |
| ninguno | Función asociada | Constructor, factory, constantes |

### `#[derive]`: traits automáticos

El atributo `#[derive]` le pide al compilador que implemente ciertos traits
automáticamente, siempre que todos los campos también los implementen:

```rust
#[derive(Debug, Clone, PartialEq)]
struct Punto {
    x: f64,
    y: f64,
}

fn main() {
    let p1 = Punto { x: 1.0, y: 2.0 };
    let p2 = p1.clone();                    // Clone
    println!("{:?}", p1);                   // Debug: Punto { x: 1.0, y: 2.0 }
    println!("iguales: {}", p1 == p2);      // PartialEq: true
}
```

---

## Enums

Los enums de Rust no son simples constantes enteras como en C o Java. Cada variante puede
**llevar datos de tipos distintos**. Esto los convierte en *tipos suma* (Sum Types), uno de
los pilares de la programación funcional integrado en Rust.

### Enum simple (estilo C)

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum Direccion {
    Norte,
    Sur,
    Este,
    Oeste,
}

fn main() {
    let dir = Direccion::Norte;
    println!("{dir:?}");    // Norte
}
```

### Enum con datos

```rust
#[derive(Debug)]
enum Mensaje {
    Salir,                          // variante vacía (unit)
    Mover { x: i32, y: i32 },      // variante con campos nombrados (struct variant)
    Escribir(String),               // variante con un valor (tuple variant)
    CambiarColor(u8, u8, u8),       // variante con varios valores
}
```

Una variable de tipo `Mensaje` puede ser **cualquiera** de esas cuatro formas, pero
solo una a la vez. El compilador sabe en todo momento cuál es.

### Enums vs structs para modelar estados

```rust
// Con structs necesitarías 4 tipos distintos o un campo "tipo" que puedes olvidar validar
// Con un enum, el tipo ES la validación:
fn procesar(msg: Mensaje) {
    // Si añades una variante al enum, el compilador te avisa aquí que falta un caso
    match msg {
        Mensaje::Salir => println!("cerrando"),
        Mensaje::Mover { x, y } => println!("mover a ({x}, {y})"),
        Mensaje::Escribir(texto) => println!("texto: {texto}"),
        Mensaje::CambiarColor(r, g, b) => println!("color: #{r:02X}{g:02X}{b:02X}"),
    }
}
```

### Métodos en enums

Los enums también pueden tener `impl`:

```rust
impl Mensaje {
    fn ejecutar(&self) {
        match self {
            Mensaje::Salir => println!("adiós"),
            Mensaje::Escribir(t) => println!("{t}"),
            _ => {}     // _ cubre los casos que no nos interesan aquí
        }
    }
}
```

---

## Pattern Matching con `match`

`match` es como un `switch` de C, pero mucho más potente: es **exhaustivo** (el
compilador exige cubrir todos los casos), puede **desestructurar** valores, y es una
**expresión** (devuelve un valor).

### Sintaxis básica

```rust
fn main() {
    let numero = 7;

    let descripcion = match numero {
        1 => "uno",
        2 | 3 | 5 | 7 | 11 => "primo",     // múltiples patrones con |
        13..=19 => "adolescente",            // rango inclusivo
        _ => "otro",                         // comodín (catch-all, DEBE ir al final)
    };

    println!("{numero} es {descripcion}");
}
```

### Desestructuración de enums

```rust
fn area(forma: &Forma) -> f64 {
    match forma {
        Forma::Circulo(radio) => std::f64::consts::PI * radio * radio,
        Forma::Rectangulo(ancho, alto) => ancho * alto,
        Forma::Triangulo { base, altura } => 0.5 * base * altura,
    }
}

#[derive(Debug)]
enum Forma {
    Circulo(f64),
    Rectangulo(f64, f64),
    Triangulo { base: f64, altura: f64 },
}
```

### Guards en `match` (condiciones adicionales)

```rust
fn clasificar(n: i32) -> &'static str {
    match n {
        x if x < 0 => "negativo",
        0 => "cero",
        x if x % 2 == 0 => "positivo par",
        _ => "positivo impar",
    }
}
```

### Binding con `@`

Captura el valor que coincide con un patrón para usarlo en el cuerpo:

```rust
fn main() {
    let num = 15;
    match num {
        n @ 1..=12 => println!("{n} es un mes válido"),
        n @ 13..=19 => println!("{n} es adolescente"),
        n => println!("{n} es otro número"),
    }
}
```

### `match` como expresión

```rust
let mensaje = match estado {
    Estado::Activo => "activo",
    Estado::Inactivo => "inactivo",
    Estado::Suspendido(razon) => razon.as_str(),
};
```

### `if let`: cuando solo importa un caso

Cuando solo te interesa un único patrón y quieres ignorar el resto, `if let` es más
conciso que un `match` con `_ => {}`:

```rust
let config = Some(String::from("debug"));

// Con match:
match config {
    Some(valor) => println!("config: {valor}"),
    None => {}
}

// Con if let (más limpio para un solo caso):
if let Some(valor) = &config {
    println!("config: {valor}");
}
```

`if let` también acepta `else`:

```rust
if let Some(valor) = config {
    println!("hay config: {valor}");
} else {
    println!("sin config");
}
```

### `while let`: loop hasta que el patrón falle

```rust
let mut pila = vec![1, 2, 3];

while let Some(tope) = pila.pop() {
    println!("{tope}");     // 3, 2, 1
}
```

---

## `Option<T>`: el fin del `null`

En Rust no existe `null`. Si un valor puede o no existir, se expresa con `Option<T>`:

```rust
enum Option<T> {
    None,       // no hay valor
    Some(T),    // hay un valor de tipo T
}
```

`Option<T>` está en el preludio; no hace falta importarlo. Puedes usar `Some(x)` y `None`
directamente.

### Métodos esenciales de `Option`

```rust
fn buscar(lista: &[i32], objetivo: i32) -> Option<usize> {
    for (i, &x) in lista.iter().enumerate() {
        if x == objetivo {
            return Some(i);
        }
    }
    None
}

fn main() {
    let datos = vec![10, 20, 30, 40];

    // unwrap_or: valor por defecto si es None
    let indice = buscar(&datos, 20).unwrap_or(999);
    println!("índice: {indice}");   // 1

    // map: transforma el valor interno si es Some
    let doble_indice = buscar(&datos, 20).map(|i| i * 2);
    println!("{doble_indice:?}");   // Some(2)

    // is_some / is_none: comprobación rápida
    if buscar(&datos, 99).is_none() {
        println!("no encontrado");
    }

    // match completo
    match buscar(&datos, 30) {
        Some(i) => println!("encontrado en posición {i}"),
        None => println!("no está"),
    }
}
```

| Método | Descripción | Equivalente JS/Python |
| :--- | :--- | :--- |
| `unwrap()` | Extrae el valor o hace **panic** | `x!` (TypeScript) — peligroso |
| `expect("msg")` | Como `unwrap` pero con mensaje | Solo en tests/main |
| `unwrap_or(default)` | Extrae o devuelve `default` | `x ?? default` |
| `unwrap_or_else(f)` | Extrae o llama a `f()` | `x ?? f()` |
| `map(f)` | Aplica `f` si es `Some`, deja `None` | `?.map(f)` |
| `and_then(f)` | Como `map` pero `f` devuelve `Option` | `?.flatMap(f)` |
| `filter(pred)` | `None` si `pred` es `false` | `?.filter(pred)` |
| `is_some()` / `is_none()` | Comprueba variante | `!== null` / `=== null` |

> ⚠️ **Regla de oro:** Nunca uses `unwrap()` en código de producción o lógica de negocio.
> Usa `match`, `if let`, o los combinadores (`map`, `unwrap_or`, `?`).

### Indexar colecciones devuelve `Option`

```rust
let v = vec![1, 2, 3];

// Acceso seguro: devuelve Option<&i32>
if let Some(primero) = v.get(0) {
    println!("{primero}");
}

// Acceso directo con []: hace panic si el índice no existe
println!("{}", v[0]);  // ok
// println!("{}", v[99]); // panic en tiempo de ejecución
```

Prefiere `v.get(i)` cuando no puedes garantizar que el índice es válido.

---

## `Result<T, E>`: errores sin excepciones

`Result<T, E>` modela operaciones que pueden fallar:

```rust
enum Result<T, E> {
    Ok(T),      // éxito, contiene el valor
    Err(E),     // fallo, contiene el error
}
```

`Result` también está en el preludio.

### Ejemplo básico: parsear un número

```rust
fn main() {
    let numero: Result<i32, _> = "42".parse();
    let error: Result<i32, _> = "abc".parse();

    println!("{numero:?}");     // Ok(42)
    println!("{error:?}");      // Err(ParseIntError { kind: InvalidDigit })

    match numero {
        Ok(n) => println!("número: {n}"),
        Err(e) => println!("error: {e}"),
    }
}
```

### El operador `?`

Es la herramienta más importante para manejar errores en Rust. Dentro de una función que
devuelve `Result`, `?` hace lo siguiente:

- Si el valor es `Ok(v)`: extrae `v` y continúa.
- Si el valor es `Err(e)`: **retorna inmediatamente** con `Err(e)`.

```rust
use std::num::ParseIntError;

fn doblar(s: &str) -> Result<i32, ParseIntError> {
    let n: i32 = s.trim().parse()?;     // si falla, retorna el Err aquí
    Ok(n * 2)
}

fn main() {
    println!("{:?}", doblar("5"));      // Ok(10)
    println!("{:?}", doblar("abc"));    // Err(ParseIntError { ... })
    println!("{:?}", doblar(" 7 "));    // Ok(14)  — trim elimina espacios
}
```

Sin `?` tendrías que escribir esto manualmente en cada línea:

```rust
let n: i32 = match s.trim().parse() {
    Ok(v) => v,
    Err(e) => return Err(e),
};
```

### Combinadores de `Result`

```rust
fn parsear_y_validar(s: &str) -> Result<u32, String> {
    s.trim()
        .parse::<i32>()
        .map_err(|e| format!("no es un número: {e}"))  // convierte el tipo de error
        .and_then(|n| {                                  // encadena otra operación
            if n >= 0 {
                Ok(n as u32)
            } else {
                Err(format!("{n} no puede ser negativo"))
            }
        })
}

fn main() {
    println!("{:?}", parsear_y_validar("42"));    // Ok(42)
    println!("{:?}", parsear_y_validar("-5"));    // Err("no puede ser negativo")
    println!("{:?}", parsear_y_validar("abc"));   // Err("no es un número: ...")
}
```

### `main` puede devolver `Result`

```rust
use std::num::ParseIntError;

fn main() -> Result<(), ParseIntError> {
    let n: i32 = "42".parse()?;
    println!("n = {n}");
    Ok(())
}
```

Si `main` devuelve `Err(e)`, Rust imprime el error y sale con código no cero.

### `Option` vs `Result`

| | `Option<T>` | `Result<T, E>` |
| :--- | :--- | :--- |
| **Representa** | Presencia/ausencia de valor | Éxito/fallo de operación |
| **Variantes** | `Some(T)` / `None` | `Ok(T)` / `Err(E)` |
| **¿Cuándo usar?** | Búsquedas, campos opcionales | I/O, parsing, operaciones externas |
| **Convierte a otro** | `.ok_or(err)` → `Result` | `.ok()` → `Option` |

---

## Desestructuración avanzada

El pattern matching no se limita a `match`. Funciona en `let`, parámetros de funciones,
y `for`:

```rust
struct Punto { x: f64, y: f64 }

fn main() {
    // Desestructuración en let
    let Punto { x, y } = Punto { x: 3.0, y: 4.0 };
    println!("x={x}, y={y}");

    // Desestructuración de tupla en let
    let (a, b, c) = (1, "hola", 3.14);
    println!("{a}, {b}, {c}");

    // Desestructuración en parámetro de función
    let puntos = vec![Punto { x: 0.0, y: 0.0 }, Punto { x: 1.0, y: 2.0 }];
    for Punto { x, y } in &puntos {
        println!("({x}, {y})");
    }

    // Ignorar campos con ..
    struct Config { debug: bool, nivel: u32, nombre: String }
    let cfg = Config { debug: true, nivel: 3, nombre: String::from("app") };
    let Config { debug, .. } = cfg;   // solo nos interesa debug
    println!("debug: {debug}");
}
```

---

## Mini-proyecto: Lista de tareas en memoria

Une todo lo aprendido en la semana: structs, enums, `impl`, `Option`, pattern matching.

Crea un proyecto con `cargo new todo_v1` y divide el código en estos archivos:

### `src/models.rs`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Estado {
    Pendiente,
    EnProgreso,
    Terminado,
}

impl Estado {
    pub fn icono(&self) -> &str {
        match self {
            Estado::Pendiente => "[ ]",
            Estado::EnProgreso => "[~]",
            Estado::Terminado => "[x]",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tarea {
    pub id: u64,
    pub descripcion: String,
    pub estado: Estado,
}

impl Tarea {
    pub fn nueva(id: u64, descripcion: String) -> Self {
        Self {
            id,
            descripcion,
            estado: Estado::Pendiente,
        }
    }
}
```

### `src/store.rs`

```rust
use crate::models::{Estado, Tarea};

pub struct Store {
    tareas: Vec<Tarea>,
    siguiente_id: u64,
}

impl Store {
    pub fn nuevo() -> Self {
        Self {
            tareas: Vec::new(),
            siguiente_id: 1,
        }
    }

    pub fn agregar(&mut self, descripcion: String) -> u64 {
        let id = self.siguiente_id;
        self.tareas.push(Tarea::nueva(id, descripcion));
        self.siguiente_id += 1;
        id
    }

    pub fn listar(&self) -> &[Tarea] {
        &self.tareas
    }

    pub fn buscar(&self, id: u64) -> Option<&Tarea> {
        self.tareas.iter().find(|t| t.id == id)
    }

    pub fn completar(&mut self, id: u64) -> Option<()> {
        let tarea = self.tareas.iter_mut().find(|t| t.id == id)?;
        tarea.estado = Estado::Terminado;
        Some(())
    }

    pub fn eliminar(&mut self, id: u64) -> Option<Tarea> {
        let pos = self.tareas.iter().position(|t| t.id == id)?;
        Some(self.tareas.remove(pos))
    }
}
```

### `src/main.rs`

```rust
mod models;
mod store;

use std::io::{self, Write};
use store::Store;

fn leer_linea() -> String {
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).expect("error al leer");
    buf.trim().to_string()
}

fn imprimir_tareas(store: &Store) {
    let tareas = store.listar();
    if tareas.is_empty() {
        println!("  (sin tareas)");
        return;
    }
    for t in tareas {
        println!("  {} [{}] {}", t.estado.icono(), t.id, t.descripcion);
    }
}

fn main() {
    let mut store = Store::nuevo();

    loop {
        println!("\n--- TODO ---");
        println!("1. Agregar   2. Listar   3. Completar   4. Eliminar   5. Salir");
        print!("> ");
        io::stdout().flush().expect("flush fallido");

        match leer_linea().as_str() {
            "1" => {
                print!("Descripción: ");
                io::stdout().flush().expect("flush fallido");
                let desc = leer_linea();
                if desc.is_empty() {
                    println!("La descripción no puede estar vacía.");
                } else {
                    let id = store.agregar(desc);
                    println!("Tarea #{id} creada.");
                }
            }
            "2" => {
                imprimir_tareas(&store);
            }
            "3" => {
                print!("ID a completar: ");
                io::stdout().flush().expect("flush fallido");
                match leer_linea().parse::<u64>() {
                    Ok(id) => match store.completar(id) {
                        Some(()) => println!("Tarea #{id} completada."),
                        None => println!("No existe la tarea #{id}."),
                    },
                    Err(_) => println!("Introduce un número válido."),
                }
            }
            "4" => {
                print!("ID a eliminar: ");
                io::stdout().flush().expect("flush fallido");
                match leer_linea().parse::<u64>() {
                    Ok(id) => match store.eliminar(id) {
                        Some(t) => println!("Eliminada: '{}'.", t.descripcion),
                        None => println!("No existe la tarea #{id}."),
                    },
                    Err(_) => println!("Introduce un número válido."),
                }
            }
            "5" | "salir" | "exit" => {
                println!("¡Hasta luego!");
                break;
            }
            otro => println!("Opción desconocida: '{otro}'."),
        }
    }
}
```

Ejecuta con `cargo run`. Comprueba que:

- `cargo clippy` no da warnings.
- No hay ningún `unwrap()` en la lógica de negocio (solo en los `flush`, que nunca
  fallan en la práctica).
- Añadir una nueva variante a `Estado` hace que el compilador indique dónde añadir el
  nuevo caso en `icono()` y en cualquier `match` sobre `Estado`.

---

## ✅ Checklist de la Semana 3

- [ ] Defino structs con campos nombrados y les añado métodos con `impl`.
- [ ] Distingo `&self`, `&mut self`, `self` (consumo) y funciones asociadas.
- [ ] Derivo `Debug`, `Clone`, `PartialEq` cuando todos los campos los soportan.
- [ ] Defino enums con variantes que llevan datos (unit, tuple y struct variants).
- [ ] Escribo `match` exhaustivos y el compilador me avisa si falta un caso.
- [ ] Uso `if let` cuando solo me interesa un patrón; `while let` para loops.
- [ ] Entiendo `Option<T>` y uso `map`, `unwrap_or`, `?` en lugar de `unwrap()`.
- [ ] Entiendo `Result<T, E>` y uso el operador `?` para propagar errores.
- [ ] Completo los ejercicios Rustlings: `structs/`, `enums/`, `option/`, `result/`, `match/`.
- [ ] El mini-proyecto `todo_v1` compila con `cargo clippy` limpio y funciona desde la terminal.

> **Siguiente paso:** Semana 4 — [Módulos, Colecciones, String vs &str, Error Handling
> Avanzado y Traits básicos](section_06.md).
