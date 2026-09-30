use criterion::{Criterion, black_box, criterion_group, criterion_main};
use regex::Regex;

const LINEA_NGINX: &str = r#"192.168.1.1 - frank [10/Oct/2000:13:55:36 -0700] "GET /apache_pb.gif HTTP/1.0" 200 2326 "http://www.example.com/start.html" "Mozilla/4.08 [en] (Win98; I ;Nav)""#;

fn bench_parsers(c: &mut Criterion) {
    let mut grupo = c.benchmark_group("nginx_line_parser");

    // Benchmark 1: nom parser
    grupo.bench_function("nom", |b| {
        b.iter(|| logparser::parser::nginx::parsear_linea(black_box(LINEA_NGINX)).unwrap())
    });

    // Benchmark 2: regex equivalente (solo extrae estado y ruta)
    let re = Regex::new(r#"^(\S+) \S+ \S+ \[[^\]]+\] "(\S+) ([^"]+) [^"]+" (\d+)"#).unwrap();

    grupo.bench_function("regex", |b| {
        b.iter(|| re.captures(black_box(LINEA_NGINX)).unwrap())
    });

    grupo.finish();
}

fn bench_volumen(c: &mut Criterion) {
    // Generar 1000 líneas de log
    let lineas: Vec<String> = (0..1000)
        .map(|i| format!(
            r#"10.0.0.{} - - [01/Jan/2024:00:00:00 +0000] "GET /api/v1/users/{} HTTP/1.1" {} {} "-" "curl/7.68""#,
            i % 255,
            i,
            if i % 10 == 0 { 500 } else { 200 },
            i * 100
        ))
        .collect();

    c.bench_function("nom_1000_lineas", |b| {
        b.iter(|| {
            let mut ok = 0u32;
            for linea in &lineas {
                if logparser::parser::nginx::parsear_linea(black_box(linea)).is_ok() {
                    ok += 1;
                }
            }
            ok
        })
    });
}

criterion_group!(benches, bench_parsers, bench_volumen);
criterion_main!(benches);
