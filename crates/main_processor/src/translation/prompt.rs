//! Prompt handling module for C to Rust translation
//!
//! This module provides functions to build translation prompts using
//! either basic or context-aware approaches based on available database information.

use crate::{read_c_source_files, ProjectInfo};
use anyhow::Result;
use db_services::DatabaseManager;
use log::{debug, warn};
use prompt_builder::PromptBuilder;
use std::path::PathBuf;

/// Build context-aware prompt using database information
pub async fn build_context_aware_prompt(
    project: &ProjectInfo,
    db_manager: &DatabaseManager,
) -> Result<String> {
    debug!(
        "Building context-aware prompt for project: {}",
        project.name
    );

    if let Some(prompt_builder) = create_prompt_builder(project, db_manager).await? {
        // Try to find the main C file to build context for
        let c_files = read_c_source_files(project).await?;
        if let Some(main_file) = c_files.first() {
            // Extract file path from the message content (assuming format "File: path\n...")
            if let Some(file_start) = main_file.find("File: ") {
                if let Some(line_end) = main_file[file_start..].find('\n') {
                    let file_path = &main_file[file_start + 6..file_start + line_end];
                    match prompt_builder
                        .build_file_context_prompt(file_path, None)
                        .await
                    {
                        Ok(context_prompt) => {
                            debug!("Successfully built context prompt for {}", file_path);
                            return Ok(context_prompt);
                        }
                        Err(e) => {
                            warn!("Failed to build context prompt for {}: {}", file_path, e);
                        }
                    }
                }
            }
        }
    }

    // Fallback to basic prompt if context-aware prompt fails
    warn!("Falling back to basic prompt for project: {}", project.name);
    build_basic_prompt(project).await
}

/// Create prompt builder if possible
async fn create_prompt_builder<'a>(
    project: &'a ProjectInfo,
    db_manager: &'a DatabaseManager,
) -> Result<Option<PromptBuilder<'a>>> {
    debug!("Creating prompt builder for project: {}", project.name);

    let indices_dir = PathBuf::from(&project.path).join("cache").join("indices");

    match PromptBuilder::new(
        db_manager,
        project.name.clone(),
        Some(indices_dir.to_string_lossy().to_string()),
    )
    .await
    {
        Ok(builder) => {
            debug!(
                "Successfully created PromptBuilder for project: {}",
                project.name
            );
            Ok(Some(builder))
        }
        Err(e) => {
            warn!(
                "Failed to create PromptBuilder for project {}: {}",
                project.name, e
            );
            Ok(None)
        }
    }
}

/// Build basic prompt without database context
pub async fn build_basic_prompt(project: &ProjectInfo) -> Result<String> {
    debug!("Building basic prompt for project: {}", project.name);

    let prompt = format!(
        "Translate the following C code to idiomatic Rust.

Project: {}
Type: {:?}

Guidelines:
1. Use Rust's ownership system instead of manual memory management
2. Replace error codes with Result<T, E> types
3. Use safe abstractions where possible
4. Maintain the same functionality and API structure
5. Add appropriate error handling
6. Use idiomatic Rust patterns (iterators, match expressions, etc.)
7. Include necessary use statements
8. Add comments for complex translations

Please provide clean, compilable Rust code:",
        project.name, project.project_type
    );

    Ok(prompt)
}

/// Build enhanced basic prompt with project analysis
pub async fn build_enhanced_basic_prompt(project: &ProjectInfo) -> Result<String> {
    debug!(
        "Building enhanced basic prompt for project: {}",
        project.name
    );

    // Analyze C files to provide more specific guidance
    let c_files = read_c_source_files(project).await.unwrap_or_default();
    let mut analysis = ProjectAnalysis::default();

    for c_file in &c_files {
        analyze_c_code(&mut analysis, c_file);
    }

    let mut prompt = format!(
        "Translate the following C code to idiomatic Rust.

Project: {}
Type: {:?}

Code Analysis:
- Functions detected: {}
- Structs detected: {}
- Memory allocations: {}
- String operations: {}
- File I/O operations: {}

Translation Guidelines:
1. Use Rust's ownership system - replace malloc/free with Box, Vec, or other smart pointers
2. Replace error codes with Result<T, E> types
3. Use CStr/CString for C string interop if needed
4. Replace NULL checks with Option<T>
5. Use safe abstractions (Vec instead of arrays, String instead of char*)
6. Maintain the same functionality and API structure
7. Add appropriate error handling with proper Error types
8. Use idiomatic Rust patterns (iterators, match expressions, if let, etc.)
9. Include necessary use statements
10. Add #[repr(C)] for structs that need C compatibility
11. Consider using unsafe blocks only where absolutely necessary

Please provide clean, compilable Rust code with proper error handling:",
        project.name,
        project.project_type,
        analysis.function_count,
        analysis.struct_count,
        analysis.malloc_count,
        analysis.string_ops,
        analysis.file_ops
    );

    // Add specific warnings based on analysis
    if analysis.malloc_count > 0 {
        prompt.push_str("\n\nWARNING: Detected dynamic memory allocation. Use Rust's owned types (Box, Vec, String) instead.");
    }

    if analysis.string_ops > 0 {
        prompt.push_str("\n\nNOTE: String operations detected. Consider using Rust's String/&str types and proper UTF-8 handling.");
    }

    if analysis.file_ops > 0 {
        prompt.push_str("\n\nNOTE: File operations detected. Use std::fs and proper error handling with Result types.");
    }

    Ok(prompt)
}

/// Simple analysis of C code to provide better translation guidance
#[derive(Default)]
struct ProjectAnalysis {
    function_count: usize,
    struct_count: usize,
    malloc_count: usize,
    string_ops: usize,
    file_ops: usize,
}

fn analyze_c_code(analysis: &mut ProjectAnalysis, c_code: &str) {
    for line in c_code.lines() {
        let line = line.trim();

        // Count function definitions (simple heuristic)
        if (line.starts_with("int ")
            || line.starts_with("void ")
            || line.starts_with("char")
            || line.starts_with("static"))
            && line.contains('(')
            && line.contains(')')
            && line.contains('{')
        {
            analysis.function_count += 1;
        }

        // Count struct definitions
        if line.starts_with("struct ") || line.starts_with("typedef struct") {
            analysis.struct_count += 1;
        }

        // Count memory allocations
        if line.contains("malloc(") || line.contains("calloc(") || line.contains("realloc(") {
            analysis.malloc_count += 1;
        }

        // Count string operations
        if line.contains("strlen(")
            || line.contains("strcpy(")
            || line.contains("strcat(")
            || line.contains("strcmp(")
            || line.contains("strncmp(")
        {
            analysis.string_ops += 1;
        }

        // Count file operations
        if line.contains("fopen(")
            || line.contains("fclose(")
            || line.contains("fread(")
            || line.contains("fwrite(")
            || line.contains("fprintf(")
        {
            analysis.file_ops += 1;
        }
    }
}

/// Parse JSON response from LLM for structured translation output
pub fn parse_llm_json_response(response: &str) -> Result<TranslationResult> {
    debug!("Parsing LLM JSON response");

    // Try to extract JSON from response that might contain markdown
    let json_str = extract_json_from_response(response)?;

    match serde_json::from_str::<TranslationResult>(&json_str) {
        Ok(result) => {
            debug!("Successfully parsed JSON response");
            Ok(result)
        }
        Err(e) => {
            warn!("Failed to parse JSON response: {}", e);
            // Create fallback result
            Ok(TranslationResult {
                original: "".to_string(),
                rust_code: clean_response_code(response),
                key_changes: vec!["JSON解析失败，使用原始响应".to_string()],
                warnings: vec![format!("JSON解析错误: {}", e)],
            })
        }
    }
}

/// Extract JSON content from potentially markdown-wrapped response
fn extract_json_from_response(response: &str) -> Result<String> {
    let response = response.trim();

    // Check if it's already clean JSON
    if response.starts_with('{') && response.ends_with('}') {
        return Ok(response.to_string());
    }

    // Try to extract from markdown code blocks
    if let Some(start) = response.find("```json") {
        let json_start = start + 7; // length of "```json"
        if let Some(actual_end) = response[json_start..].find("```") {
            let json_content = &response[json_start..json_start + actual_end].trim();
            return Ok(json_content.to_string());
        }
    }

    // Try to extract JSON block without markdown
    if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            if end > start {
                return Ok(response[start..=end].to_string());
            }
        }
    }

    Err(anyhow::anyhow!("无法从响应中提取JSON内容"))
}

/// Clean response code by removing markdown wrappers
fn clean_response_code(response: &str) -> String {
    let response = response.trim();

    // Remove rust code block markers
    if response.starts_with("```rust") && response.ends_with("```") {
        let content = &response[7..response.len() - 3];
        return content.trim().to_string();
    }

    // Remove generic code block markers
    if response.starts_with("```") && response.ends_with("```") {
        let content = &response[3..response.len() - 3];
        return content.trim().to_string();
    }

    response.to_string()
}

/// Structure for LLM translation result
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct TranslationResult {
    pub original: String,
    pub rust_code: String,
    pub key_changes: Vec<String>,
    pub warnings: Vec<String>,
}

impl TranslationResult {
    /// Validate the translation result
    pub fn validate(&self) -> Result<()> {
        if self.rust_code.trim().is_empty() {
            return Err(anyhow::anyhow!("翻译结果的Rust代码为空"));
        }

        // Basic syntax check - ensure it has some Rust-like structure
        if !self.rust_code.contains("fn ")
            && !self.rust_code.contains("struct ")
            && !self.rust_code.contains("impl ")
            && !self.rust_code.contains("mod ")
        {
            warn!("翻译结果可能不是有效的Rust代码");
        }

        Ok(())
    }

    /// Check if there are critical warnings
    pub fn has_critical_warnings(&self) -> bool {
        self.warnings.iter().any(|w| {
            w.contains("FIXME")
                || w.contains("unsafe")
                || w.contains("内存泄漏")
                || w.contains("未定义行为")
        })
    }
}

/// Enhanced prompt building with better error context
pub async fn build_context_aware_prompt_with_retry(
    project: &ProjectInfo,
    db_manager: &DatabaseManager,
    retry_context: Option<&str>,
) -> Result<String> {
    debug!(
        "Building context-aware prompt with retry for project: {}",
        project.name
    );

    let mut base_prompt = build_context_aware_prompt(project, db_manager).await?;

    // Add retry context if provided
    if let Some(context) = retry_context {
        base_prompt.push_str("\n\n**重试上下文**:\n");
        base_prompt.push_str("之前的翻译存在以下问题，请特别注意修复:\n");
        base_prompt.push_str(context);
        base_prompt.push_str("\n请重新翻译，确保代码能够编译通过。");
    }

    Ok(base_prompt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectType;

    #[tokio::test]
    async fn test_build_basic_prompt() {
        let project = ProjectInfo {
            name: "test_project".to_string(),
            path: PathBuf::from("/tmp/test"),
            project_type: ProjectType::SingleFile,
        };

        let prompt = build_basic_prompt(&project).await.unwrap();
        assert!(prompt.contains("test_project"));
        assert!(prompt.contains("SingleFile"));
    }

    #[test]
    fn test_analyze_c_code() {
        let mut analysis = ProjectAnalysis::default();
        let c_code = r#"
            #include <stdio.h>
            #include <stdlib.h>

            struct Point {
                int x, y;
            };

            int add(int a, int b) {
                return a + b;
            }

            void test() {
                char* buffer = malloc(100);
                strcpy(buffer, "hello");
                FILE* file = fopen("test.txt", "r");
                fclose(file);
                free(buffer);
            }
        "#;

        analyze_c_code(&mut analysis, c_code);

        assert!(analysis.function_count >= 1);
        assert!(analysis.struct_count >= 1);
        assert!(analysis.malloc_count >= 1);
        assert!(analysis.string_ops >= 1);
        assert!(analysis.file_ops >= 2);
    }

    #[test]
    fn test_parse_llm_json_response() {
        let json_response = r#"{
            "original": "int add(int a, int b) { return a + b; }",
            "rust_code": "fn add(a: i32, b: i32) -> i32 { a + b }",
            "key_changes": ["使用i32替代int"],
            "warnings": []
        }"#;

        let result = parse_llm_json_response(json_response).unwrap();
        assert_eq!(result.original, "int add(int a, int b) { return a + b; }");
        assert!(result.rust_code.contains("fn add"));
        assert_eq!(result.key_changes.len(), 1);
    }

    #[test]
    fn test_extract_json_from_markdown() {
        let markdown_response = r#"这是一个翻译结果：

```json
{
    "original": "int main() { return 0; }",
    "rust_code": "fn main() {}",
    "key_changes": ["移除返回值"],
    "warnings": []
}
```

翻译完成。"#;

        let json = extract_json_from_response(markdown_response).unwrap();
        assert!(json.contains("original"));
        assert!(json.contains("rust_code"));
    }

    #[test]
    fn test_translation_result_validation() {
        let valid_result = TranslationResult {
            original: "int add(int a, int b) { return a + b; }".to_string(),
            rust_code: "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            key_changes: vec!["使用i32".to_string()],
            warnings: vec![],
        };

        assert!(valid_result.validate().is_ok());
        assert!(!valid_result.has_critical_warnings());

        let warning_result = TranslationResult {
            original: "".to_string(),
            rust_code: "unsafe { }".to_string(),
            key_changes: vec![],
            warnings: vec!["包含unsafe代码".to_string()],
        };

        assert!(warning_result.has_critical_warnings());
    }
}
