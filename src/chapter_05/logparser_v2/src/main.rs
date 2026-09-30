//! `logparser_v2 [--antes] < access.log > entradas.jsonl`
//!
//! Convierte cada línea a JSON y al final imprime en stderr las 5 rutas más
//! visitadas. Con `--antes` usa las versiones sin optimizar, para comparar con
//! `time` o contar syscalls con `strace -c`.

use std::io::{self, Read};

use logparser_v2::aggregate::{EstadisticasAntes, EstadisticasDespues};
use logparser_v2::output::{escribir_json_antes, escribir_json_despues};
use logparser_v2::parser::{parsear_linea, parsear_linea_antes};

fn main() -> io::Result<()> {
    let mut texto = String::new();
    io::stdin().read_to_string(&mut texto)?;

    let top = if std::env::args().any(|a| a == "--antes") {
        let entradas: Vec<_> = texto
            .lines()
            .filter_map(|l| parsear_linea_antes(l).ok().map(|(_, e)| e))
            .collect();
        escribir_json_antes(&entradas, io::stdout())?;

        let mut stats = EstadisticasAntes::default();
        for e in entradas {
            stats.registrar(e.ip, e.ruta, e.estado);
        }
        formatear_top(&stats.top_rutas(5))
    } else {
        let entradas: Vec<_> = texto
            .lines()
            .filter_map(|l| parsear_linea(l).ok().map(|(_, e)| e))
            .collect();
        escribir_json_despues(&entradas, io::stdout().lock())?;

        let mut stats = EstadisticasDespues::default();
        for e in &entradas {
            stats.registrar(e.ip, &e.ruta, e.estado);
        }
        formatear_top(&stats.top_rutas(5))
    };

    eprint!("{top}");
    Ok(())
}

fn formatear_top(top: &[(&str, u64)]) -> String {
    use std::fmt::Write;
    top.iter().fold(String::new(), |mut s, (ruta, n)| {
        let _ = writeln!(s, "{n:>8}  {ruta}"); // escribir en un String no falla
        s
    })
}
