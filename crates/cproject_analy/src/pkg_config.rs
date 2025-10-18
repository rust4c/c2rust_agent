use config::{Config, File};
use module_installer::{MirrorSource, default_uv_mirrors};
use serde::{Deserialize, Serialize};

/// 预处理配置（集中定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UvMirrorConfig {
    pub name: String,
    #[serde(default)]
    pub index_url: Option<String>,
    #[serde(default)]
    pub extra_index_url: Option<String>,
}

impl From<MirrorSource> for UvMirrorConfig {
    fn from(source: MirrorSource) -> Self {
        Self {
            name: source.name,
            index_url: source.index_url,
            extra_index_url: source.extra_index_url,
        }
    }
}

impl From<&UvMirrorConfig> for MirrorSource {
    fn from(config: &UvMirrorConfig) -> Self {
        MirrorSource::new(
            config.name.clone(),
            config.index_url.clone(),
            config.extra_index_url.clone(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessConfig {
    /// Number of parallel workers (0=auto detect)
    pub worker_count: usize,
    /// File pairing rules (source file pattern, header file pattern)
    pub pairing_rules: Vec<(String, String)>,
    /// Exclude file patterns
    pub exclude_patterns: Vec<String>,
    /// Header file extensions
    pub header_extensions: Vec<String>,
    /// Source file extensions
    pub source_extensions: Vec<String>,
    /// Large file threshold (bytes)
    pub large_file_threshold: u64,
    /// Chunk size (bytes)
    pub chunk_size: usize,
    /// uv executable path
    #[serde(default = "default_uv_command")]
    pub uv_command: String,
    /// uv virtual environment path (relative path based on output directory)
    #[serde(default)]
    pub uv_venv_path: Option<String>,
    /// uv installation source configuration
    #[serde(default = "default_uv_mirrors_config")]
    pub uv_mirrors: Vec<UvMirrorConfig>,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            worker_count: 0,
            pairing_rules: vec![
                ("(.+)\\.c".to_string(), "\\1.h".to_string()),
                ("(.+)\\.cc".to_string(), "\\1.h".to_string()),
                ("(.+)\\.cpp".to_string(), "\\1.h".to_string()),
                ("(.+)\\.cpp".to_string(), "\\1.hpp".to_string()),
                ("(.+)\\.cxx".to_string(), "\\1.h".to_string()),
                ("(.+)\\.c\\+\\+".to_string(), "\\1.h".to_string()),
                ("src/(.+)\\.c".to_string(), "include/\\1.h".to_string()),
                ("src/(.+)\\.cpp".to_string(), "include/\\1.h".to_string()),
                ("src/(.+)\\.cpp".to_string(), "include/\\1.hpp".to_string()),
            ],
            exclude_patterns: vec![
                "*.bak".to_string(),
                "*.tmp".to_string(),
                ".git/*".to_string(),
                ".svn/*".to_string(),
                "*.o".to_string(),
                "*.obj".to_string(),
                "*.exe".to_string(),
                "*.dll".to_string(),
                "*.so".to_string(),
            ],
            header_extensions: vec![
                ".h".to_string(),
                ".hpp".to_string(),
                ".hh".to_string(),
                ".hxx".to_string(),
            ],
            source_extensions: vec![
                ".c".to_string(),
                ".cc".to_string(),
                ".cpp".to_string(),
                ".cxx".to_string(),
                ".c++".to_string(),
            ],
            large_file_threshold: 1024 * 1024,
            chunk_size: 1024 * 1024,
            uv_command: default_uv_command(),
            uv_venv_path: Some(".compiledb-venv".to_string()),
            uv_mirrors: default_uv_mirrors_config(),
        }
    }
}

impl PreprocessConfig {
    pub fn uv_mirror_sources(&self) -> Vec<MirrorSource> {
        if self.uv_mirrors.is_empty() {
            default_uv_mirrors()
        } else {
            self.uv_mirrors.iter().map(MirrorSource::from).collect()
        }
    }
}

fn default_uv_command() -> String {
    "uv".to_string()
}

fn default_uv_mirrors_config() -> Vec<UvMirrorConfig> {
    default_uv_mirrors()
        .into_iter()
        .map(UvMirrorConfig::from)
        .collect()
}

/// Preprocessor top-level configuration (centralized definition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorConfig {
    /// Database configuration (reserved placeholder)
    pub database_url: Option<String>,
    /// Qdrant configuration (reserved placeholder)
    pub qdrant_url: Option<String>,
    /// Number of worker threads
    pub worker_count: usize,
    /// Project preprocessing configuration
    pub preprocess_config: Option<PreprocessConfig>,
}

impl Default for PreprocessorConfig {
    fn default() -> Self {
        Self {
            database_url: None,
            qdrant_url: None,
            worker_count: 0,
            preprocess_config: Some(PreprocessConfig::default()),
        }
    }
}

/// NOTE: This module centrally defines PreprocessConfig/PreprocessorConfig,
/// and maps [cproject.preprocess] in the configuration file to this module's PreprocessConfig.
///
/// Configuration file example (please sync this snippet to config/config.default.toml):
///
/// [cproject.preprocess]
/// # 0 means auto-select (keep as 0 for runtime processing)
/// worker_count = 0
///
/// # File pairing rules (multiple sections)
/// [[cproject.preprocess.pairing_rules]]
/// source = "(.+)\\.c"
/// header = "\\1.h"
///
/// [[cproject.preprocess.pairing_rules]]
/// source = "(.+)\\.cpp"
/// header = "\\1.h"
///
/// [[cproject.preprocess.pairing_rules]]
/// source = "src/(.+)\\.c"
/// header = "include/\\1.h"
///
/// # Exclude patterns
/// cproject.preprocess.exclude_patterns = [ "*.bak", "*.tmp", ".git/*", ".svn/*", "*.o", "*.obj", "*.exe", "*.dll", "*.so" ]
///
/// # Extensions
/// cproject.preprocess.header_extensions = [ ".h", ".hpp", ".hh", ".hxx" ]
/// cproject.preprocess.source_extensions = [ ".c", ".cc", ".cpp", ".cxx", ".c++" ]
///
/// # Large file threshold/chunk size
/// cproject.preprocess.large_file_threshold = 52428800     # 50MB
/// cproject.preprocess.chunk_size = 8388608               # 8MB
///
/// Description:
/// - If [cproject.preprocess] is missing, PreprocessConfig::default() will be returned
/// - Only existing fields are overridden, others use default values

pub fn get_config() -> Result<PreprocessorConfig, config::ConfigError> {
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

    let cfg = config_builder.build()?;
    let config: PreprocessorConfig = cfg.try_deserialize()?;

    Ok(config)
}
