use builder::Builder;

#[derive(Builder)]
struct Malo {
    #[builder(each = "etiqueta")]
    etiquetas: String,
}

fn main() {}
