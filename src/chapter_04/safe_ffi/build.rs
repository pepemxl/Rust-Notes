fn main() {
    println!("cargo:rerun-if-changed=csrc/buffer.h");
    println!("cargo:rerun-if-changed=csrc/buffer.c");

    cc::Build::new()
        .file("csrc/buffer.c")
        .include("csrc")
        .flag_if_supported("-O2")
        .compile("buffer");
}
