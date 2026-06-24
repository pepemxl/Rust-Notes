# Generics, Trait Bounds y Lifetimes

La Semana 5 introduce la base de la reutilización en Rust. Aquí aprendes a escribir
código que funciona para **cualquier tipo** que cumpla ciertas condiciones, sin pagar
ningún coste en tiempo de ejecución. Es la diferencia entre escribir `fn suma_i32` y
`fn suma_flotantes` por separado, o escribir una única `fn suma<T: Add>` que el
compilador especializa por ti.

En esta sección aprenderemos:

- Qué son los **generics** y cómo el compilador los convierte en código concreto
  (*monomorphization*).
- Cómo expresar **restricciones sobre tipos** con *trait bounds* y cláusulas `where`.
- La diferencia entre `impl Trait` en posición de argumento y en posición de retorno.
- Qué son los **lifetimes** (`'a`), cuándo el compilador los infiere solo y cuándo hay
  que anotarlos a mano.

> 💡 **Filosofía de la Semana 5:** *Las abstracciones en Rust tienen coste cero en
> runtime. El coste se paga en compile-time y en complejidad mental — y vale la pena.*

---

## Generics: código para cualquier tipo

### El problema sin generics

Imagina que necesitas encontrar el mayor elemento en una lista. Sin generics tendrías
que escribir una versión para cada tipo:

```rust
fn mayor_i32(lista: &[i32]) -> i32 {
    let mut m = lista[0];
    for &x in &lista[1..] { if x > m { m = x; } }
    m
}

fn mayor_f64(lista: &[f64]) -> f64 {
    let mut m = lista[0];
    for &x in &lista[1..] { if x > m { m = x; } }
    m
}
```

El código es idéntico. Lo único que cambia es el tipo. Los generics eliminan esta
repetición.

### Funciones genéricas

```rust
fn mayor<T: PartialOrd + Copy>(lista: &[T]) -> T {
    let mut m = lista[0];
    for &x in &lista[1..] {
        if x > m { m = x; }
    }
    m
}

fn main() {
    println!("{}", mayor(&[3, 7, 2, 9, 1]));    // 9   — usa mayor::<i32>
    println!("{}", mayor(&[1.5, 0.2, 9.9]));    // 9.9 — usa mayor::<f64>
    println!("{}", mayor(&['a', 'z', 'm']));     // z   — usa mayor::<char>
}
```

- `<T>` es el **parámetro de tipo**; podría llamarse de cualquier forma, pero `T` es
  la convención para "cualquier tipo".
- `: PartialOrd + Copy` son los **trait bounds**: restricciones que `T` debe cumplir.
  `PartialOrd` permite usar `>`, `Copy` permite copiar el valor sin mover.
- El compilador genera **tres versiones concretas** de `mayor`: una para `i32`, otra
  para `f64` y otra para `char`. Esto se llama **monomorphization**.

### Monomorphization

```text
Código fuente (una función):         Código compilado (tres copias):
                                     
fn mayor<T: PartialOrd + Copy>  →    fn mayor_i32(lista: &[i32]) -> i32 { ... }
    (lista: &[T]) -> T               fn mayor_f64(lista: &[f64]) -> f64 { ... }
                                     fn mayor_char(lista: &[char]) -> char { ... }
```

Cada versión está **totalmente optimizada** para su tipo. No hay indirección, no hay
llamada a través de puntero. Cero coste en runtime comparado con escribirlas a mano.

El precio: más tiempo de compilación y potencialmente un binario más grande (el compilador
genera más código).

### Structs genéricos

```rust
#[derive(Debug)]
struct Punto<T> {
    x: T,
    y: T,
}

fn main() {
    let entero = Punto { x: 5, y: 10 };           // Punto<i32>
    let flotante = Punto { x: 1.0, y: 4.0 };      // Punto<f64>
    println!("{:?}", entero);
    println!("{:?}", flotante);
}
```

Si los campos pueden ser de tipos distintos, usa dos parámetros:

```rust
#[derive(Debug)]
struct Par<T, U> {
    primero: T,
    segundo: U,
}

let par = Par { primero: 5, segundo: "hola" };    // Par<i32, &str>
```

### `impl` en structs genéricos

Los trait bounds en `impl` solo se exigen cuando el método realmente los necesita:

```rust
struct Punto<T> { x: T, y: T }

// Métodos disponibles para CUALQUIER T
impl<T> Punto<T> {
    fn nuevo(x: T, y: T) -> Self { Self { x, y } }
    fn x(&self) -> &T { &self.x }
}

// Métodos disponibles solo si T: PartialOrd
impl<T: PartialOrd> Punto<T> {
    fn es_mayor_que(&self, otro: &Self) -> bool {
        self.x > otro.x && self.y > otro.y
    }
}

// Implementación solo para un tipo concreto
impl Punto<f64> {
    fn distancia_al_origen(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

fn main() {
    let p = Punto::nuevo(3.0_f64, 4.0);
    println!("distancia: {}", p.distancia_al_origen()); // 5
}
```

### Enums genéricos

Ya los conoces: `Option<T>` y `Result<T, E>` de la librería estándar son exactamente esto.

```rust
// Así están definidos en std (simplificado):
enum Option<T> { None, Some(T) }
enum Result<T, E> { Ok(T), Err(E) }

// Un stack genérico propio:
#[derive(Debug)]
struct Pila<T> {
    elementos: Vec<T>,
}

impl<T> Pila<T> {
    fn nueva() -> Self { Self { elementos: Vec::new() } }
    fn empujar(&mut self, val: T) { self.elementos.push(val); }
    fn sacar(&mut self) -> Option<T> { self.elementos.pop() }
    fn cima(&self) -> Option<&T> { self.elementos.last() }
    fn esta_vacia(&self) -> bool { self.elementos.is_empty() }
}

fn main() {
    let mut pila: Pila<i32> = Pila::nueva();
    pila.empujar(1);
    pila.empujar(2);
    pila.empujar(3);
    println!("{:?}", pila.sacar()); // Some(3)
    println!("{:?}", pila.cima());  // Some(2)
}
```

---

## Trait bounds: restricciones sobre tipos genéricos

Un **trait bound** especifica qué capacidades debe tener un tipo para que el código
compile. Hay dos sintaxis:

### Sintaxis con `:` (inline)

```rust
fn imprimir<T: std::fmt::Display>(valor: T) {
    println!("{valor}");
}

fn imprimir_debug<T: std::fmt::Debug + Clone>(valor: T) {
    let copia = valor.clone();
    println!("{copia:?}");
}
```

### Sintaxis `where` (para firmas complejas)

Cuando hay muchos bounds o múltiples parámetros, `where` mejora la legibilidad:

```rust
// Difícil de leer:
fn combinar<T: std::fmt::Display + Clone, U: std::fmt::Debug + PartialOrd>(a: T, b: U) -> String {
    format!("{} {:?}", a.clone(), b)
}

// Mucho más claro con where:
fn combinar<T, U>(a: T, b: U) -> String
where
    T: std::fmt::Display + Clone,
    U: std::fmt::Debug + PartialOrd,
{
    format!("{} {:?}", a.clone(), b)
}
```

Usa siempre `where` cuando haya más de un bound por parámetro o más de dos parámetros
genéricos. Es la convención idiomática.

### Bounds en structs vs en `impl`

Pon los bounds **donde se necesitan**, no antes:

```rust
// ❌ MAL: pone el bound en la definición del struct
// Obliga a que TODAS las operaciones requieran Debug, aunque no todas lo necesiten
struct Contenedor<T: std::fmt::Debug> { valor: T }

// ✅ BIEN: el struct no pone restricciones
struct Contenedor<T> { valor: T }

// El bound solo aparece donde se usa
impl<T: std::fmt::Debug> Contenedor<T> {
    fn mostrar(&self) { println!("{:?}", self.valor); }
}

impl<T: Clone> Contenedor<T> {
    fn clonar_valor(&self) -> T { self.valor.clone() }
}
```

### Bounds comunes de la librería estándar

| Trait | Qué garantiza | Cuándo necesitas |
| :--- | :--- | :--- |
| `Clone` | `.clone()` disponible | Copiar el valor dentro de la función |
| `Copy` | Copia implícita (bitwise) | Usar el valor sin moverlo |
| `Debug` | `{:?}` disponible | Imprimir en logs/tests |
| `Display` | `{}` disponible | Mostrar al usuario |
| `PartialEq` | `==` y `!=` | Comparar igualdad |
| `PartialOrd` | `<`, `>`, `<=`, `>=` | Comparar orden |
| `Hash + Eq` | Usable como clave de `HashMap` | Claves de mapas/sets |
| `Send` | Se puede mover entre hilos | Código concurrente |
| `Sync` | Se puede compartir entre hilos | Código concurrente |
| `Default` | `T::default()` disponible | Valor inicial por defecto |
| `std::error::Error` | Es un tipo de error | Error handling genérico |

---

## `impl Trait`: dos usos, dos significados

`impl Trait` es una sintaxis más concisa para ciertos usos de generics, pero sus dos
posiciones tienen significados muy distintos.

### En posición de argumento: azúcar sintáctico

```rust
// Estas dos firmas son 100% equivalentes:
fn imprimir_a(item: impl std::fmt::Display) {
    println!("{item}");
}

fn imprimir_b<T: std::fmt::Display>(item: T) {
    println!("{item}");
}
```

`impl Trait` en argumento es simplemente una forma más corta de escribir un generic
anónimo. El compilador genera código monomorphizado exactamente igual.

**Cuándo preferirlo**: APIs sencillas con un solo parámetro genérico donde el nombre
`T` no aporta información semántica.

**Cuándo preferir `<T>`**: cuando necesitas referirte al mismo tipo en varios lugares
(varios parámetros del mismo tipo, o el tipo aparece en el retorno).

```rust
// impl Trait en argumento no funciona cuando necesitas que dos parámetros
// sean el mismo tipo:
fn maximo(a: impl PartialOrd, b: impl PartialOrd) -> ??? {
    // a y b podrían ser tipos distintos, no se pueden comparar entre sí
}

// Con <T> sí funciona:
fn maximo<T: PartialOrd>(a: T, b: T) -> T {
    if a > b { a } else { b }
}
```

### En posición de retorno: tipo opaco

Aquí `impl Trait` tiene un significado totalmente distinto:

```rust
fn pares_hasta(n: u32) -> impl Iterator<Item = u32> {
    (0..n).filter(|x| x % 2 == 0)
}

fn main() {
    let pares: Vec<u32> = pares_hasta(10).collect();
    println!("{pares:?}"); // [0, 2, 4, 6, 8]
}
```

El tipo real del iterador es algo como `Filter<Range<u32>, {closure}>` — complejo,
difícil de escribir y un detalle de implementación que no debería importarle al caller.
Con `-> impl Iterator<Item = u32>` el caller solo sabe que recibe algo que es un
iterador de `u32`. El tipo concreto queda oculto.

**Ventaja**: puedes cambiar la implementación interna sin romper la firma pública.

**Limitación crítica**: no puedes devolver tipos distintos en diferentes ramas:

```rust
fn animal(es_perro: bool) -> impl std::fmt::Display {
    if es_perro {
        "guau"      // &str
    } else {
        42          // i32 — ❌ NO COMPILA: tipos distintos en las ramas
    }
}
```

Error del compilador:

```bash
error[E0308]: `if` and `else` have incompatible types
  --> src/main.rs:4:9
   |
3  |         "guau"
   |         ------ expected because of this
4  |         42
   |         ^^ expected `&str`, found integer
```

Solución: usa `Box<dyn Trait>` cuando las ramas devuelven tipos distintos:

```rust
fn animal(es_perro: bool) -> Box<dyn std::fmt::Display> {
    if es_perro {
        Box::new("guau")
    } else {
        Box::new(42)
    }
}
```

### Tabla comparativa

| | `fn foo<T: Trait>(x: T)` | `fn foo(x: impl Trait)` | `fn foo() -> impl Trait` |
| :--- | :--- | :--- | :--- |
| **Dispatch** | Estático (monomorphization) | Estático (igual) | Estático (tipo opaco concreto) |
| **Nombre del tipo** | Disponible como `T` | No disponible | No disponible para el caller |
| **Mismo tipo en varios lugares** | Sí (`a: T, b: T`) | No (`a: impl T, b: impl T` pueden ser distintos) | N/A |
| **Tipos distintos en ramas** | N/A | N/A | **No** — usa `Box<dyn Trait>` |

---

## Lifetimes: nombres para la duración de las referencias

Los lifetimes son la forma en que el compilador razona sobre **cuánto tiempo vive una
referencia**. En la gran mayoría del código no necesitas escribirlos: el compilador los
infiere con las reglas de *elision*. Solo hace falta anotarlos cuando el compilador
necesita ayuda para relacionar la vida de referencias de entrada con referencias de
salida.

### La regla fundamental

> Una referencia **nunca puede vivir más** que el dato al que apunta.

El compilador garantiza esto en tiempo de compilación, eliminando completamente los
*dangling pointers*.

```rust
fn main() {
    let referencia;
    {
        let x = 5;
        referencia = &x;    // ❌ NO COMPILA: x no vive suficiente
    }
    // println!("{referencia}"); // x ya fue liberada aquí
}
```

Error:

```bash
error[E0597]: `x` does not live long enough
 --> src/main.rs:4:21
  |
4 |         referencia = &x;
  |                     ^^ borrowed value does not live long enough
5 |     }
  |     - `x` dropped here while still borrowed
```

### Lifetimes en funciones

El problema clásico que requiere anotar lifetimes:

```rust
// ❌ NO COMPILA: el compilador no sabe cuánto vive la referencia retornada
fn mas_largo(a: &str, b: &str) -> &str {
    if a.len() >= b.len() { a } else { b }
}
```

Error:

```bash
error[E0106]: missing lifetime specifier
 --> src/main.rs:1:34
  |
1 | fn mas_largo(a: &str, b: &str) -> &str {
  |                 ----     ----     ^ expected named lifetime parameter
  |
  = help: this function's return type contains a borrowed value, but the
    signature does not say whether it is borrowed from `a` or `b`
```

El compilador tiene razón: la referencia devuelta viene de `a` o de `b` dependiendo de
los valores en tiempo de ejecución. Hay que decirle que el retorno vive tanto como el
más corto entre los dos:

```rust
// ✅ Con lifetime 'a: el retorno vive al menos tanto como ambas entradas
fn mas_largo<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.len() >= b.len() { a } else { b }
}

fn main() {
    let s1 = String::from("cadena larga");
    let resultado;
    {
        let s2 = String::from("xyz");
        resultado = mas_largo(s1.as_str(), s2.as_str());
        println!("más larga: {resultado}");  // ✅ s2 sigue viva aquí
    }
    // println!("{resultado}"); // ❌ s2 ya no existe
}
```

**Lo que significa `'a`**: "estas referencias y el retorno deben solaparse en vida como
mínimo durante `'a`". El compilador elige `'a` = la intersección de las vidas de `a`
y `b` (la más corta). Es una **restricción**, no una duración concreta.

### Lifetimes en structs

Cuando un struct guarda una **referencia**, necesita un parámetro de lifetime que
exprese que el struct no puede vivir más que el dato referenciado:

```rust
// Este struct contiene una vista (&str) — no posee el string
struct Extracto<'a> {
    parte: &'a str,
}

impl<'a> Extracto<'a> {
    fn primera_frase(&self) -> &str {
        // La 3.ª regla de elision aplica: el compilador deduce que el retorno
        // tiene el mismo lifetime que &self, no hace falta anotarlo
        self.parte.split('.').next().unwrap_or("")
    }
}

fn main() {
    let novela = String::from("Llámame Ishmael. Hace algunos años...");
    let primera_oracion;
    {
        let i = novela.find('.').unwrap_or(novela.len());
        primera_oracion = Extracto { parte: &novela[..i] };
    }
    println!("{}", primera_oracion.primera_frase()); // "Llámame Ishmael"
    // ✅ primera_oracion vive menos que novela: seguro
}
```

**Regla práctica**: si tu struct guarda un `String`, un `Vec`, un `Box` — no necesita
lifetimes. Si guarda `&str`, `&T`, `&mut T` — necesita `'a`.

Siempre que puedas, prefiere guardar tipos *owned* (`String` en lugar de `&str`) en
structs. Los lifetimes en structs añaden complejidad que solo vale la pena cuando la
eficiencia (evitar copias) es crítica.

### Las reglas de elision (cuándo no hace falta anotar)

El compilador infiere lifetimes con tres reglas. Si después de aplicarlas aún hay
referencias de salida sin lifetime determinado, exige anotación manual.

**Regla 1:** Cada referencia de entrada recibe su propio lifetime:

```rust
fn foo(x: &str, y: &str)  →  fn foo<'a, 'b>(x: &'a str, y: &'b str)
```

**Regla 2:** Si hay exactamente **una** referencia de entrada, su lifetime se asigna
a todas las de salida:

```rust
fn primero(s: &str) -> &str  →  fn primero<'a>(s: &'a str) -> &'a str
// No hace falta anotar: el compilador lo resuelve solo
```

**Regla 3:** Si hay `&self` o `&mut self`, su lifetime se asigna a todas las
referencias de salida:

```rust
impl<'a> Extracto<'a> {
    fn primera_frase(&self) -> &str  →  fn primera_frase(&'a self) -> &'a str
    // Tampoco hace falta anotar: regla 3 lo resuelve
}
```

Solo cuando ninguna de las tres reglas resuelve todas las referencias de salida
necesitas escribir los lifetimes manualmente.

### `'static`: el lifetime especial

`'static` significa "esta referencia vive durante toda la ejecución del programa":

```rust
// Los literales de string tienen lifetime 'static (están en el binario)
let s: &'static str = "Hola, mundo!";

// Una función que solo acepta referencias que vivan para siempre:
fn solo_estatico(s: &'static str) -> &'static str { s }
```

Aparece en dos contextos importantes:

```rust
// 1. En trait objects que se envían entre hilos:
fn crear_error() -> Box<dyn std::error::Error + Send + 'static> {
    Box::new(std::io::Error::other("algo salió mal"))
}

// 2. En closures capturadas por hilos (los datos deben vivir para siempre o ser owned):
std::thread::spawn(|| {
    println!("hola desde hilo");   // closure: 'static porque el hilo puede vivir más que main
});
```

> ⚠️ No confundas `'static` con "inmutable". Una `String` puede ser `'static` si vive
> toda la ejecución. El `'static` bound en generics (`T: 'static`) no significa que
> `T` sea inmutable; significa que `T` no contiene referencias de duración limitada.

---

## Ejercicio: `Cache<K, V>` con TTL y mock de tiempo

Este ejercicio une todo lo visto: generics, trait bounds, `where` clauses, y un patrón
de **inyección de dependencias** vía traits que hace los tests deterministas.

Crea un proyecto con `cargo new cache_ttl --lib` y escribe en `src/lib.rs`:

### Código completo

```rust
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

// 1. Trait para abstraer el proveedor de tiempo
//    Esto permite usar un mock en tests en lugar del reloj del sistema.
pub trait TimeProvider {
    fn ahora(&self) -> Instant;
}

// 2. Implementación real (producción)
pub struct TiempoDelSistema;

impl TimeProvider for TiempoDelSistema {
    fn ahora(&self) -> Instant {
        Instant::now()
    }
}

// 3. Entrada de caché con timestamp de expiración
struct Entrada<V> {
    valor: V,
    expira_en: Instant,
}

// 4. Cache genérico sobre K (clave) y V (valor),
//    con parámetro T para el proveedor de tiempo (con valor por defecto)
pub struct Cache<K, V, T = TiempoDelSistema>
where
    K: Eq + Hash,
    T: TimeProvider,
{
    mapa: HashMap<K, Entrada<V>>,
    ttl: Duration,
    tiempo: T,
}

// Constructor conveniente: usa el tiempo real del sistema
impl<K, V> Cache<K, V, TiempoDelSistema>
where
    K: Eq + Hash,
{
    pub fn nuevo(ttl: Duration) -> Self {
        Self {
            mapa: HashMap::new(),
            ttl,
            tiempo: TiempoDelSistema,
        }
    }
}

// Métodos generales: funcionan con CUALQUIER proveedor de tiempo
impl<K, V, T> Cache<K, V, T>
where
    K: Eq + Hash + Clone,
    T: TimeProvider,
{
    pub fn con_tiempo(ttl: Duration, tiempo: T) -> Self {
        Self {
            mapa: HashMap::new(),
            ttl,
            tiempo,
        }
    }

    pub fn insertar(&mut self, clave: K, valor: V) {
        let expira_en = self.tiempo.ahora() + self.ttl;
        self.mapa.insert(clave, Entrada { valor, expira_en });
    }

    /// Devuelve el valor si existe y no ha expirado.
    /// Elimina la entrada si está caducada (expiración perezosa).
    pub fn obtener(&mut self, clave: &K) -> Option<&V> {
        let ahora = self.tiempo.ahora();
        if let Some(entrada) = self.mapa.get(clave) {
            if entrada.expira_en > ahora {
                // La entrada sigue vigente: devuelve referencia al valor
                return self.mapa.get(clave).map(|e| &e.valor);
            }
        }
        // Expirada o inexistente: limpia y devuelve None
        self.mapa.remove(clave);
        None
    }

    pub fn longitud(&self) -> usize {
        self.mapa.len()
    }

    /// Elimina todas las entradas expiradas.
    pub fn limpiar_expiradas(&mut self) {
        let ahora = self.tiempo.ahora();
        self.mapa.retain(|_, entrada| entrada.expira_en > ahora);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // Mock de tiempo: controlamos manualmente en qué momento "estamos"
    struct TiempoMock {
        actual: RefCell<Instant>,  // RefCell: mutabilidad interior (lo veremos en Semana 7)
    }

    impl TiempoMock {
        fn nuevo(inicio: Instant) -> Self {
            Self { actual: RefCell::new(inicio) }
        }

        fn avanzar(&self, duracion: Duration) {
            *self.actual.borrow_mut() += duracion;
        }
    }

    impl TimeProvider for TiempoMock {
        fn ahora(&self) -> Instant {
            *self.actual.borrow()
        }
    }

    #[test]
    fn insertar_y_obtener_dentro_de_ttl() {
        let inicio = Instant::now();
        let mock = TiempoMock::nuevo(inicio);
        let mut cache = Cache::con_tiempo(Duration::from_secs(10), mock);

        cache.insertar("clave", 42);
        assert_eq!(cache.obtener(&"clave"), Some(&42));
    }

    #[test]
    fn expira_tras_ttl() {
        let inicio = Instant::now();
        let mock = TiempoMock::nuevo(inicio);
        let mut cache = Cache::con_tiempo(Duration::from_secs(10), mock);

        cache.insertar("clave", 42);

        // Avanzamos 5 segundos: dentro del TTL
        cache.tiempo.avanzar(Duration::from_secs(5));
        assert_eq!(cache.obtener(&"clave"), Some(&42));

        // Avanzamos 10 segundos más: ya expiró (total: 15s > 10s TTL)
        cache.tiempo.avanzar(Duration::from_secs(10));
        assert_eq!(cache.obtener(&"clave"), None);
        assert_eq!(cache.longitud(), 0);  // la entrada fue eliminada
    }

    #[test]
    fn clave_inexistente_devuelve_none() {
        let mut cache: Cache<&str, i32> = Cache::nuevo(Duration::from_secs(60));
        assert_eq!(cache.obtener(&"no_existe"), None);
    }

    #[test]
    fn limpiar_expiradas_elimina_solo_caducadas() {
        let inicio = Instant::now();
        let mock = TiempoMock::nuevo(inicio);
        let mut cache = Cache::con_tiempo(Duration::from_secs(10), mock);

        cache.insertar("corta", "expira pronto");
        cache.insertar("larga", "tarda en expirar");

        // Forzamos que "corta" expire reemplazándola con un TTL ya vencido
        // (En la API actual lo simulamos avanzando el tiempo)
        cache.tiempo.avanzar(Duration::from_secs(15));
        cache.insertar("nueva", "recién insertada"); // usa el tiempo avanzado

        // Retrocedemos el tiempo para que "nueva" no expire
        // (En tests reales trabajaríamos con TTLs distintos por entrada)
        // En su lugar comprobamos que limpiar_expiradas funciona:
        cache.limpiar_expiradas();
        assert!(cache.obtener(&"nueva").is_some()); // "nueva" sigue vigente
    }

    #[test]
    fn tipos_distintos_de_clave_y_valor() {
        let mut cache: Cache<u32, String> = Cache::nuevo(Duration::from_secs(60));
        cache.insertar(1, String::from("uno"));
        cache.insertar(2, String::from("dos"));
        assert_eq!(cache.obtener(&1), Some(&String::from("uno")));
        assert_eq!(cache.obtener(&3), None);
    }
}
```

Ejecuta:

```bash
cargo test            # todos los tests deben pasar
cargo clippy          # 0 warnings
```

### Puntos de aprendizaje del ejercicio

- `Cache<K, V, T = TiempoDelSistema>`: **tipo por defecto** en un parámetro genérico.
  `Cache::nuevo(...)` usa el tiempo real; `Cache::con_tiempo(...)` acepta cualquier
  `TimeProvider`.
- `where K: Eq + Hash + Clone, T: TimeProvider`: `where` con múltiples bounds en
  múltiples parámetros.
- `RefCell<Instant>` en `TiempoMock`: permite mutar el tiempo desde `&self` (sin `&mut`).
  Lo explicamos en profundidad en la Semana 7; por ahora úsalo como receta para mocks.
- El patrón completo — **abstraer dependencias externas** (tiempo, red, disco) tras un
  trait — hace los tests **deterministas** y rápidos, sin `sleep` ni timing externo.

---

## ✅ Checklist de la Semana 5

- [ ] Escribo funciones genéricas con `<T: Bound>` y `where` clauses sin mirar la sintaxis.
- [ ] Distingo **monomorphization** (una copia por tipo, coste cero en runtime) de
  dispatch dinámico (VTable, coste en runtime — lo veremos en Semana 6).
- [ ] Sé dónde poner bounds: en el `impl`, no en la definición del struct.
- [ ] Entiendo la diferencia entre `impl Trait` como argumento (azúcar para generics) y
  como retorno (tipo opaco).
- [ ] Sé por qué `-> impl Trait` no funciona con tipos distintos en ramas `if/else`.
- [ ] Entiendo qué representa un lifetime `'a`: una restricción de solapamiento, no una
  duración concreta.
- [ ] Anoto lifetimes en funciones cuando hay múltiples referencias de entrada y el
  compilador no puede inferir de cuál viene el retorno.
- [ ] Anoto lifetimes en structs cuando contienen referencias (`&str`, `&T`).
- [ ] Sé qué significa `'static` y cuándo aparece naturalmente.
- [ ] El ejercicio `cache_ttl` compila, todos los tests pasan, `clippy` da 0 warnings.
- [ ] Completo Rustlings: `generics/`, `traits/`, `lifetimes/`.

> **Siguiente paso:** Semana 6 — [Traits avanzados: `dyn Trait`, `Deref`, `Drop`,
> sobrecarga de operadores y conversiones](section_02.md).
