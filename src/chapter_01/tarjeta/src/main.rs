fn main() {
    let nombre = "Ada Lovelace";
    let lenguaje = "Rust";
    let edicion = 2024;
    let horas = 12.5_f64;

    let linea = format!("+{}+", "-".repeat(28));
    println!("{linea}");
    println!("|{nombre:^28}|"); // centrado en 28 columnas
    println!("{linea}");
    println!("| {:<18}{:>8} |", "Lenguaje:", lenguaje); // 18 a la izquierda + 8 a la derecha
    println!("| {:<18}{:>8} |", "Edición:", edicion);
    println!("| {:<18}{:>8.1} |", "Horas de estudio:", horas); // .1 → un decimal
    println!("{linea}");
}
