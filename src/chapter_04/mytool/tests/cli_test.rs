use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn cmd() -> Command {
    Command::cargo_bin("mytool").unwrap()
}

#[test]
fn sin_argumentos_muestra_ayuda() {
    cmd()
        .assert()
        .failure() // arg_required_else_help = true → exit code 2
        .stderr(predicate::str::contains("Usage")); // los textos de clap están en inglés
}

#[test]
fn hash_blake3_archivo_conocido() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"hello world\n").unwrap();
    let ruta = f.path().to_str().unwrap();

    cmd()
        .args(["hash", ruta])
        .assert()
        .success()
        .stdout(predicate::str::contains("blake3").not()) // solo_hash = false
        .stdout(predicate::str::is_match(r"[0-9a-f]{64}").unwrap()); // hex 64 chars
}

#[test]
fn hash_salida_json() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(b"test").unwrap();
    let ruta = f.path().to_str().unwrap();

    let salida = cmd()
        .args(["--json", "hash", ruta])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&salida).unwrap();
    assert_eq!(json["algo"], "blake3");
    assert!(json["hash"].as_str().unwrap().len() == 64);
}

#[test]
fn hash_archivo_inexistente_falla_con_mensaje() {
    cmd()
        .args(["hash", "/no/existe/archivo.txt"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no se pudo"));
}

#[test]
fn genpass_longitud_valida() {
    cmd()
        .args(["genpass", "--longitud", "32"])
        .assert()
        .success()
        .stdout(predicate::function(|output: &[u8]| {
            let s = std::str::from_utf8(output).unwrap();
            // La contraseña debe tener 32 caracteres antes del espacio
            s.split_whitespace()
                .next()
                .map(|p| p.len() == 32)
                .unwrap_or(false)
        }));
}

#[test]
fn genpass_longitud_invalida_rechazada() {
    cmd()
        .args(["genpass", "--longitud", "3"]) // menor que 8
        .assert()
        .failure()
        .stderr(predicate::str::contains("8")); // mensaje menciona el mínimo
}

#[test]
fn genpass_diceware_produce_palabras_separadas_por_guion() {
    cmd()
        .args(["genpass", "--diceware", "--palabras", "4"])
        .assert()
        .success()
        .stdout(predicate::function(|output: &[u8]| {
            let s = std::str::from_utf8(output).unwrap();
            let passphrase = s.split_whitespace().next().unwrap_or("");
            passphrase.matches('-').count() == 3 // 4 palabras → 3 guiones
        }));
}

#[test]
fn version_flag() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}
