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
  distribuido comparadas con `criterion` variando threads de 1 a 64.

> *"Concurrent programming is hard not because threads are hard; it's hard because
> you're reasoning about multiple temporal orderings of events at once. Atomic types
> give you the minimum vocabulary to do that reasoning precisely."*
> — Mara Bos, *Rust Atomics and Locks*

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

> ⚠️ Este stack tiene el **ABA problem**: si un hilo lee la cabeza, se suspende,
> y mientras tanto otro hilo hace pop y push del mismo nodo, el CAS tiene éxito
> aunque el estado haya cambiado. En producción usa `crossbeam::epoch` o
> `crossbeam::queue::SegQueue`.

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
                    .or_else(|| stealers.iter().map(|s| s.steal()).find(|s| !s.is_retry()))
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

### Estructura

```
sharded_counter/
├── Cargo.toml
├── src/
│   └── lib.rs        ← trait Counter + 4 implementaciones
└── benches/
    └── counter_bench.rs
```

### `Cargo.toml`

```toml
[package]
name    = "sharded-counter"
version = "0.1.0"
edition = "2021"

[dependencies]
parking_lot  = "0.12"
crossbeam    = "0.8"
dashmap      = "6"
cache-padded = "1"

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
num_cpus  = "1"

[[bench]]
name    = "counter_bench"
harness = false
```

### `src/lib.rs` — trait y cuatro implementaciones

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// ── Trait común ────────────────────────────────────────────────────────────

pub trait Contador: Send + Sync + 'static {
    /// Incrementar el shard sugerido. La implementación puede ignorar el hint.
    fn incrementar(&self, shard_hint: usize);
    /// Leer el total global. Puede ser aproximado bajo carga (Relaxed).
    fn total(&self) -> u64;
}

// ── Implementación 1: Atómico con padding ─────────────────────────────────

/// Cada shard ocupa su propia línea de cache (64 bytes).
/// Ordering::Relaxed: sin barreras, solo atomicidad.
/// El más rápido bajo alta contención de escritura.
#[repr(align(64))]
struct ShardPadded(AtomicU64);

pub struct AtomicPadded {
    shards: Box<[ShardPadded]>,
}

impl AtomicPadded {
    pub fn nuevo(n_shards: usize) -> Self {
        let shards = (0..n_shards.max(1))
            .map(|_| ShardPadded(AtomicU64::new(0)))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        AtomicPadded { shards }
    }
}

impl Contador for AtomicPadded {
    fn incrementar(&self, shard_hint: usize) {
        let idx = shard_hint % self.shards.len();
        self.shards[idx].0.fetch_add(1, Ordering::Relaxed);
    }

    fn total(&self) -> u64 {
        self.shards.iter().map(|s| s.0.load(Ordering::Relaxed)).sum()
    }
}

// ── Implementación 2: Mutex sharded (parking_lot) ─────────────────────────

/// parking_lot::Mutex es ~2x más rápido que std::sync::Mutex.
/// Aún así, con alta contención el lock serializa las escrituras.
pub struct MutexSharded {
    shards: Box<[parking_lot::Mutex<u64>]>,
}

impl MutexSharded {
    pub fn nuevo(n_shards: usize) -> Self {
        let shards = (0..n_shards.max(1))
            .map(|_| parking_lot::Mutex::new(0u64))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        MutexSharded { shards }
    }
}

impl Contador for MutexSharded {
    fn incrementar(&self, shard_hint: usize) {
        let idx = shard_hint % self.shards.len();
        *self.shards[idx].lock() += 1;
    }

    fn total(&self) -> u64 {
        self.shards.iter().map(|s| *s.lock()).sum()
    }
}

// ── Implementación 3: DashMap ─────────────────────────────────────────────

/// DashMap es un HashMap sharded internamente con RwLock por shard.
/// Útil cuando también necesitas lookup por clave (no solo contador).
pub struct DashmapContador {
    mapa: dashmap::DashMap<usize, u64>,
    n_shards: usize,
}

impl DashmapContador {
    pub fn nuevo(n_shards: usize) -> Self {
        let n = n_shards.max(1);
        let mapa = dashmap::DashMap::with_capacity_and_shard_amount(n, n * 4);
        for i in 0..n { mapa.insert(i, 0u64); }
        DashmapContador { mapa, n_shards: n }
    }
}

impl Contador for DashmapContador {
    fn incrementar(&self, shard_hint: usize) {
        let key = shard_hint % self.n_shards;
        *self.mapa.get_mut(&key).unwrap() += 1;
    }

    fn total(&self) -> u64 {
        self.mapa.iter().map(|e| *e.value()).sum()
    }
}

// ── Implementación 4: Actor con crossbeam::channel ───────────────────────

/// Cada shard es un hilo independiente con su propio canal.
/// Cero sharing: estado completamente privado de cada actor.
/// Ideal cuando las escrituras van a 1 clave (no se disputan shards).
use crossbeam::channel::{self, Sender};

enum MensajeActor {
    Inc,
    Get(Sender<u64>),
}

pub struct ActorCanal {
    shards: Vec<Sender<MensajeActor>>,
}

impl ActorCanal {
    pub fn nuevo(n_shards: usize) -> Self {
        let n = n_shards.max(1);
        let mut shards = Vec::with_capacity(n);

        for _ in 0..n {
            let (tx, rx) = channel::bounded::<MensajeActor>(4096);
            std::thread::spawn(move || {
                let mut count = 0u64;
                while let Ok(msg) = rx.recv() {
                    match msg {
                        MensajeActor::Inc       => count += 1,
                        MensajeActor::Get(resp) => { let _ = resp.send(count); }
                    }
                }
            });
            shards.push(tx);
        }

        ActorCanal { shards }
    }
}

impl Contador for ActorCanal {
    fn incrementar(&self, shard_hint: usize) {
        let idx = shard_hint % self.shards.len();
        // send puede fallar si el canal está lleno; en benchmark ignoramos
        let _ = self.shards[idx].send(MensajeActor::Inc);
    }

    fn total(&self) -> u64 {
        let mut suma = 0u64;
        for tx in &self.shards {
            let (resp_tx, resp_rx) = channel::bounded(1);
            let _ = tx.send(MensajeActor::Get(resp_tx));
            suma += resp_rx.recv().unwrap_or(0);
        }
        suma
    }
}

// ── Utilidad de prueba ─────────────────────────────────────────────────────

/// Ejecuta N_HILOS incrementando TOTAL_OPS/N_HILOS veces cada uno.
/// Devuelve el total registrado (debe ser == TOTAL_OPS).
pub fn ejecutar_carga<C: Contador>(
    contador: Arc<C>,
    n_hilos:   usize,
    total_ops: u64,
) -> u64 {
    let ops_por_hilo = total_ops / n_hilos as u64;

    std::thread::scope(|s| {
        for t in 0..n_hilos {
            let c = Arc::clone(&contador);
            s.spawn(move || {
                for _ in 0..ops_por_hilo {
                    c.incrementar(t);
                }
            });
        }
    });

    contador.total()
}
```

### `benches/counter_bench.rs`

```rust
use criterion::{
    black_box, criterion_group, criterion_main,
    BenchmarkId, Criterion, Throughput,
};
use sharded_counter::*;
use std::sync::Arc;

const TOTAL_OPS: u64 = 1_000_000;

fn bench_implementacion<C: Contador>(
    grupo: &mut criterion::BenchmarkGroup<criterion::measurement::WallTime>,
    nombre: &str,
    factory: impl Fn(usize) -> C,
    n_hilos: usize,
) {
    let n_shards = n_hilos; // un shard por hilo para mínima contención
    let contador = Arc::new(factory(n_shards));

    grupo.bench_with_input(
        BenchmarkId::new(nombre, n_hilos),
        &n_hilos,
        |b, &t| {
            b.iter(|| {
                // Reiniciamos el contador antes de cada iteración
                // (no podemos reiniciar directamente; creamos uno nuevo)
                let c = Arc::new(factory(t.max(1)));
                black_box(ejecutar_carga(c, t.max(1), TOTAL_OPS))
            });
        },
    );
    drop(contador);
}

fn bench_todos(c: &mut Criterion) {
    let n_cpus = num_cpus::get();
    let thread_counts = vec![1, 2, 4, 8, n_cpus, n_cpus * 2];

    let mut grupo = c.benchmark_group("contador-distribuido");
    grupo.throughput(Throughput::Elements(TOTAL_OPS));
    grupo.sample_size(20);

    for &n_hilos in &thread_counts {
        bench_implementacion(
            &mut grupo,
            "AtomicPadded",
            |n| AtomicPadded::nuevo(n),
            n_hilos,
        );
        bench_implementacion(
            &mut grupo,
            "MutexSharded",
            |n| MutexSharded::nuevo(n),
            n_hilos,
        );
        bench_implementacion(
            &mut grupo,
            "DashMap",
            |n| DashmapContador::nuevo(n),
            n_hilos,
        );
        bench_implementacion(
            &mut grupo,
            "ActorCanal",
            |n| ActorCanal::nuevo(n),
            n_hilos,
        );
    }

    grupo.finish();
}

fn bench_false_sharing(c: &mut Criterion) {
    // Demostrar el impacto de false sharing comparando
    // AtomicPadded (1 shard = 1 línea de cache) vs
    // un "mal" contador sin padding
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Default)]
    struct SinPadding {
        shards: Vec<AtomicU64>,  // todos en memoria contigua → false sharing
    }

    let n_cpus = num_cpus::get();
    let mut grupo = c.benchmark_group("false-sharing");
    grupo.throughput(Throughput::Elements(TOTAL_OPS));
    grupo.sample_size(20);

    for &n_hilos in &[1usize, 2, 4, n_cpus] {
        // CON padding
        grupo.bench_with_input(
            BenchmarkId::new("con-padding", n_hilos),
            &n_hilos,
            |b, &t| {
                let c = Arc::new(AtomicPadded::nuevo(t));
                b.iter(|| black_box(ejecutar_carga(Arc::clone(&c), t, TOTAL_OPS)));
            },
        );

        // SIN padding (contención de cache line)
        grupo.bench_with_input(
            BenchmarkId::new("sin-padding", n_hilos),
            &n_hilos,
            |b, &t| {
                let shards: Arc<Vec<AtomicU64>> = Arc::new(
                    (0..t).map(|_| AtomicU64::new(0)).collect()
                );
                b.iter(|| {
                    std::thread::scope(|s| {
                        for i in 0..t {
                            let sh = Arc::clone(&shards);
                            let ops = TOTAL_OPS / t as u64;
                            s.spawn(move || {
                                for _ in 0..ops {
                                    sh[i % sh.len()].fetch_add(1, Ordering::Relaxed);
                                }
                            });
                        }
                    });
                    black_box(shards.iter().map(|s| s.load(Ordering::Relaxed)).sum::<u64>())
                });
            },
        );
    }

    grupo.finish();
}

criterion_group!(benches, bench_todos, bench_false_sharing);
criterion_main!(benches);
```

### Tests de corrección

```rust
// tests/correctness.rs
use sharded_counter::*;
use std::sync::Arc;

const N_OPS: u64 = 100_000;

macro_rules! test_correctitud {
    ($nombre:ident, $tipo:expr) => {
        #[test]
        fn $nombre() {
            for n_hilos in [1usize, 2, 4, 8] {
                let contador = Arc::new($tipo(n_hilos));
                let resultado = ejecutar_carga(Arc::clone(&contador), n_hilos, N_OPS);
                assert_eq!(
                    resultado, N_OPS,
                    "fallo con {n_hilos} hilos: esperado {N_OPS}, obtenido {resultado}"
                );
            }
        }
    };
}

test_correctitud!(atomic_padded_correcto, AtomicPadded::nuevo);
test_correctitud!(mutex_sharded_correcto, MutexSharded::nuevo);
test_correctitud!(dashmap_correcto, DashmapContador::nuevo);
test_correctitud!(actor_canal_correcto, ActorCanal::nuevo);

#[test]
fn atomic_padded_tamano_correcto() {
    use std::mem::size_of;
    // Verificar que el padding funciona: cada shard = 1 línea de cache
    #[repr(align(64))]
    struct ShardTest(std::sync::atomic::AtomicU64);
    assert_eq!(size_of::<ShardTest>(), 64);
    assert_eq!(size_of::<std::sync::atomic::AtomicU64>(), 8);
}

#[test]
fn actor_canal_total_exacto() {
    let actor = Arc::new(ActorCanal::nuevo(4));
    for i in 0..1000 {
        actor.incrementar(i % 4);
    }
    // Los mensajes son sincrónos en los hilos del actor,
    // pero el canal puede tener mensajes en vuelo.
    // Esperamos un poco para que se procesen.
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(actor.total(), 1000);
}

#[test]
fn compare_exchange_saturating() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let a = AtomicU64::new(u64::MAX - 5);
    // Saturating add: no debe superar u64::MAX
    let mut current = a.load(Ordering::Relaxed);
    loop {
        let nuevo = current.saturating_add(10);
        match a.compare_exchange_weak(
            current, nuevo,
            Ordering::AcqRel, Ordering::Relaxed,
        ) {
            Ok(_)    => break,
            Err(act) => current = act,
        }
    }
    assert_eq!(a.load(Ordering::Relaxed), u64::MAX);
}
```

---

## Análisis de resultados esperados

```text
THROUGHPUT (millones de ops/segundo) — resultados típicos en 8 cores:

Implementación  │ 1 hilo │ 2 hilos │ 4 hilos │ 8 hilos │ 16 hilos
────────────────┼────────┼─────────┼─────────┼─────────┼─────────
AtomicPadded    │  320   │   580   │  1050   │  1900   │  2100  ← escala bien
MutexSharded    │  180   │   310   │   490   │   720   │   650  ← contención
DashMap         │  120   │   200   │   350   │   510   │   490  ← overhead hash
ActorCanal      │   90   │   170   │   310   │   580   │   520  ← latencia canal

FALSE SHARING (4 hilos):
Sin padding:    │                          │  320 Mops/s
Con padding:    │                          │ 1050 Mops/s  ← 3x más rápido

ANÁLISIS:
• AtomicPadded + Relaxed escala casi linealmente hasta saturar la
  bandwidth de memoria: sin barreras, sin locks, sin invalidaciones
  de cache entre shards → cada core trabaja en su propia línea.

• MutexSharded y DashMap colapsan porque bajo alta contención el
  lock tiene que ir al kernel (futex syscall) → latencia de 1-10 µs
  por adquisición en lugar de ~1 ns de un Relaxed atómico.

• ActorCanal tiene latencia de canal (~100 ns) pero es predecible
  y sin jitter → mejor para workloads con fairness estricta.

• False Sharing: 3x diferencia solo por el alignment. El benchmark
  demuestra que "más cores" puede significar "menos throughput" si
  no se controla el layout de memoria.
```

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
let val = mapa.get("clave-a").map(|r| *r);  // copia y suelta el guard
drop(a);
let b = mapa.get_mut("clave-a");

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
- [ ] `rayon::spawn_blocking` es el puente correcto para combinar Tokio (async I/O)
  con Rayon (CPU-bound parallelism). Nunca bloqueo el runtime de Tokio.
- [ ] `cargo bench` genera el informe HTML en `target/criterion/`. El benchmark
  muestra que `AtomicPadded` escala mejor que `MutexSharded` con ≥ 4 hilos.
- [ ] `cargo test --test correctness` pasa los 5 tests (4 de corrección + 1
  de tamaño de shard).

> **Siguiente sección:** [Semana 19 — Profiling y optimización](section_03.md)
