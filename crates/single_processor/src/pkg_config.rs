use config::{Config, File};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessorConfig {
    pub max_retries: u32,
    pub concurrency: usize,
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
    CONFIG_SEARCH_PATHS
        .iter()
        .map(Path::new)
        .find(|candidate| candidate.exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            config::ConfigError::NotFound(
                "config.toml not found in any expected location".to_string(),
            )
        })
}

/// Build the runtime configuration from a resolved config file path.
fn build_config(path: &Path) -> Result<Config, config::ConfigError> {
    let path_str = path.to_string_lossy();
    Config::builder()
        .add_source(File::with_name(path_str.as_ref()))
        .build()
}
