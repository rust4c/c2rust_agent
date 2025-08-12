use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct qdrant_config {
    host: String,
    port: Option<u16>,
    collection_name: String,
    vector_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct sqlite_config {
    path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    qdrant: qdrant_config,
    sqlite: sqlite_config,
}

pub fn get_config() -> Result<AppConfig, config::ConfigError> {
    let config = Config::builder()
        .add_source(File::with_name("config.toml"))
        .build()?;
    let config = config.try_deserialize()?;
    Ok(config)
}
