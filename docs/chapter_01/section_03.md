# Variables, Tipos, Funciones y Control de Flujo

En esta sección cubrimos el resto de la **sintaxis básica de la Semana 1**. Después de
haber visto cómo [instalar Rust e imprimir a pantalla](section_01.md) y la
[aritmética](section_02.md), aquí aprenderemos:

- Cómo declarar variables y qué significa que sean **inmutables por defecto**.
- La diferencia entre **mutabilidad** (`mut`) y **shadowing**.
- Las constantes (`const`) y por qué no son lo mismo que una variable inmutable.
- Los **tipos escalares** (enteros, flotantes, booleanos, caracteres) y los
  **tipos compuestos** (tuplas y arreglos).
- Cómo escribir **funciones**, y la distinción clave entre **sentencias** y **expresiones**.
- El **control de flujo**: `if`/`else`, `loop`, `while` y `for`.

> 💡 **Filosofía de la Semana 1:** *El compilador es tu pair programmer más estricto.*
> Si algo no compila, lee el mensaje de error completo: casi siempre te dice exactamente
> qué arreglar.

---

## Variables

En Rust las variables se declaran con la palabra clave `let`. A diferencia de C/C++ o
Python, en Rust las variables son **inmutables por defecto**: una vez que les asignas un
valor, no puedes cambiarlo.

```rust
fn main() {
    let x = 5;
    println!("El valor de x es: {x}");
    x = 6; // ❌ NO COMPILA
    println!("El valor de x es: {x}");
}
```

El compilador nos detiene con un mensaje muy claro:

```bash
error[E0384]: cannot assign twice to immutable variable `x`
 --> src/main.rs:4:5
  |
2 |     let x = 5;
  |         - first assignment to `x`
3 |     ...
4 |     x = 6;
  |     ^^^^^ cannot assign twice to immutable variable
```

Esto **no es un capricho del lenguaje**: la inmutabilidad por defecto evita una enorme
cantidad de bugs en programas grandes, donde un valor cambia "por accidente" en un lugar
inesperado.

### Variables mutables: `mut`

Cuando *sí* queremos poder cambiar el valor, lo indicamos explícitamente con `mut`:

```rust
fn main() {
    let mut x = 5;
    println!("El valor de x es: {x}"); // 5
    x = 6;
    println!("El valor de x es: {x}"); // 6
}
```

La intención queda escrita en el código: cualquiera que lo lea sabe que `x` está pensada
para cambiar.

### Interpolación dentro de las llaves

Nota que escribimos `{x}` directamente dentro del literal string. Desde Rust 1.58 podemos
poner el nombre de la variable dentro de las llaves. Las dos formas siguientes son
equivalentes:

```rust
let nombre = "Rust";
println!("Hola {nombre}");      // forma moderna (1.58+)
println!("Hola {}", nombre);    // forma clásica
```

> ⚠️ Solo funciona con **nombres de variables simples**. Para expresiones (`a + b`,
> `obj.campo`) hay que seguir usando `{}` con el argumento por separado:
> `println!("{}", a + b);`.

---

## Shadowing (sombreado)

El *shadowing* permite declarar una **nueva** variable con el mismo nombre de una anterior.
No es lo mismo que `mut`: aquí creamos una variable totalmente nueva que "tapa" a la previa.

```rust
fn main() {
    let x = 5;
    let x = x + 1;      // nueva x = 6
    let x = x * 2;      // nueva x = 12
    println!("El valor de x es: {x}"); // 12
}
```

La gran ventaja del shadowing sobre `mut` es que **podemos cambiar el tipo** del valor
manteniendo el mismo nombre:

```rust
fn main() {
    let espacios = "   ";          // &str (texto)
    let espacios = espacios.len(); // ahora es usize (número)
    println!("Hay {espacios} espacios"); // 3
}
```

Con `mut` esto **no** sería posible, porque `mut` cambia el valor pero **no el tipo**:

```rust
fn main() {
    let mut espacios = "   ";
    espacios = espacios.len(); // ❌ NO COMPILA: expected `&str`, found `usize`
}
```

| | `let mut` | Shadowing (`let` repetido) |
| :--- | :--- | :--- |
| ¿Cambia el valor? | Sí | Sí (es una variable nueva) |
| ¿Cambia el tipo? | **No** | **Sí** |
| ¿Necesita `mut`? | Sí | No |
| Dirección en memoria | La misma | Nueva |

---

## Constantes: `const`

Las constantes se parecen a las variables inmutables, pero tienen diferencias importantes:

```rust
const MAX_PUNTOS: u32 = 100_000;

fn main() {
    println!("El máximo de puntos es: {MAX_PUNTOS}");
}
```

- Se declaran con `const` (no con `let`).
- El **tipo es obligatorio** (`: u32`), no se infiere.
- **Nunca** pueden ser `mut`.
- Su valor debe ser conocido en **tiempo de compilación** (no puede ser el resultado de
  una llamada en tiempo de ejecución).
- Por convención se escriben en `SCREAMING_SNAKE_CASE`.
- Pueden declararse en cualquier ámbito, incluso fuera de `main` (ámbito global).

> 💡 El guion bajo `_` en `100_000` es solo un separador visual de miles; el compilador lo
> ignora. `1_000_000` se lee mucho mejor que `1000000`.

---

## Tipos de datos

Rust es un lenguaje de **tipado estático**: el tipo de cada valor debe conocerse en tiempo
de compilación. Casi siempre el compilador lo **infiere** por nosotros, pero a veces hay
que anotarlo explícitamente.

```rust
let inferido = 42;          // el compilador deduce i32
let explicito: i64 = 42;    // anotamos el tipo manualmente
```

### Tipos escalares

Un tipo escalar representa **un solo valor**. Rust tiene cuatro:

#### 1. Enteros

| Longitud | Con signo | Sin signo |
| :--- | :--- | :--- |
| 8 bits | `i8` | `u8` |
| 16 bits | `i16` | `u16` |
| 32 bits | `i32` (default) | `u32` |
| 64 bits | `i64` | `u64` |
| 128 bits | `i128` | `u128` |
| arch | `isize` | `usize` |

- `i` = *signed* (con signo, admite negativos). `u` = *unsigned* (solo ≥ 0).
- El tipo por defecto de un entero es **`i32`**.
- `usize` / `isize` dependen de la arquitectura (64 bits en una máquina de 64 bits). Se usan
  para **índices y tamaños** (por ejemplo, indexar un arreglo siempre usa `usize`).

```rust
let decimal = 98_222;       // i32
let hex = 0xff;             // hexadecimal
let octal = 0o77;           // octal
let binario = 0b1111_0000;  // binario
let byte = b'A';            // u8 (solo para u8)
```

#### 2. Flotantes

```rust
let x = 2.0;        // f64 (default)
let y: f32 = 3.0;   // f32
```

Hay dos: `f32` y `f64`. El **default es `f64`** porque en CPUs modernas tiene
prácticamente la misma velocidad que `f32` pero más precisión.

#### 3. Booleanos

```rust
let verdadero = true;
let falso: bool = false;
```

Ocupan 1 byte. Se usan en condicionales (`if`, `while`).

#### 4. Caracteres: `char`

```rust
let letra = 'z';
let signo = '$';
let emoji = '🦀';   // ¡válido!
```

Un detalle importante para quien viene de C/C++: en Rust un `char` **NO** es un byte. Es un
**Unicode Scalar Value** de **4 bytes**, capaz de representar mucho más que ASCII (acentos,
emoji, etc.). Se escribe con **comillas simples** `'a'`; las comillas dobles `"a"` son para
strings.

### Tipos compuestos

Agrupan varios valores en uno solo. Los dos primitivos son la **tupla** y el **arreglo**.

#### Tuplas

Agrupan valores de tipos **potencialmente distintos**. Tienen tamaño fijo.

```rust
fn main() {
    let tupla: (i32, f64, char) = (500, 6.4, 'a');

    // Acceso por índice con punto:
    let primero = tupla.0;
    let segundo = tupla.1;
    println!("{primero}, {segundo}"); // 500, 6.4

    // O por desestructuración:
    let (x, y, z) = tupla;
    println!("x={x}, y={y}, z={z}"); // x=500, y=6.4, z=a
}
```

La tupla vacía `()` se llama **unit** y representa "ningún valor"; es lo que devuelve
implícitamente una función que no retorna nada.

#### Arreglos

Agrupan valores del **mismo tipo** y tienen **tamaño fijo** (conocido en compilación). Viven
en el *stack*.

```rust
fn main() {
    let a = [1, 2, 3, 4, 5];        // [i32; 5]
    let b: [i32; 5] = [1, 2, 3, 4, 5];
    let ceros = [0; 3];             // [0, 0, 0]  (valor; cantidad)

    println!("{}", a[0]);           // 1
    println!("{}", a.len());        // 5
}
```

> ⚠️ Si intentas acceder a un índice fuera de rango (`a[10]`), Rust hace **panic** en
> tiempo de ejecución en lugar de leer memoria inválida como haría C. Esta verificación de
> límites es parte de las garantías de seguridad del lenguaje.

Cuando necesites una lista de **tamaño dinámico**, usarás `Vec<T>` (lo veremos más adelante);
para la Semana 1 nos basta con arreglos.

---

## Funciones

Las funciones se declaran con `fn`. La convención de nombres en Rust es `snake_case`.

```rust
fn main() {
    println!("Inicio");
    saludar();
    let suma = sumar(3, 4);
    println!("3 + 4 = {suma}");
}

fn saludar() {
    println!("¡Hola desde otra función!");
}

fn sumar(a: i32, b: i32) -> i32 {
    a + b
}
```

Puntos clave:

- Los **parámetros** deben llevar su tipo anotado obligatoriamente: `a: i32`.
- El **tipo de retorno** se indica con `-> Tipo`.
- El orden de definición **no importa**: `main` puede llamar a funciones definidas después.

### Sentencias vs Expresiones

Esta es una de las ideas centrales de Rust y conviene entenderla bien:

- Una **sentencia** (*statement*) ejecuta una acción pero **no devuelve un valor**.
  Por ejemplo, `let x = 5;`.
- Una **expresión** (*expression*) **evalúa a un valor**. Por ejemplo, `5 + 6`, una llamada
  a función, o un bloque `{ ... }`.

Fíjate en la función `sumar` anterior: la última línea es `a + b` **sin punto y coma**.
Eso es una **expresión** y su valor es lo que la función devuelve. Si le pusiéramos `;` se
convertiría en una sentencia y la función devolvería `()` (unit), provocando un error de
tipo:

```rust
fn sumar(a: i32, b: i32) -> i32 {
    a + b;   // ❌ con el ; devuelve (), pero se esperaba i32
}
```

Un bloque `{}` también es una expresión: su valor es el de su última línea sin `;`.

```rust
fn main() {
    let y = {
        let x = 3;
        x + 1      // sin ; -> este es el valor del bloque
    };
    println!("y = {y}"); // 4
}
```

La palabra clave `return` existe, pero en Rust **idiomático** se reserva para los *early
returns* (salir antes del final de la función). El valor normal de retorno se da con la
expresión final sin `;`.

---

## Control de flujo

### `if` / `else if` / `else`

```rust
fn main() {
    let numero = 6;

    if numero % 4 == 0 {
        println!("divisible por 4");
    } else if numero % 3 == 0 {
        println!("divisible por 3");
    } else {
        println!("ni 4 ni 3");
    }
}
```

A diferencia de C/C++, la **condición debe ser un `bool`**. No existe la conversión
automática de números a booleanos:

```rust
let numero = 3;
if numero {            // ❌ NO COMPILA: expected `bool`, found integer
    println!("...");
}
// Correcto: if numero != 0 { ... }
```

Como `if` es una **expresión**, podemos usarlo para asignar un valor (similar al operador
ternario `? :` de C, pero más legible):

```rust
fn main() {
    let condicion = true;
    let numero = if condicion { 5 } else { 6 };
    println!("numero = {numero}"); // 5
}
```

> ⚠️ Ambas ramas del `if` deben devolver el **mismo tipo**. `if condicion { 5 } else { "seis" }`
> no compila.

### `loop`

`loop` repite **indefinidamente** hasta que un `break` lo detenga. Lo interesante: `break`
puede **devolver un valor**, convirtiendo el `loop` en una expresión.

```rust
fn main() {
    let mut contador = 0;

    let resultado = loop {
        contador += 1;
        if contador == 10 {
            break contador * 2; // devuelve 20 al salir
        }
    };

    println!("resultado = {resultado}"); // 20
}
```

### `while`

Repite **mientras** la condición sea verdadera.

```rust
fn main() {
    let mut n = 3;
    while n != 0 {
        println!("{n}!");
        n -= 1;
    }
    println!("¡Despegue!");
}
```

### `for`

La forma **idiomática y más segura** de recorrer colecciones. Evita errores de índice y es
más legible.

```rust
fn main() {
    let a = [10, 20, 30, 40, 50];

    for elemento in a {
        println!("valor: {elemento}");
    }

    // Recorrer un rango (1..4 = 1, 2, 3; el límite superior es exclusivo):
    for numero in 1..4 {
        println!("{numero}");
    }

    // Rango inverso con .rev():
    for numero in (1..4).rev() {
        println!("{numero}"); // 3, 2, 1
    }
}
```

| Bucle | Cuándo usarlo |
| :--- | :--- |
| `for` | Recorrer una colección o un rango. **El más usado.** |
| `while` | Repetir mientras se cumpla una condición dinámica. |
| `loop` | Repetir hasta un `break`; útil cuando necesitas devolver un valor. |

---

## 🧪 Mini-reto de la Semana 1

Crea un proyecto con `cargo new fizzbuzz` y resuelve el clásico **FizzBuzz** usando lo
aprendido en esta sección (variables, `for`, rangos, `if`/`else if`/`else`):

```rust
fn main() {
    for n in 1..=20 {              // 1..=20 incluye el 20 (rango inclusivo)
        if n % 15 == 0 {
            println!("FizzBuzz");
        } else if n % 3 == 0 {
            println!("Fizz");
        } else if n % 5 == 0 {
            println!("Buzz");
        } else {
            println!("{n}");
        }
    }
}
```

Ejecútalo con `cargo run` y verifica la salida. Luego, como extensión:

1. Convierte la lógica en una función `fn fizzbuzz(n: u32) -> String` que **devuelva** el
   texto (practica sentencias vs expresiones y el tipo de retorno).
2. Cambia el rango a `1..=100`.

---

## ✅ Checklist de la Semana 1

- [ ] Entorno con `rustup`, `cargo`, `clippy` y `rust-analyzer` funcionando.
- [ ] Sé declarar variables y explico por qué son **inmutables por defecto**.
- [ ] Distingo **`mut`** de **shadowing** (y sé que el shadowing puede cambiar el tipo).
- [ ] Conozco la diferencia entre una **constante** (`const`) y una variable inmutable.
- [ ] Identifico los tipos escalares (`i32`, `f64`, `bool`, `char`) y compuestos (tuplas, arreglos).
- [ ] Escribo funciones con parámetros y retorno; entiendo **sentencias vs expresiones**.
- [ ] Uso `if`/`else`, `loop`, `while` y `for` con soltura; sé que `if`/`loop` son expresiones.

> **Siguiente paso:** completa los ejercicios de
> [Rustlings](section_00.md) (`variables`, `functions`, `if`, `primitive_types`) y prepárate
> para la Semana 2, donde llega el corazón de Rust: **ownership y borrowing**.
