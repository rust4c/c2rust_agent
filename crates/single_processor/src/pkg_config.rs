use config::{Config, File};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessorConfig {
    pub max_retry_attempts: u32,
}

pub fn get_config() -> Result<ProcessorConfig, config::ConfigError> {
    let config_path = locate_config_file()?;
    let config: ProcessorConfig = build_config(&config_path)?.try_deserialize()?;
    Ok(config)
}

/// Ordered search locations for `config.toml` relative to the running binary.
const CONFIG_SEARCH_PATHS: &[&str] = &[
    "config/config.toml",
    "../config/config.toml",
    "../../config/config.toml",
];

/// Locate the first existing config file among the supported search locations.
fn locate_config_file() -> Result<PathBuf, config::ConfigError> {
    let mut attempted = Vec::new();

    for raw_path in CONFIG_SEARCH_PATHS {
        let candidate = Path::new(raw_path);
        if candidate.exists() {
            return Ok(candidate.to_path_buf());
        }
        attempted.push(candidate.display().to_string());
    }

    Err(config::ConfigError::NotFound(format!(
        "config file not found. searched paths: [{}]",
        attempted.join(", ")
    )))
}

/// Build the runtime configuration from a resolved config file path.
fn build_config(path: &Path) -> Result<Config, config::ConfigError> {
    Config::builder()
        .add_source(File::from(path.to_path_buf()))
        .build()
}
