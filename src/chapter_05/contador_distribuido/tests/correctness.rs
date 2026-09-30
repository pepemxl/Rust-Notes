use contador_distribuido::*;

const N_OPS: u64 = 100_003; // primo: no se reparte exacto entre los hilos

macro_rules! test_correccion {
    ($nombre:ident, $constructor:expr) => {
        #[test]
        fn $nombre() {
            for n_hilos in [1usize, 2, 3, 4, 8] {
                let contador = $constructor(n_hilos);
                let resultado = ejecutar_carga(&contador, n_hilos, N_OPS);
                assert_eq!(
                    resultado, N_OPS,
                    "fallo con {n_hilos} hilos: esperado {N_OPS}, obtenido {resultado}"
                );
            }
        }
    };
}

test_correccion!(atomic_padded_correcto, AtomicPadded::nuevo);
test_correccion!(atomic_sin_padding_correcto, AtomicSinPadding::nuevo);
test_correccion!(mutex_sharded_correcto, MutexSharded::nuevo);
test_correccion!(dashmap_correcto, DashmapContador::nuevo);
test_correccion!(actor_canal_correcto, ActorCanal::nuevo);

#[test]
fn shard_con_padding_ocupa_una_linea_de_cache() {
    use std::mem::{align_of, size_of};
    use std::sync::atomic::AtomicU64;

    #[repr(align(64))]
    struct Shard(#[allow(dead_code)] AtomicU64);

    assert_eq!(size_of::<AtomicU64>(), 8); // sin padding: 8 por línea de 64 bytes
    assert_eq!(size_of::<Shard>(), 64);
    assert_eq!(align_of::<Shard>(), 64);
}

#[test]
fn actor_canal_total_exacto_sin_esperar() {
    let actor = ActorCanal::nuevo(4);
    for i in 0..1000 {
        actor.incrementar(i % 4);
    }
    // Sin sleep: cada canal es FIFO, así que el Get de total() llega a cada
    // actor después de todos los Inc que ya se le enviaron.
    assert_eq!(actor.total(), 1000);
}
