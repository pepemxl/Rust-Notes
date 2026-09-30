use ahash::AHashMap;
use lasso::{Rodeo, Spur};
use std::collections::HashMap;

// ANTES: HashMap<String, u64> con SipHash; cada clave nueva es un String propio
#[derive(Default)]
pub struct EstadisticasAntes {
    pub por_ruta: HashMap<String, u64>,
    pub por_ip: HashMap<String, u64>,
    pub por_estado: HashMap<u16, u64>,
}

impl EstadisticasAntes {
    pub fn registrar(&mut self, ip: String, ruta: String, estado: u16) {
        *self.por_ip.entry(ip).or_default() += 1;
        *self.por_ruta.entry(ruta).or_default() += 1;
        *self.por_estado.entry(estado).or_default() += 1;
    }

    pub fn top_rutas(&self, n: usize) -> Vec<(&str, u64)> {
        top(self.por_ruta.iter().map(|(r, &c)| (r.as_str(), c)), n)
    }
}

// DESPUÉS: ahash como hasher y lasso para guardar cada string una sola vez.
// Un Spur es un u32: se copia y se hashea sin recorrer el texto.
#[derive(Default)]
pub struct EstadisticasDespues {
    pub por_ruta: AHashMap<Spur, u64>,
    pub por_ip: AHashMap<Spur, u64>,
    pub por_estado: AHashMap<u16, u64>,
    pub interner: Rodeo, // dueño de los strings únicos
}

impl EstadisticasDespues {
    pub fn registrar(&mut self, ip: &str, ruta: &str, estado: u16) {
        // get_or_intern solo alloca la PRIMERA vez que ve un string
        let spur_ip = self.interner.get_or_intern(ip);
        let spur_ruta = self.interner.get_or_intern(ruta);

        *self.por_ip.entry(spur_ip).or_default() += 1;
        *self.por_ruta.entry(spur_ruta).or_default() += 1;
        *self.por_estado.entry(estado).or_default() += 1;
    }

    pub fn top_rutas(&self, n: usize) -> Vec<(&str, u64)> {
        let rutas = self
            .por_ruta
            .iter()
            .map(|(spur, &c)| (self.interner.resolve(spur), c));
        top(rutas, n)
    }
}

/// Las `n` entradas con más conteo; empates por orden alfabético (salida estable).
fn top<'a>(conteos: impl Iterator<Item = (&'a str, u64)>, n: usize) -> Vec<(&'a str, u64)> {
    let mut v: Vec<_> = conteos.collect();
    v.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v.truncate(n);
    v
}
