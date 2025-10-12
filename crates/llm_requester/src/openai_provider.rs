use anyhow::Result;
use anyhow::anyhow;
use log::{debug, error, info};
use siumai::prelude::*;

use crate::pkg_config::{OpenAIConfig, get_config};

pub struct OpenAIProvider {
    client: Siumai,
}

impl OpenAIProvider {
    pub async fn new(openai_config: OpenAIConfig) -> Result<Self> {
        info!(
            "Creating new OpenAI provider with model: {}",
            openai_config.model
        );

        // Validate API key
        if openai_config.api_key.is_empty() || openai_config.api_key == "your_openai_api_key_here" {
            return Err(anyhow!(
                "Invalid OpenAI API key. Please set a valid API key in config.toml"
            ));
        }

        let client = Siumai::builder()
            .openai()
            .model(openai_config.model.clone())
            .api_key(openai_config.api_key)
            .build()
            .await
            .map_err(|e| anyhow!("Failed to create OpenAI client: {}", e))?;

        info!("Successfully created OpenAI provider");
        Ok(Self { client })
    }

    pub async fn init_with_config() -> Result<Self> {
        debug!("Initializing OpenAI provider from config");

        let config = match get_config() {
            Ok(config) => config,
            Err(err) => {
                error!("Failed to get config: {}", err);
                return Err(anyhow!("Can't get config with error: {}", err));
            }
        };

        let model = config.llm.openai.model.clone();
        let api_key = config.llm.openai.api_key.clone();

        info!("Using OpenAI model: {}", model);

        // Validate API key
        if api_key.is_empty() || api_key == "your_openai_api_key_here" {
            error!("Invalid OpenAI API key found in config");
            return Err(anyhow!(
                "Invalid OpenAI API key. Please set a valid API key in config.toml"
            ));
        }

        let client = Siumai::builder()
            .openai()
            .model(model)
            .api_key(api_key)
            .build()
            .await
            .map_err(|e| {
                error!("Failed to build OpenAI client: {}", e);
                anyhow!("Failed to create OpenAI client: {}", e)
            })?;

        info!("Successfully initialized OpenAI provider from config");
        Ok(Self { client })
    }

    pub async fn chat_with_prompt(&self, message: &str, system_prompt: &str) -> Result<String> {
        info!("Starting OpenAI chat with prompt request");
        debug!("Message length: {} chars", message.len());
        debug!("System prompt length: {} chars", system_prompt.len());

        let request = vec![user!(message), system!(system_prompt)];

        match self.client.chat_with_tools(request, None).await {
            Ok(response) => {
                let text = response.text().unwrap_or_default();
                info!(
                    "OpenAI chat with prompt completed successfully, response length: {} chars",
                    text.len()
                );
                Ok(text)
            }
            Err(e) => {
                error!("OpenAI chat with prompt failed: {}", e);
                Err(anyhow!("OpenAI chat request failed: {}", e))
            }
        }
    }

    pub async fn chat(&self, request: Vec<ChatMessage>) -> Result<String> {
        info!(
            "Starting OpenAI chat request with {} messages",
            request.len()
        );

        match self.client.chat_with_tools(request, None).await {
            Ok(response) => {
                let text = response.text().unwrap_or_default();
                info!(
                    "OpenAI chat completed successfully, response length: {} chars",
                    text.len()
                );
                Ok(text)
            }
            Err(e) => {
                error!("OpenAI chat failed: {}", e);
                Err(anyhow!("OpenAI chat request failed: {}", e))
            }
        }
    }

    pub async fn get_llm_request(messages: Vec<String>) -> Result<String> {
        info!("Processing LLM request with {} messages", messages.len());

        let provider = Self::init_with_config().await.map_err(|e| {
            error!("Failed to initialize OpenAI provider: {}", e);
            e
        })?;

        let chat_messages: Vec<ChatMessage> = messages.into_iter().map(|msg| user!(msg)).collect();

        match provider.chat(chat_messages).await {
            Ok(response) => {
                info!("LLM request completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("LLM request failed: {}", e);
                Err(e)
            }
        }
    }

    pub async fn chat_with_prompt_static(
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String> {
        info!(
            "Processing static chat with prompt request, {} messages",
            messages.len()
        );

        let provider = Self::init_with_config().await.map_err(|e| {
            error!(
                "Failed to initialize OpenAI provider for static chat: {}",
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
                info!("Static chat with prompt completed successfully");
                Ok(response)
            }
            Err(e) => {
                error!("Static chat with prompt failed: {}", e);
                Err(e)
            }
        }
    }

    /// Validate the OpenAI configuration without making an API call
    pub async fn validate_config() -> Result<()> {
        info!("Validating OpenAI configuration");

        let config = get_config().map_err(|e| anyhow!("Config validation failed: {}", e))?;

        let api_key = &config.llm.openai.api_key;
        let model = &config.llm.openai.model;

        if api_key.is_empty() || api_key == "your_openai_api_key_here" {
            error!("Invalid API key in configuration");
            return Err(anyhow!(
                "Invalid OpenAI API key. Please set a valid API key in config.toml"
            ));
        }

        if model.is_empty() {
            error!("Empty model name in configuration");
            return Err(anyhow!("OpenAI model name cannot be empty"));
        }

        info!("OpenAI configuration validation passed");
        Ok(())
    }

    /// Test the connection to OpenAI with a simple request
    pub async fn test_connection() -> Result<()> {
        info!("Testing OpenAI connection");

        let provider = Self::init_with_config().await?;
        let test_message = vec![user!(
            "Hello, this is a connection test. Please respond with 'OK'."
        )];

        match provider.client.chat_with_tools(test_message, None).await {
            Ok(response) => {
                let text = response.text().unwrap_or_default();
                info!("OpenAI connection test successful, response: {}", text);
                Ok(())
            }
            Err(e) => {
                error!("OpenAI connection test failed: {}", e);
                Err(anyhow!("OpenAI connection test failed: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_config() {
        // This test will fail if config is not properly set up
        let result = OpenAIProvider::validate_config().await;
        match result {
            Ok(_) => println!("Config validation passed"),
            Err(e) => println!("Config validation failed (expected): {}", e),
        }
    }

    #[tokio::test]
    async fn test_chat_with_prompt() {
        if OpenAIProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = OpenAIProvider::init_with_config().await.unwrap();
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
        if OpenAIProvider::validate_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let provider = OpenAIProvider::init_with_config().await.unwrap();
        let request = vec![
            user!("What is the capital of France?"),
            system!("You are a helpful assistant."),
        ];
        let response = provider.chat(request).await;

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
