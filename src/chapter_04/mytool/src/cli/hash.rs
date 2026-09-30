use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

use anyhow::{Context, Result};
use indicatif::MultiProgress;
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::Serialize;

use crate::{
    cli::{AlgoHash, HashArgs},
    util,
};

enum Hasher {
    // Box: blake3::Hasher ocupa ~1.9 KB; sin él, cada variante reservaría ese tamaño
    Blake3(Box<blake3::Hasher>),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl Hasher {
    fn nuevo(algo: &AlgoHash) -> Self {
        match algo {
            AlgoHash::Blake3 => Hasher::Blake3(Box::new(blake3::Hasher::new())),
            AlgoHash::Sha256 => {
                use sha2::Digest;
                Hasher::Sha256(sha2::Sha256::new())
            }
            AlgoHash::Sha512 => {
                use sha2::Digest;
                Hasher::Sha512(sha2::Sha512::new())
            }
        }
    }

    fn actualizar(&mut self, data: &[u8]) {
        use sha2::Digest;
        match self {
            Hasher::Blake3(h) => {
                h.update(data);
            }
            Hasher::Sha256(h) => h.update(data),
            Hasher::Sha512(h) => h.update(data),
        }
    }

    fn finalizar(self) -> String {
        use sha2::Digest;
        match self {
            Hasher::Blake3(h) => h.finalize().to_hex().to_string(),
            Hasher::Sha256(h) => hex::encode(h.finalize()),
            Hasher::Sha512(h) => hex::encode(h.finalize()),
        }
    }
}

#[derive(Serialize)]
struct ResultadoHash {
    archivo: String,
    algo: String,
    hash: String,
    bytes: u64,
    ok: bool,
}

pub fn ejecutar(args: HashArgs, json: bool) -> Result<()> {
    let mp = MultiProgress::new();
    let algo = args.algo.clone();

    // Procesar en paralelo usando Rayon
    let resultados: Vec<ResultadoHash> = args
        .archivos
        .par_iter()
        .map(|ruta| hashear_archivo(ruta, &algo, &mp))
        .collect::<Result<Vec<_>, _>>()?;

    // Mostrar resultados
    for r in &resultados {
        if json {
            println!("{}", serde_json::to_string(r).unwrap());
        } else if args.solo_hash {
            println!("{}", r.hash);
        } else {
            let estado = match (r.ok, util::es_tty()) {
                (true, true) => "✓".green().bold().to_string(),
                (false, true) => "✗".red().bold().to_string(),
                (true, false) => "OK".to_string(),
                (false, false) => "FAIL".to_string(),
            };
            println!(
                "{estado}  {}  {} ({})",
                r.hash,
                r.archivo,
                util::formato_bytes(r.bytes)
            );
        }
    }

    let errores = resultados.iter().filter(|r| !r.ok).count();
    if errores > 0 {
        eprintln!("{}", format!("{errores} archivo(s) fallaron").red());
        std::process::exit(1);
    }

    Ok(())
}

fn hashear_archivo(ruta: &PathBuf, algo: &AlgoHash, mp: &MultiProgress) -> Result<ResultadoHash> {
    let meta = ruta
        .metadata()
        .with_context(|| format!("no se pudo leer: {}", ruta.display()))?;
    let total_bytes = meta.len();

    let pb = mp.add(util::crear_barra(total_bytes, &ruta.display().to_string()));

    let archivo =
        File::open(ruta).with_context(|| format!("no se pudo abrir: {}", ruta.display()))?;
    let mut reader = BufReader::with_capacity(256 * 1024, archivo); // 256 KB buffer
    let mut hasher = Hasher::nuevo(algo);
    let mut buf = vec![0u8; 64 * 1024]; // 64 KB por iteración

    let mut bytes_leidos = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("error leyendo: {}", ruta.display()))?;
        if n == 0 {
            break;
        }
        hasher.actualizar(&buf[..n]);
        bytes_leidos += n as u64;
        pb.inc(n as u64);
    }

    let hash = hasher.finalizar();
    pb.finish_and_clear();

    Ok(ResultadoHash {
        archivo: ruta.display().to_string(),
        algo: algo.to_string(),
        hash,
        bytes: bytes_leidos,
        ok: true,
    })
}
