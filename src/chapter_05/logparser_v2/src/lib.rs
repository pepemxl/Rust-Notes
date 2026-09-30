//! Log Parser v2: cada optimización convive con su versión "antes" para poder
//! compararlas con criterion y comprobar con tests que dan el mismo resultado.

pub mod aggregate;
pub mod output;
pub mod parser;

/// `n` líneas de log nginx sintéticas pero realistas: 250 IPs, 40 rutas
/// (1 de cada 8 con %-encoding) y un 10 % de errores 500.
pub fn lineas_de_prueba(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let ruta = if i % 8 == 0 {
                format!("/buscar?q=rust%20async%20{}", i % 5)
            } else {
                format!("/api/v1/items/{}", i % 35)
            };
            format!(
                r#"10.0.{}.{} - - [01/Jan/2024:00:00:{:02} +0000] "GET {ruta} HTTP/1.1" {} {} "-" "curl/8.5.0""#,
                (i / 50) % 5,
                i % 50,
                i % 60,
                if i % 10 == 0 { 500 } else { 200 },
                100 + i % 900,
            )
        })
        .collect()
}
