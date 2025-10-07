//! Agent Demo - Example usage of the C to Rust Translation Agent
//!
//! This example demonstrates how to use the Agent for C to Rust translation
//! in the context of a project like chibicc.
//!
//! Usage: cargo run --example agent_demo

use agent::{Agent, ProjectConfig};
use std::path::PathBuf;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🚀 C to Rust Translation Agent Demo");
    println!("=====================================");

    // Example 1: Create an agent for a chibicc project
    let project_path =
        PathBuf::from("/Users/peng/Documents/Tmp/chibicc_cache/individual_files/chibicc");

    println!(
        "\n📁 Creating agent for project: {}",
        project_path.display()
    );

    let mut agent = Agent::new(
        "chibicc".to_string(),
        project_path.clone(),
        Some(project_path.clone()), // cache path same as project path
    )
    .await?;

    println!("✅ Agent created with ID: {}", agent.agent_id);

    // Initialize the agent components
    println!("\n🔧 Initializing agent components...");

    // Initialize file manager for Rust project management
    if let Err(e) = agent.initialize_file_manager().await {
        println!("⚠️  File manager initialization failed: {}", e);
        println!("   This is expected if no Rust project exists yet");
    } else {
        println!("✅ File manager initialized");
    }

    // Initialize prompt builder for AI interactions
    if let Err(e) = agent.initialize_prompt_builder().await {
        println!("⚠️  Prompt builder initialization failed: {}", e);
        println!("   This is expected if database is not properly configured");
    } else {
        println!("✅ Prompt builder initialized");
    }

    // Example 2: Demonstrate information gathering
    println!("\n🔍 Demonstrating information gathering...");

    let c_file = project_path.join("chibicc.c");
    if c_file.exists() {
        match agent.gather_source_info(&c_file).await {
            Ok(source_info) => {
                println!("✅ Gathered source information:");
                println!("   - Functions found: {}", source_info.functions.len());
                println!("   - Includes found: {}", source_info.includes.len());
                println!("   - Complexity score: {:.2}", source_info.complexity_score);
                println!("   - Dependencies: {}", source_info.dependencies.len());

                // Show first few functions
                if !source_info.functions.is_empty() {
                    println!(
                        "   - First few functions: {:?}",
                        source_info.functions.iter().take(3).collect::<Vec<_>>()
                    );
                }
            }
            Err(e) => println!("⚠️  Failed to gather source info: {}", e),
        }
    } else {
        println!("⚠️  Source file not found: {}", c_file.display());
    }

    // Example 3: Demonstrate error analysis
    println!("\n🐛 Demonstrating error analysis...");

    let sample_error = r#"
    error[E0382]: use of moved value: `parser`
      --> src/main.rs:42:5
       |
    40 | let tokens = tokenizer.tokenize();
       |              --------- value moved here
    42 | parser.parse(tokens);
       |        ^^^^^ value used here after move
    "#;

    match agent.search_error_solution(sample_error).await {
        Ok(solution) => {
            println!("✅ Found error solution:");
            println!(
                "   - Error category: {}",
                solution.error_info.error_category
            );
            println!("   - Solutions found: {}", solution.solutions.len());
            println!("   - Code examples: {}", solution.code_examples.len());
            println!("   - Confidence: {:?}", solution.metadata.confidence_level);

            // Show first solution
            if let Some(first_solution) = solution.solutions.first() {
                println!("   - Top solution: {}", first_solution.title);
            }
        }
        Err(e) => println!("⚠️  Error solution search failed: {}", e),
    }

    // Example 4: Demonstrate code translation (mock)
    println!("\n🔄 Demonstrating code translation...");

    let sample_c_code = r#"
    #include <stdio.h>
    #include <stdlib.h>

    int add(int a, int b) {
        return a + b;
    }

    int main() {
        int result = add(5, 3);
        printf("Result: %d\n", result);
        return 0;
    }
    "#;

    // Create a temporary C file for translation
    let temp_c_file = project_path.join("temp_demo.c");
    if let Err(e) = tokio::fs::write(&temp_c_file, sample_c_code).await {
        println!("⚠️  Failed to write temporary C file: {}", e);
    } else {
        // Note: This would normally call the AI, but we'll simulate it
        println!("✅ Created temporary C file for translation demo");
        println!("   - File: {}", temp_c_file.display());
        println!("   - Content preview: First 100 chars of C code");

        // In a real scenario, you would call:
        // let result = agent.translate_code(&temp_c_file, None).await?;

        // Clean up
        let _ = tokio::fs::remove_file(&temp_c_file).await;
        println!("✅ Cleaned up temporary file");
    }

    // Example 5: Demonstrate inter-agent communication
    println!("\n📢 Demonstrating inter-agent communication...");

    // Send a status update message
    agent
        .send_message(agent::AgentMessage {
            from_agent: agent.agent_id.clone(),
            to_agent: None, // Broadcast
            message_type: agent::MessageType::StatusUpdate,
            content: "Demo completed successfully".to_string(),
            metadata: std::collections::HashMap::new(),
            timestamp: chrono::Utc::now(),
        })
        .await;

    // Request help (mock scenario)
    agent
        .request_help(
            "Need assistance with complex pointer arithmetic translation",
            Some("Working on tokenizer.c, line 245"),
        )
        .await?;

    // Check messages
    let messages = agent.receive_messages().await;
    println!("✅ Message system working:");
    println!("   - Messages in queue: {}", messages.len());

    // Example 6: Show agent status
    println!("\n📊 Agent Status:");
    let status = agent.get_status().await;
    println!("   - Agent ID: {}", status.agent_id);
    println!("   - Project: {}", status.project_name);
    println!("   - Current file: {:?}", status.current_file);
    println!("   - Compilation attempts: {}", status.compilation_attempts);
    println!("   - Recent errors: {}", status.recent_errors_count);
    println!("   - File manager ready: {}", status.is_file_manager_ready);
    println!("   - Message queue size: {}", status.message_queue_size);

    println!("\n🎉 Agent demo completed successfully!");
    println!("\nNext steps:");
    println!("1. Set up a real project directory structure");
    println!("2. Configure database connections properly");
    println!("3. Set up prompt templates in config/prompts/");
    println!("4. Use agent.translate_code() for actual C to Rust translation");
    println!("5. Implement proper inter-agent message broker for multi-threading");

    Ok(())
}

/// Example of how multiple agents might work together
#[allow(dead_code)]
async fn multi_agent_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🤖 Multi-Agent Example");
    println!("======================");

    let base_path = PathBuf::from("/Users/peng/Documents/Tmp/chibicc_cache/individual_files");

    // Create agents for different parts of the project
    let agents = vec![
        ("chibicc", base_path.join("chibicc")),
        ("tokenize", base_path.join("tokenize")),
        ("parse", base_path.join("parse")),
        ("codegen", base_path.join("codegen")),
    ];

    let mut agent_handles = Vec::new();

    for (name, path) in agents {
        if path.exists() {
            println!("Creating agent for: {}", name);
            let agent = Agent::new(name.to_string(), path, None).await?;
            agent_handles.push(agent);
        }
    }

    println!(
        "✅ Created {} agents for parallel processing",
        agent_handles.len()
    );

    // In a real implementation, you would:
    // 1. Spawn each agent in its own tokio task
    // 2. Set up a message broker for inter-agent communication
    // 3. Coordinate the translation process
    // 4. Handle dependencies between modules

    Ok(())
}

/// Example configuration for a typical chibicc translation project
#[allow(dead_code)]
fn example_project_config() -> ProjectConfig {
    ProjectConfig {
        project_name: "chibicc".to_string(),
        project_path: PathBuf::from(
            "/Users/peng/Documents/Tmp/chibicc_cache/individual_files/chibicc",
        ),
        cache_path: PathBuf::from("/Users/peng/Documents/Tmp/chibicc_cache"),
        source_language: "c".to_string(),
        target_language: "rust".to_string(),
        max_retry_attempts: 3,
    }
}
