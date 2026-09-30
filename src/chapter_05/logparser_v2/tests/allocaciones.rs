//! Cuenta las allocaciones de cada versión con un allocator global propio: la
//! misma pregunta que responde heaptrack, pero como test que corre en CI.
//! Para ver la tabla: `cargo test --test allocaciones -- --nocapture`

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io;

use logparser_v2::aggregate::{EstadisticasAntes, EstadisticasDespues};
use logparser_v2::lineas_de_prueba;
use logparser_v2::output::{escribir_json_antes, escribir_json_despues};
use logparser_v2::parser::{parsear_linea, parsear_linea_antes};

struct Contador;

thread_local! {
    // Por hilo: el harness de tests no contamina la cuenta del hilo que mide.
    // `const` evita que inicializar el thread_local tenga que allocar.
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Contador {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.with(|n| n.set(n.get() + 1));
        // SAFETY: mismo contrato que GlobalAlloc::alloc; delegamos en System
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` viene de System.alloc con este mismo `layout`
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Contador = Contador;

fn contar(f: impl FnOnce()) -> u64 {
    let antes = ALLOCS.with(Cell::get);
    f();
    ALLOCS.with(Cell::get) - antes
}

#[test]
fn allocaciones_por_optimizacion() {
    const N: usize = 100_000;
    let lineas = lineas_de_prueba(N);
    let entradas: Vec<_> = lineas.iter().map(|l| parsear_linea(l).unwrap().1).collect();

    let parser_antes = contar(|| {
        for l in &lineas {
            drop(parsear_linea_antes(l));
        }
    });
    let parser_despues = contar(|| {
        for l in &lineas {
            drop(parsear_linea(l));
        }
    });

    let agregacion_antes = contar(|| {
        let mut stats = EstadisticasAntes::default();
        for e in &entradas {
            stats.registrar(e.ip.to_string(), e.ruta.to_string(), e.estado);
        }
    });
    let agregacion_despues = contar(|| {
        let mut stats = EstadisticasDespues::default();
        for e in &entradas {
            stats.registrar(e.ip, &e.ruta, e.estado);
        }
    });

    let output_antes = contar(|| escribir_json_antes(&entradas, io::sink()).unwrap());
    let output_despues = contar(|| escribir_json_despues(&entradas, io::sink()).unwrap());

    println!("allocaciones para {N} líneas:");
    println!("  parser      antes {parser_antes:>7}   después {parser_despues:>7}");
    println!("  agregación  antes {agregacion_antes:>7}   después {agregacion_despues:>7}");
    println!("  output      antes {output_antes:>7}   después {output_despues:>7}");

    // Parser: 3 Strings por línea antes; después, solo las rutas con %-encoding (1 de 8)
    assert_eq!(parser_antes, 3 * N as u64);
    assert_eq!(parser_despues, (N / 8) as u64);
    // Agregación y output: al menos 10 veces menos allocaciones
    assert!(agregacion_despues * 10 < agregacion_antes);
    assert!(output_despues * 10 < output_antes);
}
