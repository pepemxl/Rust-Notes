use anyhow::Result;
use owo_colors::OwoColorize;
use rand::rngs::OsRng;
use rand::{Rng, TryRngCore};
use serde::Serialize;

use crate::cli::GenPassArgs;

const CHARS_LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz"; // sin l
const CHARS_UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ"; // sin I, O
const CHARS_DIGITS: &[u8] = b"23456789"; // sin 0, 1
const CHARS_SIMBOLOS: &[u8] = b"!@#$%^&*-_=+";

// Subconjunto de la EFF Long Wordlist (muestra; la lista real tiene 7776 palabras)
const DICEWARE_PALABRAS: &[&str] = &[
    "abaco", "bruma", "calma", "delta", "enero", "fauna", "globo", "hongo", "intro", "justo",
    "karma", "limon", "marco", "novel", "opera", "pluma", "queso", "radar", "salsa", "tango",
    "union", "valor", "watts", "xerox", "yunta", "zafra", "atlas", "brisa", "cielo", "drago",
];

#[derive(Serialize)]
struct Contrasena {
    valor: String,
    entropia: f64,
    tipo: String,
}

pub fn ejecutar(args: GenPassArgs, json: bool) -> Result<()> {
    // Fuente criptográfica del sistema operativo. En rand 0.9, OsRng solo implementa
    // TryRngCore (puede fallar); unwrap_err() lo adapta a Rng y hace panic si el SO falla.
    let mut rng = OsRng.unwrap_err();

    for _ in 0..args.cantidad {
        let resultado = if args.diceware {
            generar_diceware(&mut rng, args.palabras)
        } else {
            generar_aleatoria(&mut rng, args.longitud, args.sin_ambiguos)
        };

        if json {
            println!("{}", serde_json::to_string(&resultado).unwrap());
        } else {
            mostrar_contrasena(&resultado);
        }
    }

    Ok(())
}

fn generar_aleatoria(rng: &mut impl Rng, longitud: u32, sin_ambiguos: bool) -> Contrasena {
    let chars_lower = if sin_ambiguos {
        CHARS_LOWER
    } else {
        b"abcdefghijklmnopqrstuvwxyz"
    };
    let chars_upper = if sin_ambiguos {
        CHARS_UPPER
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    };
    let chars_digits = if sin_ambiguos {
        CHARS_DIGITS
    } else {
        b"0123456789"
    };

    let mut charset = Vec::new();
    charset.extend_from_slice(chars_lower);
    charset.extend_from_slice(chars_upper);
    charset.extend_from_slice(chars_digits);
    charset.extend_from_slice(CHARS_SIMBOLOS);

    let pass: String = (0..longitud)
        .map(|_| charset[rng.random_range(0..charset.len())] as char)
        .collect();

    let entropia = (longitud as f64) * (charset.len() as f64).log2();

    Contrasena {
        valor: pass,
        entropia,
        tipo: "aleatoria".into(),
    }
}

fn generar_diceware(rng: &mut impl Rng, num_palabras: u32) -> Contrasena {
    let palabras: Vec<&str> = (0..num_palabras)
        .map(|_| DICEWARE_PALABRAS[rng.random_range(0..DICEWARE_PALABRAS.len())])
        .collect();

    let pass = palabras.join("-");
    let entropia = (num_palabras as f64) * (DICEWARE_PALABRAS.len() as f64).log2();

    Contrasena {
        valor: pass,
        entropia,
        tipo: "diceware".into(),
    }
}

fn mostrar_contrasena(c: &Contrasena) {
    if !crate::util::es_tty() {
        println!(
            "{}  ({:.1} bits de entropía, {})",
            c.valor, c.entropia, c.tipo
        );
        return;
    }
    println!(
        "{}  ({} bits de entropía, {})",
        c.valor.green().bold(),
        format!("{:.1}", c.entropia).yellow(),
        c.tipo.dimmed()
    );
}
