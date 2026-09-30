use anyhow::Result;
use clap::Parser;
use mytool::cli::{self, Cli, Comandos};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Configurar nivel de log según -v/-vv/-vvv
    let nivel = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(nivel))
        .with_target(false)
        .init();

    match cli.command {
        Comandos::Hash(args) => cli::hash::ejecutar(args, cli.json),
        Comandos::GenPass(args) => cli::genpass::ejecutar(args, cli.json),
    }
}
