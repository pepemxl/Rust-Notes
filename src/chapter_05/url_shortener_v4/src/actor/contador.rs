use crate::domain::url_entry::UrlCode;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

// El protocolo del actor: privado, el resto del sistema solo ve los handles.
enum Mensaje {
    Incrementar(UrlCode),
    Obtener {
        codigo: UrlCode,
        tx: oneshot::Sender<u64>,
    },
    Snapshot(oneshot::Sender<HashMap<String, u64>>),
}

// El estado del actor: nadie más tiene acceso, así que no necesita Mutex.
struct ContadorInterno {
    conteos: HashMap<String, u64>,
    rx: mpsc::Receiver<Mensaje>,
}

impl ContadorInterno {
    async fn ejecutar(mut self) {
        // Termina cuando se sueltan todos los Sender (todos los handles)
        while let Some(msg) = self.rx.recv().await {
            match msg {
                Mensaje::Incrementar(code) => {
                    *self.conteos.entry(code.to_string()).or_default() += 1;
                }
                Mensaje::Obtener { codigo, tx } => {
                    let n = self.conteos.get(codigo.as_str()).copied().unwrap_or(0);
                    let _ = tx.send(n);
                }
                Mensaje::Snapshot(tx) => {
                    let _ = tx.send(self.conteos.clone());
                }
            }
        }
    }
}

#[derive(Clone)]
struct ContadorHandle {
    tx: mpsc::Sender<Mensaje>,
}

impl ContadorHandle {
    fn iniciar() -> Self {
        let (tx, rx) = mpsc::channel(512); // bounded: backpressure si el actor no da abasto
        let actor = ContadorInterno {
            conteos: HashMap::new(),
            rx,
        };
        tokio::spawn(actor.ejecutar());
        ContadorHandle { tx }
    }

    async fn incrementar(&self, code: UrlCode) {
        let _ = self.tx.send(Mensaje::Incrementar(code)).await;
    }

    async fn obtener(&self, codigo: &UrlCode) -> u64 {
        let (tx, rx) = oneshot::channel();
        let msg = Mensaje::Obtener {
            codigo: codigo.clone(),
            tx,
        };
        if self.tx.send(msg).await.is_err() {
            return 0;
        }
        rx.await.unwrap_or(0)
    }

    async fn snapshot(&self) -> HashMap<String, u64> {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Mensaje::Snapshot(tx)).await.is_err() {
            return HashMap::new();
        }
        rx.await.unwrap_or_default()
    }
}

// ── Sharded: N actores para alta concurrencia ─────────────────────────────

/// N actores; cada código de URL va siempre al mismo (`hash(code) % N`), así
/// que cada conteo vive en un solo actor y no hay que sincronizar nada.
#[derive(Clone)]
pub struct ContadorSharded {
    shards: Vec<ContadorHandle>,
}

impl ContadorSharded {
    /// Debe llamarse dentro de un runtime de Tokio (hace `tokio::spawn`).
    pub fn iniciar(n: usize) -> Self {
        let shards = (0..n.max(1)).map(|_| ContadorHandle::iniciar()).collect();
        ContadorSharded { shards }
    }

    fn shard(&self, code: &UrlCode) -> &ContadorHandle {
        // FNV-1a: estable y barato. Solo reparte carga, no necesita resistir DoS.
        let h = code
            .as_str()
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |acc, b| {
                (acc ^ b as u64).wrapping_mul(0x100_0000_01b3)
            });
        &self.shards[(h % self.shards.len() as u64) as usize]
    }

    pub async fn incrementar(&self, code: UrlCode) {
        self.shard(&code).incrementar(code).await;
    }

    pub async fn obtener(&self, codigo: &UrlCode) -> u64 {
        self.shard(codigo).obtener(codigo).await
    }

    /// Une los conteos de todos los shards (cada código está en uno solo).
    pub async fn snapshot(&self) -> HashMap<String, u64> {
        let mut total = HashMap::new();
        for shard in &self.shards {
            total.extend(shard.snapshot().await);
        }
        total
    }
}
