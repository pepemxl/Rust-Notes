// --8<-- [start:solucion]
fn split_manual<'a>(input: &'a str, delimiter: char) -> Vec<&'a str> {
    let mut result = Vec::new();
    let mut start = 0;
    for (i, c) in input.char_indices() {
        if c == delimiter {
            result.push(&input[start..i]);
            start = i + delimiter.len_utf8(); // avanzamos en bytes, no en chars
        }
    }
    result.push(&input[start..]); // último trozo
    result
}

fn main() {
    let partes = split_manual("hola,mundo,rust", ',');
    for p in &partes {
        println!("{p}");
    }
}
// --8<-- [end:solucion]

// --8<-- [start:tests]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basico() {
        assert_eq!(split_manual("a,b,c", ','), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_espacios() {
        assert_eq!(split_manual("hola mundo", ' '), vec!["hola", "mundo"]);
    }

    #[test]
    fn test_delimitador_en_medio() {
        assert_eq!(split_manual("texto", 'x'), vec!["te", "to"]);
    }

    #[test]
    fn test_vacio() {
        assert_eq!(split_manual("", ','), vec![""]);
    }

    #[test]
    fn test_delimitador_al_final() {
        assert_eq!(split_manual("a,", ','), vec!["a", ""]);
    }
}
// --8<-- [end:tests]
