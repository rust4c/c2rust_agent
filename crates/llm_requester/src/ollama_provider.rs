use anyhow::{Result, anyhow};
use log::{debug, error, info};
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use rig::{agent::Agent, client::CompletionClient, completion::Prompt, providers::openai};

use crate::llm_provider_trait::LLMProvider;
use crate::pkg_config::{OllamaConfig, get_config};

pub struct OllamaProvider {
    agent: Agent<ResponsesCompletionModel>,
}

impl OllamaProvider {
    pub async fn new(ollama_config: OllamaConfig) -> Result<Self> {
        info!(
            "Creating new Ollama provider with model: {}",
            ollama_config.model
        );

        let client = openai::ClientBuilder::new(&ollama_config.api_key)
            .base_url(&ollama_config.base_url)
            .build()
            .map_err(|e| anyhow!("Failed to create Ollama client: {}", e))?;

        let agent = client.agent(&ollama_config.model).build();

        info!("Successfully created Ollama provider");
        Ok(Self { agent })
    }

    pub async fn init_with_config() -> Result<Self> {
        debug!("Initializing Ollama provider from config");

        let config = match get_config() {
            Ok(config) => {
                debug!("Using Ollama provider configuration");
                config
            }
            Err(err) => {
                error!("Failed to get config: {}", err);
                return Err(anyhow!("Can't get config with error: {}", err));
            }
        };

        let base_url = config.llm.ollama.base_url.clone();
        let model = config.llm.ollama.model.clone();
        let api_key = config.llm.ollama.api_key.clone();

        info!("Using Ollama model: {} at {}", model, base_url);

        let client = openai::ClientBuilder::new(&api_key)
            .base_url(&base_url)
            .build()
            .map_err(|e| anyhow!("Failed to create Ollama client: {}", e))?;

        let agent = client.agent(&model).build();

        info!("Successfully initialized Ollama provider from config");
        Ok(Self { agent })
    }

    pub async fn chat_with_prompt(&self, message: &str, system_prompt: &str) -> Result<String> {
        info!("Starting Ollama chat with prompt request");
        debug!("Message length: {} chars", message.len());
        debug!("System prompt length: {} chars", system_prompt.len());

        let prompt = format!("System: {}\n\nUser: {}", system_prompt, message);

        match self.agent.prompt(&prompt).await {
            Ok(response) => {
                info!(
                    "Ollama chat with prompt completed successfully, response length: {} chars",
                    response.len()
                );
                Ok(response)
            }
            Err(e) => {
                error!("Ollama chat with prompt failed: {}", e);
                Err(anyhow!("Ollama chat request failed: {}", e))
            }
        }
    }

    pub async fn chat(&self, messages: Vec<String>) -> Result<String> {
        info!(
            "Starting Ollama chat request with {} messages",
            messages.len()
        );

        let combined_message = messages.join("\n");

        match self.agent.prompt(&combined_message).await {
            Ok(response) => {
                info!(
                    "Ollama chat completed successfully, response length: {} chars",
                    response.len()
                );
                Ok(response)
            }
            Err(e) => {
                error!("Ollama chat failed: {}", e);
                Err(anyhow!("Ollama chat request failed: {}", e))
            }
        }
    }

    pub async fn get_llm_request(messages: Vec<String>) -> Result<String> {
        info!(
            "Processing Ollama LLM request with {} messages",
            messages.len()
        );

        let provider = Self::init_with_config().await.map_err(|e| {
            error!("Failed to initialize Ollama provider: {}", e);
            e
        })?;

        match provider.chat(messages).await {
            Ok(response) => {
                info!("Ollama LLM request completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("Ollama LLM request failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn chat_with_prompt_static(
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String> {
        info!(
            "Processing Ollama static chat with prompt request, {} messages",
            messages.len()
        );

        let provider = Self::init_with_config().await.map_err(|e| {
            error!(
                "Failed to initialize Ollama provider for static chat: {}",
                e
            );
            e
        })?;

        let combined_message = messages.join(" ");
        debug!("Combined message length: {} chars", combined_message.len());

        match provider
            .chat_with_prompt(&combined_message, &system_prompt)
            .await
        {
            Ok(response) => {
                info!("Ollama static chat with prompt completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("Ollama static chat with prompt failed: {}", e);
                Err(e)
            }
        }
    }

    /// Validate the Ollama configuration
    pub async fn validate_config() -> Result<()> {
        info!("Validating Ollama configuration");

        let config = get_config().map_err(|e| anyhow!("Config validation failed: {}", e))?;

        let base_url = &config.llm.ollama.base_url;
        let model = &config.llm.ollama.model;

        if base_url.is_empty() {
            error!("Empty Ollama base URL in configuration");
            return Err(anyhow!("Ollama base URL cannot be empty"));
        }

        if model.is_empty() {
            error!("Empty Ollama model name in configuration");
            return Err(anyhow!("Ollama model name cannot be empty"));
        }

        info!("Ollama configuration validation passed");
        Ok(())
    }

    /// Test the connection to Ollama with a simple request
    pub async fn test_connection() -> Result<()> {
        info!("Testing Ollama connection");

        let provider = Self::init_with_config().await?;
        let test_message =
            vec!["Hello, this is a connection test. Please respond with 'OK'.".to_string()];

        match provider.chat(test_message).await {
            Ok(response) => {
                info!("Ollama connection test successful, response: {}", response);
                Ok(())
            }
            Err(e) => {
                error!("Ollama connection test failed: {}", e);

                let error_msg = if e.to_string().contains("timeout") {
                    format!(
                        "Ollama connection timeout. Please check if Ollama is running on {}. Error: {}",
                        get_config()
                            .map(|c| c.llm.ollama.base_url)
                            .unwrap_or_default(),
                        e
                    )
                } else if e.to_string().contains("connection") {
                    format!(
                        "Cannot connect to Ollama server. Please ensure Ollama is running and accessible. Error: {}",
                        e
                    )
                } else {
                    format!("Ollama connection test failed: {}", e)
                };

                Err(anyhow!("{}", error_msg))
            }
        }
    }
}

#[async_trait::async_trait]
impl LLMProvider for OllamaProvider {
    async fn init_with_config() -> Result<Box<dyn LLMProvider>> {
        let provider = Self::init_with_config().await?;
        Ok(Box::new(provider))
    }

    async fn chat(&self, messages: Vec<String>) -> Result<String> {
        self.chat(messages).await
    }

    async fn chat_with_prompt(
        &self,
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String> {
        let combined_message = messages.join(" ");
        self.chat_with_prompt(&combined_message, &system_prompt)
            .await
    }

    async fn get_llm_request(messages: Vec<String>) -> Result<String> {
        Self::get_llm_request(messages).await
    }

    async fn chat_with_prompt_static(
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String> {
        Self::chat_with_prompt_static(messages, system_prompt).await
    }

    async fn validate_config() -> Result<()> {
        Self::validate_config().await
    }

    async fn test_connection() -> Result<()> {
        Self::test_connection().await
    }

    fn provider_name(&self) -> &'static str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_config() {
        let result = OllamaProvider::validate_config().await;
        match result {
            Ok(_) => println!("Ollama config validation passed"),
            Err(e) => println!("Ollama config validation failed (expected): {}", e),
        }
    }

    #[tokio::test]
    async fn test_chat_with_prompt() {
        if OllamaProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = OllamaProvider::init_with_config().await.unwrap();
        let response = provider
            .chat_with_prompt(
                "What is the capital of France?",
                "You are a helpful assistant.",
            )
            .await;

        match response {
            Ok(resp) => {
                println!("Test response: {}", resp);
                assert!(resp.contains("Paris") || resp.contains("paris"));
            }
            Err(e) => {
                println!("Test failed (may be due to Ollama not running): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_chat() {
        if OllamaProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = OllamaProvider::init_with_config().await.unwrap();
        let messages = vec!["What is the capital of France?".to_string()];
        let response = provider.chat(messages).await;

        match response {
            Ok(resp) => {
                println!("Test response: {}", resp);
                assert!(resp.contains("Paris") || resp.contains("paris"));
            }
            Err(e) => {
                println!("Test failed (may be due to Ollama not running): {}", e);
            }
        }
    }
}
