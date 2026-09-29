fn main() {
    // 1. Horas, minutos y segundos con división entera y residuo
    let total_segundos: u32 = 100_000;
    let horas = total_segundos / 3600;
    let minutos = total_segundos % 3600 / 60;
    let segundos = total_segundos % 60;
    println!("{total_segundos} s = {horas}:{minutos:02}:{segundos:02}");

    // 2. Celsius → Fahrenheit: todos los literales son flotantes (9.0, 5.0, 32.0)
    let celsius = 36.6;
    let fahrenheit = celsius * 9.0 / 5.0 + 32.0;
    println!("{celsius} °C = {fahrenheit:.1} °F");

    // 3. Tres formas de manejar el overflow de u8 (200 + 100 = 300 > 255)
    let (a, b) = (200_u8, 100_u8);
    println!(
        "checked: {:?} | wrapping: {} | saturating: {}",
        a.checked_add(b),
        a.wrapping_add(b),
        a.saturating_add(b)
    );
}
