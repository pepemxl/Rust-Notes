use builder::Builder;

#[derive(Builder)]
struct Malo {
    #[builder(atributo_inventado)]
    campo: String,
}

fn main() {}
