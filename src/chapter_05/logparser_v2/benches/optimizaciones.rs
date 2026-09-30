use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use logparser_v2::aggregate::{EstadisticasAntes, EstadisticasDespues};
use logparser_v2::lineas_de_prueba;
use logparser_v2::output::{escribir_json_antes, escribir_json_despues};
use logparser_v2::parser::{parsear_linea, parsear_linea_antes};

const N_LINEAS: usize = 100_000;

// ── OPT-01: parser (String por campo vs Cow zero-copy) ────────────────────

fn bench_parser(c: &mut Criterion) {
    let lineas = lineas_de_prueba(N_LINEAS);
    let bytes: u64 = lineas.iter().map(|l| l.len() as u64).sum();

    let mut grupo = c.benchmark_group("parser");
    grupo.throughput(Throughput::Bytes(bytes));

    grupo.bench_function("antes", |b| {
        b.iter(|| {
            for l in &lineas {
                black_box(parsear_linea_antes(black_box(l)).ok());
            }
        })
    });
    grupo.bench_function("despues", |b| {
        b.iter(|| {
            for l in &lineas {
                black_box(parsear_linea(black_box(l)).ok());
            }
        })
    });
    grupo.finish();
}

// ── OPT-02: agregación (SipHash + String vs ahash + Spur) ─────────────────
// Ambas versiones reciben las mismas entradas ya parseadas: solo se mide agregar.

fn bench_agregacion(c: &mut Criterion) {
    let lineas = lineas_de_prueba(N_LINEAS);
    let entradas: Vec<_> = lineas.iter().map(|l| parsear_linea(l).unwrap().1).collect();

    let mut grupo = c.benchmark_group("agregacion");
    grupo.throughput(Throughput::Elements(N_LINEAS as u64));

    grupo.bench_function("antes", |b| {
        b.iter(|| {
            let mut stats = EstadisticasAntes::default();
            for e in &entradas {
                // La versión original recibía Strings: la copia es parte de su costo
                stats.registrar(e.ip.to_string(), e.ruta.to_string(), e.estado);
            }
            black_box(stats.por_ruta.len())
        })
    });
    grupo.bench_function("despues", |b| {
        b.iter(|| {
            let mut stats = EstadisticasDespues::default();
            for e in &entradas {
                stats.registrar(e.ip, &e.ruta, e.estado);
            }
            black_box(stats.por_ruta.len())
        })
    });
    grupo.finish();
}

// ── OPT-03: salida (to_string + write por línea vs to_writer + BufWriter) ─
// Escribimos a un archivo temporal real: cada write() es una syscall de verdad.

fn bench_output(c: &mut Criterion) {
    let lineas = lineas_de_prueba(N_LINEAS);
    let entradas: Vec<_> = lineas.iter().map(|l| parsear_linea(l).unwrap().1).collect();

    let mut grupo = c.benchmark_group("output");
    grupo.throughput(Throughput::Elements(N_LINEAS as u64));
    grupo.sample_size(20);

    grupo.bench_function("antes", |b| {
        b.iter_batched(
            || tempfile::tempfile().unwrap(),
            |archivo| escribir_json_antes(&entradas, archivo).unwrap(),
            BatchSize::PerIteration,
        )
    });
    grupo.bench_function("despues", |b| {
        b.iter_batched(
            || tempfile::tempfile().unwrap(),
            |archivo| escribir_json_despues(&entradas, archivo).unwrap(),
            BatchSize::PerIteration,
        )
    });
    grupo.finish();
}

criterion_group!(benches, bench_parser, bench_agregacion, bench_output);
criterion_main!(benches);
