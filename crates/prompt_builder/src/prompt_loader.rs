//! Prompt template loader
//!
//! Simple, stupid file reader. No fancy template engines, just read files.
//! Following Linus's KISS principle.

use anyhow::{Context, Result};
use log::debug;
use std::path::PathBuf;
use tokio::fs;

/// Prompt loader for reading prompt templates from config directory
pub struct PromptLoader {
    prompts_dir: PathBuf,
}

impl PromptLoader {
    /// Create a new PromptLoader with the given prompts directory
    pub fn new(prompts_dir: PathBuf) -> Self {
        Self { prompts_dir }
    }

    /// Create a PromptLoader with default config/prompts directory
    pub fn default() -> Result<Self> {
        let prompts_dir = PathBuf::from("config/prompts");
        if !prompts_dir.exists() {
            anyhow::bail!(
                "Prompts directory not found: {}. Create it and add prompt templates.",
                prompts_dir.display()
            );
        }
        Ok(Self { prompts_dir })
    }

    /// Load file conversion prompt template
    pub async fn load_file_conversion_prompt(&self) -> Result<String> {
        self.load_prompt_file("file_conversion.md").await
    }

    /// Load function conversion prompt template
    pub async fn load_function_conversion_prompt(&self) -> Result<String> {
        self.load_prompt_file("function_conversion.md").await
    }

    /// Load Linus role definition prompt
    pub async fn load_linus_role_prompt(&self) -> Result<String> {
        self.load_prompt_file("linus_role.md").await
    }

    /// Generic method to load any prompt file
    pub async fn load_prompt_file(&self, filename: &str) -> Result<String> {
        let path = self.prompts_dir.join(filename);
        debug!("Loading prompt from: {}", path.display());

        let content = fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read prompt file: {}", path.display()))?;

        Ok(content)
    }

    /// Check if a specific prompt file exists
    pub fn prompt_exists(&self, filename: &str) -> bool {
        self.prompts_dir.join(filename).exists()
    }

    /// List all available prompt files
    pub async fn list_prompts(&self) -> Result<Vec<String>> {
        let mut prompts = Vec::new();
        let mut entries = fs::read_dir(&self.prompts_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if let Some(filename) = entry.file_name().to_str() {
                if filename.ends_with(".md") {
                    prompts.push(filename.to_string());
                }
            }
        }

        Ok(prompts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn get_test_prompts_dir() -> PathBuf {
        // Try to find config/prompts from current dir or parent dirs
        let mut current = env::current_dir().unwrap();
        loop {
            let candidate = current.join("config/prompts");
            if candidate.exists() {
                return candidate;
            }
            if !current.pop() {
                break;
            }
        }
        PathBuf::from("config/prompts")
    }

    #[tokio::test]
    async fn test_load_file_conversion_prompt() {
        let prompts_dir = get_test_prompts_dir();
        if !prompts_dir.exists() {
            // Skip test if prompts dir not found (e.g., in CI without setup)
            return;
        }

        let loader = PromptLoader::new(prompts_dir);
        let result = loader.load_file_conversion_prompt().await;
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.is_empty());
        // Just check it's a markdown file with some content
        assert!(content.contains("Rust") || content.contains("conversion"));
    }

    #[tokio::test]
    async fn test_load_function_conversion_prompt() {
        let prompts_dir = get_test_prompts_dir();
        if !prompts_dir.exists() {
            return;
        }

        let loader = PromptLoader::new(prompts_dir);
        let result = loader.load_function_conversion_prompt().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_prompt_exists() {
        let prompts_dir = get_test_prompts_dir();
        if !prompts_dir.exists() {
            return;
        }

        let loader = PromptLoader::new(prompts_dir);
        assert!(loader.prompt_exists("file_conversion.md"));
        assert!(!loader.prompt_exists("nonexistent.md"));
    }
}
