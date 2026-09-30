use std::hint::black_box;

use contador_distribuido::{
    ActorCanal, AtomicPadded, AtomicSinPadding, Contador, DashmapContador, MutexSharded,
    ejecutar_carga,
};
use criterion::{
    BatchSize, BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
    measurement::WallTime,
};

const TOTAL_OPS: u64 = 1_000_000;

/// 1, 2, 4, 8… hasta 2 × núcleos lógicos.
fn cantidades_de_hilos() -> Vec<usize> {
    let n_cpus = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut hilos: Vec<usize> = (0..).map(|i| 1 << i).take_while(|&h| h < n_cpus).collect();
    hilos.extend([n_cpus, n_cpus * 2]);
    hilos
}

fn medir<C: Contador>(
    grupo: &mut BenchmarkGroup<'_, WallTime>,
    nombre: &str,
    nuevo: impl Fn(usize) -> C,
    n_hilos: usize,
) {
    grupo.bench_with_input(BenchmarkId::new(nombre, n_hilos), &n_hilos, |b, &t| {
        // Un contador nuevo por iteración (un shard por hilo); crearlo y soltarlo
        // queda FUERA de la medición gracias a iter_batched_ref
        b.iter_batched_ref(
            || nuevo(t),
            |c| black_box(ejecutar_carga(c, t, TOTAL_OPS)),
            BatchSize::PerIteration,
        );
    });
}

fn bench_implementaciones(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("contador-distribuido");
    grupo.throughput(Throughput::Elements(TOTAL_OPS));
    grupo.sample_size(20);

    for n_hilos in cantidades_de_hilos() {
        medir(&mut grupo, "AtomicPadded", AtomicPadded::nuevo, n_hilos);
        medir(&mut grupo, "MutexSharded", MutexSharded::nuevo, n_hilos);
        medir(&mut grupo, "DashMap", DashmapContador::nuevo, n_hilos);
        medir(&mut grupo, "ActorCanal", ActorCanal::nuevo, n_hilos);
    }
    grupo.finish();
}

fn bench_false_sharing(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("false-sharing");
    grupo.throughput(Throughput::Elements(TOTAL_OPS));
    grupo.sample_size(20);

    for n_hilos in cantidades_de_hilos() {
        medir(&mut grupo, "con-padding", AtomicPadded::nuevo, n_hilos);
        medir(&mut grupo, "sin-padding", AtomicSinPadding::nuevo, n_hilos);
    }
    grupo.finish();
}

criterion_group!(benches, bench_implementaciones, bench_false_sharing);
criterion_main!(benches);
