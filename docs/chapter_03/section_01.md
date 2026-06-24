# Fundamentos Async: bajo el capó

La Semana 9 desmitifica `async`/`await`. Muchos programadores usan estas palabras clave
sin entender qué genera el compilador, por qué existe `Pin`, o cómo un executor decide
cuándo y cómo avanzar una tarea. Sin ese entendimiento, los errores como
`future cannot be sent between threads` o un deadlock silencioso se vuelven opacos.

Esta sección enseña el **modelo mental correcto desde el principio**.

En esta sección aprenderemos:

- Por qué async existe y qué problema resuelve.
- El trait `Future` y el ciclo `poll` / `Pending` / `Ready`.
- Cómo el compilador convierte `async fn` en una máquina de estados.
- Qué es `Pin` y por qué las futures lo necesitan.
- Cómo funciona un executor y qué hace `#[tokio::main]`.
- Las primitivas de concurrencia de Tokio: `spawn`, `join!`, `select!`,
  `spawn_blocking`.
- La regla de oro de `Send + Sync` en async.

> 💡 **Filosofía de la Semana 9:** *Async en Rust no es magia — es una máquina de
> estados generada por el compilador que necesita un executor para avanzar. Cuando
> entiendes eso, cada error de compilación relacionado con `async` se vuelve legible.*

---

## Por qué async existe

Un servidor web que gestiona conexiones con un hilo por conexión escala mal: cada hilo
ocupa cientos de KB de stack, el scheduler del OS tiene que cambiar entre ellos, y el
número de conexiones simultáneas queda limitado.

La alternativa es el **modelo event-driven**: un número pequeño de hilos gestiona
miles de conexiones concurrentes. Cada vez que una conexión necesita esperar (leer del
socket, consultar la base de datos), el hilo pasa a gestionar otra en lugar de dormir.

```text
Modelo de hilos (síncrono):           Modelo async:
                                       
Hilo 1: [=conn1=][ESPERA][=conn1=]    Worker 1: [=conn1=][=conn3=][=conn1=][=conn2=]
Hilo 2: [=conn2=][ESPERA][=conn2=]    Worker 2: [=conn4=][=conn2=][=conn5=][=conn4=]
Hilo 3: [=conn3=][ESPERA][=conn3=]
...                                    (mismo throughput, muchos menos hilos)
Hilo N: dormido, esperando
```

Rust implementa esto con el trait `Future` y la sintaxis `async`/`await`, sin garbage
collector y con garantías de memoria en tiempo de compilación.

---

## El trait `Future`

Una `Future` representa una **computación perezosa**: un trabajo que puede no estar
listo todavía y que se avanza preguntándole si ya terminó.

```rust
use std::pin::Pin;
use std::task::{Context, Poll};

// Definición simplificada de std::future::Future
trait Future {
    type Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}

enum Poll<T> {
    Ready(T),   // la computación terminó, aquí está el valor
    Pending,    // aún no está lista, volveré a avisar via Waker
}
```

Puntos cruciales:

- `poll` **no bloquea jamás**. Devuelve control inmediato. Si el trabajo no está listo,
  devuelve `Pending`.
- `Pin<&mut Self>` garantiza que la future no se moverá en memoria mientras está siendo
  polleada (lo explicamos en detalle más abajo).
- `Context` contiene el `Waker`: un mecanismo para notificar al executor cuando la
  future vuelve a tener trabajo pendiente.

### La future más simple posible

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

struct Listo(i32);

impl Future for Listo {
    type Output = i32;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<i32> {
        Poll::Ready(self.0)   // siempre lista en el primer poll
    }
}
```

### Una future con estado: `Pending` antes de `Ready`

```rust
struct Cuenta {
    restante: u32,
}

impl Future for Cuenta {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
        if self.restante == 0 {
            Poll::Ready("terminé")
        } else {
            self.restante -= 1;
            // Avisa al executor: "vuelve a pollearme pronto"
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
```

`wake_by_ref()` no sería lo que haría una future real de I/O. Una future de socket real
registraría el waker con el loop de eventos del OS (epoll/kqueue/IOCP) y solo lo
llamaría cuando hubiera datos disponibles.

---

## Un mini-executor: `block_on`

Para ver el ciclo completo, construyamos el executor más simple posible con solo `std`:

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

// Un Waker vacío: en un runtime real re-encolaría la tarea
struct WakerVacio;

impl Wake for WakerVacio {
    fn wake(self: Arc<Self>) {}
}

/// Ejecuta una future en el hilo actual hasta que termine.
/// Hace "busy-poll": itera sin dormir entre polls. Solo para demostración.
fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = Box::pin(fut);                          // (1) fijar en heap
    let waker = Waker::from(Arc::new(WakerVacio));
    let mut cx = Context::from_waker(&waker);

    loop {
        match fut.as_mut().poll(&mut cx) {               // (2) preguntar
            Poll::Ready(val) => return val,               // (3a) terminó
            Poll::Pending => continue,                    // (3b) no aún, volver
        }
    }
}

fn main() {
    // Ejecutar nuestra future hecha a mano:
    let resultado = block_on(Listo(42));
    println!("{resultado}");   // 42

    let resultado2 = block_on(Cuenta { restante: 3 });
    println!("{resultado2}");  // "terminé"
}
```

Esto es **exactamente** lo que hace Tokio, pero con un executor real que:

1. Tiene un pool de hilos OS para distribuir tareas.
2. Usa `epoll`/`kqueue`/`IOCP` para registrar wakers y dormirse hasta que haya I/O real.
3. Re-encola tareas cuando su waker las despierta, en lugar de girar en bucle.

---

## `async`/`await`: la máquina de estados

La sintaxis `async fn` es azúcar que el compilador convierte en un tipo que implementa
`Future`. Cada `.await` es un **punto de suspensión**: el lugar donde la función puede
devolver control al executor si el trabajo no está listo.

```rust
async fn obtener_usuario(id: u64) -> String {
    // Simula latencia de red
    formato_async(id).await
}

async fn obtener_posts(usuario: &str) -> Vec<String> {
    vec![format!("post de {usuario}")]
}

async fn flujo(id: u64) -> (String, Vec<String>) {
    let usuario = obtener_usuario(id).await;      // punto de yield 1
    let posts = obtener_posts(&usuario).await;    // punto de yield 2
    (usuario, posts)
}
```

Lo que el compilador genera es aproximadamente:

```rust
// Cada estado de la máquina corresponde a un punto entre .await
enum FlujoDatos {
    Inicio { id: u64 },
    EsperandoUsuario { fut_usuario: ObtenerUsuarioFut },
    EsperandoPosts   { usuario: String, fut_posts: ObtenerPostsFut },
    Hecho,
}

struct FlujoDatosFut { estado: FlujoDatos }

impl Future for FlujoDatosFut {
    type Output = (String, Vec<String>);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match &mut self.estado {
                FlujoDatos::Inicio { id } => {
                    // Crea la future del primer await y pasa al siguiente estado
                    let id = *id;
                    self.estado = FlujoDatos::EsperandoUsuario {
                        fut_usuario: obtener_usuario(id),
                    };
                }
                FlujoDatos::EsperandoUsuario { fut_usuario } => {
                    // Poll la future interna
                    match Pin::new(fut_usuario).poll(cx) {
                        Poll::Pending => return Poll::Pending,        // propaga Pending
                        Poll::Ready(usuario) => {
                            // Avanza al siguiente estado guardando el resultado
                            self.estado = FlujoDatos::EsperandoPosts {
                                usuario: usuario.clone(),
                                fut_posts: obtener_posts(&usuario),
                            };
                        }
                    }
                }
                FlujoDatos::EsperandoPosts { usuario, fut_posts } => {
                    match Pin::new(fut_posts).poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(posts) => {
                            let resultado = (usuario.clone(), posts);
                            self.estado = FlujoDatos::Hecho;
                            return Poll::Ready(resultado);
                        }
                    }
                }
                FlujoDatos::Hecho => panic!("polleada después de Ready"),
            }
        }
    }
}
```

Esto explica varias cosas que antes parecían arbitrarias:

- Una `async fn` **no ejecuta nada** cuando la llamas. Solo construye la máquina de
  estados. Necesita `.await` o un executor para avanzar.
- El tamaño de la future es el **tamaño del estado más grande** que necesita guardar,
  no el tamaño de la pila de llamadas.
- Las variables locales que atraviesan un `.await` (como `usuario` en el ejemplo) se
  guardan **dentro de la propia future** como campos de la enum. Esto las convierte en
  estructuras auto-referenciales — el motivo de `Pin`.

---

## `Pin<P>` y `Unpin`: la garantía de dirección

### El problema: estructuras auto-referenciales

Imagina una future que guarda una referencia a una de sus propias variables locales:

```text
Antes de mover:                     Después de mover (sin Pin):
                                     
Memoria 0x100:                       Memoria 0x200 (nueva dirección):
┌─────────────────────────┐          ┌─────────────────────────┐
│  datos: [1, 2, 3]       │          │  datos: [1, 2, 3]       │
│  referencia: 0x100 ─────┼──┐       │  referencia: 0x100 ─────┼──┐ APUNTA AL LUGAR
└─────────────────────────┘  │       └─────────────────────────┘  │ ANTIGUO → UB ☠️
                             ▼                                      │
                         [1, 2, 3] (en 0x100)                      │
                                                      0x100: ???   ◄┘
```

Si movemos la future después de que la referencia fue creada, la referencia apunta a
memoria inválida. En C esto sería undefined behavior. Rust lo evita con `Pin`.

### `Pin<P>`: la promesa de no mover

`Pin<P>` envuelve un puntero `P` (como `&mut T` o `Box<T>`) y **prohíbe mover el
valor apuntado** desde ese momento:

```rust
use std::pin::Pin;

let mut valor = 42_i32;
let pinned = Pin::new(&mut valor);
// *pinned = 100;      // ✅ se puede modificar el valor
// let movido = *pinned; // ✅ copiar (Copy) está bien
// mover_fuera(pinned); // ❌ no se puede obtener &mut i32 para moverlo
```

### `Unpin`: tipos que sí se pueden mover aunque estén en `Pin`

La mayoría de los tipos son `Unpin` (es un auto-trait, como `Send` y `Sync`): no tienen
referencias internas que se invalidarían al moverlos, así que `Pin<&mut T>` no añade
restricción real para ellos.

```rust
// i32, String, Vec<T>, Box<T>... son Unpin
let mut s = String::from("hola");
let pin = Pin::new(&mut s);
// Con Unpin, se puede obtener &mut String del Pin:
let inner: &mut String = Pin::into_inner(pin);  // ✅ porque String: Unpin
```

Las futures generadas por `async fn` son **`!Unpin`** cuando capturan referencias a sus
propias variables locales. Para ellas, `Pin` sí impone la restricción real.

### Regla práctica: raramente escribes `Pin` a mano

En el 99 % del código async solo necesitas saber:

```rust
// Para fijar una future en el heap:
let fut = Box::pin(mi_future_async());

// Para fijar una future en el stack (macro de Tokio):
tokio::pin!(mi_future_async());  // crea una variable local fijada

// El error más común y su solución:
// error: the `poll` method requires the value to be stable in memory
// → usa Box::pin() o tokio::pin!() sobre la future
```

---

## Tokio: el executor de producción

### `#[tokio::main]`: qué hace realmente

```rust
#[tokio::main]
async fn main() {
    println!("hola desde async");
}

// Se expande aproximadamente a:
fn main() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            println!("hola desde async");
        });
}
```

Crea un **runtime multi-thread** con tantos worker threads como núcleos lógicos tiene
la CPU, un reactor de I/O (epoll en Linux, kqueue en macOS, IOCP en Windows) y una
cola de tareas listas (*run queue*).

Para servicios de I/O intensivo (servidores web, clientes de BD) ese es el setting
correcto. Para scripts simples o tests:

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() { /* ... */ }
```

Usa un solo hilo OS, ideal cuando no necesitas paralelismo real.

---

## Primitivas de concurrencia de Tokio

### `tokio::spawn`: tareas en background

Ejecuta una future de forma independiente en el runtime. Devuelve un `JoinHandle`:

```rust
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() {
    let handle: JoinHandle<i32> = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        42
    });

    println!("la tarea sigue corriendo...");

    // await sobre el JoinHandle espera a que termine y extrae el resultado
    let resultado = handle.await.unwrap();
    println!("resultado: {resultado}");
}
```

**Restricciones de `spawn`**: la future debe ser `Send + 'static`. Esto significa:

- No puede capturar `Rc<T>`, `RefCell<T>` ni `MutexGuard<T>` (no son `Send`).
- No puede capturar referencias a variables locales que no duren toda la ejecución
  del programa (`'static` bound).

```rust
// ❌ NO COMPILA: Rc no es Send
let rc = std::rc::Rc::new(42);
tokio::spawn(async move { println!("{rc}"); });
// error: `Rc<i32>` cannot be sent between threads safely

// ✅ Arc sí es Send:
let arc = std::sync::Arc::new(42);
tokio::spawn(async move { println!("{arc}"); });
```

### `tokio::join!`: concurrencia dentro de una tarea

Ejecuta **múltiples futures concurrentemente en la misma tarea**. Termina cuando
**todas** han completado:

```rust
use tokio::time::{sleep, Duration};

async fn consulta_bd(id: u32) -> String {
    sleep(Duration::from_millis(100)).await;
    format!("fila_{id}")
}

#[tokio::main]
async fn main() {
    let inicio = std::time::Instant::now();

    // Sin join!: secuencial, ~300ms
    // let a = consulta_bd(1).await;
    // let b = consulta_bd(2).await;
    // let c = consulta_bd(3).await;

    // Con join!: concurrente en la misma tarea, ~100ms
    let (a, b, c) = tokio::join!(
        consulta_bd(1),
        consulta_bd(2),
        consulta_bd(3),
    );

    println!("{a}, {b}, {c}");
    println!("tardó: {:?}", inicio.elapsed()); // ~100ms, no ~300ms
}
```

`join!` es ideal para **consultas independientes dentro de un handler**: no crea nuevas
tareas, no impone `Send`, y no tiene el overhead de scheduling de `spawn`.

### `tokio::select!`: la primera que termine gana

Ejecuta múltiples futures concurrentemente y completa cuando **cualquiera** termina,
cancelando las demás:

```rust
use tokio::time::{sleep, Duration, timeout};

#[tokio::main]
async fn main() {
    // Patrón 1: timeout
    let resultado = tokio::select! {
        val = operacion_lenta()             => format!("éxito: {val}"),
        _ = sleep(Duration::from_secs(1))  => "timeout".to_string(),
    };
    println!("{resultado}");

    // Patrón 2: apagado graceful
    tokio::select! {
        _ = servidor()      => println!("servidor terminó"),
        _ = senal_ctrl_c()  => println!("Ctrl+C recibido, apagando..."),
    }
}

async fn operacion_lenta() -> i32 {
    sleep(Duration::from_millis(500)).await;
    42
}

async fn servidor() { /* loop que acepta conexiones */ loop { sleep(Duration::from_secs(10)).await; } }
async fn senal_ctrl_c() { tokio::signal::ctrl_c().await.unwrap(); }
```

**Cancelación**: cuando `select!` elige una rama, las otras futures se **dropean**.
Para una future, ser dropeada en cualquier punto de suspensión es un comportamiento
válido y esperado — es la mecánica de la cancelación async en Rust. Por eso es
importante que las futures limpien recursos en `Drop`.

### `timeout`: envoltorio simple para `select!`

```rust
use tokio::time::{timeout, Duration};

async fn con_timeout() -> Result<i32, ()> {
    timeout(Duration::from_secs(2), operacion())
        .await
        .map_err(|_elapsed| ())   // Elapsed → error propio
}

async fn operacion() -> i32 { 42 }
```

### `spawn_blocking`: código síncrono en async

Nunca ejecutes código bloqueante directamente en una tarea async. Bloquear un worker
thread impide que procese otras tareas:

```rust
// ❌ MAL: bloquea el worker thread del runtime
async fn hashear_contrasena(pass: String) -> String {
    bcrypt::hash(&pass, 12).unwrap()   // CPU-intensivo, bloquea segundos
}

// ✅ BIEN: ejecuta en un pool de hilos bloqueantes separado
async fn hashear_contrasena(pass: String) -> String {
    tokio::task::spawn_blocking(move || {
        bcrypt::hash(&pass, 12).unwrap()
    })
    .await
    .unwrap()
}
```

Usa `spawn_blocking` para:

- Código CPU-intensivo (criptografía, compresión, procesamiento de imágenes).
- Librerías síncronas de C/C++ vía FFI.
- `std::fs` (usa `tokio::fs` si puedes, pero a veces no es posible).
- Cualquier llamada que pueda tardar más de ~100µs sin awaitar.

### Resumen de primitivas de concurrencia

| Primitiva | Ejecuta en | Cuándo termina | Restricción | Cuándo usar |
| :--- | :--- | :--- | :--- | :--- |
| `spawn` | Nueva tarea (otro hilo posible) | Independiente | `Send + 'static` | Tareas largas, fire-and-forget |
| `join!` | Misma tarea | Cuando **todas** | Ninguna | Consultas paralelas en handler |
| `select!` | Misma tarea | Cuando **una** | Ninguna | Timeouts, shutdown, race |
| `spawn_blocking` | Pool bloqueante | Independiente | `Send + 'static` | CPU/IO bloqueante |

---

## `Send` + `Sync` en async: la regla de oro

### La regla

> **Todo lo que atraviese un `.await` dentro de una tarea `spawn` debe ser `Send`.**

El compilador verifica esto. Si guardas un valor `!Send` en una variable y luego haces
`.await` sin haberlo soltado, el compilador emite un error que puede ser confuso:

```rust
use std::rc::Rc;

async fn tarea_con_rc() {
    let rc = Rc::new(42);   // Rc es !Send
    alguna_future().await;  // rc atraviesa el await
    println!("{rc}");
}

// Si hacemos spawn:
tokio::spawn(tarea_con_rc());
// error: future cannot be sent between threads safely
//        └── `Rc<i32>` cannot be shared between threads safely
```

### Solución: `Arc` + `tokio::sync::Mutex`

```rust
use std::sync::Arc;
use tokio::sync::Mutex;   // ← Mutex de Tokio, no de std

async fn tarea_correcta(datos: Arc<Mutex<Vec<i32>>>) {
    let mut guard = datos.lock().await;   // await: el lock de Tokio es async-aware
    guard.push(42);
    // guard se libera automáticamente al salir de scope (antes de cualquier otro .await)
}
```

### El peligro del `MutexGuard` a través de `.await`

```rust
use std::sync::Mutex;   // ← std::sync, NO tokio::sync

async fn peligroso(m: &Mutex<Vec<i32>>) {
    let mut guard = m.lock().unwrap();   // guarda a través del await: PROBLEMA
    guard.push(1);
    alguna_future().await;   // ❌ guard (std MutexGuard) atraviesa el await
    guard.push(2);           // podría deadlock: el lock no se libera entre polls
}
```

Soluciones:

```rust
// Opción 1: soltar el guard ANTES del .await
async fn correcto_1(m: &Mutex<Vec<i32>>) {
    {
        let mut guard = m.lock().unwrap();
        guard.push(1);
    }   // guard liberado aquí
    alguna_future().await;
    {
        let mut guard = m.lock().unwrap();
        guard.push(2);
    }
}

// Opción 2: usar tokio::sync::Mutex (su guard es Send y async-aware)
async fn correcto_2(m: &tokio::sync::Mutex<Vec<i32>>) {
    let mut guard = m.lock().await;
    guard.push(1);
    // alguna_future().await; — con tokio::sync::Mutex esto es seguro
    guard.push(2);
}
```

### Tabla: qué tipos son seguros en async

| Tipo | `Send` | `Sync` | Uso en `spawn` | Usar en async |
| :--- | :--- | :--- | :--- | :--- |
| `Arc<T>` (T: Send+Sync) | ✅ | ✅ | ✅ | ✅ |
| `Rc<T>` | ❌ | ❌ | ❌ | Solo en `current_thread` |
| `std::sync::Mutex<T>` | ✅ | ✅ | ✅ (el Mutex sí) | ⚠️ no hagas `.await` con el guard activo |
| `tokio::sync::Mutex<T>` | ✅ | ✅ | ✅ | ✅ (guard es Send) |
| `RefCell<T>` | ❌ | ❌ | ❌ | Solo en `current_thread` |
| `std::sync::MutexGuard<T>` | ❌ | ❌ | ❌ | Libera antes del `.await` |

---

## Ejercicio: mini-runtime "Toykio"

Este ejercicio implementa un executor de un solo hilo desde cero usando solo `std`.
El objetivo no es llegar a un runtime de producción — es ver cada pieza de la
maquinaria: `RawWaker`, `Waker`, `Context`, `Pin`, y la cola de tareas.

Crea `cargo new toykio --bin` y escribe en `src/main.rs`:

```rust
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
use std::time::{Duration, Instant};

// ── Cola de tareas compartida ──────────────────────────────────────────────

type ColaTareas = Arc<Mutex<VecDeque<Arc<Tarea>>>>;

struct Tarea {
    future: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    cola: ColaTareas,   // referencia a la cola para re-encolar en wake
}

// ── Waker: cómo una tarea se despierta a sí misma ─────────────────────────
// RawWakerVTable define las operaciones que el runtime ejecuta sobre un Waker.
// Usamos Arc<Tarea> como puntero concreto.

unsafe fn clonar_waker(ptr: *const ()) -> RawWaker {
    let arc = unsafe { Arc::from_raw(ptr as *const Tarea) };
    let clonado = arc.clone();
    std::mem::forget(arc);     // no decrementar el refcount del original
    RawWaker::new(Arc::into_raw(clonado) as *const (), &VTABLE)
}

unsafe fn despertar(ptr: *const ()) {
    let arc = unsafe { Arc::from_raw(ptr as *const Tarea) };
    let mut cola = arc.cola.lock().unwrap();
    cola.push_back(arc.clone());   // re-encola la tarea para que el runtime la pollee
    // arc se dropea aquí decrementando el refcount
}

unsafe fn despertar_por_ref(ptr: *const ()) {
    let arc = unsafe { Arc::from_raw(ptr as *const Tarea) };
    {
        let mut cola = arc.cola.lock().unwrap();
        cola.push_back(arc.clone());
    }
    std::mem::forget(arc);   // no decrementar: la referencia original sigue viva
}

unsafe fn soltar(ptr: *const ()) {
    drop(unsafe { Arc::from_raw(ptr as *const Tarea) }); // decrementa refcount
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    clonar_waker,
    despertar,
    despertar_por_ref,
    soltar,
);

fn crear_waker(tarea: Arc<Tarea>) -> Waker {
    let ptr = Arc::into_raw(tarea) as *const ();
    let raw = RawWaker::new(ptr, &VTABLE);
    unsafe { Waker::from_raw(raw) }
}

// ── Runtime ────────────────────────────────────────────────────────────────

struct MiniRuntime {
    cola: ColaTareas,
}

impl MiniRuntime {
    fn nuevo() -> Self {
        Self {
            cola: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    fn spawn<F>(&self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let tarea = Arc::new(Tarea {
            future: Mutex::new(Box::pin(future)),
            cola: self.cola.clone(),
        });
        self.cola.lock().unwrap().push_back(tarea);
    }

    fn run(&self) {
        loop {
            // Tomar una tarea de la cola
            let tarea = {
                let mut cola = self.cola.lock().unwrap();
                cola.pop_front()
            };

            match tarea {
                Some(t) => {
                    let waker = crear_waker(t.clone());
                    let mut cx = Context::from_waker(&waker);
                    let mut future = t.future.lock().unwrap();

                    match future.as_mut().poll(&mut cx) {
                        Poll::Ready(()) => {
                            // Tarea terminada; se libera al salir de scope
                        }
                        Poll::Pending => {
                            // No hacemos nada: el Waker la re-encolará cuando esté lista
                        }
                    }
                }
                None => {
                    // Cola vacía: verificar si ya terminamos o esperar
                    thread::sleep(Duration::from_millis(1));
                    // Un runtime real usaría park/unpark o una variable de condición
                }
            }
        }
    }
}

// ── Future "Sleep" sin bloquear el hilo ───────────────────────────────────

struct Esperar {
    hora_despertar: Instant,
    waker_guardado: Option<Waker>,
}

impl Esperar {
    fn nueva(duracion: Duration) -> Self {
        Self {
            hora_despertar: Instant::now() + duracion,
            waker_guardado: None,
        }
    }
}

impl Future for Esperar {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if Instant::now() >= self.hora_despertar {
            return Poll::Ready(());
        }

        // Guardar el waker para despertarnos desde el hilo temporizador
        self.waker_guardado = Some(cx.waker().clone());
        let hora = self.hora_despertar;
        let waker = cx.waker().clone();

        // Spawneamos un hilo OS que duerme y luego despierta la tarea.
        // Un runtime real usaría una min-heap de timers + el loop de eventos del OS.
        thread::spawn(move || {
            let ahora = Instant::now();
            if hora > ahora {
                thread::sleep(hora - ahora);
            }
            waker.wake();
        });

        Poll::Pending
    }
}

// ── Función helper async ───────────────────────────────────────────────────

async fn esperar(dur: Duration) {
    Esperar::nueva(dur).await;
}

// ── Main ───────────────────────────────────────────────────────────────────

fn main() {
    let rt = MiniRuntime::nuevo();
    let inicio = Instant::now();

    // Tarea A: imprime 3 veces con 100ms entre cada una
    rt.spawn(async move {
        for i in 0..3 {
            println!("[A] iteración {i} a {:?}", inicio.elapsed());
            esperar(Duration::from_millis(100)).await;
        }
        println!("[A] terminada");
    });

    // Tarea B: imprime 3 veces con 150ms entre cada una
    rt.spawn(async move {
        for i in 0..3 {
            println!("[B] iteración {i} a {:?}", inicio.elapsed());
            esperar(Duration::from_millis(150)).await;
        }
        println!("[B] terminada");
    });

    // Tarea C: termina inmediatamente sin suspenderse
    rt.spawn(async {
        println!("[C] tarea instantánea");
    });

    // El runtime no sabe cuándo terminaron todas las tareas en esta versión simple.
    // En producción usarías JoinHandles. Aquí esperamos un tiempo fijo:
    thread::sleep(Duration::from_secs(1));
    println!("runtime finalizado (tiempo total: {:?})", inicio.elapsed());
}
```

Ejecuta con:

```bash
cargo run
```

Verás que las tareas A, B y C se intercalan según sus timers, todo en un solo hilo OS,
demostrando la concurrencia cooperativa de async Rust.

### Puntos de aprendizaje del ejercicio

- `RawWakerVTable` define cuatro operaciones (clone, wake, wake\_by\_ref, drop) que el
  runtime ejecuta sobre el puntero opaco `*const ()`. El puntero concreto es un
  `Arc<Tarea>`.
- `despertar` re-encola la tarea: cuando la I/O está lista, el waker notifica al
  executor que hay trabajo disponible.
- El hilo extra de `thread::spawn` en `Esperar::poll` simula lo que el reactor del OS
  haría con `epoll`/`kqueue`: esperar en background y despertar la tarea cuando sea el
  momento.
- La `Tarea` guarda `Mutex<Pin<Box<dyn Future>>>`. El `Mutex` permite acceder
  mutuamente (runtime + waker), `Pin<Box<...>>` garantiza que la future no se mueve, y
  `dyn Future` permite almacenar cualquier future concreta.

---

## ✅ Checklist de la Semana 9

- [ ] Explico qué hace `poll`: devuelve `Ready(T)` o `Pending`, nunca bloquea.
- [ ] Explico qué hace un executor: llama a `poll`, recibe `Waker`, duerme, se
  despierta cuando `waker.wake()` es llamado.
- [ ] Describo la máquina de estados que genera el compilador para `async fn`: un enum
  con un estado por cada punto de yield, y los campos que atraviesan cada `.await`.
- [ ] Explico por qué las futures son `!Unpin` y qué problema resuelve `Pin`.
- [ ] Sé usar `Box::pin(fut)` y `tokio::pin!(fut)` sin entrar en pánico.
- [ ] Distingo `spawn` (nueva tarea, `Send + 'static`), `join!` (concurrencia en la
  misma tarea) y `select!` (primera que termine, cancela las demás).
- [ ] Sé cuándo usar `spawn_blocking` (CPU intensivo, I/O bloqueante, librerías C).
- [ ] Entiendo por qué no se puede mantener un `std::sync::MutexGuard` a través de
  un `.await` y cuáles son las dos soluciones.
- [ ] El ejercicio "Toykio" compila y muestra las tres tareas intercaladas.

> **Siguiente paso:** Semana 10 — [Tokio Ecosistema y Axum: construyendo el servidor](section_02.md).
