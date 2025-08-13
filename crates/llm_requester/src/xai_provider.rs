use anyhow::Result;
use anyhow::anyhow;
use siumai::prelude::*;

use crate::pkg_config::{XAIConfig, get_config};

struct OpenAIProvider {
    client: Siumai,
}

impl OpenAIProvider {
    pub async fn new(xai_config: XAIConfig) -> Result<Self> {
        let client = Siumai::builder()
            .xai()
            .model(xai_config.model)
            .api_key(xai_config.api_key)
            .build()
            .await?;
        Ok(Self { client })
    }

    pub async fn init_with_config() -> Result<Self> {
        let config = match get_config() {
            Ok(config) => config,
            Err(err) => return Err(anyhow!("can't get config with error: {}", err)),
        };

        let model = config.xai_config.model;
        let api_key = config.xai_config.api_key;

        let client = Siumai::builder()
            .xai()
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
}
