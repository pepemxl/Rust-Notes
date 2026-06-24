# Ownership, Borrowing y Slices

Esta es la semana más importante del mes. Ownership es **la idea central que distingue a
Rust** de todos los demás lenguajes de sistemas. Una vez que la entiendas, el compilador
pasará de ser un obstáculo a ser tu aliado más valioso.

En esta sección aprenderemos:

- Qué son el *stack* y el *heap* y por qué importa saberlo.
- Las **3 reglas de Ownership** y qué pasa cuando un valor "se mueve".
- La diferencia entre **Move**, **Clone** y **Copy**.
- Las **referencias** (`&T` y `&mut T`) y la regla del *borrow checker*.
- Los **slices** (`&str`, `&[T]`) como vistas sin ownership.

> 💡 **Filosofía de la Semana 2:** *El borrow checker no te persigue; te protege. Cada
> error de compilación que evita hoy es un segfault o un data race que no verás en
> producción.*

---

## Stack y Heap: por qué importa

Para entender ownership hay que entender dónde viven los datos en memoria.

### Stack (pila)

- Almacenamiento **LIFO** (Last In, First Out): el último en entrar es el primero en salir.
- **Muy rápido**: asignar y liberar es solo mover un puntero.
- Solo puede guardar datos de **tamaño conocido en tiempo de compilación**: `i32`, `f64`,
  `bool`, `char`, tuplas y arreglos de tamaño fijo.
- Cuando una función termina, todo lo que puso en el stack **desaparece automáticamente**.

### Heap (montículo)

- Almacenamiento **dinámico**: puedes pedir memoria en tiempo de ejecución.
- **Más lento**: hay que pedir al sistema operativo espacio, y alguien debe liberarlo.
- Aquí viven los datos de tamaño variable o desconocido en compilación: `String`, `Vec<T>`,
  `Box<T>`, etc.
- En el stack se guarda un **puntero** (dirección) hacia los datos reales en el heap.

```text
STACK                              HEAP
┌──────────────────────┐          ┌──────────────────────┐
│  let x: i32 = 5;     │          │                      │
│  ┌────┐              │          │                      │
│  │ 5  │ <── x        │          │                      │
│  └────┘              │          │                      │
│                      │          │                      │
│  let s = String::    │          │ ┌────────────────┐   │
│  from("hola");       │─────────▶│ │ 'h','o','l','a'│   │
│  ┌─────────────────┐ │          │ └────────────────┘   │
│  │ ptr | len | cap │ │          │                      │
│  └─────────────────┘ │          │                      │
└──────────────────────┘          └──────────────────────┘
```

El tipo `String` guarda en el stack: un **puntero** al heap, la **longitud** actual (`len`)
y la **capacidad** reservada (`cap`). Los caracteres reales están en el heap.

---

## Las 3 Reglas de Ownership

Rust impone tres reglas que el compilador verifica en cada programa:

1. **Cada valor tiene exactamente un *owner* (dueño).**
2. **Solo puede haber un owner a la vez.**
3. **Cuando el owner sale de scope, el valor se libera automáticamente** (se llama a `drop`).

Estas tres reglas, juntas, eliminan la necesidad de un *garbage collector* **y** eliminan
los errores de memoria que plagan C/C++.

### Scope y `drop`

```rust
fn main() {
    {                                   // s aún no existe
        let s = String::from("hola");   // s entra en scope, se pide memoria en el heap
        println!("{s}");                // se puede usar s
    }                                   // s sale de scope -> Rust llama a drop(s)
                                        // la memoria del heap se libera aquí
    // println!("{s}");                 // ❌ NO COMPILA: s ya no existe
}
```

No hay que llamar a `free()` ni confiar en un *garbage collector*. Rust inserta la
liberación automáticamente al final del scope del owner.

---

## Move: transferencia de ownership

¿Qué pasa cuando asignamos una variable a otra?

Para tipos que viven **en el heap** (como `String`), Rust hace un **Move**: transfiere el
ownership al nuevo nombre y **invalida el anterior**.

```rust
fn main() {
    let s1 = String::from("hola");
    let s2 = s1;            // MOVE: s2 es el nuevo owner del heap
    println!("{s2}");       // ✅ OK

    // println!("{s1}");    // ❌ NO COMPILA: "borrow of moved value: `s1`"
}
```

El error del compilador:

```bash
error[E0382]: borrow of moved value: `s1`
 --> src/main.rs:5:20
  |
2 |     let s1 = String::from("hola");
  |         -- move occurs because `s1` has type `String`, which does not implement the `Copy` trait
3 |     let s2 = s1;
  |              -- value moved here
4 |     println!("{s2}");
5 |     // println!("{s1}");
  |                    ^^ value borrowed here after move
```

**¿Por qué?** Si tanto `s1` como `s2` apuntaran al mismo heap y ambos llamaran a `drop`
al salir de scope, liberaríamos la misma memoria dos veces: el clásico *double free*, una
vulnerabilidad grave en C. Rust lo impide en tiempo de compilación.

```text
ANTES del move:        DESPUÉS del move (let s2 = s1):
s1 → ptr ──┐           s1 → [INVALIDADO]
           ▼           s2 → ptr ──┐
        [h,o,l,a]                 ▼
                               [h,o,l,a]
```

### Move en llamadas a funciones

El mismo move ocurre cuando pasas un valor a una función: la función pasa a ser el owner.

```rust
fn imprimir(s: String) {          // s entra como owner
    println!("{s}");
}                                 // s sale de scope -> drop

fn main() {
    let s = String::from("hola");
    imprimir(s);                  // ownership transferido a imprimir
    // println!("{s}");           // ❌ s ya no es válida aquí
}
```

Si quieres seguir usando el valor después de llamar a la función, tienes dos opciones:
**devolver el ownership** o **usar referencias** (lo vemos más abajo).

---

## Clone: copia explícita del heap

Cuando realmente necesitas dos copias independientes de un dato en heap, usa `.clone()`:

```rust
fn main() {
    let s1 = String::from("hola");
    let s2 = s1.clone();          // deep copy: nuevo heap asignado
    println!("s1={s1}, s2={s2}"); // ambos válidos
}
```

```text
s1 → ptr ──▶ [h,o,l,a]    (heap original)
s2 → ptr ──▶ [h,o,l,a]    (nueva copia en heap)
```

> ⚠️ `.clone()` es **costoso** cuando los datos son grandes: O(N) en tiempo y memoria.
> No lo uses por pánico para "que compile". Úsalo cuando de verdad necesites dos versiones
> independientes del dato.

---

## Copy: copia implícita del stack

Para tipos que viven enteramente en el **stack** y son baratos de copiar bit a bit, Rust
implementa el trait `Copy`. Cuando un tipo es `Copy`, la asignación **no mueve** sino que
**copia silenciosamente**, y el original sigue siendo válido.

```rust
fn main() {
    let x = 5;
    let y = x;              // COPY: x e y son copias independientes en stack
    println!("x={x}, y={y}"); // ✅ ambos válidos
}
```

**Tipos que implementan `Copy`:**

| Tipo | Nota |
| :--- | :--- |
| Todos los enteros (`i32`, `u64`, `usize`…) | Viven en stack |
| Flotantes (`f32`, `f64`) | Viven en stack |
| `bool` | 1 byte en stack |
| `char` | 4 bytes en stack |
| Tuplas de tipos `Copy` | `(i32, bool)` es `Copy`; `(i32, String)` no |
| Arreglos de tipos `Copy` | `[i32; 5]` es `Copy` |
| Referencias inmutables `&T` | El puntero se copia, no el dato apuntado |

**`String`, `Vec<T>`, `Box<T>` y structs propios NO son `Copy`** porque tienen datos en el
heap que no se pueden "copiar gratis".

### Resumen: Move vs Clone vs Copy

| Operación | Cuándo ocurre | Costo | Resultado |
| :--- | :--- | :--- | :--- |
| **Move** | Tipos sin `Copy` al asignar o pasar a función | O(1) — solo se copia el puntero en stack | Original **invalidado** |
| **Clone** | Llamada explícita a `.clone()` | O(N) — copia todo el heap | Dos copias **independientes** |
| **Copy** | Tipos con `Copy` al asignar o pasar a función | O(1) — copia bit a bit en stack | Original **sigue válido** |

---

## Referencias y Borrowing

¿Cómo usamos un valor en una función sin transferirle el ownership? Con una **referencia**.

Una referencia es un **puntero** que apunta al valor original pero **no lo posee**: solo lo
"presta" temporalmente. A esto se le llama *borrowing* (préstamo).

```rust
fn longitud(s: &String) -> usize {   // &String = referencia inmutable a String
    s.len()
}                                    // s sale de scope pero NO hace drop (no es owner)

fn main() {
    let s = String::from("hola");
    let n = longitud(&s);            // prestamos s, sin moverla
    println!("'{s}' tiene {n} caracteres"); // s sigue siendo válida
}
```

```text
STACK (main)          STACK (longitud)      HEAP
┌──────────────┐      ┌──────────────┐      ┌────────────┐
│ s: ptr─────────────▶│ s: ptr ────────────▶│ h,o,l,a    │
│    len=4     │      │    (ref)      │      └────────────┘
│    cap=4     │      └──────────────┘
└──────────────┘
```

### Referencias inmutables: `&T`

- Puedes tener **muchas referencias inmutables al mismo tiempo**.
- Solo permiten **leer** el valor, no modificarlo.
- Son `Copy`: pasar `&s` no mueve `s`.

```rust
fn main() {
    let s = String::from("hola");
    let r1 = &s;
    let r2 = &s;
    let r3 = &s;
    println!("{r1}, {r2}, {r3}"); // ✅ múltiples lectores simultáneos
}
```

### Referencias mutables: `&mut T`

Para modificar el valor prestado, usamos `&mut T`:

```rust
fn agregar(s: &mut String) {
    s.push_str(" mundo");
}

fn main() {
    let mut s = String::from("hola");   // la variable base debe ser `mut`
    agregar(&mut s);                    // referencia mutable
    println!("{s}");                    // "hola mundo"
}
```

### La regla del borrow checker: `&T` XOR `&mut T`

Esta es la regla más importante del *borrow checker*, y la que más confunde al principio:

> **En cualquier momento, puedes tener CUALQUIERA de estas dos cosas, pero NUNCA ambas:**
> - Una o más referencias inmutables (`&T`)
> - Exactamente una referencia mutable (`&mut T`)

```rust
fn main() {
    let mut s = String::from("hola");

    let r1 = &s;         // ✅ primera referencia inmutable
    let r2 = &s;         // ✅ segunda referencia inmutable
    let r3 = &mut s;     // ❌ NO COMPILA: ya hay referencias inmutables activas

    println!("{r1}, {r2}, {r3}");
}
```

Error del compilador:

```bash
error[E0502]: cannot borrow `s` as mutable because it is also borrowed as immutable
 --> src/main.rs:6:14
  |
4 |     let r1 = &s;
  |              -- immutable borrow occurs here
5 |     let r2 = &s;
6 |     let r3 = &mut s;
  |              ^^^^^^^ mutable borrow occurs here
7 |
8 |     println!("{r1}, {r2}, {r3}");
  |               ---- immutable borrow later used here
```

**¿Por qué esta regla?** Imagina dos hilos: uno lee `s` y otro la modifica al mismo tiempo.
Obtienes un *data race*. Rust hace imposible ese escenario en tiempo de compilación, incluso
en código de un solo hilo.

#### Los scopes de las referencias importan

El *borrow checker* es más inteligente de lo que parece: una referencia "termina" en el
último punto donde se usa, no necesariamente al final del bloque. Esto se llama
*Non-Lexical Lifetimes* (NLL):

```rust
fn main() {
    let mut s = String::from("hola");

    let r1 = &s;
    let r2 = &s;
    println!("{r1} y {r2}");  // r1 y r2 se usan aquí por última vez -> su "vida" termina
                               // a partir de aquí ya no hay referencias inmutables activas

    let r3 = &mut s;          // ✅ ahora sí se puede
    println!("{r3}");
}
```

#### Solución clásica: acotar el scope

Otra solución cuando hay conflicto es usar un bloque `{}` para acortar el scope:

```rust
fn main() {
    let mut v = vec![1, 2, 3];

    {
        let primero = &v[0];  // referencia inmutable activa dentro del bloque
        println!("{primero}");
    }                         // referencia inmutable termina aquí

    v.push(4);                // ✅ ahora sí: no hay referencias activas
    println!("{v:?}");        // [1, 2, 3, 4]
}
```

### Referencias colgantes (*dangling references*)

En C/C++ es posible devolver un puntero a memoria que ya fue liberada. Rust lo impide:

```rust
fn colgante() -> &String {       // ❌ NO COMPILA
    let s = String::from("hola");
    &s                           // s se libera al terminar la función
}                                // el caller recibiría un puntero a memoria inválida
```

Error del compilador:

```bash
error[E0106]: missing lifetime specifier
 --> src/main.rs:1:19
  |
1 | fn colgante() -> &String {
  |                  ^ expected named lifetime parameter
  ...
  = help: this function's return type contains a borrowed value, but there is no value for it to be borrowed from
```

La solución: devuelve la `String` directamente (transfiere ownership) en lugar de una
referencia a algo que va a desaparecer.

---

## Slices: vistas sin ownership

Un **slice** es una vista de una secuencia de elementos contiguos sin poseer los datos.
Internamente es un *fat pointer*: `(puntero_al_primer_elemento, longitud)`.

### String slices: `&str`

```rust
fn main() {
    let s = String::from("hola mundo");

    let hola = &s[0..4];    // vista de los bytes 0..3 (4 no incluido)
    let mundo = &s[5..10];

    println!("{hola}");     // "hola"
    println!("{mundo}");    // "mundo"

    // Atajos: omitir el inicio o el fin
    let hola2 = &s[..4];   // equivalente a &s[0..4]
    let mundo2 = &s[5..];   // equivalente a &s[5..10]
    let todo = &s[..];      // vista de toda la String
}
```

Los rangos de slices de `&str` operan en **bytes**, no en caracteres. Para texto ASCII
todo funciona bien; con Unicode hay que tener cuidado de no cortar en medio de un carácter
multibyte.

#### Literales de string son `&str`

```rust
let s: &str = "Hola, mundo!";   // literal: &str 'static (vive toda la ejecución)
```

Un literal de string está compilado dentro del binario. Su tipo es `&str`, no `String`.

#### Función idiomática con `&str`

```rust
fn primera_palabra(s: &str) -> &str {
    for (i, c) in s.char_indices() {
        if c == ' ' {
            return &s[..i];
        }
    }
    s
}

fn main() {
    let s = String::from("hola mundo");
    let palabra = primera_palabra(&s);  // &String coacciona a &str automáticamente
    println!("{palabra}");              // "hola"

    let literal = "hola mundo cruel";
    let primera = primera_palabra(literal); // también funciona directamente con &str
    println!("{primera}");              // "hola"
}
```

Acepta `&str` en lugar de `&String` porque así funciona con ambos tipos. Esta es la
práctica idiomática en Rust para parámetros de texto.

### Por qué `&str` es más útil que `&String` como parámetro

```rust
fn longitud_mala(s: &String) -> usize { s.len() }  // obliga al caller a tener una String
fn longitud_bien(s: &str) -> usize { s.len() }      // acepta &str, String, &String, etc.

fn main() {
    let owned = String::from("hola");
    let literal = "hola";

    longitud_mala(&owned);          // ✅ funciona
    // longitud_mala(literal);      // ❌ literal es &str, no &String

    longitud_bien(&owned);          // ✅ &String coacciona a &str
    longitud_bien(literal);         // ✅ también funciona directamente
}
```

### Slices de arreglos: `&[T]`

El mismo concepto aplica a cualquier tipo de colección:

```rust
fn suma(numeros: &[i32]) -> i32 {   // acepta &Vec<i32>, &[i32; N], slices parciales
    let mut total = 0;
    for &n in numeros {
        total += n;
    }
    total
}

fn main() {
    let arr = [1, 2, 3, 4, 5];
    let vec = vec![10, 20, 30];

    println!("{}", suma(&arr));         // 15 (slice de todo el arreglo)
    println!("{}", suma(&arr[1..3]));   // 5 (solo elementos 1 y 2)
    println!("{}", suma(&vec));         // 60 (Vec coacciona a &[i32])
}
```

### Tabla resumen: tipos de referencia

| Tipo | Descripción | Coste | Permite modificar |
| :--- | :--- | :--- | :--- |
| `&T` | Referencia inmutable | O(1), solo copia puntero | No |
| `&mut T` | Referencia mutable | O(1), solo copia puntero | Sí |
| `&str` | Slice de string (fat ptr) | O(1) | No |
| `&[T]` | Slice de colección (fat ptr) | O(1) | No |
| `&mut [T]` | Slice mutable de colección | O(1) | Sí |

---

## El ciclo de ownership completo

Antes de ver el ejercicio, un ejemplo que une todo lo visto:

```rust
fn es_palindromo(s: &str) -> bool {         // préstamo inmutable: solo leer
    let bytes = s.as_bytes();
    let n = bytes.len();
    for i in 0..n / 2 {
        if bytes[i] != bytes[n - 1 - i] {
            return false;
        }
    }
    true
}

fn duplicar(v: &[i32]) -> Vec<i32> {        // préstamo inmutable de slice
    v.iter().map(|&x| x * 2).collect()     // devuelve Vec<i32> nuevo (owned)
}

fn agregar_al_final(v: &mut Vec<i32>, n: i32) {  // préstamo mutable
    v.push(n);
}

fn main() {
    // &str y &String
    let palabra = String::from("reconocer");
    println!("{} es palíndromo: {}", palabra, es_palindromo(&palabra));  // true

    // &[T] y Vec<T>
    let numeros = vec![1, 2, 3];
    let dobles = duplicar(&numeros);             // numeros sigue siendo válido
    println!("originales: {numeros:?}");         // [1, 2, 3]
    println!("duplicados: {dobles:?}");          // [2, 4, 6]

    // &mut Vec<T>
    let mut lista = vec![10, 20];
    agregar_al_final(&mut lista, 30);
    println!("lista: {lista:?}");                // [10, 20, 30]
}
```

---

## 🧪 Ejercicio: `split_manual`

Implementa tu propia función de división de strings **sin usar** `.split()`,
`.split_whitespace()` ni `.chars().collect()`.

Crea un proyecto con `cargo new split_manual` y escribe en `src/main.rs`:

```rust
fn split_manual<'a>(input: &'a str, delimiter: char) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut start = 0;
    for (i, c) in input.char_indices() {
        if c == delimiter {
            result.push(&input[start..i]);
            start = i + delimiter.len_utf8(); // avanzamos en bytes, no en chars
        }
    }
    result.push(&input[start..]);            // último trozo
    result
}

fn main() {
    let partes = split_manual("hola,mundo,rust", ',');
    for p in &partes {
        println!("{p}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basico() {
        assert_eq!(split_manual("a,b,c", ','), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_espacios() {
        assert_eq!(split_manual("hola mundo", ' '), vec!["hola", "mundo"]);
    }

    #[test]
    fn test_sin_delimitador() {
        assert_eq!(split_manual("texto", 'x'), vec!["te", "to"]);
    }

    #[test]
    fn test_vacio() {
        assert_eq!(split_manual("", ','), vec![""]);
    }

    #[test]
    fn test_delimitador_al_final() {
        assert_eq!(split_manual("a,", ','), vec!["a", ""]);
    }
}
```

Ejecútalo con `cargo run` y luego `cargo test`. Fíjate en:

- Por qué el retorno es `Vec<&'a str>` y no `Vec<String>` (eficiencia: cero copias).
- Por qué usamos `char_indices()` en vez de `chars()` (necesitamos los índices en bytes).
- Por qué avanzamos `start` con `delimiter.len_utf8()` y no con `+1` (seguridad UTF-8).

---

## ✅ Checklist de la Semana 2

- [ ] Explico la diferencia entre **stack** y **heap** sin mirar apuntes.
- [ ] Recito de memoria las **3 reglas de Ownership**.
- [ ] Distingo **Move**, **Clone** y **Copy** y sé cuándo usar cada uno.
- [ ] Sé qué tipos implementan `Copy` y cuáles no.
- [ ] Escribo funciones que toman `&T` y `&mut T` correctamente.
- [ ] Entiendo la regla **`&T` XOR `&mut T`** y sé acotar el scope cuando hay conflicto.
- [ ] Uso `&str` (no `&String`) como tipo de parámetro para texto.
- [ ] Uso `&[T]` (no `&Vec<T>`) como tipo de parámetro para slices.
- [ ] El ejercicio `split_manual` compila y pasa todos los tests con `cargo test`.
- [ ] Completo los ejercicios de Rustlings: `move_semantics/`, `references/`, `slices/`.

> **Siguiente paso:** Semana 3 — [Structs, Enums, Pattern Matching y manejo de errores básico](section_00.md).

---

## Errores comunes y cómo resolverlos

| Error del compilador | Causa más frecuente | Solución |
| :--- | :--- | :--- |
| `borrow of moved value` | Usas una variable después de moverla | Usa `&variable` para prestar, o `.clone()` si necesitas dos copias |
| `cannot borrow as mutable because also borrowed as immutable` | Tienes `&v` y `&mut v` activas al mismo tiempo | Termina de usar `&v` antes de crear `&mut v`, o abre un bloque `{}` |
| `cannot assign twice to immutable variable` | Olvidaste `mut` | `let mut x = ...` |
| `missing lifetime specifier` al devolver `&T` | Intentas devolver referencia a variable local | Devuelve el tipo owned (`String`, `Vec`, etc.) |
| `does not implement the Copy trait` | Intentas usar un valor movido sin `.clone()` | Pasa como referencia `&v` o llama a `.clone()` |
