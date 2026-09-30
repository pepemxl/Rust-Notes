use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub base_url: String,
    pub actor_shards: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "0.0.0.0".to_string(),
            port: 8080,
            base_url: "http://localhost:8080".to_string(),
            // Un shard por núcleo: más shards que núcleos solo añade tareas que esperan
            actor_shards: std::thread::available_parallelism().map_or(4, |n| n.get()),
        }
    }
}

impl Config {
    /// Capas, de menor a mayor prioridad: defaults → `config.toml` → variables `APP_*`.
    pub fn cargar() -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file("config.toml")) // si no existe, la capa queda vacía
            .merge(Env::prefixed("APP_")) // APP_PORT=9090, APP_ACTOR_SHARDS=32…
            .extract()
            .map_err(Box::new) // figment::Error ocupa 208 bytes: clippy (result_large_err) pide encajonarlo
    }
}
