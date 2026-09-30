use crate::parser::nginx::EntradaNginx;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Estadisticas {
    pub total: u64,
    pub errores_4xx: u64,
    pub errores_5xx: u64,
    pub bytes_total: u64,
    pub por_estado: HashMap<u16, u64>,
    pub por_metodo: HashMap<String, u64>,
    pub top_rutas: HashMap<String, u64>,
    pub top_ips: HashMap<String, u64>,
}

impl Estadisticas {
    pub fn registrar(&mut self, e: &EntradaNginx) {
        self.total += 1;
        if e.estado >= 400 && e.estado < 500 {
            self.errores_4xx += 1;
        }
        if e.estado >= 500 {
            self.errores_5xx += 1;
        }
        self.bytes_total += e.bytes.unwrap_or(0);

        *self.por_estado.entry(e.estado).or_default() += 1;
        *self.por_metodo.entry(e.metodo.clone()).or_default() += 1;
        *self.top_rutas.entry(e.ruta.clone()).or_default() += 1;
        *self.top_ips.entry(e.ip.clone()).or_default() += 1;
    }

    pub fn top_n(mapa: &HashMap<String, u64>, n: usize) -> Vec<(&str, u64)> {
        let mut pares: Vec<_> = mapa.iter().map(|(k, &v)| (k.as_str(), v)).collect();
        pares.sort_unstable_by_key(|&(_, n)| std::cmp::Reverse(n)); // de mayor a menor
        pares.truncate(n);
        pares
    }
}
