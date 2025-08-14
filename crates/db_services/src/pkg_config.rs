use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct qdrant_config {
    pub host: String,
    pub port: Option<u16>,
    pub collection_name: String,
    pub vector_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct sqlite_config {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DBConfig {
    pub qdrant: qdrant_config,
    pub sqlite: sqlite_config,
}

impl DBConfig {
    pub fn new(qdrant_config: qdrant_config, sqlite_config: sqlite_config) -> Self {
        DBConfig {
            qdrant: qdrant_config,
            sqlite: sqlite_config,
        }
    }
    pub fn get_qdrant_config(&self) -> &qdrant_config {
        &self.qdrant
    }
    pub fn get_sqlite_config(&self) -> &sqlite_config {
        &self.sqlite
    }
}

pub fn get_config() -> Result<DBConfig, config::ConfigError> {
    let config = Config::builder()
        .add_source(File::with_name("config.toml"))
        .build()?;
    let config: DBConfig = config.try_deserialize()?;
    Ok(config)
}
