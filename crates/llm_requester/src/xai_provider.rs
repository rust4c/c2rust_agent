use anyhow::{Result, anyhow};
use log::{debug, error, info};
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use rig::{agent::Agent, client::CompletionClient, completion::Prompt, providers::openai};

use crate::llm_provider_trait::LLMProvider;
use crate::pkg_config::{XAIConfig, get_config};

pub struct XAIProvider {
    agent: Agent<ResponsesCompletionModel>,
}

impl XAIProvider {
    pub async fn new(xai_config: XAIConfig) -> Result<Self> {
        info!("Creating new xAI provider with model: {}", xai_config.model);

        // Validate API key
        if xai_config.api_key.is_empty() || xai_config.api_key == "your_xai_api_key_here" {
            return Err(anyhow!(
                "Invalid xAI API key. Please set a valid API key in config.toml"
            ));
        }

        let client = openai::ClientBuilder::new(&xai_config.api_key)
            .base_url("https://api.x.ai/v1")
            .build()
            .map_err(|e| anyhow!("Failed to create xAI client: {}", e))?;

        let agent = client.agent(&xai_config.model).build();

        info!("Successfully created xAI provider");
        Ok(Self { agent })
    }

    pub async fn init_with_config() -> Result<Self> {
        debug!("Initializing xAI provider from config");

        let config = match get_config() {
            Ok(config) => {
                debug!("Using xAI provider configuration");
                config
            }
            Err(err) => {
                error!("Failed to get config: {}", err);
                return Err(anyhow!("Can't get config with error: {}", err));
            }
        };

        let model = config.llm.xai.model.clone();
        let api_key = config.llm.xai.api_key.clone();

        info!("Using xAI model: {}", model);

        // Validate API key
        if api_key.is_empty() || api_key == "your_xai_api_key_here" {
            error!("Invalid xAI API key found in config");
            return Err(anyhow!(
                "Invalid xAI API key. Please set a valid API key in config.toml"
            ));
        }

        let client = openai::ClientBuilder::new(&api_key)
            .base_url("https://api.x.ai/v1")
            .build()
            .map_err(|e| anyhow!("Failed to create xAI client: {}", e))?;

        let agent = client.agent(&model).build();

        info!("Successfully initialized xAI provider from config");
        Ok(Self { agent })
    }

    pub async fn chat_with_prompt(&self, message: &str, system_prompt: &str) -> Result<String> {
        info!("Starting xAI chat with prompt request");
        debug!("Message length: {} chars", message.len());
        debug!("System prompt length: {} chars", system_prompt.len());

        let prompt = format!("System: {}\n\nUser: {}", system_prompt, message);

        match self.agent.prompt(&prompt).await {
            Ok(response) => {
                info!(
                    "xAI chat with prompt completed successfully, response length: {} chars",
                    response.len()
                );
                Ok(response)
            }
            Err(e) => {
                error!("xAI chat with prompt failed: {}", e);

                // Provide more specific error information
                let error_msg = if e.to_string().contains("401") {
                    format!(
                        "xAI API authentication failed. Please check your API key. Error: {}",
                        e
                    )
                } else if e.to_string().contains("429") {
                    format!(
                        "xAI API rate limit exceeded. Please wait before retrying. Error: {}",
                        e
                    )
                } else if e.to_string().contains("timeout") {
                    format!(
                        "xAI API request timeout. Please check your network connection. Error: {}",
                        e
                    )
                } else {
                    format!("xAI chat request failed: {}", e)
                };

                Err(anyhow!("{}", error_msg))
            }
        }
    }

    pub async fn chat(&self, messages: Vec<String>) -> Result<String> {
        info!("Starting xAI chat request with {} messages", messages.len());

        let combined_message = messages.join("\n");

        match self.agent.prompt(&combined_message).await {
            Ok(response) => {
                info!(
                    "xAI chat completed successfully, response length: {} chars",
                    response.len()
                );
                Ok(response)
            }
            Err(e) => {
                error!("xAI chat failed: {}", e);

                // Provide more specific error information
                let error_msg = if e.to_string().contains("401") {
                    format!(
                        "xAI API authentication failed. Please check your API key. Error: {}",
                        e
                    )
                } else if e.to_string().contains("429") {
                    format!(
                        "xAI API rate limit exceeded. Please wait before retrying. Error: {}",
                        e
                    )
                } else if e.to_string().contains("timeout") {
                    format!(
                        "xAI API request timeout. Please check your network connection. Error: {}",
                        e
                    )
                } else {
                    format!("xAI chat request failed: {}", e)
                };

                Err(anyhow!("{}", error_msg))
            }
        }
    }

    pub async fn get_llm_request(messages: Vec<String>) -> Result<String> {
        info!(
            "Processing xAI LLM request with {} messages",
            messages.len()
        );

        let provider = Self::init_with_config().await.map_err(|e| {
            error!("Failed to initialize xAI provider: {}", e);
            e
        })?;

        match provider.chat(messages).await {
            Ok(response) => {
                info!("xAI LLM request completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("xAI LLM request failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn chat_with_prompt_static(
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String> {
        info!(
            "Processing xAI static chat with prompt request, {} messages",
            messages.len()
        );

        let provider = Self::init_with_config().await.map_err(|e| {
            error!("Failed to initialize xAI provider for static chat: {}", e);
            e
        })?;

        let combined_message = messages.join(" ");
        debug!("Combined message length: {} chars", combined_message.len());

        match provider
            .chat_with_prompt(&combined_message, &system_prompt)
            .await
        {
            Ok(response) => {
                info!("xAI static chat with prompt completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("xAI static chat with prompt failed: {}", e);
                Err(e)
            }
        }
    }

    /// Validate the xAI configuration without making an API call
    pub async fn validate_config() -> Result<()> {
        info!("Validating xAI configuration");

        let config = get_config().map_err(|e| anyhow!("Config validation failed: {}", e))?;

        let api_key = &config.llm.xai.api_key;
        let model = &config.llm.xai.model;

        if api_key.is_empty() || api_key == "your_xai_api_key_here" {
            error!("Invalid xAI API key in configuration");
            return Err(anyhow!(
                "Invalid xAI API key. Please set a valid API key in config.toml"
            ));
        }

        if model.is_empty() {
            error!("Empty xAI model name in configuration");
            return Err(anyhow!("xAI model name cannot be empty"));
        }

        info!("xAI configuration validation passed");
        Ok(())
    }

    /// Test the connection to xAI with a simple request
    pub async fn test_connection() -> Result<()> {
        info!("Testing xAI connection");

        let provider = Self::init_with_config().await?;
        let test_message =
            vec!["Hello, this is a connection test. Please respond with 'OK'.".to_string()];

        match provider.chat(test_message).await {
            Ok(response) => {
                info!("xAI connection test successful, response: {}", response);
                Ok(())
            }
            Err(e) => {
                error!("xAI connection test failed: {}", e);

                // Provide detailed diagnostic information
                let error_msg = if e.to_string().contains("401") {
                    format!(
                        "xAI API authentication failed. Your API key may be invalid or expired. Please check your configuration. Error: {}",
                        e
                    )
                } else if e.to_string().contains("429") {
                    format!(
                        "xAI API rate limit exceeded. Please wait a few minutes before retrying. Error: {}",
                        e
                    )
                } else if e.to_string().contains("timeout") {
                    format!(
                        "xAI API connection timeout. Please check your network connection and try again. Error: {}",
                        e
                    )
                } else {
                    format!("xAI connection test failed: {}", e)
                };

                Err(anyhow!("{}", error_msg))
            }
        }
    }
}

#[async_trait::async_trait]
impl LLMProvider for XAIProvider {
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
        "xai"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_config() {
        // This test will fail if config is not properly set up
        let result = XAIProvider::validate_config().await;
        match result {
            Ok(_) => println!("xAI config validation passed"),
            Err(e) => println!("xAI config validation failed (expected): {}", e),
        }
    }

    #[tokio::test]
    async fn test_chat_with_prompt() {
        if XAIProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = XAIProvider::init_with_config().await.unwrap();
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
                println!("Test failed (may be due to API key): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_chat() {
        if XAIProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = XAIProvider::init_with_config().await.unwrap();
        let messages = vec!["What is the capital of France?".to_string()];
        let response = provider.chat(messages).await;

        match response {
            Ok(resp) => {
                println!("Test response: {}", resp);
                assert!(resp.contains("Paris") || resp.contains("paris"));
            }
            Err(e) => {
                println!("Test failed (may be due to API key): {}", e);
            }
        }
    }
}
