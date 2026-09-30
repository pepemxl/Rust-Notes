// Compila cada tests/ui/*.rs, exige que falle y compara el error con su .stderr.
// Para regenerar los .stderr tras cambiar un mensaje: TRYBUILD=overwrite cargo test
#[test]
fn errores_de_compilacion() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
