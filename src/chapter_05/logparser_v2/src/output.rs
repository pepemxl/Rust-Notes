use serde::Serialize;
use std::io::{self, BufWriter, Write};

// ANTES: un String temporal por elemento y un write() por línea.
// Con stdout o un File sin buffer, cada writeln! es al menos una syscall.
pub fn escribir_json_antes<T: Serialize>(items: &[T], mut out: impl Write) -> io::Result<()> {
    for item in items {
        let json = serde_json::to_string(item)?;
        writeln!(out, "{json}")?;
    }
    Ok(())
}

// DESPUÉS: to_writer serializa directo al buffer (sin String temporal) y
// BufWriter solo llama a write() cuando junta 256 KiB.
pub fn escribir_json_despues<T: Serialize>(items: &[T], out: impl Write) -> io::Result<()> {
    let mut out = BufWriter::with_capacity(256 * 1024, out);
    for item in items {
        serde_json::to_writer(&mut out, item)?;
        out.write_all(b"\n")?;
    }
    out.flush() // sin flush, un error de escritura al soltar el BufWriter se perdería
}
