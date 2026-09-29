/// Devuelve el texto de FizzBuzz para `n`.
fn fizzbuzz(n: u32) -> String {
    // `if` es una expresión: cada rama produce el String que devuelve la función
    if n % 15 == 0 {
        String::from("FizzBuzz")
    } else if n % 3 == 0 {
        String::from("Fizz")
    } else if n % 5 == 0 {
        String::from("Buzz")
    } else {
        n.to_string()
    }
}

fn main() {
    for n in 1..=100 {
        println!("{}", fizzbuzz(n));
    }
}

#[cfg(test)]
mod tests {
    use super::fizzbuzz;

    #[test]
    fn multiplos() {
        assert_eq!(fizzbuzz(3), "Fizz");
        assert_eq!(fizzbuzz(5), "Buzz");
        assert_eq!(fizzbuzz(15), "FizzBuzz");
        assert_eq!(fizzbuzz(7), "7");
    }
}
