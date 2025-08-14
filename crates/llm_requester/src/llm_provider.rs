use anyhow::Result;
use siumai::models;
use siumai::prelude::*;

// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // Build a unified client, backed by Anthropic
//     let client = Siumai::builder()
//         .anthropic()
//         .api_key("your-anthropic-key")
//         .model(models::anthropic::CLAUDE_SONNET_3_5)
//         .build()
//         .await?;

//     // Your code uses the standard Siumai interface
//     let request = vec![user!("What is the capital of France?")];
//     let response = client.chat(request).await?;

//     // If you decide to switch to OpenAI, you only change the builder.
//     // The `.chat(request)` call remains identical.
//     println!(
//         "The unified client says: {}",
//         response.text().unwrap_or_default()
//     );
//     Ok(())
// }

// pub struct LLM_Provider {
//     llm_provider: Siumai,
// }

// impl LLM_Provider {
//     pub async fn new() -> Result(LLM_Provider) {
//         let llm_provider = Siumai::builder()
//             .anthropic()
//             .api_key("your-anthropic-key")
//             .model(models::anthropic::CLAUDE_SONNET_3_5)
//             .build()
//             .await?;

//         Ok(LLM_Provider { llm_provider })
//     }
// }
