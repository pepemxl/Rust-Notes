use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;

#[derive(Parser)]
#[grammar = "grammar/config.pest"]
pub struct ConfigParser;

#[derive(Debug)]
pub struct Config {
    pub secciones: HashMap<String, HashMap<String, String>>,
}

pub fn parsear_config(input: &str) -> Result<Config, pest::error::Error<Rule>> {
    let archivo = ConfigParser::parse(Rule::archivo, input)?.next().unwrap();

    let mut config = Config {
        secciones: HashMap::new(),
    };
    for item in archivo.into_inner() {
        match item.as_rule() {
            Rule::seccion => {
                let mut inner = item.into_inner();
                // primer hijo: encabezado
                let enc = inner.next().unwrap();
                let seccion_actual = enc.into_inner().next().unwrap().as_str().to_string();
                config.secciones.entry(seccion_actual.clone()).or_default();

                // resto: pares clave=valor
                for par in inner {
                    if par.as_rule() == Rule::par {
                        let mut kv = par.into_inner();
                        let clave = kv.next().unwrap().as_str().to_string();
                        let val_nodo = kv.next().unwrap().into_inner().next().unwrap();
                        let valor = match val_nodo.as_rule() {
                            Rule::cadena => {
                                let s = val_nodo.as_str();
                                s[1..s.len() - 1].to_string() // quitar comillas
                            }
                            _ => val_nodo.as_str().to_string(),
                        };
                        config
                            .secciones
                            .get_mut(&seccion_actual)
                            .unwrap()
                            .insert(clave, valor);
                    }
                }
            }
            // NEWLINE, ANY, SOI... son reglas predefinidas de pest: no tienen
            // variante en `Rule`, así que no se pueden nombrar aquí.
            Rule::EOI => {}
            _ => {}
        }
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_secciones_y_valores() {
        let texto = "[servidor]\npuerto = 8080\nhost = \"localhost\"\n\n[log]\nactivo = true\n";
        let config = parsear_config(texto).unwrap();
        assert_eq!(config.secciones["servidor"]["puerto"], "8080");
        assert_eq!(config.secciones["servidor"]["host"], "localhost"); // sin comillas
        assert_eq!(config.secciones["log"]["activo"], "true");
    }

    #[test]
    fn error_senala_linea_y_columna() {
        let texto_malo = "[seccion]\nclave = @invalido";
        let error = parsear_config(texto_malo).unwrap_err().to_string();
        println!("{error}");
        assert!(error.contains("2:9"), "{error}");
    }
}
