use config::{Config, File};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct provider_config {
    pub provider: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub provider_config: provider_config,
}

pub fn get_config() -> Result<AppConfig, config::ConfigError> {
    let config = Config::builder()
        .add_source(File::with_name("config.toml"))
        .build()?;
    let config = config.try_deserialize()?;
    Ok(config)
}
