use safe_ffi::{Buffer, ErrorBuffer};

fn main() -> Result<(), ErrorBuffer> {
    println!("=== Demo: Buffer seguro sobre C ===\n");

    // Crear buffer de 16 bytes
    let mut buf = Buffer::nuevo(16)?;
    println!("Creado: {buf:?}");

    // Escribir un mensaje
    let mensaje = b"Hola desde Rust!";
    buf.escribir(0, mensaje)?;
    println!("Escrito: {:?}", buf.contenido()?);

    // Cifrar con XOR
    buf.aplicar_xor(0x42)?;
    println!("Cifrado (XOR 0x42): {:?}", buf.contenido()?);

    // Descifrar (XOR es simétrico)
    buf.aplicar_xor(0x42)?;
    let descifrado = buf.contenido()?;
    println!("Descifrado: {:?}", String::from_utf8_lossy(&descifrado));

    // Error controlado: fuera de rango
    match buf.escribir(10, b"texto demasiado largo") {
        Ok(_) => panic!("debería haber fallado"),
        Err(e) => println!("\nError esperado: {e}"),
    }

    // buf se libera aquí (Drop → buf_liberar → memset + free)
    println!("\n✓ Buffer liberado automáticamente");
    Ok(())
}

#[cfg(test)]
mod tests {
    use safe_ffi::Buffer;

    #[test]
    fn crear_y_leer_cero() {
        let buf = Buffer::nuevo(8).unwrap();
        assert_eq!(buf.len(), 8);
        assert_eq!(buf.contenido().unwrap(), vec![0u8; 8]);
    }

    #[test]
    fn escribir_y_leer() {
        let mut buf = Buffer::nuevo(16).unwrap();
        buf.escribir(0, b"test").unwrap();
        assert_eq!(&buf.leer(0, 4).unwrap(), b"test");
        assert_eq!(buf.leer(4, 1).unwrap(), vec![0u8]);
    }

    #[test]
    fn escritura_fuera_de_rango() {
        let mut buf = Buffer::nuevo(4).unwrap();
        let err = buf.escribir(2, b"largo").unwrap_err();
        assert!(matches!(err, safe_ffi::ErrorBuffer::FueraDeRango { .. }));
    }

    #[test]
    fn xor_simetrico() {
        let datos = b"secreto";
        let mut buf = Buffer::nuevo(datos.len()).unwrap();
        buf.escribir(0, datos).unwrap();

        buf.aplicar_xor(0x5A).unwrap();
        let cifrado = buf.contenido().unwrap();
        assert_ne!(cifrado, datos);

        buf.aplicar_xor(0x5A).unwrap();
        assert_eq!(buf.contenido().unwrap(), datos);
    }

    #[test]
    fn limpiar_borra_contenido() {
        let mut buf = Buffer::nuevo(8).unwrap();
        buf.escribir(0, b"datos").unwrap();
        buf.limpiar().unwrap();
        assert_eq!(buf.contenido().unwrap(), vec![0u8; 8]);
    }

    #[test]
    fn drop_no_doble_free() {
        // Simplemente crea y deja que Drop lo limpie.
        // Si hubiera doble-free, el OS o Miri lo detectarían.
        let mut buf = Buffer::nuevo(32).unwrap();
        buf.escribir(0, &[42u8; 32]).unwrap();
        // drop implícito aquí
    }

    #[test]
    fn buffer_vacio_falla() {
        // C permite buf_crear(0) pero es un caso borde
        let buf = Buffer::nuevo(0).unwrap();
        assert_eq!(buf.len(), 0);
        assert!(buf.is_empty());
    }
}
