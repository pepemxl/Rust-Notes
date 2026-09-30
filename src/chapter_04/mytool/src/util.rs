use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use std::io::IsTerminal;
use std::time::Duration;

/// ¿Colores? Solo si stdout es una terminal y el usuario no pidió `NO_COLOR`
/// (convención de https://no-color.org). En un pipe o archivo, texto limpio.
pub fn es_tty() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

pub fn crear_barra(total_bytes: u64, nombre: &str) -> ProgressBar {
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] \
             {bytes}/{total_bytes} ({bytes_per_sec}) {msg}",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message(nombre.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

pub fn formato_bytes(bytes: u64) -> String {
    const U: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, U[0])
    } else {
        format!("{:.1} {}", v, U[i])
    }
}

pub fn ok(msg: &str) {
    println!("{} {}", "✓".green().bold(), msg);
}
pub fn err(msg: &str) {
    eprintln!("{} {}", "✗".red().bold(), msg);
}
pub fn info(msg: &str) {
    println!("{} {}", "→".cyan(), msg);
}
