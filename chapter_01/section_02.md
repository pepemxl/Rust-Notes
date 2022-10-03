# Aritmética en Rust

## Suma de enteros

```rust
print!("{}+{} = {}", 2,3,2+3)
```
salida
```rust
2+3 = 5
```

Aqui todos los literal numbers y su suma son convertidos a literal strings en tiempo de ejecución.

La aritmética de los números occure tal cual ocurre en los compiladores de C, asi que tenemos los operatores comunes
- `+`
- `-`
- `*`
- `/`
- `%`

con la misma precedencia de operaciones.

Es decir, la multiplicación y división tienen precedencia sobre suma y resta, y operaciones de la misma precedencia se evaluan en orden de izquierda a derecha.


Con esto en mente tiene sentido las siguientes evaluación de operaciones:


```rust
println!("{}", 2*2+3*(4-1))
println!("{}", 2*2%3+3*(4-1))
println!("{}", 4%3*2+3*(4-1))
println!("{}", 15/4))
```
con salida:

```bash
D:\Rust-Notes\chapter_01\codes\example_06> cargo run
   Compiling example_06 v0.1.0 (D:\Rust-Notes\chapter_01\codes\example_06)
    Finished dev [unoptimized + debuginfo] target(s) in 1.63s
     Running `target\debug\example_06.exe`
13
10
11
3
```
```bash
2*2+3*(4-1) -> 2*2+3*3 -> 4 + 9 -> 13
```

```bash
2*2%3+3*(4-1) -> 4%3+3*3 -> 1 + 9 -> 10
```


```bash
4%3*2+3*(4-1) -> 1*2+3*3 -> 11
```


Operaciones con números enteros son tratadas como operaciones de números enteros con resultado como número entero, por lo cual división entre números enteros es por defecto truncada.




## Aritmética de punto flotante

Las operaciones aritméticas como suma, resta, multiplicación y división solo se puede realizar entre elementos del mismo tipo, enteros con enteros o flotantes con flotantes, si intentamos hacer una división con dos tipos distintos obtendremos el siguiente mensaje por parte del compilador:

```bash
error[E0277]: cannot divide `{integer}` by `{float}`
 --> src\main.rs:5:19
  |
5 |     print!("{}", 1/2.0);
  |                   ^ no implementation for `{integer} / {float}`
  |
  = help: the trait `Div<{float}>` is not implemented for `{integer}`
```

```rust
error[E0277]: cannot divide `{float}` by `{integer}`
 --> src\main.rs:4:21
  |
4 |     print!("{}", 1.0/2);
  |                     ^ no implementation for `{float} / {integer}`
  |
  = help: the trait `Div<{integer}>` is not implemented for `{float}`
```

Pero inclusive en operaciones sencillas

```rust
error[E0277]: cannot add an integer to a float
 --> src\main.rs:8:23
  |
8 |     print!("{}", 10.3 + 1);
  |                       ^ no implementation for `{float} + {integer}`
  |
  = help: the trait `Add<{integer}>` is not implemented for `{float}`

error[E0277]: cannot multiply `{float}` by `{integer}`
 --> src\main.rs:9:23
  |
9 |     print!("{}", 10.3 * 1);
  |                       ^ no implementation for `{float} * {integer}`
  |
  = help: the trait `Mul<{integer}>` is not implemented for `{float}`
```

Sin embargo la solución para estos casos es muy sencilla, agregar un punto al final de cada número entero lo convierte en un literal flotante y con ello puede ser operado como cualquier otro número flotante. En muchos lenguajes la conversión de entero a flotante se realiza de manera implicita sin embargo en Rust esto debe realizarse de manera explicita.

Además la suma se realiza con mantiza lo cual provoca efectos  que acostumbramos ver en lenaguajes que utilizan mantiza, como el siguiente

```rust
fn main() {
    println!("{}", 10.3 + 10.8);
    println!("{}", 10.3 + 10.9); 
}
```
salida:

```bash
21.1
21.200000000000003
```


### Aritmética Modular


Como en C/C++ tenemos la operación modulo que nos permnite calcular residuos de divisiones, inclusive para casos no enteros.



```rust
10%3: 1
-10%3: -1
10%(-3): 1
-10%(-3): -1
10.%3.: 1
-10.%3.: -1
10.1%3.: 1.0999999999999996
```

Aunque es igual de sencillo de utilizar que los demas operadores, tenemos que tener cuidado con el comportamiento de los números flotantes.



## Rompiendo cadena de caracteres

A diferencia de C/C++ aqui podemos definir cadena de caracteres a tráves de varias lineas de código:

```rust
fn main() {
    println!("Hello, 
    world!");
}
```
salida:
```bash
D:\Rust-Notes\chapter_01\codes\example_08> cargo run
   Compiling example_08 v0.1.0 (D:\Rust-Notes\chapter_01\codes\example_08)
    Finished dev [unoptimized + debuginfo] target(s) in 1.12s
     Running `target\debug\example_08.exe`
Hello, 
    world!
```

Si queremos prevenir que se agregue el salto de linea asi como los espacios basta con que agreguemos una diagonal invertida


```rust
fn main() {
    println!("Hello, \
    world!");
}
```

salida:

```bash
D:\Rust-Notes\chapter_01\codes\example_08> cargo run
   Compiling example_08 v0.1.0 (D:\Rust-Notes\chapter_01\codes\example_08)
    Finished dev [unoptimized + debuginfo] target(s) in 1.12s
     Running `target\debug\example_08.exe`
Hello, world!
```


