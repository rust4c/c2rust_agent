# Automatic Message Chunking for Large Context Handling

This document describes the automatic chunking system implemented to handle large contexts that exceed LLM token limits.

## Problem

When translating large C files or processing extensive codebases, the context (including source code, dependencies, and prompts) can exceed the maximum token limit of LLM providers. For example:

- OpenAI GPT models have a context limit of ~131K tokens
- DeepSeek models have similar limitations
- Large C files with dependencies can easily exceed these limits

## Solution

The chunking system automatically detects when a request would exceed token limits and splits it into smaller, manageable chunks.

## Key Components

### 1. Token Estimation (`utils/token_counter.rs`)

```rust
pub fn estimate_token_count(text: &str) -> usize
```

- Uses a conservative estimate of 2.5 characters per token
- Provides rough but safe token counting
- Includes logging for large contexts

### 2. Message Chunking

```rust
pub fn chunk_message(message: &str, max_tokens: usize) -> Vec<String>
```

- Splits messages at line boundaries when possible
- Handles very long lines by word boundaries
- Preserves context as much as possible
- Reserves tokens for system prompts and responses

### 3. Automatic Detection

The main `llm_request_with_prompt` function automatically:
1. Estimates total token count (messages + prompt)
2. If > 100K tokens, uses chunked approach
3. Otherwise, uses normal single request

## Configuration

Add to your `config/config.toml`:

```toml
[chunking]
enabled = true
max_tokens = 100000
chunk_overlap = 100
```

### Options

- `enabled`: Enable/disable automatic chunking (default: true)
- `max_tokens`: Maximum tokens per chunk (default: 100,000)
- `chunk_overlap`: Reserved overlap between chunks (default: 100)

## Usage

### Automatic (Recommended)

The chunking is completely transparent:

```rust
use llm_requester::llm_request_with_prompt;

// This automatically handles chunking if needed
let response = llm_request_with_prompt(large_messages, prompt).await?;
```

### Manual Chunking

For more control:

```rust
use llm_requester::{llm_request_with_prompt_chunked, utils};

// Force chunked approach
let responses = llm_request_with_prompt_chunked(
    messages, 
    prompt, 
    Some(80000)  // Custom token limit
).await?;

// Combine responses as needed
let combined = responses.join("\n\n--- SECTION ---\n\n");
```

## Token Limits by Provider

| Provider | Model | Context Limit | Safe Chunk Size |
|----------|-------|---------------|-----------------|
| OpenAI | GPT-3.5/4 | 131,072 | 100,000 |
| DeepSeek | deepseek-chat | ~131,072 | 100,000 |
| Other | Various | Check docs | 80,000 |

## How It Works

1. **Detection**: Every request estimates total tokens
2. **Splitting**: Large contexts split at natural boundaries
3. **Processing**: Each chunk processed separately
4. **Combination**: Responses combined with clear separators

### Example Flow

```
Original Request (150K tokens)
    ↓
Detect: > 100K tokens
    ↓
Split into 2 chunks (75K each)
    ↓
Process chunk 1 → Response 1
Process chunk 2 → Response 2
    ↓
Combine: "Response 1\n\n--- CHUNK BOUNDARY ---\n\nResponse 2"
```

## Performance Impact

- **Latency**: Multiple API calls increase total time
- **Cost**: May increase token usage due to prompt repetition
- **Quality**: Some context may be lost between chunks

## Best Practices

### 1. Configure Conservative Limits

```toml
[chunking]
max_tokens = 80000  # Well below API limits
```

### 2. Monitor Chunking Events

Look for log messages:
```
INFO: Large context detected (152904 tokens), using chunked requests
INFO: Message split into 3 chunks
```

### 3. Design Prompts for Chunking

- Make prompts self-contained per chunk
- Include important context in each chunk
- Design responses that can be meaningfully combined

### 4. Test with Large Inputs

Verify your workflow handles chunked responses correctly.

## Error Handling

The system provides graceful fallbacks:

1. **Token estimation fails**: Uses character count approximation
2. **Chunking disabled**: Attempts single request (may fail at API)
3. **Chunk too large**: Further subdivides automatically

## Debugging

### Enable Detailed Logging

```bash
RUST_LOG=llm_requester=debug your_application
```

### Check Token Estimates

```rust
use llm_requester::utils::estimate_token_count;

let tokens = estimate_token_count(&your_text);
println!("Estimated tokens: {}", tokens);
```

### Verify Configuration

```rust
use llm_requester::pkg_config::get_config;

let config = get_config()?;
let chunking = config.chunking.unwrap_or_default();
println!("Chunking enabled: {}", chunking.enabled.unwrap_or(true));
```

## Limitations

1. **Context Loss**: Information may not flow between chunks
2. **Response Quality**: May be inconsistent across chunks  
3. **Cost**: Multiple API calls increase usage
4. **Complexity**: Harder to debug chunked vs single requests

## Migration Guide

### From Non-Chunked Code

No changes needed! Existing code automatically benefits:

```rust
// This code remains unchanged but now handles large contexts
let response = llm_request_with_prompt(messages, prompt).await?;
```

### Custom Chunking Logic

Replace manual chunking with the automatic system:

```rust
// Before: Manual chunking
for chunk in manual_chunks {
    let response = llm_request_with_prompt(vec![chunk], prompt).await?;
    // ...
}

// After: Automatic chunking
let response = llm_request_with_prompt(all_messages, prompt).await?;
```

## Future Improvements

- [ ] Semantic chunking (preserve logical boundaries)
- [ ] Cross-chunk context preservation
- [ ] Dynamic token limit detection per model
- [ ] Parallel chunk processing
- [ ] Response quality metrics

## Troubleshooting

### Still Getting Token Limit Errors?

1. Check if chunking is enabled in config
2. Verify token limits are conservative enough
3. Look for direct provider calls bypassing chunking
4. Consider more aggressive token estimation

### Poor Response Quality?

1. Increase chunk overlap
2. Redesign prompts for chunk-friendly processing
3. Use manual chunking at logical boundaries
4. Consider if the task requires full context

### Performance Issues?

1. Increase chunk size (within limits)
2. Use async processing where possible
3. Cache repeated prompt processing
4. Profile token estimation overhead