use anyhow::Result;
use rig::{client::CompletionClient, providers::openai};

#[tokio::main]
async fn main() -> Result<()> {
    // Test basic OpenAI usage
    let client = openai::Client::new("fake-key");
    let gpt4 = client.agent("gpt-3.5-turbo").build();

    println!("Agent created successfully");
    println!("Agent type: {:?}", std::any::type_name_of_val(&gpt4));

    // Test with custom base URL (for DeepSeek/xAI)
    let custom_client = openai::ClientBuilder::new("fake-key")
        .base_url("https://api.deepseek.com/v1")
        .build()
        .expect("Failed to build client");

    let custom_agent = custom_client.agent("deepseek-chat").build();
    println!("Custom agent created successfully");
    println!(
        "Custom agent type: {:?}",
        std::any::type_name_of_val(&custom_agent)
    );

    // This would fail with fake key, but shows the API
    // let response = gpt4.prompt("Hello, world!").await?;
    // println!("Response: {}", response);

    Ok(())
}
