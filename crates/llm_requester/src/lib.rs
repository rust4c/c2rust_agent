use anyhow::Result;

pub mod deepseek_provider;
pub mod ollama_provider;
pub mod openai_provider;
pub mod utils;
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
    // Check if we need chunking first
    let total_tokens = messages
        .iter()
        .map(|m| utils::estimate_token_count(m))
        .sum::<usize>()
        + utils::estimate_token_count(&prompt);

    if total_tokens > 100000 {
        // Use chunked approach and combine results
        let chunked_results =
            llm_request_with_prompt_chunked(messages, prompt, Some(100000)).await?;
        return Ok(chunked_results.join("\n\n--- CHUNK BOUNDARY ---\n\n"));
    }

    // Original single request approach
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

/// Get chunking configuration with fallback defaults
fn get_chunking_config() -> (bool, usize) {
    if let Ok(config) = pkg_config::get_config() {
        let chunking_config = config.chunking.unwrap_or_default();
        (
            chunking_config.enabled.unwrap_or(true),
            chunking_config.max_tokens.unwrap_or(120000),
        )
    } else {
        (true, 100000) // Default values
    }
}

/// Chunked request function that automatically splits large messages
pub async fn llm_request_chunked(
    messages: Vec<String>,
    max_tokens: Option<usize>,
) -> Result<Vec<String>> {
    let (chunking_enabled, default_max_tokens) = get_chunking_config();
    let max_tokens = max_tokens.unwrap_or(default_max_tokens.min(100000));

    if !chunking_enabled {
        return Ok(vec![llm_request(messages).await?]);
    }

    let mut results = Vec::new();
    for message in messages {
        let chunks = utils::chunk_message(&message, max_tokens);
        for chunk in chunks {
            let response = llm_request(vec![chunk]).await?;
            results.push(response);
        }
    }

    Ok(results)
}

/// Chunked request with prompt that automatically handles large contexts
pub async fn llm_request_with_prompt_chunked(
    messages: Vec<String>,
    prompt: String,
    max_tokens: Option<usize>,
) -> Result<Vec<String>> {
    let (chunking_enabled, default_max_tokens) = get_chunking_config();
    let max_tokens = max_tokens.unwrap_or(default_max_tokens.min(100000));

    if !chunking_enabled {
        return Ok(vec![
            llm_request_with_prompt_direct(messages, prompt).await?,
        ]);
    }

    let chunked_requests = utils::prepare_chunked_messages(messages, &prompt, max_tokens);
    let mut results = Vec::new();

    for (chunk_messages, chunk_prompt) in chunked_requests {
        let response = llm_request_with_prompt_direct(chunk_messages, chunk_prompt).await?;
        results.push(response);
    }

    Ok(results)
}

/// Direct LLM request without automatic chunking (for internal use)
async fn llm_request_with_prompt_direct(messages: Vec<String>, prompt: String) -> Result<String> {
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

    #[tokio::test]
    async fn test_llm_request_chunked() {
        let large_message = "test ".repeat(50000);
        let messages = vec![large_message];
        let result = llm_request_chunked(messages, Some(1000)).await;
        assert!(result.is_ok());
        let responses = result.unwrap();
        assert!(responses.len() >= 1);
    }

    #[tokio::test]
    async fn test_llm_request_with_prompt_chunked() {
        let large_message = "test ".repeat(50000);
        let messages = vec![large_message];
        let prompt = "Analyze this text".to_string();
        let result = llm_request_with_prompt_chunked(messages, prompt, Some(1000)).await;
        assert!(result.is_ok());
        let responses = result.unwrap();
        assert!(responses.len() >= 1);
    }
}
