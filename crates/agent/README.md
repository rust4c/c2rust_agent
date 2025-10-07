# Agent - Intelligent C to Rust Translation Agent

## Overview

The Agent crate provides a unified, intelligent agent system for translating C code to Rust. Each agent is designed to handle a single project (like chibicc) and can work independently or coordinate with other agents for complex multi-module projects.

## Key Features

- **Active Information Gathering**: Source code analysis, web search for error solutions, database queries
- **Precise File Modification**: Line-by-line editing, function replacement, project structure management
- **AI-Powered Translation**: Context-aware prompt building and LLM integration
- **Inter-Agent Communication**: Message passing for multi-threaded processing
- **Error Analysis**: Automatic error location and solution suggestion

## Architecture

```
Agent
├── File Manager (RustFileManager)     # File operations and project management
├── Database Manager                   # Code storage and retrieval
├── Web Searcher                      # Error solution search
├── Prompt Builder                    # Context-aware AI prompts
└── Message Queue                     # Inter-agent communication
```

## Quick Start

### 1. Basic Agent Setup

```rust
use agent::Agent;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create an agent for a specific project
    let project_path = PathBuf::from("/path/to/chibicc_cache/individual_files/chibicc");
    
    let mut agent = Agent::new(
        "chibicc".to_string(),
        project_path,
        None, // Use project_path as cache_path
    ).await?;

    // Initialize components
    agent.initialize_file_manager().await?;
    agent.initialize_prompt_builder().await?;
    
    Ok(())
}
```

### 2. Information Gathering

```rust
// Analyze C source code
let source_info = agent.gather_source_info(&Path::new("main.c")).await?;
println!("Functions: {:?}", source_info.functions);
println!("Complexity: {:.2}", source_info.complexity_score);

// Search for error solutions
let solution = agent.search_error_solution("error[E0382]: use of moved value").await?;
println!("Found {} solutions", solution.solutions.len());
```

### 3. Code Translation

```rust
// Translate C code to Rust
let result = agent.translate_code(
    &Path::new("tokenizer.c"),
    Some("compilation errors from previous attempt") // Optional
).await?;

println!("Generated Rust code: {}", result.rust_code);
println!("Required crates: {:?}", result.cargo_dependencies);
```

### 4. File Modification

```rust
// Modify entire file
agent.modify_file(&Path::new("src/main.rs"), &new_rust_code).await?;

// Modify specific lines
agent.modify_file_section(&Path::new("src/lib.rs"), 10, 20, &new_function).await?;
```

### 5. Inter-Agent Communication

```rust
// Send message to other agents
agent.send_message(AgentMessage {
    from_agent: agent.agent_id.clone(),
    to_agent: None, // Broadcast
    message_type: MessageType::FileModified,
    content: "Updated tokenizer module".to_string(),
    metadata: HashMap::new(),
    timestamp: chrono::Utc::now(),
}).await;

// Request help from other agents
agent.request_help(
    "Complex pointer arithmetic in parse.c line 245",
    Some("Context: parsing function declarations")
).await?;
```

## Project Structure Requirements

The Agent expects a specific directory structure based on the cache directory pattern:

```
/Users/peng/Documents/Tmp/chibicc_cache/
├── individual_files/
│   ├── chibicc/
│   │   ├── chibicc.c           # Original C source
│   │   ├── chibicc.h           # Original C header
│   │   ├── two-stage-translation/
│   │   └── final-output/
│   ├── tokenize/
│   ├── parse/
│   └── ...
├── mapping.json                # File path mappings
└── paired_files/
```

Each agent handles one project directory (e.g., `individual_files/chibicc`).

## Configuration

### Environment Setup

Ensure these environment variables or config files are set up:

- **Database Configuration**: SQLite and Qdrant connections
- **LLM Provider**: DeepSeek, OpenAI, or other provider settings
- **Prompt Templates**: Located in `config/prompts/`

### Prompt Templates

The agent uses these templates from `config/prompts/`:

- `file_conversion.md`: Main C-to-Rust translation instructions
- `function_conversion.md`: Function-level translation
- `linus_role.md`: Code review and quality standards
- `web_search.md`: Error solution search guidance

## Multi-Agent Processing

For large projects, create multiple agents and coordinate them:

```rust
use tokio::task;

// Create agents for different modules
let agents = vec![
    ("chibicc", path.join("chibicc")),
    ("tokenize", path.join("tokenize")),
    ("parse", path.join("parse")),
    ("codegen", path.join("codegen")),
];

let mut handles = Vec::new();

for (name, module_path) in agents {
    let handle = task::spawn(async move {
        let mut agent = Agent::new(name.to_string(), module_path, None).await?;
        
        // Initialize agent
        agent.initialize_file_manager().await?;
        agent.initialize_prompt_builder().await?;
        
        // Process the module
        let c_file = agent.config.project_path.join(format!("{}.c", name));
        if c_file.exists() {
            let result = agent.translate_code(&c_file, None).await?;
            
            // Save result
            let rust_file = agent.config.project_path.join("src").join("main.rs");
            agent.modify_file(&rust_file, &result.rust_code).await?;
        }
        
        Ok::<(), anyhow::Error>(())
    });
    
    handles.push(handle);
}

// Wait for all agents to complete
for handle in handles {
    handle.await??;
}
```

## Error Handling

The agent provides comprehensive error handling and recovery:

```rust
// Handle translation errors
match agent.translate_code(&source_file, None).await {
    Ok(result) => {
        // Check compilation status
        match result.compilation_status {
            CompilationStatus::Success => println!("Translation successful!"),
            CompilationStatus::Failed(error) => {
                // Retry with error context
                let retry_result = agent.translate_code(&source_file, Some(&error)).await?;
                // Handle retry...
            },
            CompilationStatus::Warning(warning) => {
                println!("Translation succeeded with warnings: {}", warning);
            },
            _ => {}
        }
    },
    Err(e) => {
        // Request help from other agents
        agent.request_help(&format!("Translation failed: {}", e), None).await?;
    }
}
```

## Best Practices

### 1. Resource Management
- Initialize file manager only when needed
- Close database connections properly
- Use appropriate timeout settings for LLM calls

### 2. Error Recovery
- Always provide compilation errors for retry attempts
- Use incremental translation for large files
- Implement fallback strategies for AI failures

### 3. Inter-Agent Coordination
- Use meaningful message types and content
- Include sufficient context in help requests
- Implement proper message routing for directed communication

### 4. Performance Optimization
- Cache frequently used prompt templates
- Batch database operations when possible
- Use parallel processing for independent modules

## API Reference

### Core Types

- `Agent`: Main agent struct
- `ProjectConfig`: Configuration for agent behavior
- `TranslationResult`: Output from code translation
- `AgentMessage`: Inter-agent communication message
- `SourceInfo`: Analysis results from source code
- `ErrorLocation`: Error analysis and location info

### Key Methods

- `Agent::new()`: Create new agent instance
- `gather_source_info()`: Analyze source code
- `translate_code()`: Perform C to Rust translation
- `search_error_solution()`: Find solutions for compilation errors
- `modify_file()`: Update files with new content
- `send_message()` / `receive_messages()`: Inter-agent communication

## Examples

Run the demo to see the agent in action:

```bash
cargo run --example agent_demo
```

This demonstrates:
- Agent creation and initialization
- Information gathering from C source
- Error solution search
- Mock translation process
- Inter-agent messaging
- Status monitoring

## Integration with Existing Code

The Agent is designed to replace `ai_optimizer.rs` while maintaining compatibility:

```rust
// Old way (ai_optimizer.rs)
let result = ai_optimize_rust_code(
    Some(&rust_path),
    &c_path,
    &output_dir,
    Some(&compile_errors)
).await?;

// New way (Agent)
let mut agent = Agent::new("project".to_string(), project_path, None).await?;
agent.initialize_file_manager().await?;
agent.initialize_prompt_builder().await?;

let result = agent.translate_code(&c_path, Some(&compile_errors)).await?;
```

## Contributing

When extending the Agent:

1. **Follow Linus's Principles**: Simple data structures, eliminate special cases
2. **Maintain Thread Safety**: Use proper Arc<Mutex<>> patterns
3. **Add Comprehensive Tests**: Include both unit and integration tests
4. **Document New Features**: Update this README and add examples

## License

This crate is part of the c2rust_agent project and follows the same licensing terms.