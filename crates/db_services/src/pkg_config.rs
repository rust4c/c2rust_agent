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

pub fn get_config() -> Result<DBConfig, config::ConfigError> {
    // Try multiple possible paths for the config file
    let possible_paths = [
        "config/config.toml",       // From project root
        "../config/config.toml",    // From crates subdirectory
        "../../config/config.toml", // From deeper nested directories
    ];

    let mut config_builder = Config::builder();
    let mut found_config = false;

    for path in &possible_paths {
        if std::path::Path::new(path).exists() {
            config_builder = config_builder.add_source(File::with_name(path));
            found_config = true;
            break;
        }
    }

    if !found_config {
        return Err(config::ConfigError::NotFound(
            "config.toml not found in any expected location".to_string(),
        ));
    }

    let config = config_builder.build()?;
    let config: DBConfig = config.try_deserialize()?;

    Ok(config)
}
