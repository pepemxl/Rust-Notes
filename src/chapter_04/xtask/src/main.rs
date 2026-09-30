use clap::CommandFactory;
use clap_complete::{Shell, generate_to};
use clap_mangen::Man;
use mytool::cli::Cli;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let tarea = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Uso: cargo xtask <tarea>");
        eprintln!("Tareas: completions, manpage, dist");
        std::process::exit(1);
    });

    match tarea.as_str() {
        "completions" => generar_completions(),
        "manpage" => generar_manpage(),
        "dist" => {
            generar_completions();
            generar_manpage();
            println!("✓ artefactos generados en {}", directorio_dist().display());
        }
        t => {
            eprintln!("tarea desconocida: {t}");
            std::process::exit(1);
        }
    }
}

fn directorio_dist() -> PathBuf {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // raíz del workspace
        .unwrap()
        .to_path_buf();
    let dist = raiz.join("dist");
    fs::create_dir_all(&dist).unwrap();
    dist
}

fn generar_completions() {
    let dist = directorio_dist();
    // Cli::command() construye la definición de clap sin parsear argumentos:
    // la misma que usa el binario, así las completions nunca se desincronizan.
    let mut comando = Cli::command();
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
        let ruta = generate_to(shell, &mut comando, "mytool", &dist).unwrap();
        println!("  → {shell}: {}", ruta.display());
    }
    println!("✓ completions listas");
}

fn generar_manpage() {
    let ruta = directorio_dist().join("mytool.1");
    let mut archivo = fs::File::create(&ruta).unwrap();
    Man::new(Cli::command()).render(&mut archivo).unwrap();
    println!("✓ man page generada en {}", ruta.display());
}
