use crate::pkg_config::{LLMProvider, provider_config};
use anyhow::{Result, anyhow};
use rig::agent::Agent;
use rig::completion::{Chat, Prompt};
use rig::providers::{anthropic, deepseek, gemini, mistral, ollama, openai};
use std::sync::Arc;

pub struct LLMFactory {
    provider: LLMProvider,
    api_key: Option<String>,
}

impl LLMFactory {
    pub fn new(provider_config: provider_config) -> Self {
        LLMFactory {
            provider: provider_config.provider,
            api_key: provider_config.api_key,
        }
    }

    /// 创建带有系统提示的Agent
    pub async fn create_agent(&self, model: &str, system_prompt: &str) -> Result<LLMAgent> {
        match self.provider {
            LLMProvider::Deepseek => {
                let client = deepseek::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for DeepSeek"))?,
                )?;
                let agent = client.agent(model).preamble(system_prompt).build();
                Ok(LLMAgent::new(Arc::new(agent)))
            }
            LLMProvider::OpenAI => {
                let client = openai::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for OpenAI"))?,
                )?;
                let agent = client.agent(model).preamble(system_prompt).build();
                Ok(LLMAgent::new(Arc::new(agent)))
            }
            LLMProvider::Anthropic => {
                let client = anthropic::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for Anthropic"))?,
                )?;
                let agent = client.agent(model).preamble(system_prompt).build();
                Ok(LLMAgent::new(Arc::new(agent)))
            }
            LLMProvider::Gemini => {
                let client = gemini::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for Gemini"))?,
                )?;
                let agent = client.agent(model).preamble(system_prompt).build();
                Ok(LLMAgent::new(Arc::new(agent)))
            }
            LLMProvider::Mistral => {
                let client = mistral::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for Mistral"))?,
                )?;
                let agent = client.agent(model).preamble(system_prompt).build();
                Ok(LLMAgent::new(Arc::new(agent)))
            }
            LLMProvider::Ollama => {
                let client = ollama::Client::new()?;
                let agent = client.agent(model).preamble(system_prompt).build();
                Ok(LLMAgent::new(Arc::new(agent)))
            }
        }
    }

    /// 创建简单的聊天客户端
    pub async fn create_chat(&self, model: &str) -> Result<LLMChat> {
        match self.provider {
            LLMProvider::Deepseek => {
                let client = deepseek::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for DeepSeek"))?,
                )?;
                let chat = client.chat(model);
                Ok(LLMChat::new(Box::new(chat)))
            }
            LLMProvider::OpenAI => {
                let client = openai::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for OpenAI"))?,
                )?;
                let chat = client.chat(model);
                Ok(LLMChat::new(Box::new(chat)))
            }
            LLMProvider::Anthropic => {
                let client = anthropic::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for Anthropic"))?,
                )?;
                let chat = client.chat(model);
                Ok(LLMChat::new(Box::new(chat)))
            }
            LLMProvider::Gemini => {
                let client = gemini::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for Gemini"))?,
                )?;
                let chat = client.chat(model);
                Ok(LLMChat::new(Box::new(chat)))
            }
            LLMProvider::Mistral => {
                let client = mistral::Client::new(
                    self.api_key
                        .as_ref()
                        .ok_or_else(|| anyhow!("API key required for Mistral"))?,
                )?;
                let chat = client.chat(model);
                Ok(LLMChat::new(Box::new(chat)))
            }
            LLMProvider::Ollama => {
                let client = ollama::Client::new()?;
                let chat = client.chat(model);
                Ok(LLMChat::new(Box::new(chat)))
            }
        }
    }
}

/// LLM Agent包装器
pub struct LLMAgent {
    agent: Arc<dyn Agent>,
}

impl LLMAgent {
    pub fn new(agent: Arc<dyn Agent>) -> Self {
        Self { agent }
    }

    /// 执行提示并获取响应
    pub async fn prompt(&self, user_input: &str) -> Result<String> {
        self.agent
            .prompt(user_input)
            .await
            .map_err(|e| anyhow!("Agent prompt failed: {}", e))
    }

    /// 执行聊天并获取响应
    pub async fn chat(&self, messages: Vec<String>) -> Result<String> {
        // 将消息转换为聊天格式
        let mut conversation = String::new();
        for (i, message) in messages.iter().enumerate() {
            if i > 0 {
                conversation.push('\n');
            }
            conversation.push_str(message);
        }

        self.prompt(&conversation).await
    }
}

/// LLM Chat包装器
pub struct LLMChat {
    chat: Box<dyn Chat>,
}

impl LLMChat {
    pub fn new(chat: Box<dyn Chat>) -> Self {
        Self { chat }
    }

    /// 发送消息并获取响应
    pub async fn send_message(&self, message: &str) -> Result<String> {
        self.chat
            .prompt(message)
            .await
            .map_err(|e| anyhow!("Chat failed: {}", e))
    }

    /// 发送带有系统提示的消息
    pub async fn send_message_with_system(&self, system: &str, message: &str) -> Result<String> {
        let prompt = Prompt::new().system(system).user(message);

        self.chat
            .prompt(&prompt.to_string())
            .await
            .map_err(|e| anyhow!("Chat with system prompt failed: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg_config::LLMProvider;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_llm_factory_creation() {
        let config = provider_config {
            provider: LLMProvider::Ollama,
            api_key: None,
        };

        let factory = LLMFactory::new(config);
        assert_eq!(factory.provider, LLMProvider::Ollama);
        assert_eq!(factory.api_key, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_llm_factory_with_api_key() {
        let config = provider_config {
            provider: LLMProvider::OpenAI,
            api_key: Some("test-key".to_string()),
        };

        let factory = LLMFactory::new(config);
        assert_eq!(factory.provider, LLMProvider::OpenAI);
        assert_eq!(factory.api_key, Some("test-key".to_string()));
    }

    #[tokio::test]
    #[serial]
    async fn test_create_agent_without_api_key_for_openai() {
        let config = provider_config {
            provider: LLMProvider::OpenAI,
            api_key: None,
        };

        let factory = LLMFactory::new(config);
        let result = factory
            .create_agent("gpt-3.5-turbo", "You are a helpful assistant")
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("API key required"));
    }

    #[tokio::test]
    #[serial]
    async fn test_create_chat_without_api_key_for_anthropic() {
        let config = provider_config {
            provider: LLMProvider::Anthropic,
            api_key: None,
        };

        let factory = LLMFactory::new(config);
        let result = factory.create_chat("claude-3-sonnet-20240229").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("API key required"));
    }

    // 注意：以下测试需要真实的API密钥或运行的Ollama服务
    // 在实际环境中测试时请取消注释并提供正确的配置

    /*
    #[tokio::test]
    #[serial]
    async fn test_ollama_agent_creation() {
        let config = provider_config {
            provider: LLMProvider::Ollama,
            api_key: None,
        };

        let factory = LLMFactory::new(config);
        let result = factory.create_agent("llama2", "You are a helpful assistant").await;

        // 这个测试只有在本地运行Ollama服务时才会通过
        if result.is_ok() {
            let agent = result.unwrap();
            let response = agent.prompt("Hello").await;
            assert!(response.is_ok());
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_ollama_chat_creation() {
        let config = provider_config {
            provider: LLMProvider::Ollama,
            api_key: None,
        };

        let factory = LLMFactory::new(config);
        let result = factory.create_chat("llama2").await;

        if result.is_ok() {
            let chat = result.unwrap();
            let response = chat.send_message("Hello").await;
            assert!(response.is_ok());
        }
    }
    */
}
