pub mod genpass;
pub mod hash;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "mytool",
    version,
    about = "Navaja suiza en Rust",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Comandos,

    /// Nivel de detalle: -v (info), -vv (debug), -vvv (trace)
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Salida en JSON estructurado
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum Comandos {
    /// Calcula el hash criptográfico de archivos
    Hash(HashArgs),
    /// Genera contraseñas seguras y fáciles de recordar
    // Sin `name`, clap usaría "gen-pass" (kebab-case del nombre de la variante)
    #[command(name = "genpass")]
    GenPass(GenPassArgs),
}

#[derive(Args, Debug)]
pub struct HashArgs {
    /// Archivos a hashear
    #[arg(value_name = "ARCHIVO", num_args = 1.., required = true)]
    pub archivos: Vec<PathBuf>,

    /// Algoritmo de hash
    #[arg(short = 'a', long, value_enum, default_value_t = AlgoHash::Blake3, env = "MYTOOL_ALGO")]
    pub algo: AlgoHash,

    /// Verificar hashes contra un archivo de checksums
    #[arg(short = 'c', long, value_name = "CHECKSUMS")]
    pub verificar: Option<PathBuf>,

    /// Solo el hash, sin nombre de archivo
    #[arg(long)]
    pub solo_hash: bool,
}

#[derive(Args, Debug)]
pub struct GenPassArgs {
    /// Longitud de la contraseña (8-128)
    #[arg(short, long, default_value_t = 20,
          value_parser = clap::value_parser!(u32).range(8..=128))]
    pub longitud: u32,

    /// Generar passphrase con palabras diceware (EFF wordlist)
    #[arg(long)]
    pub diceware: bool,

    /// Número de palabras diceware
    #[arg(long, default_value_t = 6, requires = "diceware")]
    pub palabras: u32,

    /// Cantidad de contraseñas a generar
    #[arg(short = 'n', long, default_value_t = 1)]
    pub cantidad: u32,

    /// Excluir caracteres ambiguos (0/O, 1/l/I)
    #[arg(long)]
    pub sin_ambiguos: bool,
}

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum AlgoHash {
    #[default]
    Blake3,
    Sha256,
    Sha512,
}

impl std::fmt::Display for AlgoHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlgoHash::Blake3 => write!(f, "blake3"),
            AlgoHash::Sha256 => write!(f, "sha256"),
            AlgoHash::Sha512 => write!(f, "sha512"),
        }
    }
}
