use config;
use config::{Config, File};
use serde::Deserialize;

#[derive(Deserialize, Default, Clone)]
pub struct MainProcessorConfig {
    pub max_retry_attempts: usize,
    pub concurrent_limit: usize,
}

impl MainProcessorConfig {
    // Methods for initializing and configuring the MainProcessorConfig
}

pub fn get_config() -> Result<MainProcessorConfig, config::ConfigError> {
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
    let config: MainProcessorConfig = config.try_deserialize()?;

    Ok(config)
}
