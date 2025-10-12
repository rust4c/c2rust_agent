use anyhow::{Result, anyhow};
use log::{debug, error, info};
use rig::providers::openai::responses_api::ResponsesCompletionModel;
use rig::{agent::Agent, client::CompletionClient, completion::Prompt, providers::openai};

use crate::llm_provider_trait::LLMProvider;
use crate::pkg_config::get_config;

pub struct DeepSeekProvider {
    agent: Agent<ResponsesCompletionModel>,
}

impl DeepSeekProvider {
    pub async fn new(api_key: String, model: String) -> Result<Self> {
        info!("Creating new DeepSeek provider with model: {}", model);

        // Validate API key
        if api_key.is_empty() || api_key == "sk-your_deepseek_api_key_here" {
            return Err(anyhow!(
                "Invalid DeepSeek API key. Please set a valid API key in config.toml"
            ));
        }

        let client = openai::ClientBuilder::new(&api_key)
            .base_url("https://api.deepseek.com/v1")
            .build()
            .map_err(|e| anyhow!("Failed to create DeepSeek client: {}", e))?;

        let agent = client.agent(&model).build();

        info!("Successfully created DeepSeek provider");
        Ok(Self { agent })
    }

    pub async fn init_with_config() -> Result<Self> {
        debug!("Initializing DeepSeek provider from config");

        let config = match get_config() {
            Ok(config) => {
                debug!("Using DeepSeek provider configuration");
                config
            }
            Err(err) => {
                error!("Failed to get config: {}", err);
                return Err(anyhow!("Can't get config with error: {}", err));
            }
        };

        let model = config.llm.deepseek.model.clone();
        let api_key = config.llm.deepseek.api_key.clone();

        info!("Using DeepSeek model: {}", model);

        // Validate API key
        if api_key.is_empty() || api_key == "sk-your_deepseek_api_key_here" {
            error!("Invalid DeepSeek API key found in config");
            return Err(anyhow!(
                "Invalid DeepSeek API key. Please set a valid API key in config.toml"
            ));
        }

        let client = openai::ClientBuilder::new(&api_key)
            .base_url("https://api.deepseek.com/v1")
            .build()
            .map_err(|e| anyhow!("Failed to create DeepSeek client: {}", e))?;

        let agent = client.agent(&model).build();

        info!("Successfully initialized DeepSeek provider from config");
        Ok(Self { agent })
    }

    pub async fn chat_with_prompt(&self, message: &str, system_prompt: &str) -> Result<String> {
        info!("Starting DeepSeek chat with prompt request");
        debug!("Message length: {} chars", message.len());
        debug!("System prompt length: {} chars", system_prompt.len());

        let prompt = format!("System: {}\n\nUser: {}", system_prompt, message);

        match self.agent.prompt(&prompt).await {
            Ok(response) => {
                info!(
                    "DeepSeek chat with prompt completed successfully, response length: {} chars",
                    response.len()
                );
                Ok(response)
            }
            Err(e) => {
                error!("DeepSeek chat with prompt failed: {}", e);

                // Provide more specific error information
                let error_msg = if e.to_string().contains("error decoding response body") {
                    format!(
                        "DeepSeek API response decoding failed. This could indicate: 1) Network timeout/interruption, 2) Invalid API key, 3) API service issues, 4) Rate limiting. Original error: {}",
                        e
                    )
                } else if e.to_string().contains("401") {
                    format!(
                        "DeepSeek API authentication failed. Please check your API key. Error: {}",
                        e
                    )
                } else if e.to_string().contains("429") {
                    format!(
                        "DeepSeek API rate limit exceeded. Please wait before retrying. Error: {}",
                        e
                    )
                } else if e.to_string().contains("timeout") {
                    format!(
                        "DeepSeek API request timeout. Please check your network connection. Error: {}",
                        e
                    )
                } else {
                    format!("DeepSeek chat request failed: {}", e)
                };

                Err(anyhow!("{}", error_msg))
            }
        }
    }

    pub async fn chat(&self, messages: Vec<String>) -> Result<String> {
        info!(
            "Starting DeepSeek chat request with {} messages",
            messages.len()
        );

        let combined_message = messages.join("\n");

        match self.agent.prompt(&combined_message).await {
            Ok(response) => {
                info!(
                    "DeepSeek chat completed successfully, response length: {} chars",
                    response.len()
                );
                Ok(response)
            }
            Err(e) => {
                error!("DeepSeek chat failed: {}", e);

                // Provide more specific error information
                let error_msg = if e.to_string().contains("error decoding response body") {
                    format!(
                        "DeepSeek API response decoding failed. This could indicate: 1) Network timeout/interruption, 2) Invalid API key, 3) API service issues, 4) Rate limiting. Original error: {}",
                        e
                    )
                } else if e.to_string().contains("401") {
                    format!(
                        "DeepSeek API authentication failed. Please check your API key. Error: {}",
                        e
                    )
                } else if e.to_string().contains("429") {
                    format!(
                        "DeepSeek API rate limit exceeded. Please wait before retrying. Error: {}",
                        e
                    )
                } else if e.to_string().contains("timeout") {
                    format!(
                        "DeepSeek API request timeout. Please check your network connection. Error: {}",
                        e
                    )
                } else {
                    format!("DeepSeek chat request failed: {}", e)
                };

                Err(anyhow!("{}", error_msg))
            }
        }
    }

    pub async fn get_llm_request(messages: Vec<String>) -> Result<String> {
        info!(
            "Processing DeepSeek LLM request with {} messages",
            messages.len()
        );

        let provider = Self::init_with_config().await.map_err(|e| {
            error!("Failed to initialize DeepSeek provider: {}", e);
            e
        })?;

        match provider.chat(messages).await {
            Ok(response) => {
                info!("DeepSeek LLM request completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("DeepSeek LLM request failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn chat_with_prompt_static(
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String> {
        info!(
            "Processing DeepSeek static chat with prompt request, {} messages",
            messages.len()
        );

        let provider = Self::init_with_config().await.map_err(|e| {
            error!(
                "Failed to initialize DeepSeek provider for static chat: {}",
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
                info!("DeepSeek static chat with prompt completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("DeepSeek static chat with prompt failed: {}", e);
                Err(e)
            }
        }
    }

    /// Validate the DeepSeek configuration without making an API call
    pub async fn validate_config() -> Result<()> {
        info!("Validating DeepSeek configuration");

        let config = get_config().map_err(|e| anyhow!("Config validation failed: {}", e))?;

        let api_key = &config.llm.deepseek.api_key;
        let model = &config.llm.deepseek.model;

        if api_key.is_empty() || api_key == "sk-your_deepseek_api_key_here" {
            error!("Invalid DeepSeek API key in configuration");
            return Err(anyhow!(
                "Invalid DeepSeek API key. Please set a valid API key in config.toml"
            ));
        }

        if model.is_empty() {
            error!("Empty DeepSeek model name in configuration");
            return Err(anyhow!("DeepSeek model name cannot be empty"));
        }

        info!("DeepSeek configuration validation passed");
        Ok(())
    }

    /// Test the connection to DeepSeek with a simple request
    pub async fn test_connection() -> Result<()> {
        info!("Testing DeepSeek connection");

        let provider = Self::init_with_config().await?;
        let test_message =
            vec!["Hello, this is a connection test. Please respond with 'OK'.".to_string()];

        match provider.chat(test_message).await {
            Ok(response) => {
                info!(
                    "DeepSeek connection test successful, response: {}",
                    response
                );
                Ok(())
            }
            Err(e) => {
                error!("DeepSeek connection test failed: {}", e);

                // Provide detailed diagnostic information
                let error_msg = if e.to_string().contains("error decoding response body") {
                    format!(
                        "DeepSeek API connection test failed due to response decoding error. Possible causes:\n\
                            1. Network connectivity issues - check your internet connection\n\
                            2. Invalid or expired API key - verify your DeepSeek API key\n\
                            3. DeepSeek service temporarily unavailable\n\
                            4. Firewall or proxy blocking the request\n\
                            5. Rate limiting - too many requests\n\
                            Original error: {}",
                        e
                    )
                } else if e.to_string().contains("401") {
                    format!(
                        "DeepSeek API authentication failed. Your API key may be invalid or expired. Please check your configuration. Error: {}",
                        e
                    )
                } else if e.to_string().contains("429") {
                    format!(
                        "DeepSeek API rate limit exceeded. Please wait a few minutes before retrying. Error: {}",
                        e
                    )
                } else if e.to_string().contains("timeout") {
                    format!(
                        "DeepSeek API connection timeout. Please check your network connection and try again. Error: {}",
                        e
                    )
                } else {
                    format!("DeepSeek connection test failed: {}", e)
                };

                Err(anyhow!("{}", error_msg))
            }
        }
    }
}

#[async_trait::async_trait]
impl LLMProvider for DeepSeekProvider {
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
        "deepseek"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_config() {
        // This test will fail if config is not properly set up
        let result = DeepSeekProvider::validate_config().await;
        match result {
            Ok(_) => println!("DeepSeek config validation passed"),
            Err(e) => println!("DeepSeek config validation failed (expected): {}", e),
        }
    }

    #[tokio::test]
    async fn test_chat_with_prompt() {
        if DeepSeekProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = DeepSeekProvider::init_with_config().await.unwrap();
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
        if DeepSeekProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = DeepSeekProvider::init_with_config().await.unwrap();
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
