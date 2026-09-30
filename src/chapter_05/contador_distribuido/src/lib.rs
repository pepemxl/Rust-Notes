//! Cuatro contadores concurrentes (más uno deliberadamente malo) detrás del mismo
//! trait, para compararlos con `criterion` bajo carga de escritura creciente.

use std::sync::atomic::{AtomicU64, Ordering};

use crossbeam::channel::{self, Sender};

// ── Trait común ────────────────────────────────────────────────────────────

pub trait Contador: Send + Sync {
    /// Incrementa el shard sugerido (cada hilo pasa su índice).
    fn incrementar(&self, shard_hint: usize);
    /// Total global. Exacto si ya no hay escrituras en curso.
    fn total(&self) -> u64;
}

// ── Implementación 1: atómico con padding ─────────────────────────────────

/// Cada shard ocupa su propia línea de cache (64 bytes): dos hilos que escriben
/// en shards distintos no se invalidan la cache entre sí.
#[repr(align(64))]
struct ShardPadded(AtomicU64);

pub struct AtomicPadded {
    shards: Box<[ShardPadded]>,
}

impl AtomicPadded {
    pub fn nuevo(n_shards: usize) -> Self {
        let shards = (0..n_shards.max(1))
            .map(|_| ShardPadded(AtomicU64::new(0)))
            .collect();
        AtomicPadded { shards }
    }
}

impl Contador for AtomicPadded {
    fn incrementar(&self, shard_hint: usize) {
        let idx = shard_hint % self.shards.len();
        // Relaxed: solo necesitamos atomicidad, ningún otro dato depende de este valor
        self.shards[idx].0.fetch_add(1, Ordering::Relaxed);
    }

    fn total(&self) -> u64 {
        self.shards
            .iter()
            .map(|s| s.0.load(Ordering::Relaxed))
            .sum()
    }
}

// ── Implementación 1b: atómico SIN padding (el contraejemplo) ─────────────

/// Igual que `AtomicPadded`, pero los shards son contiguos: 8 caben en una
/// línea de cache, así que hilos que escriben en shards distintos se pelean
/// por la misma línea (false sharing).
pub struct AtomicSinPadding {
    shards: Box<[AtomicU64]>,
}

impl AtomicSinPadding {
    pub fn nuevo(n_shards: usize) -> Self {
        let shards = (0..n_shards.max(1)).map(|_| AtomicU64::new(0)).collect();
        AtomicSinPadding { shards }
    }
}

impl Contador for AtomicSinPadding {
    fn incrementar(&self, shard_hint: usize) {
        let idx = shard_hint % self.shards.len();
        self.shards[idx].fetch_add(1, Ordering::Relaxed);
    }

    fn total(&self) -> u64 {
        self.shards.iter().map(|s| s.load(Ordering::Relaxed)).sum()
    }
}

// ── Implementación 2: Mutex sharded (parking_lot) ─────────────────────────

/// Un `parking_lot::Mutex<u64>` por shard. Sin contención, bloquear es un CAS;
/// con contención, los hilos esperan su turno.
pub struct MutexSharded {
    shards: Box<[parking_lot::Mutex<u64>]>,
}

impl MutexSharded {
    pub fn nuevo(n_shards: usize) -> Self {
        let shards = (0..n_shards.max(1))
            .map(|_| parking_lot::Mutex::new(0))
            .collect();
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

/// Un `DashMap<usize, u64>` con una clave por shard. Paga hash + RwLock en cada
/// incremento; tiene sentido cuando además necesitas buscar por clave.
pub struct DashmapContador {
    mapa: dashmap::DashMap<usize, u64>,
    n_shards: usize,
}

impl DashmapContador {
    pub fn nuevo(n_shards: usize) -> Self {
        let n = n_shards.max(1);
        let mapa = dashmap::DashMap::with_capacity(n);
        for i in 0..n {
            mapa.insert(i, 0);
        }
        DashmapContador { mapa, n_shards: n }
    }
}

impl Contador for DashmapContador {
    fn incrementar(&self, shard_hint: usize) {
        let key = shard_hint % self.n_shards;
        *self
            .mapa
            .get_mut(&key)
            .expect("las claves se crean en nuevo()") += 1;
    }

    fn total(&self) -> u64 {
        self.mapa.iter().map(|e| *e.value()).sum()
    }
}

// ── Implementación 4: actores con crossbeam::channel ──────────────────────

enum MensajeActor {
    Inc,
    Get(Sender<u64>),
}

/// Cada shard es un hilo con estado privado; se le habla solo por su canal.
/// Los hilos terminan cuando se suelta el `ActorCanal` (se cierran los canales).
pub struct ActorCanal {
    shards: Vec<Sender<MensajeActor>>,
}

impl ActorCanal {
    pub fn nuevo(n_shards: usize) -> Self {
        let shards = (0..n_shards.max(1))
            .map(|_| {
                let (tx, rx) = channel::bounded::<MensajeActor>(4096);
                std::thread::spawn(move || {
                    let mut cuenta = 0u64;
                    while let Ok(msg) = rx.recv() {
                        match msg {
                            MensajeActor::Inc => cuenta += 1,
                            MensajeActor::Get(resp) => {
                                let _ = resp.send(cuenta);
                            }
                        }
                    }
                });
                tx
            })
            .collect();
        ActorCanal { shards }
    }
}

impl Contador for ActorCanal {
    fn incrementar(&self, shard_hint: usize) {
        let idx = shard_hint % self.shards.len();
        // Canal lleno → send() bloquea (backpressure); solo falla si el actor murió
        self.shards[idx]
            .send(MensajeActor::Inc)
            .expect("el actor terminó");
    }

    fn total(&self) -> u64 {
        // El canal es FIFO: el Get se procesa después de todos los Inc ya enviados
        self.shards
            .iter()
            .map(|tx| {
                let (resp_tx, resp_rx) = channel::bounded(1);
                tx.send(MensajeActor::Get(resp_tx))
                    .expect("el actor terminó");
                resp_rx.recv().expect("el actor terminó")
            })
            .sum()
    }
}

// ── Generador de carga ─────────────────────────────────────────────────────

/// Reparte `total_ops` incrementos entre `n_hilos` hilos (el hilo `t` usa el
/// shard `t`) y devuelve el total leído al terminar. Debe ser `total_ops`.
pub fn ejecutar_carga<C: Contador>(contador: &C, n_hilos: usize, total_ops: u64) -> u64 {
    let n = n_hilos.max(1) as u64;
    // thread::scope permite prestar &C a los hilos: no hace falta Arc
    std::thread::scope(|s| {
        for t in 0..n {
            // Los primeros `total_ops % n` hilos hacen una operación extra
            let ops = total_ops / n + u64::from(t < total_ops % n);
            s.spawn(move || {
                for _ in 0..ops {
                    contador.incrementar(t as usize);
                }
            });
        }
    });
    contador.total()
}
