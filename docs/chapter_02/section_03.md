# Smart Pointers e Interior Mutability

La Semana 7 rompe el modelo mental básico de ownership: hasta ahora cada valor tenía
exactamente un dueño. Ahora aprenderás a compartir ownership legítimamente con `Rc` y
`Arc`, a mutar datos a través de referencias compartidas con `RefCell` y `Mutex`, y a
romper ciclos de referencia con `Weak` — todo ello de forma que el compilador (o el
runtime) sigue garantizando la ausencia de bugs de memoria.

En esta sección aprenderemos:

- **`Box<T>`**: el puntero más simple — un único owner en el heap.
- **`Rc<T>` y `Arc<T>`**: ownership compartido mediante conteo de referencias.
- **Interior mutability**: `RefCell<T>`, `Cell<T>`, `Mutex<T>`, `RwLock<T>`.
- **`Weak<T>`**: referencias no propietarias para romper ciclos.
- Los patrones canónicos `Rc<RefCell<T>>` y `Arc<Mutex<T>>`.
- Ejercicio: grafo con punteros padre/hijo bidireccionales.

> 💡 **Filosofía de la Semana 7:** *Los smart pointers no son magia — son tipos normales
> que implementan `Deref`, `Drop` y, en algunos casos, `Deref` + `Drop` + conteo de
> referencias. Cuando los entiendes como tipos, los usas con precisión.*

---

## `Box<T>`: el puntero de heap más simple

`Box<T>` coloca un valor de tipo `T` en el heap y guarda en el stack un puntero a él.
Tiene **exactamente un owner**: cuando el `Box` sale de scope, el valor del heap
se libera automáticamente (`Drop` implícito).

```rust
fn main() {
    let x = Box::new(5);        // 5 vive en el heap; x (en stack) apunta a él
    println!("{x}");            // Deref automático: Box<i32> → i32

    let s = Box::new(String::from("hola"));
    println!("{s}");            // Box<String> → String → str (cadena Deref)
}   // x y s salen de scope → Box::drop libera el heap
```

```text
Stack                Heap
┌──────────┐        ┌───────┐
│  x: ptr ──────────▶  5    │
└──────────┘        └───────┘
```

### Cuándo usar `Box<T>`

#### 1. Tipos recursivos (tamaño desconocido en compilación)

El compilador necesita conocer el tamaño de cada tipo en tiempo de compilación. Un tipo
que se refiere a sí mismo directamente tiene tamaño infinito:

```rust
// ❌ NO COMPILA: tamaño infinito
enum Lista {
    Cons(i32, Lista),
    Nil,
}
```

Error:

```bash
error[E0072]: recursive type `Lista` has infinite size
  |
  | enum Lista {
  |      ----- recursive type has infinite size
  |     Cons(i32, Lista),
  |              ^^^^^ recursive without indirection
  |
  = help: insert some indirection (e.g., a `Box`, `Rc`, or `&`) to break the cycle
```

`Box<T>` tiene tamaño fijo (un puntero), rompiendo el ciclo:

```rust
// ✅ Tamaño conocido: Cons = tamaño(i32) + tamaño(puntero)
#[derive(Debug)]
enum Lista {
    Cons(i32, Box<Lista>),
    Nil,
}

fn main() {
    let lista = Lista::Cons(1,
        Box::new(Lista::Cons(2,
            Box::new(Lista::Cons(3,
                Box::new(Lista::Nil))))));
    println!("{lista:?}");
}
```

#### 2. Trait objects (`Box<dyn Trait>`)

Como vimos en la Semana 6, `dyn Trait` es Unsized y necesita vivir detrás de un
puntero. `Box<dyn Trait>` es la forma más común:

```rust
trait Figura { fn area(&self) -> f64; }

struct Circulo(f64);
struct Rectangulo(f64, f64);

impl Figura for Circulo    { fn area(&self) -> f64 { std::f64::consts::PI * self.0 * self.0 } }
impl Figura for Rectangulo { fn area(&self) -> f64 { self.0 * self.1 } }

fn crear_figura(tipo: &str) -> Box<dyn Figura> {
    match tipo {
        "circulo"     => Box::new(Circulo(3.0)),
        _             => Box::new(Rectangulo(4.0, 5.0)),
    }
}

fn main() {
    let figuras: Vec<Box<dyn Figura>> = vec![
        Box::new(Circulo(2.0)),
        Box::new(Rectangulo(3.0, 4.0)),
        crear_figura("circulo"),
    ];
    for f in &figuras {
        println!("área: {:.2}", f.area());
    }
}
```

#### 3. Mover datos grandes sin copiarlos

```rust
fn procesar(datos: Box<[u8; 1_000_000]>) {
    println!("procesando {} bytes", datos.len());
}   // libera el heap aquí

fn main() {
    let buffer = Box::new([0u8; 1_000_000]);    // 1 MB en heap, no en stack
    procesar(buffer);                            // mueve el Box (8 bytes), no el array
}
```

---

## `Rc<T>`: ownership compartido en un solo hilo

`Rc<T>` (*Reference Counted*) permite que **múltiples variables sean dueñas del mismo
valor** mediante un contador de referencias. Cuando el contador llega a cero, el valor
se libera.

```rust
use std::rc::Rc;

fn main() {
    let a = Rc::new(String::from("compartido"));

    let b = Rc::clone(&a);   // incrementa el contador, NO copia el String
    let c = Rc::clone(&a);   // incrementa de nuevo

    println!("a = {a}, conteo = {}", Rc::strong_count(&a));  // 3
    println!("b = {b}, conteo = {}", Rc::strong_count(&b));  // 3
    println!("c = {c}, conteo = {}", Rc::strong_count(&c));  // 3

    drop(b);
    println!("tras drop(b): conteo = {}", Rc::strong_count(&a));  // 2

    // a, c comparten exactamente el mismo String en heap:
    println!("misma dirección: {}", std::ptr::eq(a.as_ptr(), c.as_ptr())); // true (o similar)
}   // c sale de scope → conteo=1; a sale de scope → conteo=0 → String liberado
```

```text
Stack                   Heap
┌────────┐              ┌──────────────────────────────┐
│ a: ptr ──────────────▶│  strong_count: 3             │
│ b: ptr ──────────────▶│  weak_count:   1 (siempre ≥1)│
│ c: ptr ──────────────▶│  valor: "compartido"         │
└────────┘              └──────────────────────────────┘
```

### `Rc<T>` es inmutable

El dato dentro de un `Rc<T>` solo se puede leer, no modificar directamente. La razón:
si hubiera múltiples dueños y uno modificara el dato mientras otro lo lee, habría un
data race. Para mutación necesitas combinarlo con `RefCell` (lo vemos más abajo).

```rust
let rc = Rc::new(vec![1, 2, 3]);
// rc.push(4);  // ❌ cannot borrow as mutable
```

### `Rc<T>` no es thread-safe

`Rc<T>` usa un contador de referencias **no atómico** para mayor velocidad. Por eso
no implementa `Send` ni `Sync` y el compilador impide enviarlo entre hilos:

```rust
use std::rc::Rc;

let rc = Rc::new(42);
// std::thread::spawn(move || println!("{rc}"));
// ❌ error: `Rc<i32>` cannot be sent between threads safely
```

Para hilos usa `Arc<T>` (ver más abajo).

---

## `Arc<T>`: ownership compartido entre hilos

`Arc<T>` (*Atomically Reference Counted*) es idéntico en API a `Rc<T>` pero usa
operaciones atómicas para el contador, lo que lo hace seguro entre hilos. El coste:
las operaciones atómicas son más lentas que las no atómicas.

```rust
use std::sync::Arc;
use std::thread;

fn main() {
    let datos = Arc::new(vec![1, 2, 3, 4, 5]);

    let mut handles = vec![];
    for i in 0..3 {
        let datos_clon = Arc::clone(&datos);   // incremento atómico del contador
        handles.push(thread::spawn(move || {
            println!("hilo {i}: suma = {}", datos_clon.iter().sum::<i32>());
        }));
    }

    for h in handles { h.join().unwrap(); }
    println!("conteo final: {}", Arc::strong_count(&datos));  // 1
}
```

### Tabla comparativa `Box` / `Rc` / `Arc`

| | `Box<T>` | `Rc<T>` | `Arc<T>` |
| :--- | :--- | :--- | :--- |
| Owners | **1** | Múltiples | Múltiples |
| Thread-safe | Sí (`Send` si `T: Send`) | **No** | **Sí** (`Send + Sync` si `T: Send + Sync`) |
| Contador | No | No atómico (rápido) | Atómico (más lento) |
| Mutación directa | Sí (si owner único y `mut`) | No | No |
| Tamaño en stack | 1 puntero | 1 puntero | 1 puntero |
| Overhead en heap | Ninguno | 2 contadores (strong+weak) | 2 contadores atómicos |

---

## Interior mutability: mutar a través de `&T`

La regla del borrow checker dice: `&T` es inmutable, `&mut T` es exclusivo. Los tipos
de interior mutability permiten **romper esta regla de forma controlada**, pagando el
precio del chequeo en runtime (o a través de primitivas de sincronización en hilos).

La primitiva mágica subyacente es `UnsafeCell<T>`, que le dice al compilador "no
asumas que este valor es inmutable aunque tengas `&T`". Todos los tipos de esta
sección envuelven `UnsafeCell` y añaden sus propias garantías de seguridad.

### `Cell<T>`: para tipos `Copy`

`Cell<T>` es la opción más simple y de menor coste. Solo funciona con tipos `Copy`
(nunca guarda referencias a su interior — solo copia valores):

```rust
use std::cell::Cell;

struct Contador {
    valor: Cell<u32>,
}

impl Contador {
    fn nuevo() -> Self { Self { valor: Cell::new(0) } }
    fn incrementar(&self) { self.valor.set(self.valor.get() + 1); }  // &self, no &mut self
    fn leer(&self) -> u32 { self.valor.get() }
}

fn main() {
    let c = Contador::nuevo();
    c.incrementar();
    c.incrementar();
    c.incrementar();
    println!("{}", c.leer());  // 3

    // Ambas referencias pueden llamar a incrementar:
    let r1 = &c;
    let r2 = &c;
    r1.incrementar();
    r2.incrementar();
    println!("{}", c.leer());  // 5
}
```

`Cell<T>` no tiene runtime overhead: se basa en que los tipos `Copy` nunca tienen
referencias internas que puedan invalidarse.

### `RefCell<T>`: borrow checking en runtime

`RefCell<T>` mueve las reglas del borrow checker de compile-time a runtime. Permite
**cero o más lectores** O **exactamente un escritor** — pero lo verifica en ejecución:

```rust
use std::cell::RefCell;

fn main() {
    let datos = RefCell::new(vec![1, 2, 3]);

    // borrow() devuelve Ref<T> — referencias inmutables, puede haber varias
    {
        let r1 = datos.borrow();
        let r2 = datos.borrow();
        println!("{r1:?} y {r2:?}");   // ✅ dos lectores simultáneos
    }   // r1 y r2 salen de scope → préstamos liberados

    // borrow_mut() devuelve RefMut<T> — referencia mutable exclusiva
    {
        let mut w = datos.borrow_mut();
        w.push(4);
        println!("{w:?}");             // ✅ escritor exclusivo
    }

    // Violación detectada en runtime → panic
    let _r = datos.borrow();
    // let _w = datos.borrow_mut();    // PANIC: already borrowed: BorrowMutError
}
```

#### `try_borrow` / `try_borrow_mut`: versión sin panic

```rust
let datos = RefCell::new(42);
let r = datos.borrow();

match datos.try_borrow_mut() {
    Ok(mut w) => *w += 1,
    Err(e) => eprintln!("no se pudo obtener préstamo mutable: {e}"),
}
// Output: no se pudo obtener préstamo mutable: already borrowed
```

#### Cuándo usar `RefCell<T>`

- Cuando el compilador no puede verificar en compile-time que los borrows son correctos,
  pero **tú sabes** que en runtime solo habrá un escritor a la vez.
- En mocks de test que necesitan mutar estado interno a través de `&self`.
- Combinado con `Rc<T>` para el patrón de estado compartido mutable en un hilo.

#### Coste de `RefCell<T>`

Guarda un contador de préstamos activos. Cada `borrow()` / `borrow_mut()` actualiza el
contador (muy barato — una instrucción). El panic solo ocurre si la invariante se viola.

### `Mutex<T>`: interior mutability entre hilos

`Mutex<T>` es el `RefCell<T>` seguro entre hilos. En lugar de panic, **bloquea** el
hilo hasta que el lock esté disponible:

```rust
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let contador = Arc::new(Mutex::new(0_u32));
    let mut handles = vec![];

    for _ in 0..10 {
        let c = Arc::clone(&contador);
        handles.push(thread::spawn(move || {
            let mut guard = c.lock().unwrap();   // bloquea; devuelve MutexGuard<u32>
            *guard += 1;
        }));  // guard sale de scope → lock liberado automáticamente (RAII)
    }

    for h in handles { h.join().unwrap(); }
    println!("contador final: {}", contador.lock().unwrap()); // 10
}
```

#### `PoisonError`: cuando un hilo entra en panic con el lock

Si un hilo entra en panic mientras sostiene el lock, el Mutex queda "envenenado". Las
llamadas a `lock()` devolverán `Err(PoisonError)`:

```rust
use std::sync::{Arc, Mutex};

let m = Arc::new(Mutex::new(0));
let m2 = Arc::clone(&m);

// Hilo que hace panic con el lock activo
let _ = std::thread::spawn(move || {
    let _guard = m2.lock().unwrap();
    panic!("hilo con panic");
}).join();

// El Mutex está ahora "envenenado"
match m.lock() {
    Ok(v) => println!("valor: {v}"),
    Err(e) => {
        // into_inner() extrae el valor incluso del mutex envenenado
        let v = e.into_inner();
        println!("mutex envenenado, valor recuperado: {v}");
    }
}
```

### `RwLock<T>`: múltiples lectores o un escritor

Cuando las lecturas son mucho más frecuentes que las escrituras:

```rust
use std::sync::{Arc, RwLock};
use std::thread;

fn main() {
    let cache = Arc::new(RwLock::new(std::collections::HashMap::<String, i32>::new()));

    // Escritura inicial
    cache.write().unwrap().insert("clave".to_string(), 42);

    let mut handles = vec![];

    // Múltiples lectores simultáneos
    for i in 0..5 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            let lector = c.read().unwrap();   // múltiples lectores ok
            println!("hilo {i}: {:?}", lector.get("clave"));
        }));
    }

    for h in handles { h.join().unwrap(); }
}
```

### Tabla completa de tipos de interior mutability

| Tipo | Thread-safe | Para qué tipos | Mecanismo | Si viola regla |
| :--- | :--- | :--- | :--- | :--- |
| `Cell<T>` | No | Solo `Copy` | Sin referencias internas | No compila |
| `RefCell<T>` | No | Cualquier `T` | Contador runtime | **Panic** |
| `Mutex<T>` | **Sí** | Cualquier `T: Send` | Lock OS | **Bloquea** |
| `RwLock<T>` | **Sí** | Cualquier `T: Send + Sync` | Read/Write Lock | **Bloquea** |
| `AtomicU32` etc. | **Sí** | Solo `Copy` primitivos | Instrucciones atómicas | N/A |

---

## `Weak<T>`: romper ciclos de referencia

`Rc<T>` mantiene los datos vivos mientras el `strong_count > 0`. Si dos valores se
apuntan mutuamente con `Rc`, sus contadores nunca llegan a cero: **fuga de memoria**.

### Demostración del problema

```rust
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug)]
struct Nodo {
    valor: i32,
    siguiente: Option<Rc<RefCell<Nodo>>>,
}

fn main() {
    let a = Rc::new(RefCell::new(Nodo { valor: 1, siguiente: None }));
    let b = Rc::new(RefCell::new(Nodo { valor: 2, siguiente: None }));

    // a → b
    a.borrow_mut().siguiente = Some(Rc::clone(&b));
    // b → a  (ciclo!)
    b.borrow_mut().siguiente = Some(Rc::clone(&a));

    println!("strong_count(a) = {}", Rc::strong_count(&a)); // 2
    println!("strong_count(b) = {}", Rc::strong_count(&b)); // 2
}
// a sale de scope → strong_count(a) = 1 (b sigue apuntando a a)
// b sale de scope → strong_count(b) = 1 (a sigue apuntando a b)
// → NINGUNO llega a 0 → FUGA DE MEMORIA
```

### Solución: `Weak<T>`

`Weak<T>` es una referencia **no propietaria**: no incrementa el `strong_count`, solo
el `weak_count`. Cuando el `strong_count` llega a cero, el valor se libera aunque
haya `Weak` vivos. Para acceder al valor hay que llamar a `upgrade()`, que devuelve
`Option<Rc<T>>`: `None` si el valor ya fue liberado.

```rust
use std::rc::{Rc, Weak};
use std::cell::RefCell;

#[derive(Debug)]
struct Nodo {
    valor: i32,
    // ✅ padre usa Weak: no impide la liberación del padre
    padre: Option<Weak<RefCell<Nodo>>>,
    // hijos usan Rc: el nodo es dueño de sus hijos
    hijos: Vec<Rc<RefCell<Nodo>>>,
}

fn main() {
    let padre = Rc::new(RefCell::new(Nodo {
        valor: 1, padre: None, hijos: vec![],
    }));

    let hijo = Rc::new(RefCell::new(Nodo {
        valor: 2,
        padre: Some(Rc::downgrade(&padre)),   // Weak al padre
        hijos: vec![],
    }));

    padre.borrow_mut().hijos.push(Rc::clone(&hijo));

    println!("strong(padre) = {}", Rc::strong_count(&padre)); // 1  ← Weak no cuenta
    println!("strong(hijo)  = {}", Rc::strong_count(&hijo));  // 2  ← padre y main

    // Navegar al padre desde el hijo:
    if let Some(p) = hijo.borrow().padre.as_ref().and_then(|w| w.upgrade()) {
        println!("padre del hijo: {}", p.borrow().valor); // 1
    }
}
// padre sale de scope → strong_count = 0 → liberado
// hijo sale de scope → strong_count = 0 → liberado
// ✅ sin fugas
```

### `Rc::strong_count` vs `Rc::weak_count`

```text
Heap (bloque de control de Rc/Weak):
┌─────────────────────────────────────┐
│  strong_count: N   ← Rc::clone()   │  cuando llega a 0: libera el valor T
│  weak_count:   M   ← Weak::clone() │  cuando llega a 0: libera el bloque de control
│  valor: T                           │
└─────────────────────────────────────┘
```

El bloque de control se libera cuando **ambos** contadores llegan a cero.

---

## Los patrones canónicos

### `Rc<RefCell<T>>`: estado compartido mutable en un solo hilo

```rust
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug)]
struct Config {
    nivel_log: u32,
    modo_debug: bool,
}

fn main() {
    let cfg = Rc::new(RefCell::new(Config {
        nivel_log: 1,
        modo_debug: false,
    }));

    // Múltiples "propietarios" de la configuración
    let modulo_a = Rc::clone(&cfg);
    let modulo_b = Rc::clone(&cfg);

    // modulo_a la modifica
    modulo_a.borrow_mut().nivel_log = 3;

    // modulo_b la lee (ve el cambio)
    println!("nivel desde B: {}", modulo_b.borrow().nivel_log); // 3

    // Patrón: obtener referencia temporal, hacer la operación, soltar
    {
        let mut c = cfg.borrow_mut();
        c.nivel_log = 5;
        c.modo_debug = true;
    }   // RefMut liberado aquí — crítico si luego hay más borrows

    println!("{:?}", cfg.borrow()); // Config { nivel_log: 5, modo_debug: true }
}
```

**Trampa frecuente**: mantener un `borrow_mut()` activo e intentar otro `borrow()` o
`borrow_mut()` en el mismo hilo causa panic. Minimiza el scope del `borrow_mut` con
un bloque `{}`.

### `Arc<Mutex<T>>`: estado compartido mutable entre hilos

```rust
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct ColaBloqueante {
    elementos: Vec<String>,
}

impl ColaBloqueante {
    fn nueva() -> Self { Self { elementos: Vec::new() } }
    fn push(&mut self, s: String) { self.elementos.push(s); }
    fn pop(&mut self) -> Option<String> { self.elementos.pop() }
}

fn main() {
    let cola = Arc::new(Mutex::new(ColaBloqueante::nueva()));

    // Hilo productor
    let cola_prod = Arc::clone(&cola);
    let prod = thread::spawn(move || {
        for i in 0..5 {
            cola_prod.lock().unwrap().push(format!("tarea-{i}"));
            thread::sleep(Duration::from_millis(10));
        }
    });

    // Hilo consumidor
    let cola_cons = Arc::clone(&cola);
    let cons = thread::spawn(move || {
        thread::sleep(Duration::from_millis(25));
        let mut c = cola_cons.lock().unwrap();
        while let Some(tarea) = c.pop() {
            println!("consumida: {tarea}");
        }
    });

    prod.join().unwrap();
    cons.join().unwrap();
}
```

### `Arc<RwLock<T>>`: caché de lectura intensiva

```rust
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

type Cache = Arc<RwLock<HashMap<String, String>>>;

fn obtener(cache: &Cache, clave: &str) -> Option<String> {
    cache.read().unwrap().get(clave).cloned()   // múltiples lectores simultáneos
}

fn insertar(cache: &Cache, clave: String, valor: String) {
    cache.write().unwrap().insert(clave, valor); // escritor exclusivo
}
```

---

## Ejercicio: árbol con navegación bidireccional

Implementa un árbol donde cada nodo puede navegar tanto hacia sus hijos como hacia
su padre, sin ciclos de referencia.

Crea un proyecto con `cargo new arbol_rc --lib` y escribe en `src/lib.rs`:

```rust
use std::cell::RefCell;
use std::fmt;
use std::rc::{Rc, Weak};

// Alias de tipo para mejorar legibilidad
type NodoRef<T> = Rc<RefCell<Nodo<T>>>;
type NodoDebil<T> = Weak<RefCell<Nodo<T>>>;

pub struct Nodo<T> {
    pub id: usize,
    pub dato: T,
    padre: Option<NodoDebil<T>>,
    hijos: Vec<NodoRef<T>>,
}

impl<T: fmt::Debug> fmt::Debug for Nodo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nodo(id={}, dato={:?}, hijos={})", self.id, self.dato, self.hijos.len())
    }
}

impl<T> Nodo<T> {
    fn nuevo_ref(id: usize, dato: T) -> NodoRef<T> {
        Rc::new(RefCell::new(Nodo {
            id,
            dato,
            padre: None,
            hijos: vec![],
        }))
    }
}

pub struct Arbol<T> {
    raiz: Option<NodoRef<T>>,
    siguiente_id: usize,
}

impl<T> Arbol<T> {
    pub fn nuevo() -> Self {
        Self { raiz: None, siguiente_id: 0 }
    }

    pub fn agregar_raiz(&mut self, dato: T) -> NodoRef<T> {
        let nodo = Nodo::nuevo_ref(self.siguiente_id, dato);
        self.siguiente_id += 1;
        self.raiz = Some(Rc::clone(&nodo));
        nodo
    }

    pub fn agregar_hijo(&mut self, padre: &NodoRef<T>, dato: T) -> NodoRef<T> {
        let hijo = Nodo::nuevo_ref(self.siguiente_id, dato);
        self.siguiente_id += 1;

        // hijo → padre: Weak (no propietario, evita ciclo)
        hijo.borrow_mut().padre = Some(Rc::downgrade(padre));

        // padre → hijo: Rc (propietario)
        padre.borrow_mut().hijos.push(Rc::clone(&hijo));

        hijo
    }

    pub fn raiz(&self) -> Option<&NodoRef<T>> {
        self.raiz.as_ref()
    }
}

// Funciones de navegación
pub fn padre_de<T>(nodo: &NodoRef<T>) -> Option<NodoRef<T>> {
    nodo.borrow().padre.as_ref()?.upgrade()
}

pub fn num_hijos<T>(nodo: &NodoRef<T>) -> usize {
    nodo.borrow().hijos.len()
}

pub fn hijo_en<T>(nodo: &NodoRef<T>, indice: usize) -> Option<NodoRef<T>> {
    nodo.borrow().hijos.get(indice).map(Rc::clone)
}

pub fn profundidad<T>(nodo: &NodoRef<T>) -> usize {
    let mut prof = 0;
    let mut actual = padre_de(nodo);
    while let Some(p) = actual {
        prof += 1;
        actual = padre_de(&p);
    }
    prof
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construir_arbol_simple() {
        let mut arbol = Arbol::nuevo();
        let raiz = arbol.agregar_raiz("raiz");
        let hijo1 = arbol.agregar_hijo(&raiz, "hijo1");
        let _hijo2 = arbol.agregar_hijo(&raiz, "hijo2");
        let nieto = arbol.agregar_hijo(&hijo1, "nieto");

        assert_eq!(num_hijos(&raiz), 2);
        assert_eq!(num_hijos(&hijo1), 1);
        assert_eq!(num_hijos(&nieto), 0);
    }

    #[test]
    fn navegar_al_padre() {
        let mut arbol = Arbol::nuevo();
        let raiz = arbol.agregar_raiz(1);
        let hijo = arbol.agregar_hijo(&raiz, 2);
        let nieto = arbol.agregar_hijo(&hijo, 3);

        let padre_del_nieto = padre_de(&nieto).expect("nieto debe tener padre");
        assert_eq!(padre_del_nieto.borrow().dato, 2);

        let abuelo = padre_de(&padre_del_nieto).expect("hijo debe tener padre");
        assert_eq!(abuelo.borrow().dato, 1);

        assert!(padre_de(&raiz).is_none(), "la raíz no tiene padre");
    }

    #[test]
    fn profundidad_correcta() {
        let mut arbol = Arbol::nuevo();
        let raiz = arbol.agregar_raiz("r");
        let hijo = arbol.agregar_hijo(&raiz, "h");
        let nieto = arbol.agregar_hijo(&hijo, "n");
        let bisnieto = arbol.agregar_hijo(&nieto, "b");

        assert_eq!(profundidad(&raiz), 0);
        assert_eq!(profundidad(&hijo), 1);
        assert_eq!(profundidad(&nieto), 2);
        assert_eq!(profundidad(&bisnieto), 3);
    }

    #[test]
    fn sin_fuga_de_memoria() {
        // Verifica que los strong_count llegan a 0 correctamente.
        // Si hubiera ciclos de Rc, estos contadores nunca bajarían de 1.
        let hijo_debil;
        {
            let mut arbol = Arbol::nuevo();
            let raiz = arbol.agregar_raiz(0);
            let hijo = arbol.agregar_hijo(&raiz, 1);
            hijo_debil = Rc::downgrade(&hijo);

            assert_eq!(Rc::strong_count(&raiz), 2); // arbol.raiz + variable local
            assert_eq!(Rc::strong_count(&hijo), 2); // en raiz.hijos + variable local
        }
        // Arbol y variables locales salen de scope.
        // Si no hay fugas, el Weak ya no puede hacer upgrade:
        assert!(hijo_debil.upgrade().is_none(), "el nodo debería haber sido liberado");
    }

    #[test]
    fn weak_invalido_tras_drop_del_padre() {
        let mut arbol = Arbol::nuevo();
        let raiz = arbol.agregar_raiz("raiz");
        let hijo = arbol.agregar_hijo(&raiz, "hijo");

        // Guardamos un Weak al padre
        let raiz_debil = Rc::downgrade(&raiz);
        assert!(raiz_debil.upgrade().is_some());

        // Eliminamos la raíz del árbol soltando todas las Rc propietarias
        drop(raiz);
        drop(arbol);   // arbol.raiz también era una Rc

        // La raíz fue liberada; el hijo también (era el único propietario la raíz de él)
        // El Weak ya no puede hacer upgrade:
        assert!(raiz_debil.upgrade().is_none());
        // El hijo_debil tampoco (el hijo era hijo de raiz que ya se liberó)
        let hijo_debil = Rc::downgrade(&hijo);
        drop(hijo);
        assert!(hijo_debil.upgrade().is_none());
    }

    #[test]
    fn modificar_dato_a_traves_de_refcell() {
        let mut arbol = Arbol::<i32>::nuevo();
        let raiz = arbol.agregar_raiz(0);
        let hijo = arbol.agregar_hijo(&raiz, 10);

        // Modificamos el dato del hijo a través de una referencia compartida
        hijo.borrow_mut().dato = 99;

        // Accedemos al hijo a través del árbol (navegando desde la raíz)
        let hijo_desde_raiz = hijo_en(&raiz, 0).unwrap();
        assert_eq!(hijo_desde_raiz.borrow().dato, 99);
    }
}
```

Ejecuta:

```bash
cargo test
cargo clippy
```

### Puntos de aprendizaje del ejercicio

- `Rc<RefCell<Nodo<T>>>` es el patrón canónico para árboles y grafos en un solo hilo:
  `Rc` para ownership compartido entre padre e hijo, `RefCell` para modificar el nodo
  a través de referencias compartidas.
- El test `sin_fuga_de_memoria` verifica con `Rc::strong_count` y `Weak::upgrade`
  que los contadores llegan a cero y la memoria se libera. Si hubiera ciclos de `Rc`,
  `upgrade()` seguiría devolviendo `Some` después del drop.
- `profundidad` muestra el patrón idiomático de navegar por `Weak` con `upgrade()` en
  un loop: `while let Some(p) = actual { actual = padre_de(&p); }`.
- Minimizar el scope de `borrow_mut()` (bloques `{}`) evita panics por préstamos
  anidados — ver la implementación de `agregar_hijo` que termina ambos borrows antes
  de retornar.

---

## Errores comunes y soluciones

| Error / Síntoma | Causa | Solución |
| :--- | :--- | :--- |
| `already borrowed: BorrowMutError` (panic) | `borrow()` y `borrow_mut()` activos simultáneamente | Acorta el scope del `borrow_mut` con `{}` o reestructura el flujo |
| `Rc<T> cannot be sent between threads` | `Rc` no es `Send` | Cambia a `Arc<T>` |
| `RefCell<T> is not Sync` | `RefCell` no es seguro entre hilos | Usa `Mutex<T>` en su lugar |
| Fuga de memoria (programa no libera) | Ciclo de `Rc` fuerte | Usa `Weak` para al menos una dirección del ciclo |
| Deadlock con `Mutex` | El mismo hilo llama a `lock()` dos veces | Usa `try_lock()` o reestructura para no necesitar locks anidados |
| `PoisonError` en `lock()` | Un hilo entró en panic con el lock | Maneja el `Err` con `unwrap_or_else(|e| e.into_inner())` |

---

## ✅ Checklist de la Semana 7

- [ ] Sé cuándo usar `Box<T>`: tipos recursivos, `dyn Trait`, mover datos grandes.
- [ ] Distingo `Rc<T>` (un hilo, no atómico) de `Arc<T>` (multi-hilo, atómico).
- [ ] Entiendo que `Rc`/`Arc` no permiten mutación directa — para eso necesito
  `RefCell` o `Mutex`.
- [ ] Uso `Cell<T>` para tipos `Copy` cuando no necesito referencias internas.
- [ ] Uso `RefCell<T>` para interior mutability en un hilo; `try_borrow_mut()` cuando
  no quiero panic.
- [ ] Uso `Mutex<T>` para interior mutability entre hilos; gestiono `PoisonError`.
- [ ] Uso `RwLock<T>` cuando las lecturas dominan sobre las escrituras.
- [ ] Entiendo por qué `Rc`+ciclo provoca fuga y cómo `Weak` lo resuelve.
- [ ] El test `sin_fuga_de_memoria` del ejercicio pasa y entiendo por qué verifica
  lo que verifica.
- [ ] Aplico el patrón `Rc<RefCell<T>>` y `Arc<Mutex<T>>` en el contexto correcto.
- [ ] Veo el video de Jon Gjengset sobre Interior Mutability.

> **Siguiente paso:** Semana 8 — [Testing, documentación y tooling profesional](section_04.md).
