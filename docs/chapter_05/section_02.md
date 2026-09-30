# Concurrencia avanzada y lock-free: la verdad bajo el capó

La Semana 18 baja al nivel del hardware: caches de CPU, líneas de cache, barreras
de memoria y las garantías reales que ofrecen los atomics. La abstracción `Mutex`
es conveniente, pero entender qué hay debajo permite escribir código que escala con
el número de núcleos en lugar de colapsar bajo ellos.

En esta sección aprenderemos:

- **Modelo de memoria**: el contrato CPU-compilador-programador.
- **Memory Ordering**: por qué `Relaxed`, `Acquire`, `Release` y `SeqCst` existen
  y cuándo usar cada uno. Referencia base: *Rust Atomics and Locks* de Mara Bos.
- **Operaciones atómicas**: `load`, `store`, `fetch_add`, `compare_exchange`,
  `compare_exchange_weak` y el patrón CAS loop.
- **False sharing**: cómo dos atomics en la misma línea de cache hacen que el
  throughput *disminuya* al añadir hilos. Solución: `#[repr(align(64))]`.
- **`parking_lot`**: Mutex y RwLock en user-space, sin envenenamiento, más rápido
  que `std::sync`.
- **`crossbeam`**: canales MPMC, el macro `select!`, epoch-based reclamation.
- **`DashMap`**: mapa concurrent sharded lista para usar.
- **`Rayon`**: paralelismo de datos con work stealing, integración con async.
- **Proyecto**: benchmark científico — cuatro implementaciones de un contador
  distribuido comparadas con `criterion` variando los hilos de 1 a 2 × núcleos.

!!! quote ""

    *"Concurrent programming is hard not because threads are hard; it's hard because
    you're reasoning about multiple temporal orderings of events at once. Atomic types
    give you the minimum vocabulary to do that reasoning precisely."*
    — Mara Bos, *Rust Atomics and Locks*

---

## El modelo de memoria: lo que el hardware realmente hace

El modelo de memoria de un CPU moderno NO es una RAM global donde todas las
operaciones ocurren en el orden en que las escribiste:

```text
LO QUE CREES QUE PASA:

Thread A          RAM           Thread B
────────────────────────────────────────
data = 42    →   data=42
ready = true →   ready=true   → lee ready → lee data
                               ✅ data == 42, siempre

LO QUE REALMENTE PUEDE PASAR (sin barreras):

Thread A          Store Buffer  L1/L2     RAM           Thread B
─────────────────────────────────────────────────────────────────
data  = 42   →  [data=42]
ready = true →  [ready=true]
                [data=42]  → L1 A
                [ready=true] no ha llegado a RAM/L1-B todavía →  Thread B
                                                                  lee ready=true (stale)
                                                                  lee data=0 ❌

El Store Buffer del core A puede reordenar data= y ready= antes
de que sean visibles a otros cores. ready=true puede volverse
visible ANTES que data=42.
```

La solución son las **barreras de memoria** (`fence`) o los **orderings de atomic**.
El programador elige explícitamente cuántas garantías necesita y paga el precio
exacto de hardware que esas garantías cuestan.

---

## Memory Ordering: el vocabulario de la sincronización

```text
┌──────────────────────────────────────────────────────────────────────────┐
│  ORDERING       GARANTÍA                        COSTE HARDWARE           │
├──────────────────────────────────────────────────────────────────────────┤
│  Relaxed        Solo atomicidad (no tearing).   0 (ninguna barrera)      │
│                 Sin orden relativo a otras ops.  Más rápido posible.      │
├──────────────────────────────────────────────────────────────────────────┤
│  Release        Store: todo lo escrito ANTES     Barrera SFENCE (store)  │
│  (solo stores)  es visible a quien haga          ARM: stlr               │
│                 Acquire sobre esta variable.                              │
├──────────────────────────────────────────────────────────────────────────┤
│  Acquire        Load: ve todo lo escrito         Barrera LFENCE (load)   │
│  (solo loads)   antes del Release correspondiente ARM: ldar              │
├──────────────────────────────────────────────────────────────────────────┤
│  AcqRel         Ambos (para RMW: fetch_add,      Ambas barreras          │
│  (RMW ops)      compare_exchange, swap).         ARM: ldaxr/stlxr        │
├──────────────────────────────────────────────────────────────────────────┤
│  SeqCst         Orden total global entre         MFENCE (x86)            │
│                 todas las operaciones SeqCst.    ARM: dmb ish            │
│                 Más conservador y costoso.                               │
└──────────────────────────────────────────────────────────────────────────┘
```

### El patrón Acquire/Release: la sincronización punto a punto

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

fn patron_acquire_release() {
    let datos  = Arc::new(AtomicU64::new(0));
    let listo  = Arc::new(AtomicBool::new(false));

    let d = Arc::clone(&datos);
    let l = Arc::clone(&listo);

    // Productor
    thread::spawn(move || {
        d.store(42, Ordering::Relaxed);   // (1) Escribe el dato
        // Release: garantiza que (1) es visible ANTES de que
        // alguien vea listo=true con Acquire
        l.store(true, Ordering::Release); // (2) Publica la bandera
    });

    // Consumidor
    loop {
        // Acquire: si veo listo=true, entonces TAMBIÉN veo
        // todo lo escrito antes del Release correspondiente
        if listo.load(Ordering::Acquire) {
            // Garantizado por la sincronización Acquire/Release:
            // datos.load aquí verá 42, no 0
            assert_eq!(datos.load(Ordering::Relaxed), 42); // ✅
            break;
        }
        thread::yield_now();
    }
}
```

### Relaxed: contadores donde el orden no importa

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

/// Correcto con Relaxed: solo necesitamos que cada fetch_add sea atómico
/// (no tearing), no que un hilo vea los incrementos de otro en orden exacto.
fn contador_estadisticas() {
    let total = Arc::new(AtomicU64::new(0));
    let mut handles = vec![];

    for _ in 0..8 {
        let t = Arc::clone(&total);
        handles.push(thread::spawn(move || {
            for _ in 0..1_000_000 {
                // Relaxed: sin barrera. Cada fetch_add es atómico,
                // pero el orden global entre hilos no está definido.
                // Para un contador global de peticiones, esto es suficiente.
                t.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles { h.join().unwrap(); }

    // El total SIEMPRE es 8_000_000:
    // fetch_add es atómico → no hay tearing → no perdemos incrementos
    assert_eq!(total.load(Ordering::Relaxed), 8_000_000);
}
```

### SeqCst: cuando necesitas orden total

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// Dekker's algorithm requiere que los stores de A sean visibles a B
/// en el mismo orden que B ve sus propios loads. Acquire/Release no basta:
/// necesitas SeqCst para orden total entre todos los hilos.
fn dekker_simplificado() {
    let a_quiere = Arc::new(AtomicBool::new(false));
    let b_quiere = Arc::new(AtomicBool::new(false));

    let aq = Arc::clone(&a_quiere);
    let bq = Arc::clone(&b_quiere);

    let h = thread::spawn(move || {
        aq.store(true, Ordering::SeqCst);    // "A quiere entrar"
        // Con SeqCst, si B también hizo store(true) antes, lo veremos aquí
        if !bq.load(Ordering::SeqCst) {
            // sección crítica de A
        }
        aq.store(false, Ordering::SeqCst);
    });

    a_quiere.store(false, Ordering::SeqCst); // simplificado
    h.join().unwrap();
}
```

---

## Operaciones atómicas

```rust
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};

fn demo_atomics() {
    let a = AtomicU64::new(0);

    // load / store
    let v = a.load(Ordering::Acquire);
    a.store(v + 1, Ordering::Release);

    // fetch_add: devuelve el valor ANTERIOR, luego suma
    let anterior = a.fetch_add(10, Ordering::Relaxed);
    println!("era: {anterior}, ahora: {}", a.load(Ordering::Relaxed));

    // fetch_or, fetch_and, fetch_xor: operaciones bit a bit atómicas
    let flags = AtomicUsize::new(0b0000);
    flags.fetch_or(0b0001, Ordering::Relaxed);   // set bit 0
    flags.fetch_or(0b0010, Ordering::Relaxed);   // set bit 1
    flags.fetch_and(!0b0001, Ordering::Relaxed); // clear bit 0

    // swap: escribe y devuelve el valor anterior
    let viejo = a.swap(999, Ordering::AcqRel);
    println!("viejo: {viejo}, nuevo: {}", a.load(Ordering::Relaxed));
}
```

### compare_exchange: la operación fundamental de los lock-free

```text
compare_exchange(expected, new, success_ord, failure_ord):

  atomic_operation {
      let current = *self;
      if current == expected {
          *self = new;          ← éxito: escribe y retorna Ok(old)
          return Ok(current);
      } else {
          return Err(current);  ← fallo: retorna el valor actual
      }
  }

Uso en un CAS loop (Compare-And-Swap):
  1. Lee el valor actual.
  2. Calcula el nuevo valor.
  3. Intenta CAS: si nadie cambió el valor entre 1 y 3, éxito.
     Si alguien lo cambió, reintenta desde 1 con el valor nuevo.
```

```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Implementar saturating_add atómico (sin que el valor supere MAX)
fn saturating_atomic_add(a: &AtomicU64, delta: u64, max: u64) {
    let mut current = a.load(Ordering::Relaxed);
    loop {
        let nuevo = current.saturating_add(delta).min(max);
        match a.compare_exchange_weak(
            current,
            nuevo,
            Ordering::AcqRel,   // éxito: barrera completa
            Ordering::Relaxed,  // fallo: solo necesitamos el valor actual
        ) {
            Ok(_)    => break,          // ✅ CAS exitoso
            Err(act) => current = act,  // alguien más actualizó; reintentamos
        }
    }
}

// compare_exchange_weak vs compare_exchange:
// - weak: puede fallar "spuriously" (en ARM sin LL/SC) → más rápido en loops
// - strong: nunca falla si current == expected → útil fuera de loops
```

### Stack lock-free con compare_exchange

```rust
use std::sync::atomic::{AtomicPtr, Ordering};
use std::ptr;

struct Nodo<T> {
    valor: T,
    siguiente: *mut Nodo<T>,
}

pub struct StackLockFree<T> {
    cabeza: AtomicPtr<Nodo<T>>,
}

unsafe impl<T: Send> Send for StackLockFree<T> {}
unsafe impl<T: Send> Sync for StackLockFree<T> {}

impl<T> StackLockFree<T> {
    pub fn new() -> Self {
        StackLockFree { cabeza: AtomicPtr::new(ptr::null_mut()) }
    }

    pub fn push(&self, valor: T) {
        let nodo = Box::into_raw(Box::new(Nodo {
            valor,
            siguiente: ptr::null_mut(),
        }));

        loop {
            let cabeza_actual = self.cabeza.load(Ordering::Relaxed);
            // SAFETY: nodo es exclusivo hasta que CAS tenga éxito
            unsafe { (*nodo).siguiente = cabeza_actual; }

            match self.cabeza.compare_exchange_weak(
                cabeza_actual,
                nodo,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_)  => break,
                Err(_) => {} // alguien hizo push antes; reintentamos
            }
        }
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            let cabeza_actual = self.cabeza.load(Ordering::Acquire);
            if cabeza_actual.is_null() { return None; }

            // SAFETY: leímos cabeza con Acquire; el nodo existe
            let siguiente = unsafe { (*cabeza_actual).siguiente };

            match self.cabeza.compare_exchange_weak(
                cabeza_actual,
                siguiente,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // SAFETY: somos los únicos propietarios de este nodo ahora
                    let nodo = unsafe { Box::from_raw(cabeza_actual) };
                    return Some(nodo.valor);
                }
                Err(_) => {} // alguien hizo pop antes; reintentamos
            }
        }
    }
}

impl<T> Drop for StackLockFree<T> {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}
```

!!! warning "Cuidado"

    Este stack tiene el **ABA problem**: si un hilo lee la cabeza, se suspende,
    y mientras tanto otro hilo hace pop y push del mismo nodo, el CAS tiene éxito
    aunque el estado haya cambiado. En producción usa `crossbeam::epoch` o
    `crossbeam::queue::SegQueue`.

---

## False Sharing: el asesino silencioso del rendimiento

```text
ANATOMÍA DE UNA LÍNEA DE CACHE (x86-64: 64 bytes)

SIN PADDING:

┌─────────────────────────────── 64 bytes ───────────────────────────────┐
│ contador[0]  │ contador[1]  │ contador[2]  │ ...  │ contador[7]        │
│   8 bytes    │   8 bytes    │   8 bytes    │      │   8 bytes          │
└─────────────────────────────────────────────────────────────────────────┘
         ↑                ↑
    Core 0 escribe   Core 1 escribe
    → Core 0 invalida TODA la línea
    → Core 1 sufre cache miss
    → Core 1 invalida TODA la línea
    → Core 0 sufre cache miss
    → "Ping-pong" indefinido → throughput BAJA al añadir cores

CON PADDING (#[repr(align(64))]):

┌────────────── 64 bytes ──────────────┐ ┌────────────── 64 bytes ──────────┐
│ contador[0]  │ (padding)             │ │ contador[1]  │ (padding)         │
│   8 bytes    │ 56 bytes de relleno   │ │   8 bytes    │ 56 bytes          │
└──────────────────────────────────────┘ └──────────────────────────────────┘
         ↑                                        ↑
    Core 0 su propia línea                   Core 1 su propia línea
    → Sin invalidaciones cruzadas
    → Throughput ESCALA con los cores ✅
```

```rust
use std::sync::atomic::{AtomicU64, Ordering};

// SIN padding: los 8 AtomicU64 comparten 1 o 2 líneas de cache
struct ContadorNoPadded {
    shards: [AtomicU64; 8],
}

// CON padding: cada AtomicU64 ocupa una línea de cache completa
#[repr(align(64))]
struct ShardPadded(AtomicU64);

struct ContadorPadded {
    shards: Box<[ShardPadded]>,  // Box para heap allocation (no stack overflow)
}

impl ContadorPadded {
    pub fn nuevo(n: usize) -> Self {
        let shards = (0..n)
            .map(|_| ShardPadded(AtomicU64::new(0)))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        ContadorPadded { shards }
    }

    pub fn incrementar(&self, shard: usize) {
        // Relaxed: para un contador estadístico, solo importa atomicidad
        self.shards[shard % self.shards.len()]
            .0
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn total(&self) -> u64 {
        self.shards.iter().map(|s| s.0.load(Ordering::Relaxed)).sum()
    }
}

fn verificar_tamano_padding() {
    use std::mem::size_of;
    // Cada shard ocupa exactamente 64 bytes (una línea de cache)
    assert_eq!(size_of::<ShardPadded>(), 64);
    // Sin padding, 8 AtomicU64 ocuparían 64 bytes total → false sharing
    assert_eq!(size_of::<AtomicU64>(), 8);
}
```

---

## `parking_lot`: Mutex sin overhead de envenenamiento

`std::sync::Mutex` envenena (poison) el lock cuando un hilo paniquea mientras lo
tiene tomado. Verificar esto en cada `lock().unwrap()` añade overhead.
`parking_lot::Mutex` elimina el envenenamiento y usa futex directamente:

```text
COMPARACIÓN std::sync vs parking_lot:

┌─────────────────────┬────────────────────────┬───────────────────────────┐
│ Característica      │ std::sync::Mutex       │ parking_lot::Mutex        │
├─────────────────────┼────────────────────────┼───────────────────────────┤
│ API lock()          │ Returns LockResult     │ Returns MutexGuard directo│
│                     │ (unwrap() necesario)   │ (no Result)               │
│ Envenenamiento      │ Sí (poison on panic)   │ No                        │
│ Tamaño              │ ~40 bytes              │ ~8 bytes                  │
│ Rendimiento         │ Base                   │ 2x-5x más rápido          │
│ Send/Sync en guard  │ Sí                     │ Sí                        │
│ try_lock()          │ TryLockResult          │ Option<MutexGuard>        │
│ Timeout             │ No                     │ try_lock_for(Duration)    │
└─────────────────────┴────────────────────────┴───────────────────────────┘

REGLA: En código sync (no .await), usa siempre parking_lot.
       En código async, NUNCA sostengas un MutexGuard a través de .await
       → usa tokio::sync::Mutex para eso.
```

```rust
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::Arc;

// Mutex básico — API más limpia que std
let mapa: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));

{
    let mut guard = mapa.lock(); // no .unwrap() necesario
    guard.insert("visitas".to_string(), 42);
} // guard se dropea, lock se libera

// RwLock: múltiples lectores O un solo escritor
let cache: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(vec![]));

// Múltiples readers simultáneos:
let r1 = cache.read();
let r2 = cache.read();
println!("{} items", r1.len() + r2.len());
drop(r1);
drop(r2);

// Un solo writer:
cache.write().push("nuevo".to_string());

// try_lock: no bloquear si está ocupado
if let Some(mut guard) = mapa.try_lock() {
    *guard.entry("intentos".to_string()).or_default() += 1;
} else {
    // El lock estaba tomado, seguimos sin bloquear
}
```

---

## `crossbeam`: canales MPMC y epoch reclamation

### Canales: `crossbeam::channel`

```rust
use crossbeam::channel::{self, select, Receiver, Sender};
use std::thread;
use std::time::Duration;

// bounded: backpressure natural cuando el buffer está lleno
// unbounded: nunca bloquea el sender (puede consumir memoria ilimitada)
let (tx, rx) = channel::bounded::<String>(1024);
let (tx2, rx2) = channel::bounded::<u64>(256);

// Multi-Producer: los senders son Clone
let tx_clone = tx.clone();
thread::spawn(move || {
    for i in 0..100 {
        tx_clone.send(format!("mensaje {i}")).unwrap();
    }
});

thread::spawn(move || {
    for i in 100..200 {
        tx.send(format!("mensaje {i}")).unwrap();
    }
});

// select!: espera en múltiples canales a la vez (como io::select!)
thread::spawn(move || {
    let timeout = channel::after(Duration::from_secs(5));
    let mut total = 0u64;

    loop {
        select! {
            recv(rx) -> msg => {
                match msg {
                    Ok(s) => {
                        total += s.len() as u64;
                        tx2.send(total).unwrap();
                    }
                    Err(_) => break,  // canal cerrado
                }
            }
            recv(timeout) -> _ => {
                println!("timeout: procesamos {total} bytes");
                break;
            }
        }
    }
});
```

### Ventajas sobre `std::sync::mpsc`

```text
┌─────────────────────┬─────────────────────┬─────────────────────────────┐
│ Característica      │ std::sync::mpsc     │ crossbeam::channel          │
├─────────────────────┼─────────────────────┼─────────────────────────────┤
│ Tipo                │ MPSC (1 receiver)   │ MPMC (n receivers)          │
│ Receiver Clone      │ No                  │ Sí                          │
│ select!             │ No (inestable)      │ Sí (macro estable)          │
│ bounded             │ sync_channel(N)     │ bounded(N)                  │
│ disconnect detection│ Err en send/recv    │ Err en send/recv + is_empty │
│ Rendimiento         │ Base                │ 2x-4x más rápido            │
└─────────────────────┴─────────────────────┴─────────────────────────────┘
```

### `crossbeam::deque`: work-stealing queue

```rust
use crossbeam::deque::{Injector, Stealer, Worker};
use std::sync::Arc;

/// Work-stealing para distribución dinámica de carga
fn trabajo_stealing_basico() {
    let injector: Arc<Injector<u64>> = Arc::new(Injector::new());

    // Crear workers (uno por hilo de trabajo)
    let workers: Vec<Worker<u64>> = (0..4).map(|_| Worker::new_fifo()).collect();
    let stealers: Vec<Stealer<u64>> = workers.iter().map(|w| w.stealer()).collect();

    // Productor: inyecta trabajo
    for i in 0..100 {
        injector.push(i);
    }

    // Consumidor: cada worker roba de otros si su cola está vacía
    let mut suma = 0u64;
    let worker = &workers[0];
    loop {
        // Intenta de la cola local primero
        let tarea = worker.pop().or_else(|| {
            // Cola local vacía: roba del injector o de otros workers
            std::iter::repeat_with(|| {
                injector.steal_batch_and_pop(worker)
                    // Steal implementa FromIterator: collect() devuelve el primer robo
                    // exitoso, o Retry si alguno pidió reintentar
                    .or_else(|| stealers.iter().map(|s| s.steal()).collect())
            })
            .find(|s| !s.is_retry())
            .and_then(|s| s.success())
        });

        match tarea {
            Some(t) => suma += t,
            None    => break,
        }
    }

    println!("Suma: {suma}");
}
```

---

## `DashMap`: mapa concurrente sharded

`DashMap` divide el mapa en N shards, cada uno protegido por su propio `RwLock`.
Lecturas de claves en diferentes shards son completamente paralelas:

```rust
use dashmap::DashMap;
use std::sync::Arc;

let mapa: Arc<DashMap<String, u64>> = Arc::new(DashMap::with_capacity_and_shard_amount(
    10_000, // capacidad inicial
    64,     // número de shards (por defecto: num_cpus * 4)
));

// Escritura: toma RwLock del shard correspondiente
mapa.insert("clave".to_string(), 42);

// Lectura: Ref guard (no bloquea mientras esté vivo)
if let Some(val) = mapa.get("clave") {
    println!("valor: {}", *val);
} // Ref se dropea aquí → shard RwLock se libera

// entry API: atómica para get-or-insert
mapa.entry("nuevo".to_string())
    .and_modify(|v| *v += 1)
    .or_insert(0);

// Modificación in-place sin quitar el valor
*mapa.get_mut("clave").unwrap() += 1;

// Iteración (toma locks de un shard a la vez)
for entry in mapa.iter() {
    println!("{} → {}", entry.key(), entry.value());
}

// ⚠️ NUNCA hagas .get() y luego .get_mut() en el mismo scope:
// puede deadlock si el shard es el mismo
// ⚠️ NUNCA sostengas un Ref/RefMut a través de .await en async code
```

---

## Rayon: paralelismo de datos con work stealing

```rust
use rayon::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// par_iter() es el API principal: transforma iteradores en paralelos
fn procesamiento_paralelo() {
    let datos: Vec<u64> = (0..1_000_000).collect();

    // Suma paralela: Rayon divide en chunks y roba trabajo entre hilos
    let suma: u64 = datos.par_iter().sum();
    assert_eq!(suma, (0..1_000_000u64).sum());

    // Map + filter + collect en paralelo
    let pares: Vec<u64> = datos
        .par_iter()
        .filter(|&&x| x % 2 == 0)
        .map(|&x| x * x)
        .collect();

    // par_chunks: procesar en lotes
    let resultados: Vec<u64> = datos
        .par_chunks(1000)
        .map(|chunk| chunk.iter().sum::<u64>())
        .collect();

    // reduce: combinar resultados
    let max = datos
        .par_iter()
        .copied()
        .reduce(|| 0, u64::max);

    println!("max: {max}");
}
```

### ThreadPoolBuilder: control del runtime de Rayon

```rust
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

fn pool_configurado() {
    let pool = ThreadPoolBuilder::new()
        .num_threads(4)          // no usar todos los núcleos (dejar para Tokio)
        .stack_size(4 * 1024 * 1024) // 4 MB por hilo
        .thread_name(|i| format!("rayon-worker-{i}"))
        .build()
        .unwrap();

    pool.install(|| {
        // Todo código Rayon dentro de install() usa este pool
        let resultado: Vec<u64> = (0u64..1_000_000)
            .into_par_iter()
            .map(|x| x * x)
            .collect();
        println!("{} elementos", resultado.len());
    });
}
```

### Integración con async: `spawn_blocking`

```rust
use tokio::task;
use rayon::prelude::*;

/// NUNCA bloquees el runtime de Tokio con Rayon directamente.
/// Usa spawn_blocking para mover el trabajo a un hilo OS separado.
async fn calcular_async(datos: Vec<u64>) -> u64 {
    // spawn_blocking: hilo del threadpool de bloqueo de Tokio
    task::spawn_blocking(move || {
        // Aquí sí podemos usar Rayon sin problemas
        datos.par_iter().sum()
    })
    .await
    .expect("tarea de bloqueo falló")
}

// Patrón para CPU-bound + I/O bound:
async fn pipeline() {
    // 1. I/O async: leer datos
    let datos: Vec<u64> = vec![1, 2, 3, 4, 5]; // simula I/O

    // 2. CPU-bound en spawn_blocking con Rayon
    let resultado = task::spawn_blocking(move || {
        datos.into_par_iter().map(|x| x * x).sum::<u64>()
    }).await.unwrap();

    // 3. I/O async: guardar resultado
    println!("Resultado: {resultado}");
}
```

---

## Proyecto: benchmark científico del contador distribuido

Comparamos cuatro implementaciones de un contador concurrente bajo carga creciente
de escritura, variando el número de hilos de 1 a (2 × núcleos).

El código completo está en
[`contador_distribuido`](https://github.com/pepemxl/Rust-Notes/tree/master/src/chapter_05/contador_distribuido).

### Estructura

```text
contador_distribuido/
├── Cargo.toml
├── src/
│   └── lib.rs              ← trait Contador + implementaciones + generador de carga
├── benches/
│   └── counter_bench.rs
└── tests/
    └── correctness.rs
```

### `Cargo.toml`

```toml
--8<-- "src/chapter_05/contador_distribuido/Cargo.toml"
```

### `src/lib.rs` — trait y cuatro implementaciones

Además de las cuatro variantes hay una quinta, `AtomicSinPadding`, idéntica a
`AtomicPadded` salvo por el `#[repr(align(64))]`: es el contraejemplo para medir el
false sharing. `ejecutar_carga` usa `std::thread::scope`, así que los hilos pueden
tomar prestado `&C` sin envolver el contador en `Arc`.

```rust
--8<-- "src/chapter_05/contador_distribuido/src/lib.rs"
```

### `benches/counter_bench.rs`

Cada iteración usa un contador nuevo, pero crearlo (en `ActorCanal`, lanzar N hilos)
y destruirlo no debe contar en la medición. `iter_batched_ref` resuelve eso: prepara
la entrada fuera del cronómetro y la suelta también fuera.

```rust
--8<-- "src/chapter_05/contador_distribuido/benches/counter_bench.rs"
```

### Tests de corrección

Un benchmark de un contador que pierde incrementos no mide nada. Estos tests
comprueban que cada implementación llega al total exacto, también con un número de
hilos que no divide las operaciones:

```rust
--8<-- "src/chapter_05/contador_distribuido/tests/correctness.rs"
```

---

## Análisis de resultados

Resultados de `cargo bench -p contador_distribuido` en un AMD Ryzen 7 7735HS
(8 núcleos, 16 hilos con SMT) bajo Linux (WSL2). Throughput en millones de
incrementos por segundo (valor central del intervalo de criterion):

```text
Implementación  │ 1 hilo │ 2 hilos │ 4 hilos │ 8 hilos │ 16 hilos │ 32 hilos
────────────────┼────────┼─────────┼─────────┼─────────┼──────────┼─────────
AtomicPadded    │   561  │    907  │   1392  │   1705  │    1159  │     667
MutexSharded    │   228  │     44  │     56  │     74  │     141  │     173
DashMap         │    69  │    122  │    189  │    217  │     232  │     250
ActorCanal      │    55  │    100  │    128  │    192  │     190  │     177

FALSE SHARING (mismo código, solo cambia #[repr(align(64))]):
                │ 1 hilo │ 2 hilos │ 4 hilos │ 8 hilos │ 16 hilos │ 32 hilos
con-padding     │   590  │    885  │   1402  │   1722  │    1207  │     641
sin-padding     │   569  │    132  │     95  │    107  │     170  │     247
```

Lo que dicen los números (y un par de cosas que no esperábamos):

- **`AtomicPadded` escala hasta los 8 núcleos físicos** (3x con 8 hilos) y después
  **baja**: con 16 hilos, cada par comparte un núcleo por SMT; con 32, hay más hilos
  que núcleos lógicos y el sistema operativo los alterna. "Más hilos" no es gratis.
- **El false sharing cuesta un orden de magnitud**: con 4 hilos, 1402 contra 95
  millones por segundo (**15x**) solo por el `#[repr(align(64))]`. Con un solo hilo
  no hay nadie con quien pelear la línea de cache, y ambas versiones empatan.
- **`MutexSharded` se desploma de 1 a 2 hilos** (228 → 44), exactamente como
  `sin-padding`. No es el lock: es el mismo false sharing.
  `size_of::<parking_lot::Mutex<u64>>()` es 16, así que caben 4 mutex por línea de
  cache y los hilos, aunque usen shards distintos, se invalidan la línea entre sí.
  Envolverlos en un struct con `#[repr(align(64))]` lo arreglaría. Es la lección más
  valiosa del benchmark: el patrón "un shard por hilo" no sirve de nada si los shards
  comparten línea.
- **`DashMap` escala sin sorpresas** porque ya se protege: internamente envuelve cada
  shard en `crossbeam_utils::CachePadded`. Paga hashing y un `RwLock` por incremento,
  así que es hasta 8x más lento que el atómico, pero sin colapsos.
- **`ActorCanal`** es el más lento con pocos hilos (cada incremento es un mensaje
  por canal) y se estanca a partir de 8: además de los hilos que generan carga, cada
  shard tiene su propio hilo actor, así que con N hilos hay 2N compitiendo por los
  núcleos. Su ventaja no es el throughput sino el modelo: estado privado, sin locks.

!!! warning "Tus números serán otros"

    Dependen de la CPU (tamaño de línea de cache, número de núcleos, SMT), del sistema
    operativo y de qué más esté corriendo. Lo que debe reproducirse son las
    **tendencias**: el atómico con padding gana, sin padding colapsa desde 2 hilos, y
    pasar del número de núcleos físicos no ayuda. Cierra otros programas antes de
    medir y no compiles nada mientras corre `cargo bench`.

---

## Verificación con `loom`: model checking para atomics

`loom` es una herramienta de Tokio para verificar código concurrente probando todas
las posibles interleaving de hilos. Solo para tests, no para producción:

```rust
// Añadir a Cargo.toml [dev-dependencies]: loom = "0.7"

#[cfg(loom)]
mod tests_loom {
    use loom::sync::atomic::{AtomicU64, Ordering};
    use loom::thread;
    use std::sync::Arc;

    #[test]
    fn dos_hilos_sin_races() {
        loom::model(|| {
            let counter = Arc::new(AtomicU64::new(0));
            let c1 = Arc::clone(&counter);
            let c2 = Arc::clone(&counter);

            let h1 = thread::spawn(move || {
                c1.fetch_add(1, Ordering::Relaxed);
            });
            let h2 = thread::spawn(move || {
                c2.fetch_add(1, Ordering::Relaxed);
            });

            h1.join().unwrap();
            h2.join().unwrap();

            // loom verifica TODAS las interleaving posibles
            assert_eq!(counter.load(Ordering::Relaxed), 2);
        });
    }
}

// Ejecutar: RUSTFLAGS="--cfg loom" cargo test --test loom_test
```

---

## Errores comunes y cómo evitarlos

```text
ERROR 1: MutexGuard a través de .await en código async

async fn handler(state: Arc<Mutex<Vec<String>>>) {
    let mut guard = state.lock();           // parking_lot: guard no es Send
    do_async_thing().await;                 // ❌ guard cruza un punto de yield
    guard.push("dato".to_string());
}

FIX: Usa tokio::sync::Mutex para guards que cruzan .await,
     o reduce el scope del guard:

async fn handler(state: Arc<parking_lot::Mutex<Vec<String>>>) {
    {
        let mut guard = state.lock();
        guard.push("dato".to_string());
    }  // guard se dropea ANTES del .await
    do_async_thing().await;  // ✅
}

───────────────────────────────────────────────────────────────────────

ERROR 2: Relaxed en patrón flag/dato (data race lógico)

flag.store(true, Ordering::Relaxed);  // ❌
data.store(42, Ordering::Relaxed);    // puede reordenarse con el flag

// El consumidor puede ver flag=true pero data=0

FIX:
data.store(42, Ordering::Relaxed);    // orden entre operaciones del
flag.store(true, Ordering::Release);  // ← barrera: todo lo anterior es visible
// Consumidor:
while !flag.load(Ordering::Acquire) {} // ← barrera: ve todo lo del Release
let v = data.load(Ordering::Relaxed);  // v == 42 garantizado

───────────────────────────────────────────────────────────────────────

ERROR 3: DashMap deadlock por doble acceso al mismo shard

let a = mapa.get("clave-a");         // toma read lock del shard 3
let b = mapa.get_mut("clave-a");     // intenta write lock del shard 3 → DEADLOCK
                                     // (a y b en el mismo shard)

FIX: Nunca tengas dos guards vivos del mismo shard:
let val = mapa.get("clave-a").map(|r| *r);  // copia; el guard muere en esta línea
let b = mapa.get_mut("clave-a");            // ✅ ya no hay otro guard vivo

───────────────────────────────────────────────────────────────────────

ERROR 4: compare_exchange con ordering incorrecto en success

// MAL: éxito con Relaxed no garantiza que las ops previas sean visibles
a.compare_exchange(old, new, Ordering::Relaxed, Ordering::Relaxed)?;

// BIEN: éxito con Release o AcqRel
a.compare_exchange(old, new, Ordering::AcqRel, Ordering::Acquire)?;
```

---

## ✅ Checklist de la Semana 18

- [ ] Entiendo la diferencia entre `Relaxed`, `Acquire`/`Release` y `SeqCst`:
  `Relaxed` = solo atomicidad; `Acquire`/`Release` = sincronización punto a punto;
  `SeqCst` = orden total global (más lento).
- [ ] Uso `Acquire` en loads y `Release` en stores cuando el patrón es
  "escribir datos, publicar bandera; consumir bandera, leer datos".
- [ ] Uso `Relaxed` solo para contadores estadísticos donde el orden exacto entre
  hilos no afecta la corrección.
- [ ] `compare_exchange_weak` dentro de loops CAS; `compare_exchange` (strong) fuera.
- [ ] `#[repr(align(64))]` en structs con un atómico por hilo. Demuestro el
  impacto de false sharing con el benchmark (diferencia ≥ 2x con 4+ hilos).
- [ ] `parking_lot::Mutex` en código sync; `tokio::sync::Mutex` cuando el guard
  debe cruzar un `.await`.
- [ ] Los canales de `crossbeam` son MPMC y admiten `select!`. Son mi primera
  opción para colas de trabajo en código multi-thread síncrono.
- [ ] `DashMap` nunca tiene dos guards del mismo shard vivos simultáneamente
  (riesgo de deadlock).
- [ ] `tokio::task::spawn_blocking` es el puente correcto para combinar Tokio (async
  I/O) con Rayon (CPU-bound parallelism). Nunca bloqueo el runtime de Tokio.
- [ ] `cargo bench` genera el informe HTML en `target/criterion/`. El benchmark
  muestra que `AtomicPadded` escala mejor que `MutexSharded` con ≥ 4 hilos.
- [ ] `cargo test --test correctness` pasa los 7 tests (5 de corrección, 1 de tamaño
  de shard y 1 del orden FIFO de los actores).

!!! abstract "Siguiente sección"

    [Semana 19 — Profiling y optimización](section_03.md)
