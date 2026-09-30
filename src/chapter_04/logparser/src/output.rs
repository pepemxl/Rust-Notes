use crate::aggregate::Estadisticas;
use crate::parser::nginx::EntradaNginx;
use owo_colors::OwoColorize;
use std::io::IsTerminal;
use tabled::{Table, Tabled};

/// Colores solo en una terminal: en un pipe o archivo (`logparser ... > informe.txt`)
/// los códigos ANSI ensuciarían la salida.
fn con_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

fn titulo(texto: &str) -> String {
    if con_color() {
        texto.cyan().bold().to_string()
    } else {
        texto.to_string()
    }
}

fn numero(n: u64) -> String {
    if con_color() {
        n.to_string().yellow().to_string()
    } else {
        n.to_string()
    }
}

pub enum Formato {
    JsonLines,
    Tabla,
    Resumen,
}

pub fn imprimir_entrada(entrada: &EntradaNginx, fmt: &Formato) {
    match fmt {
        Formato::JsonLines => println!("{}", serde_json::to_string(entrada).unwrap()),
        Formato::Tabla => {}   // se acumulan y se imprimen al final
        Formato::Resumen => {} // ídem
    }
}

#[derive(Tabled)]
struct FilaEstado<'a> {
    #[tabled(rename = "Estado")]
    estado: u16,
    #[tabled(rename = "Peticiones")]
    total: u64,
    #[tabled(rename = "% del total")]
    porcentaje: String,
    #[tabled(rename = "Tipo")]
    tipo: &'a str,
}

pub fn imprimir_resumen(stats: &Estadisticas) {
    println!("\n{}", titulo("=== Resumen ==="));
    println!("Total peticiones: {}", numero(stats.total));
    println!(
        "Errores 4xx:      {} ({:.1}%)",
        stats.errores_4xx,
        100.0 * stats.errores_4xx as f64 / stats.total.max(1) as f64
    );
    println!(
        "Errores 5xx:      {} ({:.1}%)",
        stats.errores_5xx,
        100.0 * stats.errores_5xx as f64 / stats.total.max(1) as f64
    );
    println!("Bytes transferidos: {} MB", stats.bytes_total / 1_048_576);

    // Tabla de estados
    let mut filas: Vec<FilaEstado> = stats
        .por_estado
        .iter()
        .map(|(&estado, &cnt)| {
            let tipo = match estado {
                200..=299 => "2xx OK",
                300..=399 => "3xx Redirect",
                400..=499 => "4xx Client Error",
                _ => "5xx Server Error",
            };
            FilaEstado {
                estado,
                total: cnt,
                porcentaje: format!("{:.1}%", 100.0 * cnt as f64 / stats.total.max(1) as f64),
                tipo,
            }
        })
        .collect();
    filas.sort_unstable_by_key(|f| f.estado);

    println!("\n{}", Table::new(&filas));

    // Top 5 rutas
    println!("\n{}", titulo("Top 5 rutas:"));
    for (ruta, cnt) in Estadisticas::top_n(&stats.top_rutas, 5) {
        println!("  {:>6}  {}", numero(cnt), ruta);
    }

    // Top 5 IPs
    println!("\n{}", titulo("Top 5 IPs:"));
    for (ip, cnt) in Estadisticas::top_n(&stats.top_ips, 5) {
        println!("  {:>6}  {}", numero(cnt), ip);
    }
}
