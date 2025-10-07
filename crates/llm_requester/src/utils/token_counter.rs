use log::{debug, info, warn};

/// Approximate token count based on character count
/// GPT models typically use ~4 characters per token for English text
/// This is a rough estimation for practical purposes
pub fn estimate_token_count(text: &str) -> usize {
    // Count characters and divide by average chars per token
    let char_count = text.chars().count();
    // Use a more conservative estimate of 2.5 characters per token to be safe
    let tokens = (char_count as f64 / 2.5).ceil() as usize;

    if tokens > 100000 {
        warn!(
            "Large token count estimated: {} tokens ({} chars)",
            tokens, char_count
        );
    } else if tokens > 50000 {
        debug!(
            "Moderate token count: {} tokens ({} chars)",
            tokens, char_count
        );
    }

    tokens
}

/// Split a large message into smaller chunks that fit within token limits
pub fn chunk_message(message: &str, max_tokens: usize) -> Vec<String> {
    let estimated_tokens = estimate_token_count(message);

    if estimated_tokens <= max_tokens {
        debug!(
            "Message fits in single chunk: {} tokens <= {}",
            estimated_tokens, max_tokens
        );
        return vec![message.to_string()];
    }

    info!(
        "Chunking message: {} tokens > {} limit, splitting into chunks",
        estimated_tokens, max_tokens
    );

    let mut chunks = Vec::new();
    let lines: Vec<&str> = message.lines().collect();
    let mut current_chunk = String::new();
    let mut current_tokens = 0;

    // Reserve some tokens for system messages and responses
    let chunk_limit = max_tokens.saturating_sub(1000);

    for (line_num, line) in lines.iter().enumerate() {
        let line_tokens = estimate_token_count(line);

        // If adding this line would exceed the limit, save current chunk and start new one
        if current_tokens + line_tokens > chunk_limit && !current_chunk.is_empty() {
            debug!(
                "Chunk boundary at line {}: {} + {} > {}",
                line_num, current_tokens, line_tokens, chunk_limit
            );
            chunks.push(current_chunk.trim().to_string());
            current_chunk = String::new();
            current_tokens = 0;
        }

        // If a single line is too long, split it further
        if line_tokens > chunk_limit {
            warn!(
                "Very long line detected at line {}: {} tokens",
                line_num, line_tokens
            );
            let sub_chunks = split_long_line(line, chunk_limit);
            for sub_chunk in sub_chunks {
                if !current_chunk.is_empty() {
                    chunks.push(current_chunk.trim().to_string());
                    current_chunk = String::new();
                    current_tokens = 0;
                }
                chunks.push(sub_chunk);
            }
        } else {
            if !current_chunk.is_empty() {
                current_chunk.push('\n');
            }
            current_chunk.push_str(line);
            current_tokens += line_tokens;
        }
    }

    // Add the last chunk if it's not empty
    if !current_chunk.is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    // Ensure we have at least one chunk
    if chunks.is_empty() {
        warn!("No chunks created, using original message as fallback");
        chunks.push(message.to_string());
    }

    info!("Message split into {} chunks", chunks.len());
    for (i, chunk) in chunks.iter().enumerate() {
        debug!("Chunk {}: {} tokens", i + 1, estimate_token_count(chunk));
    }

    chunks
}

/// Split a very long line into smaller pieces
fn split_long_line(line: &str, max_tokens: usize) -> Vec<String> {
    let max_chars = max_tokens * 2; // More conservative estimate
    let mut chunks = Vec::new();

    if line.len() <= max_chars {
        return vec![line.to_string()];
    }

    let words: Vec<&str> = line.split_whitespace().collect();
    let mut current_chunk = String::new();

    for word in words {
        if word.len() > max_chars {
            // If a single word is too long, split it by characters
            if !current_chunk.is_empty() {
                chunks.push(current_chunk.trim().to_string());
                current_chunk = String::new();
            }

            let word_chars: Vec<char> = word.chars().collect();
            for chunk_chars in word_chars.chunks(max_chars) {
                chunks.push(chunk_chars.iter().collect());
            }
        } else {
            let test_chunk = if current_chunk.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_chunk, word)
            };

            if test_chunk.len() > max_chars {
                chunks.push(current_chunk.trim().to_string());
                current_chunk = word.to_string();
            } else {
                current_chunk = test_chunk;
            }
        }
    }

    if !current_chunk.is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    chunks
}

/// Split messages into chunks and prepare them for multiple API calls
pub fn prepare_chunked_messages(
    messages: Vec<String>,
    system_prompt: &str,
    max_tokens: usize,
) -> Vec<(Vec<String>, String)> {
    let mut result = Vec::new();

    // Estimate tokens for system prompt
    let system_tokens = estimate_token_count(system_prompt);
    let available_tokens = max_tokens
        .saturating_sub(system_tokens)
        .saturating_sub(1000); // Reserve for response

    for (msg_idx, message) in messages.iter().enumerate() {
        let chunks = chunk_message(&message, available_tokens);

        if chunks.len() > 1 {
            info!("Message {} split into {} chunks", msg_idx + 1, chunks.len());
        }

        for (i, chunk) in chunks.iter().enumerate() {
            let chunk_prompt = if chunks.len() > 1 {
                format!(
                    "{}\n\nNote: This is part {} of {} of the input.",
                    system_prompt,
                    i + 1,
                    chunks.len()
                )
            } else {
                system_prompt.to_string()
            };

            result.push((vec![chunk.clone()], chunk_prompt));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_token_count() {
        let text = "Hello world";
        let tokens = estimate_token_count(text);
        assert!(tokens > 0);
        assert!(tokens < 10); // Should be around 2-3 tokens
    }

    #[test]
    fn test_chunk_message_small() {
        let message = "This is a small message";
        let chunks = chunk_message(message, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], message);
    }

    #[test]
    fn test_chunk_message_large() {
        let large_message = "word ".repeat(10000);
        let chunks = chunk_message(&large_message, 1000);
        assert!(chunks.len() > 1);

        // Verify all chunks are within reasonable size
        for chunk in &chunks {
            let tokens = estimate_token_count(chunk);
            assert!(tokens <= 1000);
        }
    }

    #[test]
    fn test_prepare_chunked_messages() {
        let messages = vec!["Short message".to_string()];
        let system_prompt = "You are a helpful assistant";
        let prepared = prepare_chunked_messages(messages, system_prompt, 4000);

        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].0.len(), 1);
    }
}
