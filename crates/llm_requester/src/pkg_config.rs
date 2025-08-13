use config::{Config, File};
use log::warn;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct provider_config {
    pub provider: LLMProvider,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub provider_config: provider_config,
}

#[derive(Debug, Clone, Deserialize)]
pub enum LLMProvider {
    Deepseek,
    Gemini,
    Mistral,
    Ollama,
    OpenAI,
    Anthropic,
}

pub fn get_config() -> Result<AppConfig, config::ConfigError> {
    let config = Config::builder()
        .add_source(File::with_name("config.toml"))
        .build()?;
    let config: AppConfig = config.try_deserialize()?;
    Ok(config)
}
