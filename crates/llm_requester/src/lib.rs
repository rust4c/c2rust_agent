use anyhow::Result;

pub mod deepseek_provider;
pub mod ollama_provider;
pub mod openai_provider;
pub mod xai_provider;

pub mod pkg_config;

pub async fn llm_request(messages: Vec<String>) -> Result<String> {
    let config = pkg_config::get_config()?;
    match config.provider.as_str() {
        "deepseek" => deepseek_provider::DeepSeekProvider::get_llm_request(messages).await,
        "ollama" => ollama_provider::OllamaProvider::get_llm_request(messages).await,
        "openai" => openai_provider::OpenAIProvider::get_llm_request(messages).await,
        "xai" => xai_provider::XAIProvider::get_llm_request(messages).await,
        _ => Err(anyhow::anyhow!("Invalid provider: {}", config.provider)),
    }
}

pub async fn llm_request_with_prompt(messages: Vec<String>, prompt: String) -> Result<String> {
    let config = pkg_config::get_config()?;
    match config.provider.as_str() {
        "deepseek" => {
            deepseek_provider::DeepSeekProvider::chat_with_prompt_static(messages, prompt).await
        }
        "ollama" => {
            ollama_provider::OllamaProvider::chat_with_prompt_static(messages, prompt).await
        }
        "openai" => {
            openai_provider::OpenAIProvider::chat_with_prompt_static(messages, prompt).await
        }
        "xai" => xai_provider::XAIProvider::chat_with_prompt_static(messages, prompt).await,
        _ => Err(anyhow::anyhow!("Invalid provider: {}", config.provider)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_llm_request() {
        let messages = vec!["Hello".to_string(), "How are you?".to_string()];
        let result = llm_request(messages).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_llm_request_with_prompt() {
        let messages = vec!["Hello".to_string(), "How are you?".to_string()];
        let prompt = "What is your name?";
        let result = llm_request_with_prompt(messages, prompt.to_string()).await;
        assert!(result.is_ok());
    }
}
