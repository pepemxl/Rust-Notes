# Aritmética en Rust

Seguimos con la **Semana 1**. Después de [imprimir texto y números](section_01.md), en
esta sección aprenderemos:

- Los operadores aritméticos, su precedencia y los operadores compuestos (`+=`, `-=`…).
- Cómo funcionan la **división entera** y el **módulo** con números negativos.
- Por qué Rust **no mezcla** enteros y flotantes, y cómo convertir con `as`.
- Qué pasa cuando un entero se **desborda** (*overflow*) y cómo controlarlo.
- Los flotantes IEEE 754: redondeo, `NaN`, infinito y métodos matemáticos.
- Cadenas en varias líneas y *raw strings*.

> 💡 **Filosofía:** *Rust no hace conversiones numéricas implícitas ni oculta
> desbordamientos. Todo lo que podría perder información tiene que estar escrito en el código.*

---

## Operadores aritméticos

Los operadores son los mismos que en C:

| Operador | Operación | Ejemplo | Resultado |
| :--- | :--- | :--- | :--- |
| `+` | Suma | `2 + 3` | `5` |
| `-` | Resta | `2 - 3` | `-1` |
| `*` | Multiplicación | `2 * 3` | `6` |
| `/` | División | `7 / 2` | `3` |
| `%` | Residuo (módulo) | `7 % 2` | `1` |

([example_05](https://github.com/pepemxl/Rust-Notes/blob/master/src/chapter_01/example_05/src/main.rs))

```rust
--8<-- "src/chapter_01/example_05/src/main.rs"
```

```text
2+3 = 5
```

### Precedencia

La precedencia también es la de C: `*`, `/` y `%` se evalúan antes que `+` y `-`, y los
operadores de la misma precedencia se evalúan de izquierda a derecha. Los paréntesis
cambian el orden ([example_06](https://github.com/pepemxl/Rust-Notes/blob/master/src/chapter_01/example_06/src/main.rs)):

```rust
--8<-- "src/chapter_01/example_06/src/main.rs"
```

```text
13
10
11
3
```

Paso a paso:

```text
2*2+3*(4-1)   -> 2*2+3*3 -> 4 + 9         -> 13
2*2%3+3*(4-1) -> 4%3+3*3 -> 1 + 9         -> 10
4%3*2+3*(4-1) -> 1*2+3*3 -> 2 + 9         -> 11
15/4          -> 3   (división entera: la parte decimal se descarta)
```

> 💡 Los espacios alrededor de los operadores son los que pone `cargo fmt`; no cambian la
> precedencia. Deja que el formateador decida el estilo y usa paréntesis cuando la
> precedencia no sea obvia para quien lee.

### Operadores compuestos

Como en C, cada operador tiene su versión compuesta, que modifica una variable **mutable**
(`mut`, lo veremos en la [siguiente sección](section_03.md)). Rust **no** tiene `++` ni
`--`: se escribe `x += 1`.

```rust
fn main() {
    let mut x = 10;
    x += 5;  // 15
    x -= 3;  // 12
    x *= 2;  // 24
    x /= 4;  // 6
    x %= 4;  // 2
    println!("{x}");
}
```

```text
2
```

---

## División entera y módulo

Una operación entre enteros siempre produce un entero. La división **trunca hacia cero**,
y el signo del residuo `%` es el del **dividendo** (el número de la izquierda), igual que
en C:

```rust
fn main() {
    println!("{}", 7 / 2);    // 3
    println!("{}", -7 / 2);   // -3  (no -4: trunca hacia cero)
    println!("{}", 7 % 2);    // 1
    println!("{}", -7 % 2);   // -1  (mismo signo que el dividendo)
}
```

Si necesitas el **módulo matemático** (siempre no negativo, útil para índices circulares
o días de la semana), usa `rem_euclid` y `div_euclid`:

```rust
fn main() {
    println!("{}", (-7_i32).rem_euclid(2)); // 1
    println!("{}", (-7_i32).div_euclid(2)); // -4
}
```

La tabla completa de signos ([example_07](https://github.com/pepemxl/Rust-Notes/blob/master/src/chapter_01/example_07/src/main.rs)):

```text
10 % 3     =  1
-10 % 3    = -1
10 % -3    =  1
-10 % -3   = -1
10. % 3.   =  1
-10. % 3.  = -1
10.1 % 3.  =  1.0999999999999996
```

`%` también funciona con flotantes; el último resultado anticipa el problema de precisión
que veremos más abajo.

Dividir un entero entre cero **no** es comportamiento indefinido como en C: el programa se
detiene con un *panic* controlado.

```text
thread 'main' panicked at src/main.rs:1:37:
attempt to divide by zero
```

---

## Enteros y flotantes no se mezclan

Las operaciones aritméticas solo se pueden hacer entre valores **del mismo tipo**. Si
mezclamos un entero con un flotante, el programa no compila:

```rust
fn main() {
    print!("{}", 1 / 2.0); // ❌ NO COMPILA
}
```

```text
error[E0277]: cannot divide `{integer}` by `{float}`
 --> src/main.rs:2:19
  |
2 |     print!("{}", 1/2.0);
  |                   ^ no implementation for `{integer} / {float}`
  |
  = help: the trait `Div<{float}>` is not implemented for `{integer}`
```

Lo mismo pasa con `1.0 / 2`, `10.3 + 1` o `10.3 * 1`. Muchos lenguajes convierten el entero
a flotante en silencio; Rust exige que lo digas.

**Con literales**, basta con escribir el número como flotante: `1.0 / 2.0` o incluso
`1. / 2.`.

**Con variables**, se convierte con `as`:

```rust
fn main() {
    let n = 7;
    let m = 2;
    println!("{}", n as f64 / m as f64); // 3.5
}
```

`as` es explícito, pero **puede perder información**, así que conviene conocer sus reglas:

```rust
fn main() {
    println!("{}", 3.99_f64 as i32);   // 3     trunca hacia cero
    println!("{}", -3.99_f64 as i32);  // -3
    println!("{}", 300.0_f64 as u8);   // 255   flotante → entero: satura al máximo
    println!("{}", 300_i32 as u8);     // 44    entero → entero más pequeño: se queda con los bits bajos
}
```

> ⚠️ `300_i32 as u8` da `44` (300 − 256) sin avisar. Cuando una conversión puede fallar,
> usa `u8::try_from(x)`, que devuelve un error en lugar de un valor incorrecto; lo veremos
> en [Traits avanzados](../chapter_02/section_02.md) (Mes 2).

El sufijo `_i32`, `_f64`, `_u8` en un literal fija su tipo (`3.99_f64` es un `f64`). Los
tipos numéricos se estudian en detalle en la [siguiente sección](section_03.md).

---

## Desbordamiento de enteros (*overflow*)

Cada tipo entero tiene un rango fijo: un `u8` va de `0` a `255` (`u8::MAX`), un `i32` de
`-2147483648` a `2147483647` (`i32::MAX`). ¿Qué pasa si una operación se sale del rango?

**Si el compilador lo puede ver**, no compila:

```rust
fn main() {
    let x: u8 = 255 + 1; // ❌ NO COMPILA
    println!("{x}");
}
```

```text
error: this arithmetic operation will overflow
 --> src/main.rs:2:17
  |
2 |     let x: u8 = 255 + 1;
  |                 ^^^^^^^ attempt to compute `u8::MAX + 1_u8`, which would overflow
```

**Si ocurre en ejecución**, depende del perfil de compilación:

```rust
fn sumar_uno(x: u8) -> u8 {
    x + 1
}

fn main() {
    println!("{}", sumar_uno(254));
    println!("{}", sumar_uno(255));
}
```

```text
$ cargo run            # perfil dev (debug)
255
thread 'main' panicked at src/main.rs:2:5:
attempt to add with overflow

$ cargo run --release  # perfil release
255
0
```

En *debug* el programa se detiene con un *panic*; en *release*, por rendimiento, el valor
**da la vuelta** (*wrap around*) y `255 + 1` da `0`. En ningún caso es comportamiento
indefinido como en C, pero depender de ese *wrap* silencioso es un bug. Cuando el overflow
sea posible, **elige explícitamente** qué debe pasar:

```rust
fn main() {
    let a: u8 = 250;
    println!("{:?}", a.checked_add(10));     // None       → avisa que falló
    println!("{:?}", a.checked_add(5));      // Some(255)
    println!("{}", a.wrapping_add(10));      // 4          → da la vuelta a propósito
    println!("{}", a.saturating_add(10));    // 255        → se queda en el límite
    println!("{:?}", a.overflowing_add(10)); // (4, true)  → resultado + ¿hubo overflow?
}
```

| Familia | Qué hace | Uso típico |
| :--- | :--- | :--- |
| `checked_*` | Devuelve `Option`: `None` si hay overflow | Validar datos externos |
| `wrapping_*` | Da la vuelta siempre, también en *debug* | Hashes, checksums, criptografía |
| `saturating_*` | Se queda en el mínimo o máximo | Contadores, volumen, colores |
| `overflowing_*` | Devuelve el resultado y un `bool` | Aritmética de precisión múltiple |

Existen para suma, resta, multiplicación, división y potencia (`checked_mul`,
`saturating_sub`, `wrapping_pow`…). `Option` (`Some`/`None`) se explica en la
[Semana 3](section_05.md); por ahora basta con leerlo como "hay valor / no hay valor".

---

## Aritmética de punto flotante

Rust tiene dos tipos flotantes, `f32` y `f64` (el predeterminado). Ambos siguen el estándar
**IEEE 754**: se guardan en binario con una mantisa de precisión finita, así que muchos
decimales (como `10.3`) no tienen representación exacta. Esto produce el mismo efecto de
redondeo que en C, Python o JavaScript:

```rust
fn main() {
    println!("{}", 10.3 + 10.8);
    println!("{}", 10.3 + 10.9);
}
```

```text
21.1
21.200000000000003
```

Por eso **nunca se comparan flotantes con `==`**; se compara contra una tolerancia:

```rust
fn main() {
    let a: f64 = 10.3 + 10.9; // el tipo explícito hace falta para llamar a .abs()
    let b = 21.2;
    assert!((a - b).abs() < 1e-9); // ✅ en vez de a == b
    println!("iguales dentro de la tolerancia");
}
```

> ⚠️ Para dinero **no uses flotantes**: guarda centavos en un entero (`i64`) o usa una
> crate de decimales como `rust_decimal`.

### Infinito y `NaN`

A diferencia de los enteros, dividir un flotante entre cero no produce *panic*:

```rust
fn main() {
    println!("{}", 1.0 / 0.0);        // inf
    println!("{}", -1.0 / 0.0);       // -inf
    let nan = 0.0_f64 / 0.0;
    println!("{}", nan);              // NaN ("Not a Number")
    println!("{}", nan == nan);       // false: NaN no es igual ni a sí mismo
    println!("{}", nan.is_nan());     // true: la forma correcta de detectarlo
}
```

Por esa rareza de `NaN`, los flotantes no implementan `Eq` ni `Ord`, solo `PartialEq` y
`PartialOrd`. Te lo encontrarás cuando intentes ordenar un `Vec<f64>` en la
[Semana 4](section_06.md).

### Métodos matemáticos

Las operaciones matemáticas son **métodos** del tipo, no funciones sueltas como en C
(`sqrt(x)`):

```rust
fn main() {
    println!("{}", 2.0_f64.sqrt());          // 1.4142135623730951
    println!("{}", 2.0_f64.powi(10));        // 1024      potencia entera
    println!("{}", 2.0_f64.powf(0.5));       // 1.4142135623730951
    println!("{}", (-4.5_f64).abs());        // 4.5
    println!("{} {} {}", 2.5_f64.round(), 2.5_f64.floor(), 2.5_f64.ceil()); // 3 2 3
    println!("{}", std::f64::consts::PI);    // 3.141592653589793
    println!("{}", 2_i32.pow(10));           // 1024      los enteros también tienen pow
}
```

Fíjate en el sufijo `_f64`: en `2.0.sqrt()` el compilador no sabría si `2.0` es `f32` o
`f64`, y los métodos dependen del tipo.

---

## Cadenas en varias líneas

A diferencia de C, un literal string puede ocupar varias líneas; el salto de línea y la
indentación **forman parte** de la cadena ([example_08](https://github.com/pepemxl/Rust-Notes/blob/master/src/chapter_01/example_08/src/main.rs)):

```rust
fn main() {
    println!("Hello,
    world!");
}
```

```text
Hello,
    world!
```

Para continuar la cadena en la siguiente línea **sin** incluir el salto ni los espacios
iniciales, termina la línea con `\`:

```rust
fn main() {
    println!("Hello, \
    world!");
}
```

```text
Hello, world!
```

### Raw strings

Un *raw string* `r"..."` no procesa secuencias de escape; es útil para rutas de Windows y
expresiones regulares. Si el texto contiene comillas, se delimita con `#`:

```rust
fn main() {
    println!(r"C:\ruta\sin\escapes");
    println!(r#"Dice "hola" sin escapar"#);
}
```

```text
C:\ruta\sin\escapes
Dice "hola" sin escapar
```

---

## 🧪 Mini-reto: calculadora de tiempo y temperatura

Crea un proyecto con `cargo new calculadora` y escribe un programa que:

1. A partir de `let total_segundos: u32 = 100_000;` calcule horas, minutos y segundos con
   `/` y `%`, e imprima el resultado con dos dígitos para minutos y segundos (`{:02}`).
2. Convierta `let celsius = 36.6;` a Fahrenheit (`F = C × 9 / 5 + 32`) con un decimal.
3. Sume `200_u8` y `100_u8` de tres formas: `checked_add`, `wrapping_add` y `saturating_add`.

Salida esperada:

```text
100000 s = 27:46:40
36.6 °C = 97.9 °F
checked: None | wrapping: 44 | saturating: 255
```

Extensiones:

- ¿Qué pasa si escribes la fórmula como `celsius * (9 / 5) + 32.0`? ¿Y como
  `celsius * 9 / 5 + 32`? Explica cada error o resultado.
- Cambia `total_segundos` a un valor mayor que `u32::MAX` y lee el error del compilador.

---

## ✅ Checklist

- [ ] Conozco la precedencia de `+ - * / %` y uso paréntesis cuando no es obvia.
- [ ] Sé que la división entera trunca hacia cero y que `%` toma el signo del dividendo.
- [ ] Sé cuándo usar `rem_euclid` en lugar de `%`.
- [ ] Entiendo por qué `1 / 2.0` no compila y sé convertir con literales flotantes o con `as`.
- [ ] Conozco los riesgos de `as` (truncar, saturar, perder bits).
- [ ] Sé qué pasa con el overflow en *debug* y en *release*, y uso `checked_`,
  `wrapping_` o `saturating_` según el caso.
- [ ] No comparo flotantes con `==` y sé detectar `NaN` con `is_nan()`.
- [ ] Uso los métodos matemáticos de `f64` (`sqrt`, `powi`, `abs`, `round`…).
- [ ] Escribo cadenas en varias líneas con `\` y *raw strings* con `r"..."`.

> **Siguiente paso:** [Variables, tipos, funciones y control de flujo](section_03.md).
