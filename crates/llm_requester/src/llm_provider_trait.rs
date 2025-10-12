use anyhow::Result;

/// Common trait for all LLM providers
#[async_trait::async_trait]
pub trait LLMProvider: Send + Sync {
    /// Initialize the provider with configuration
    async fn init_with_config() -> Result<Box<dyn LLMProvider>>
    where
        Self: Sized;

    /// Simple chat request with multiple messages
    async fn chat(&self, messages: Vec<String>) -> Result<String>;

    /// Chat with system prompt
    async fn chat_with_prompt(
        &self,
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String>;

    /// Static method for simple LLM request
    async fn get_llm_request(messages: Vec<String>) -> Result<String>
    where
        Self: Sized;

    /// Static method for chat with prompt
    async fn chat_with_prompt_static(
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String>
    where
        Self: Sized;

    /// Validate the provider configuration
    async fn validate_config() -> Result<()>
    where
        Self: Sized;

    /// Test the connection to the provider
    async fn test_connection() -> Result<()>
    where
        Self: Sized;

    /// Get the provider name
    fn provider_name(&self) -> &'static str;
}
