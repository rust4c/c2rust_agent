use config::{Config, File};
use serde::Deserialize;
use std::path::{Path, PathBuf};

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
    let mut found_config_path: Option<PathBuf> = None;

    for path in &possible_paths {
        if std::path::Path::new(path).exists() {
            config_builder = config_builder.add_source(File::with_name(path));
            found_config = true;
            found_config_path = Some(PathBuf::from(path));
            break;
        }
    }

    if !found_config {
        return Err(config::ConfigError::NotFound(
            "config.toml not found in any expected location".to_string(),
        ));
    }

    let cfg = config_builder.build()?;
    let mut config: DBConfig = cfg.try_deserialize()?;

    // Normalize sqlite.path to an absolute path relative to the project root (parent of config dir)
    if let Some(cfg_path) = found_config_path {
        // Canonicalize to absolute when possible
        let cfg_abs = if cfg_path.is_absolute() {
            cfg_path
        } else {
            // Make absolute relative to current dir
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(cfg_path)
        };

        let config_dir = cfg_abs.parent().unwrap_or(Path::new("."));
        // Project root presumed as parent of config dir
        let project_root = config_dir.parent().unwrap_or(config_dir);

        let sqlite_path = PathBuf::from(&config.sqlite.path);
        if sqlite_path.is_relative() {
            let abs_sqlite = project_root.join(sqlite_path);
            config.sqlite.path = abs_sqlite
                .to_string_lossy()
                .to_string();
        }
    }

    Ok(config)
}
