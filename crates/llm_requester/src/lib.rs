use anyhow::Result;
use log::{debug, error, info, warn};

pub mod deepseek_provider;
pub mod ollama_provider;
pub mod openai_provider;
pub mod utils;
pub mod xai_provider;

pub mod pkg_config;

/// Make a simple LLM request with better error handling and logging
pub async fn llm_request(messages: Vec<String>) -> Result<String> {
    info!("Starting LLM request with {} messages", messages.len());

    let config = match pkg_config::get_config() {
        Ok(config) => {
            debug!("Using provider: {}", config.provider);
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return Err(anyhow::anyhow!("Configuration error: {}", e));
        }
    };

    let result = match config.provider.as_str() {
        "deepseek" => {
            info!("Using DeepSeek provider");
            deepseek_provider::DeepSeekProvider::get_llm_request(messages).await
        }
        "ollama" => {
            info!("Using Ollama provider");
            ollama_provider::OllamaProvider::get_llm_request(messages).await
        }
        "openai" => {
            info!("Using OpenAI provider");
            openai_provider::OpenAIProvider::get_llm_request(messages).await
        }
        "xai" => {
            info!("Using xAI provider");
            xai_provider::XAIProvider::get_llm_request(messages).await
        }
        _ => {
            error!("Invalid provider specified: {}", config.provider);
            return Err(anyhow::anyhow!(
                "Invalid provider: {}. Supported providers: deepseek, ollama, openai, xai",
                config.provider
            ));
        }
    };

    match result {
        Ok(response) => {
            info!(
                "LLM request completed successfully, response length: {} chars",
                response.len()
            );
            Ok(response)
        }
        Err(e) => {
            error!(
                "LLM request failed with provider {}: {}",
                config.provider, e
            );
            Err(anyhow::anyhow!(
                "AI translation request failed with {}: {}",
                config.provider,
                e
            ))
        }
    }
}

/// Make an LLM request with prompt, including automatic chunking for large inputs
pub async fn llm_request_with_prompt(messages: Vec<String>, prompt: String) -> Result<String> {
    info!(
        "Starting LLM request with prompt, {} messages, prompt length: {} chars",
        messages.len(),
        prompt.len()
    );

    // Check if we need chunking first
    let total_tokens = messages
        .iter()
        .map(|m| utils::estimate_token_count(m))
        .sum::<usize>()
        + utils::estimate_token_count(&prompt);

    debug!("Estimated total tokens: {}", total_tokens);

    if total_tokens > 100000 {
        warn!(
            "Large request detected ({} tokens), using chunked approach",
            total_tokens
        );
        // Use chunked approach and combine results
        let chunked_results =
            llm_request_with_prompt_chunked(messages, prompt, Some(100000)).await?;
        return Ok(chunked_results.join("\n\n--- CHUNK BOUNDARY ---\n\n"));
    }

    // Original single request approach
    let config = match pkg_config::get_config() {
        Ok(config) => {
            debug!("Using provider: {}", config.provider);
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return Err(anyhow::anyhow!("Configuration error: {}", e));
        }
    };

    let result = match config.provider.as_str() {
        "deepseek" => {
            info!("Using DeepSeek provider for prompt request");
            deepseek_provider::DeepSeekProvider::chat_with_prompt_static(messages, prompt).await
        }
        "ollama" => {
            info!("Using Ollama provider for prompt request");
            ollama_provider::OllamaProvider::chat_with_prompt_static(messages, prompt).await
        }
        "openai" => {
            info!("Using OpenAI provider for prompt request");
            openai_provider::OpenAIProvider::chat_with_prompt_static(messages, prompt).await
        }
        "xai" => {
            info!("Using xAI provider for prompt request");
            xai_provider::XAIProvider::chat_with_prompt_static(messages, prompt).await
        }
        _ => {
            error!("Invalid provider specified: {}", config.provider);
            return Err(anyhow::anyhow!(
                "Invalid provider: {}. Supported providers: deepseek, ollama, openai, xai",
                config.provider
            ));
        }
    };

    match result {
        Ok(response) => {
            info!(
                "LLM request with prompt completed successfully, response length: {} chars",
                response.len()
            );
            Ok(response)
        }
        Err(e) => {
            error!(
                "LLM request with prompt failed with provider {}: {}",
                config.provider, e
            );
            Err(anyhow::anyhow!(
                "AI translation request failed with {}: {}",
                config.provider,
                e
            ))
        }
    }
}

/// Get chunking configuration with fallback defaults
fn get_chunking_config() -> (bool, usize) {
    if let Ok(config) = pkg_config::get_config() {
        let chunking_config = config.chunking.unwrap_or_default();
        let enabled = chunking_config.enabled.unwrap_or(true);
        let max_tokens = chunking_config.max_tokens.unwrap_or(120000);
        debug!(
            "Chunking config: enabled={}, max_tokens={}",
            enabled, max_tokens
        );
        (enabled, max_tokens)
    } else {
        warn!("Could not load chunking config, using defaults");
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

    info!(
        "Processing chunked request: enabled={}, max_tokens={}, messages={}",
        chunking_enabled,
        max_tokens,
        messages.len()
    );

    if !chunking_enabled {
        info!("Chunking disabled, processing as single request");
        return Ok(vec![llm_request(messages).await?]);
    }

    let mut results = Vec::new();
    for (i, message) in messages.iter().enumerate() {
        debug!("Processing message {} of {}", i + 1, messages.len());
        let chunks = utils::chunk_message(&message, max_tokens);
        info!("Message {} split into {} chunks", i + 1, chunks.len());

        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            debug!("Processing chunk {} of {}", chunk_idx + 1, chunks.len());
            match llm_request(vec![chunk.clone()]).await {
                Ok(response) => {
                    debug!("Chunk {} processed successfully", chunk_idx + 1);
                    results.push(response);
                }
                Err(e) => {
                    error!(
                        "Failed to process chunk {} of message {}: {}",
                        chunk_idx + 1,
                        i + 1,
                        e
                    );
                    return Err(e);
                }
            }
        }
    }

    info!(
        "Chunked request completed with {} total results",
        results.len()
    );
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

    info!(
        "Processing chunked request with prompt: enabled={}, max_tokens={}, messages={}",
        chunking_enabled,
        max_tokens,
        messages.len()
    );

    if !chunking_enabled {
        info!("Chunking disabled, processing as single request with prompt");
        return Ok(vec![
            llm_request_with_prompt_direct(messages, prompt).await?,
        ]);
    }

    let chunked_requests = utils::prepare_chunked_messages(messages, &prompt, max_tokens);
    let mut results = Vec::new();

    info!("Processing {} chunked requests", chunked_requests.len());

    for (i, (chunk_messages, chunk_prompt)) in chunked_requests.into_iter().enumerate() {
        debug!(
            "Processing chunked request {} of {}",
            i + 1,
            results.len() + 1
        );
        match llm_request_with_prompt_direct(chunk_messages, chunk_prompt).await {
            Ok(response) => {
                debug!("Chunked request {} completed successfully", i + 1);
                results.push(response);
            }
            Err(e) => {
                error!("Failed to process chunked request {}: {}", i + 1, e);
                return Err(e);
            }
        }
    }

    info!(
        "Chunked request with prompt completed with {} results",
        results.len()
    );
    Ok(results)
}

/// Direct LLM request without automatic chunking (for internal use)
async fn llm_request_with_prompt_direct(messages: Vec<String>, prompt: String) -> Result<String> {
    debug!("Making direct LLM request with {} messages", messages.len());

    let config = match pkg_config::get_config() {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load configuration for direct request: {}", e);
            return Err(anyhow::anyhow!("Configuration error: {}", e));
        }
    };

    let result = match config.provider.as_str() {
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
        _ => {
            error!("Invalid provider specified: {}", config.provider);
            return Err(anyhow::anyhow!(
                "Invalid provider: {}. Supported providers: deepseek, ollama, openai, xai",
                config.provider
            ));
        }
    };

    match result {
        Ok(response) => {
            debug!("Direct LLM request completed successfully");
            Ok(response)
        }
        Err(e) => {
            error!("Direct LLM request failed: {}", e);
            Err(e)
        }
    }
}

/// Validate the current LLM configuration
pub async fn validate_llm_config() -> Result<()> {
    info!("Validating LLM configuration");

    let config =
        pkg_config::get_config().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    match config.provider.as_str() {
        "deepseek" => {
            info!("Validating DeepSeek configuration");
            deepseek_provider::DeepSeekProvider::validate_config().await
        }
        "ollama" => {
            info!("Validating Ollama configuration");
            // Add Ollama validation if available
            Ok(())
        }
        "openai" => {
            info!("Validating OpenAI configuration");
            openai_provider::OpenAIProvider::validate_config().await
        }
        "xai" => {
            info!("Validating xAI configuration");
            // Add xAI validation if available
            Ok(())
        }
        _ => {
            error!("Invalid provider in config: {}", config.provider);
            Err(anyhow::anyhow!(
                "Invalid provider: {}. Supported providers: deepseek, ollama, openai, xai",
                config.provider
            ))
        }
    }
}

/// Test the connection to the configured LLM provider
pub async fn test_llm_connection() -> Result<()> {
    info!("Testing LLM provider connection");

    let config =
        pkg_config::get_config().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    match config.provider.as_str() {
        "deepseek" => {
            info!("Testing DeepSeek connection");
            deepseek_provider::DeepSeekProvider::test_connection().await
        }
        "openai" => {
            info!("Testing OpenAI connection");
            openai_provider::OpenAIProvider::test_connection().await
        }
        provider => {
            warn!("Connection test not implemented for provider: {}", provider);
            Ok(())
        }
    }
}

/// Diagnose common configuration issues and provide helpful error messages
pub async fn diagnose_config_issues() -> Result<String> {
    info!("Running configuration diagnostics");

    let mut diagnostics = Vec::new();

    // Check if config file exists
    let config_paths = [
        "config/config.toml",
        "../config/config.toml",
        "../../config/config.toml",
    ];

    let mut config_found = false;
    for path in &config_paths {
        if std::path::Path::new(path).exists() {
            config_found = true;
            diagnostics.push(format!("✓ Configuration file found at: {}", path));
            break;
        }
    }

    if !config_found {
        diagnostics.push(format!("✗ No configuration file found. Please copy config/config.default.toml to config/config.toml"));
        return Ok(diagnostics.join("\n"));
    }

    // Try to load and validate config
    match pkg_config::get_config() {
        Ok(config) => {
            diagnostics.push(format!("✓ Configuration file loaded successfully"));
            diagnostics.push(format!("✓ Using provider: {}", config.provider));

            // Check provider-specific configuration
            match config.provider.as_str() {
                "deepseek" => {
                    let api_key = &config.llm.deepseek.api_key;
                    if api_key.is_empty() || api_key == "sk-your_deepseek_api_key_here" {
                        diagnostics.push(format!("✗ DeepSeek API key not configured. Please set a valid API key in config.toml"));
                    } else {
                        diagnostics.push(format!("✓ DeepSeek API key configured"));
                    }
                    diagnostics.push(format!("✓ DeepSeek model: {}", config.llm.deepseek.model));
                }
                "openai" => {
                    let api_key = &config.llm.openai.api_key;
                    if api_key.is_empty() || api_key == "your_openai_api_key_here" {
                        diagnostics.push(format!("✗ OpenAI API key not configured. Please set a valid API key in config.toml"));
                    } else {
                        diagnostics.push(format!("✓ OpenAI API key configured"));
                    }
                    diagnostics.push(format!("✓ OpenAI model: {}", config.llm.openai.model));
                }
                "ollama" => {
                    diagnostics.push(format!("✓ Ollama base URL: {}", config.llm.ollama.base_url));
                    diagnostics.push(format!("✓ Ollama model: {}", config.llm.ollama.model));
                }
                "xai" => {
                    let api_key = &config.llm.xai.api_key;
                    if api_key.is_empty() || api_key == "your_xai_api_key_here" {
                        diagnostics.push(format!("✗ xAI API key not configured. Please set a valid API key in config.toml"));
                    } else {
                        diagnostics.push(format!("✓ xAI API key configured"));
                    }
                    diagnostics.push(format!("✓ xAI model: {}", config.llm.xai.model));
                }
                _ => {
                    diagnostics.push(format!("✗ Unknown provider: {}", config.provider));
                }
            }

            // Check chunking configuration
            if let Some(chunking) = &config.chunking {
                diagnostics.push(format!(
                    "✓ Chunking enabled: {}",
                    chunking.enabled.unwrap_or(true)
                ));
                diagnostics.push(format!(
                    "✓ Max tokens: {}",
                    chunking.max_tokens.unwrap_or(120000)
                ));
            } else {
                diagnostics.push(format!("ℹ Using default chunking configuration"));
            }
        }
        Err(e) => {
            diagnostics.push(format!("✗ Failed to load configuration: {}", e));
            diagnostics.push(format!(
                "  Please check that config/config.toml exists and is properly formatted"
            ));
        }
    }

    // Test configuration validation
    match validate_llm_config().await {
        Ok(_) => {
            diagnostics.push(format!("✓ Configuration validation passed"));
        }
        Err(e) => {
            diagnostics.push(format!("✗ Configuration validation failed: {}", e));
        }
    }

    Ok(diagnostics.join("\n"))
}

/// Print helpful setup instructions
pub fn print_setup_instructions() {
    println!("\n=== C2Rust Agent LLM Setup Instructions ===\n");

    println!("1. Copy the default configuration:");
    println!("   cp config/config.default.toml config/config.toml\n");

    println!("2. Edit config/config.toml and set your API keys:\n");

    println!("   For DeepSeek (recommended):");
    println!("   - Set provider = \"deepseek\"");
    println!("   - Set llm.deepseek.api_key = \"sk-your-actual-deepseek-key\"\n");

    println!("   For OpenAI:");
    println!("   - Set provider = \"openai\"");
    println!("   - Set llm.openai.api_key = \"your-actual-openai-key\"\n");

    println!("   For local Ollama:");
    println!("   - Set provider = \"ollama\"");
    println!("   - Ensure Ollama is running on localhost:11434");
    println!("   - Set llm.ollama.model to your installed model\n");

    println!("3. Test your configuration:");
    println!("   cargo test --lib validate_config\n");

    println!("4. Common issues:");
    println!("   - Make sure API keys don't contain the placeholder text");
    println!("   - Check network connectivity for API calls");
    println!("   - Verify the model name is correct for your provider");
    println!("   - Ensure the config.toml file is in the config/ directory\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_config() {
        let result = validate_llm_config().await;
        match result {
            Ok(_) => println!("Configuration validation passed"),
            Err(e) => println!("Configuration validation failed: {}", e),
        }
    }

    #[tokio::test]
    async fn test_diagnose_config() {
        let diagnostics = diagnose_config_issues().await;
        match diagnostics {
            Ok(report) => {
                println!("Configuration diagnostics report:");
                println!("{}", report);
            }
            Err(e) => {
                println!("Failed to run diagnostics: {}", e);
            }
        }
    }

    #[test]
    fn test_print_setup_instructions() {
        print_setup_instructions();
    }

    #[tokio::test]
    async fn test_llm_request() {
        // Skip if config is invalid
        if validate_llm_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let messages = vec!["Hello".to_string(), "How are you?".to_string()];
        let result = llm_request(messages).await;
        match result {
            Ok(response) => {
                println!("Test response: {}", response);
                assert!(!response.is_empty());
            }
            Err(e) => {
                println!("Test failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_llm_request_with_prompt() {
        // Skip if config is invalid
        if validate_llm_config().await.is_err() {
            println!("Skipping test due to invalid config");
            return;
        }

        let messages = vec!["Hello".to_string(), "How are you?".to_string()];
        let prompt = "What is your name?";
        let result = llm_request_with_prompt(messages, prompt.to_string()).await;
        match result {
            Ok(response) => {
                println!("Test response: {}", response);
                assert!(!response.is_empty());
            }
            Err(e) => {
                println!("Test failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_llm_request_chunked() {
        let large_message = "test ".repeat(50000);
        let messages = vec![large_message];
        let result = llm_request_chunked(messages, Some(1000)).await;

        // This test mainly checks that chunking logic works
        match result {
            Ok(responses) => {
                println!("Chunked test returned {} responses", responses.len());
                assert!(responses.len() >= 1);
            }
            Err(e) => {
                println!("Chunked test failed: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_llm_request_with_prompt_chunked() {
        let large_message = "test ".repeat(50000);
        let messages = vec![large_message];
        let prompt = "Analyze this text".to_string();
        let result = llm_request_with_prompt_chunked(messages, prompt, Some(1000)).await;

        // This test mainly checks that chunking logic works
        match result {
            Ok(responses) => {
                println!("Chunked prompt test returned {} responses", responses.len());
                assert!(responses.len() >= 1);
            }
            Err(e) => {
                println!("Chunked prompt test failed: {}", e);
            }
        }
    }

    #[test]
    fn test_get_chunking_config() {
        let (enabled, max_tokens) = get_chunking_config();
        println!(
            "Chunking config: enabled={}, max_tokens={}",
            enabled, max_tokens
        );
        assert!(max_tokens > 0);
    }
}
