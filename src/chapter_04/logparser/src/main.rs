use logparser::aggregate::Estadisticas;
use logparser::filter::{Filtro, aplicar_filtros};
use logparser::output::{Formato, imprimir_entrada, imprimir_resumen};
use logparser::parser::nginx::parsear_linea;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, IsTerminal},
    path::PathBuf,
};

#[derive(Parser)]
#[command(name = "logparser", about = "Parser streaming de logs de servidor")]
struct Cli {
    /// Archivos de log a procesar (stdin si se omite)
    #[arg(value_name = "ARCHIVO")]
    archivos: Vec<PathBuf>,

    /// Filtros (ej: "estado:>=400", "metodo:GET", "ruta:/api")
    #[arg(short = 'f', long = "filtro", value_name = "EXPR")]
    filtros: Vec<String>,

    /// Formato de salida
    #[arg(short, long, value_enum, default_value_t = FmtArg::Resumen)]
    output: FmtArg,

    /// Solo mostrar errores (>=400)
    #[arg(long)]
    errores: bool,
}

#[derive(ValueEnum, Clone)]
enum FmtArg {
    Json,
    Tabla,
    Resumen,
}

impl From<FmtArg> for Formato {
    fn from(v: FmtArg) -> Self {
        match v {
            FmtArg::Json => Formato::JsonLines,
            FmtArg::Tabla => Formato::Tabla,
            FmtArg::Resumen => Formato::Resumen,
        }
    }
}

fn procesar_lector<R: BufRead>(
    lector: R,
    filtros: &[Filtro],
    fmt: &Formato,
    stats: &mut Estadisticas,
    parseadas: &mut u64,
    fallidas: &mut u64,
) {
    for linea_res in lector.lines() {
        let linea = match linea_res {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error leyendo: {e}");
                continue;
            }
        };
        if linea.is_empty() || linea.starts_with('#') {
            continue;
        }

        match parsear_linea(&linea) {
            Ok((_, entrada)) => {
                *parseadas += 1;
                if aplicar_filtros(&entrada, filtros) {
                    stats.registrar(&entrada);
                    imprimir_entrada(&entrada, fmt);
                }
            }
            Err(_) => {
                *fallidas += 1;
                if *fallidas <= 5 {
                    eprintln!("línea no parseada: {}", &linea[..linea.len().min(80)]);
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut filtros: Vec<Filtro> = cli
        .filtros
        .iter()
        .map(|expr| Filtro::parsear(expr))
        .collect::<Result<_>>()?;

    if cli.errores {
        filtros.push(Filtro::EstadoMin(400));
    }

    let fmt: Formato = cli.output.into();
    let mut stats = Estadisticas::default();
    let mut parseadas = 0u64;
    let mut fallidas = 0u64;

    if cli.archivos.is_empty() {
        // Leer de stdin
        if io::stdin().is_terminal() {
            eprintln!("Esperando stdin (Ctrl+D para terminar)...");
        }
        let lector = BufReader::new(io::stdin().lock());
        procesar_lector(
            lector,
            &filtros,
            &fmt,
            &mut stats,
            &mut parseadas,
            &mut fallidas,
        );
    } else {
        for ruta in &cli.archivos {
            let archivo =
                File::open(ruta).map_err(|e| anyhow::anyhow!("{}: {e}", ruta.display()))?;
            let lector = BufReader::with_capacity(256 * 1024, archivo);
            procesar_lector(
                lector,
                &filtros,
                &fmt,
                &mut stats,
                &mut parseadas,
                &mut fallidas,
            );
        }
    }

    if !matches!(fmt, Formato::JsonLines) {
        imprimir_resumen(&stats);
    }

    if fallidas > 0 {
        eprintln!("\nAdvertencia: {fallidas} líneas no parseadas");
    }
    eprintln!("Procesadas: {parseadas} líneas");

    Ok(())
}
