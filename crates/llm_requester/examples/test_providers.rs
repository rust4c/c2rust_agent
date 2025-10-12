use anyhow::Result;
use rig::{client::CompletionClient, completion::Prompt, providers::openai};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Testing Rig-based LLM Providers ===\n");

    // Test OpenAI provider
    if let Ok(api_key) = env::var("OPENAI_API_KEY") {
        if !api_key.is_empty() && api_key != "your_openai_api_key_here" {
            println!("Testing OpenAI provider...");
            match test_openai_provider(&api_key).await {
                Ok(response) => println!("✓ OpenAI test successful: {}", response),
                Err(e) => println!("✗ OpenAI test failed: {}", e),
            }
        } else {
            println!("⚠ Skipping OpenAI test (no valid API key)");
        }
    } else {
        println!("⚠ Skipping OpenAI test (OPENAI_API_KEY not set)");
    }
    println!();

    // Test DeepSeek provider
    if let Ok(api_key) = env::var("DEEPSEEK_API_KEY") {
        if !api_key.is_empty() && api_key != "sk-your_deepseek_api_key_here" {
            println!("Testing DeepSeek provider...");
            match test_deepseek_provider(&api_key).await {
                Ok(response) => println!("✓ DeepSeek test successful: {}", response),
                Err(e) => println!("✗ DeepSeek test failed: {}", e),
            }
        } else {
            println!("⚠ Skipping DeepSeek test (no valid API key)");
        }
    } else {
        println!("⚠ Skipping DeepSeek test (DEEPSEEK_API_KEY not set)");
    }
    println!();

    // Test xAI provider
    if let Ok(api_key) = env::var("XAI_API_KEY") {
        if !api_key.is_empty() && api_key != "your_xai_api_key_here" {
            println!("Testing xAI provider...");
            match test_xai_provider(&api_key).await {
                Ok(response) => println!("✓ xAI test successful: {}", response),
                Err(e) => println!("✗ xAI test failed: {}", e),
            }
        } else {
            println!("⚠ Skipping xAI test (no valid API key)");
        }
    } else {
        println!("⚠ Skipping xAI test (XAI_API_KEY not set)");
    }
    println!();

    // Test Ollama provider (if running locally)
    println!("Testing Ollama provider...");
    match test_ollama_provider().await {
        Ok(response) => println!("✓ Ollama test successful: {}", response),
        Err(e) => println!("✗ Ollama test failed (may not be running): {}", e),
    }
    println!();

    println!("=== All provider tests completed ===");
    Ok(())
}

async fn test_openai_provider(api_key: &str) -> Result<String> {
    let client = openai::Client::new(api_key);
    let agent = client.agent("gpt-3.5-turbo").build();

    let response = agent
        .prompt("Say 'Hello from OpenAI!' and nothing else.")
        .await?;

    Ok(response)
}

async fn test_deepseek_provider(api_key: &str) -> Result<String> {
    let client = openai::ClientBuilder::new(api_key)
        .base_url("https://api.deepseek.com/v1")
        .build()?;

    let agent = client.agent("deepseek-chat").build();

    let response = agent
        .prompt("Say 'Hello from DeepSeek!' and nothing else.")
        .await?;

    Ok(response)
}

async fn test_xai_provider(api_key: &str) -> Result<String> {
    let client = openai::ClientBuilder::new(api_key)
        .base_url("https://api.x.ai/v1")
        .build()?;

    let agent = client.agent("grok-beta").build();

    let response = agent
        .prompt("Say 'Hello from xAI!' and nothing else.")
        .await?;

    Ok(response)
}

async fn test_ollama_provider() -> Result<String> {
    let client = openai::ClientBuilder::new("not-needed")
        .base_url("http://localhost:11434/v1")
        .build()?;

    let agent = client.agent("llama2").build();

    let response = agent
        .prompt("Say 'Hello from Ollama!' and nothing else.")
        .await?;

    Ok(response)
}
