use anyhow::Result;
use anyhow::anyhow;
use siumai::prelude::*;

use crate::pkg_config::{OpenAIConfig, get_config};

pub struct OpenAIProvider {
    client: Siumai,
}

impl OpenAIProvider {
    pub async fn new(openai_config: OpenAIConfig) -> Result<Self> {
        let client = Siumai::builder()
            .openai()
            .model(openai_config.model)
            .api_key(openai_config.api_key)
            .build()
            .await?;
        Ok(Self { client })
    }

    pub async fn init_with_config() -> Result<Self> {
        let config = match get_config() {
            Ok(config) => config,
            Err(err) => return Err(anyhow!("can't get config with error: {}", err)),
        };

        let model = config.openai_config.model;
        let api_key = config.openai_config.api_key;

        let client = Siumai::builder()
            .openai()
            .model(model)
            .api_key(api_key)
            .build()
            .await?;

        Ok(Self { client })
    }

    pub async fn chat_with_prompt(&self, message: &str, system_prompt: &str) -> Result<String> {
        let request = vec![user!(message), system!(system_prompt)];
        let response = self.client.chat_with_tools(request, None).await?;
        Ok(response.text().unwrap_or_default())
    }

    pub async fn chat(&self, request: Vec<ChatMessage>) -> Result<String> {
        let response = self.client.chat_with_tools(request, None).await?;
        Ok(response.text().unwrap_or_default())
    }

    pub async fn get_llm_request(messages: Vec<String>) -> Result<String> {
        let provider = Self::init_with_config().await?;
        let chat_messages: Vec<ChatMessage> = messages.into_iter().map(|msg| user!(msg)).collect();
        provider.chat(chat_messages).await
    }

    pub async fn chat_with_prompt_static(
        messages: Vec<String>,
        system_prompt: String,
    ) -> Result<String> {
        let provider = Self::init_with_config().await?;
        let combined_message = messages.join(" ");
        provider
            .chat_with_prompt(&combined_message, &system_prompt)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chat_with_prompt() {
        let provider = OpenAIProvider::init_with_config().await.unwrap();
        let response = provider
            .chat_with_prompt(
                "What is the capital of France?",
                "You are a helpful assistant.",
            )
            .await
            .unwrap();
        assert!(response.contains("Paris"));
    }

    #[tokio::test]
    async fn test_chat() {
        let provider = OpenAIProvider::init_with_config().await.unwrap();
        let request = vec![
            user!("What is the capital of France?"),
            system!("You are a helpful assistant."),
        ];
        let response = provider.chat(request).await.unwrap();
        assert!(response.contains("Paris"));
    }
}
