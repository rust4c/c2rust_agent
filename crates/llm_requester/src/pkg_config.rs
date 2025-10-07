use anyhow::Result;
use config::{Config, File};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaConfig {
    pub model: String,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIConfig {
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct XAIConfig {
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeepSeekConfig {
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LLMProviders {
    pub ollama: OllamaConfig,
    pub openai: OpenAIConfig,
    pub xai: XAIConfig,
    pub deepseek: DeepSeekConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkingConfig {
    pub enabled: Option<bool>,
    pub max_tokens: Option<usize>,
    pub chunk_overlap: Option<usize>,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            max_tokens: Some(120000),
            chunk_overlap: Some(100),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LLMConfig {
    pub provider: String,
    pub llm: LLMProviders,
    pub chunking: Option<ChunkingConfig>,
}

pub fn get_config() -> Result<LLMConfig, config::ConfigError> {
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
    let config: LLMConfig = config.try_deserialize()?;

    match config.provider.as_str() {
        "ollama" => Ok(config),
        "openai" => Ok(config),
        "xai" => Ok(config),
        "deepseek" => Ok(config),
        _ => Err(config::ConfigError::NotFound(format!(
            "Unsupported provider '{}'. Supported providers: ollama, openai, xai, deepseek",
            config.provider
        ))),
    }
}
