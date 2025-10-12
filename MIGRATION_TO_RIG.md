# Migration from Siumai to Rig

This document describes the migration of the LLM requester system from the `siumai` crate to the `rig-core` crate.

## Overview

The migration was completed to improve stability, performance, and API consistency across different LLM providers. Rig provides a more mature and feature-rich framework for working with LLMs in Rust.

## Changes Made

### 1. Dependency Updates

**Before (Cargo.toml):**
```toml
siumai = {git = "https://github.com/YumchaLabs/siumai.git"}
```

**After (Cargo.toml):**
```toml
rig-core = "0.21.0"
async-trait = "0.1.83"
```

### 2. Provider Architecture

#### Old Siumai-based Architecture
- Used `Siumai::builder()` with provider-specific builders
- Direct chat message handling with macros like `user!()` and `system!()`
- Provider-specific client configurations

#### New Rig-based Architecture
- Uses `openai::Client` and `openai::ClientBuilder` for all providers (OpenAI-compatible)
- Unified `Agent<ResponsesCompletionModel>` interface
- Consistent API across all providers through trait abstraction

### 3. Provider Implementations

All providers now follow a consistent pattern:

```rust
use rig::{agent::Agent, client::CompletionClient, completion::Prompt, providers::openai};
use rig::providers::openai::responses_api::ResponsesCompletionModel;

pub struct ProviderName {
    agent: Agent<ResponsesCompletionModel>,
}

impl ProviderName {
    pub async fn new(config: Config) -> Result<Self> {
        let client = openai::ClientBuilder::new(&config.api_key)
            .base_url(&config.base_url) // For non-OpenAI providers
            .build()?;
        
        let agent = client.agent(&config.model).build();
        Ok(Self { agent })
    }
}
```

#### Supported Providers

1. **OpenAI** - Direct OpenAI API integration
2. **DeepSeek** - Uses OpenAI-compatible API at `https://api.deepseek.com/v1`
3. **xAI** - Uses OpenAI-compatible API at `https://api.x.ai/v1`
4. **Ollama** - Local deployment using OpenAI-compatible API at `http://localhost:11434/v1`

### 4. API Changes

#### Message Handling
**Before:**
```rust
let request = vec![user!(message), system!(system_prompt)];
let response = client.chat_with_tools(request, None).await?;
let text = response.text().unwrap_or_default();
```

**After:**
```rust
let prompt = format!("System: {}\n\nUser: {}", system_prompt, message);
let response = agent.prompt(&prompt).await?;
```

#### Error Handling
- Improved error messages with provider-specific context
- Better handling of network timeouts and rate limiting
- More detailed diagnostic information

### 5. Configuration

No changes were required to the configuration format. All existing config files remain compatible:

```toml
provider = "deepseek"  # or "openai", "xai", "ollama"

[llm.deepseek]
model = "deepseek-chat"
api_key = "sk-your-deepseek-key"

[llm.openai]
model = "gpt-4"
api_key = "your-openai-key"

[llm.xai]
model = "grok-beta"
api_key = "your-xai-key"

[llm.ollama]
model = "llama2"
base_url = "http://localhost:11434"
api_key = "not-needed"
```

### 6. New Features

#### Trait-based Provider Interface
Added `LLMProvider` trait for consistent provider behavior:

```rust
#[async_trait::async_trait]
pub trait LLMProvider: Send + Sync {
    async fn init_with_config() -> Result<Box<dyn LLMProvider>>;
    async fn chat(&self, messages: Vec<String>) -> Result<String>;
    async fn chat_with_prompt(&self, messages: Vec<String>, system_prompt: String) -> Result<String>;
    // ... other methods
}
```

#### Enhanced Testing
- Provider validation methods
- Connection testing capabilities
- Comprehensive diagnostic functions

### 7. Backward Compatibility

All existing public APIs remain unchanged:

- `llm_request(messages: Vec<String>) -> Result<String>`
- `llm_request_with_prompt(messages: Vec<String>, prompt: String) -> Result<String>`
- `validate_llm_config() -> Result<()>`
- `test_llm_connection() -> Result<()>`
- All chunking and retry functionality

### 8. Benefits of Migration

1. **Stability**: Rig is a more mature and actively maintained library
2. **Performance**: Better handling of streaming and concurrent requests
3. **Extensibility**: Easier to add new providers through OpenAI-compatible APIs
4. **Error Handling**: More detailed error messages and better debugging
5. **Testing**: Comprehensive test suite and validation tools
6. **Documentation**: Better API documentation and examples

### 9. Testing the Migration

Run the test suite to verify the migration:

```bash
# Test basic functionality
cargo test --lib

# Test with real providers (requires API keys)
cargo run --example test_providers

# Test basic rig functionality
cargo run --example rig_test
```

Set environment variables for testing:
```bash
export OPENAI_API_KEY="your-openai-key"
export DEEPSEEK_API_KEY="sk-your-deepseek-key" 
export XAI_API_KEY="your-xai-key"
```

### 10. Migration Checklist

- [x] Replace siumai dependency with rig-core
- [x] Update all provider implementations
- [x] Maintain backward compatibility
- [x] Add comprehensive error handling
- [x] Create test examples
- [x] Update documentation
- [x] Verify all tests pass
- [x] Remove unused siumai-related files

### 11. Future Enhancements

With rig as the foundation, future enhancements can include:

- Streaming response support
- Tool/function calling capabilities
- Enhanced prompt templating
- Vector embedding integration
- RAG (Retrieval Augmented Generation) support

## Troubleshooting

### Common Issues

1. **Import Errors**: Make sure to import the necessary traits:
   ```rust
   use rig::{agent::Agent, client::CompletionClient, completion::Prompt};
   ```

2. **Type Mismatches**: Use `ResponsesCompletionModel` for the Agent type:
   ```rust
   agent: Agent<ResponsesCompletionModel>
   ```

3. **Configuration Issues**: Validate your configuration using:
   ```rust
   cargo test test_validate_config
   ```

### Getting Help

- Check the [rig documentation](https://docs.rs/rig-core/latest/rig/)
- Run diagnostic tools: `cargo test test_diagnose_config`
- Use the test examples to verify your setup

## Conclusion

The migration to rig provides a solid foundation for LLM integration while maintaining full backward compatibility. All existing code should continue to work without modifications, while new code can take advantage of rig's enhanced capabilities.