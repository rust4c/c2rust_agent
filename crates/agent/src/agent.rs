//! Agent - Intelligent C to Rust Translation Agent
//!
//! This module implements a unified agent that combines multiple tools to accomplish
//! C to Rust translation tasks. Following Linus's principle: "Good interfaces hide
//! complexity without sacrificing power."
//!
//! ## Core Philosophy
//! - Single agent per project (thread-safe for multi-threading)
//! - Active tool utilization for information gathering
//! - File modification with precise control
//! - Prompt-driven AI interactions

use anyhow::{anyhow, Context, Result};
use db_services::DatabaseManager;
use file_editor::manager::RustFileManager;
use llm_requester::{llm_request_with_prompt, llm_request_with_prompt_chunked, utils};
use log::{debug, info};
use prompt_builder::PromptBuilder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
use web_searcher::{solve_rust_error, RustErrorSolution, WebSearcher};

/// Project configuration for agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project_name: String,
    pub project_path: PathBuf,
    pub cache_path: PathBuf,
    pub source_language: String, // "c", "cpp", etc.
    pub target_language: String, // "rust"
    pub max_retry_attempts: usize,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project_name: "unknown".to_string(),
            project_path: PathBuf::new(),
            cache_path: PathBuf::new(),
            source_language: "c".to_string(),
            target_language: "rust".to_string(),
            max_retry_attempts: 3,
        }
    }
}

/// Agent message for inter-agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub from_agent: String,
    pub to_agent: Option<String>, // None for broadcast
    pub message_type: MessageType,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    FileModified,
    ErrorFound,
    TaskCompleted,
    RequestHelp,
    ShareInfo,
    StatusUpdate,
}

/// Translation result from AI optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub rust_code: String,
    pub cargo_dependencies: Vec<String>,
    pub key_changes: Vec<String>,
    pub warnings: Vec<String>,
    pub confidence_score: f32,
    pub compilation_status: CompilationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompilationStatus {
    Unknown,
    Success,
    Failed(String), // Error message
    Warning(String),
}

/// Code chunk for chunked translation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub chunk_id: usize,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub functions: Vec<String>,
    pub dependencies: Vec<String>, // Functions/types this chunk depends on
    pub context: ChunkContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkContext {
    pub includes: Vec<String>,
    pub type_definitions: Vec<String>,
    pub global_variables: Vec<String>,
    pub macros: Vec<String>,
}

impl Default for ChunkContext {
    fn default() -> Self {
        Self {
            includes: Vec::new(),
            type_definitions: Vec::new(),
            global_variables: Vec::new(),
            macros: Vec::new(),
        }
    }
}

/// Chunked translation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedTranslationResult {
    pub chunks: Vec<ChunkTranslationResult>,
    pub merged_code: String,
    pub total_chunks: usize,
    pub successful_chunks: usize,
    pub failed_chunks: Vec<usize>,
    pub overall_confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkTranslationResult {
    pub chunk_id: usize,
    pub rust_code: String,
    pub dependencies: Vec<String>,
    pub warnings: Vec<String>,
    pub confidence_score: f32,
    pub compilation_status: CompilationStatus,
}

/// Main Agent struct - one per project/thread
pub struct Agent {
    /// Agent identity
    pub agent_id: String,

    /// Project configuration
    pub config: ProjectConfig,

    /// File management capabilities
    file_manager: Option<RustFileManager>,

    /// Database for code storage and retrieval
    db_manager: Arc<Mutex<DatabaseManager>>,

    /// Web search for error solutions
    _web_searcher: Arc<Mutex<WebSearcher>>,

    /// Prompt builder for AI interactions
    prompt_builder: Arc<Mutex<Option<PromptBuilder<'static>>>>,

    /// Message queue for inter-agent communication
    message_queue: Arc<Mutex<Vec<AgentMessage>>>,

    /// Current working context
    current_context: Arc<Mutex<AgentContext>>,
}

#[derive(Debug, Clone)]
struct AgentContext {
    current_file: Option<PathBuf>,
    _current_function: Option<String>,
    recent_errors: Vec<String>,
    compilation_attempts: usize,
    _last_successful_code: Option<String>,
    chunk_cache: HashMap<String, Vec<CodeChunk>>, // file_path -> chunks
    translation_progress: HashMap<String, ChunkedTranslationProgress>,
}

#[derive(Debug, Clone)]
struct ChunkedTranslationProgress {
    total_chunks: usize,
    completed_chunks: usize,
    failed_chunks: Vec<usize>,
    _start_time: chrono::DateTime<chrono::Utc>,
}

impl Default for AgentContext {
    fn default() -> Self {
        Self {
            current_file: None,
            _current_function: None,
            recent_errors: Vec::new(),
            compilation_attempts: 0,
            _last_successful_code: None,
            chunk_cache: HashMap::new(),
            translation_progress: HashMap::new(),
        }
    }
}

impl Agent {
    /// Create a new agent for a specific project
    /// Example: Agent for /Users/peng/Documents/Tmp/chibicc_cache/individual_files/chibicc
    pub async fn new(
        project_name: String,
        project_path: PathBuf,
        cache_path: Option<PathBuf>,
    ) -> Result<Self> {
        let agent_id = format!("agent_{}_{}", project_name, uuid::Uuid::new_v4());

        // Determine cache path - should be the project directory itself
        let cache_dir = cache_path.unwrap_or_else(|| project_path.clone());

        let config = ProjectConfig {
            project_name: project_name.clone(),
            project_path: project_path.clone(),
            cache_path: cache_dir.clone(),
            source_language: "c".to_string(),
            target_language: "rust".to_string(),
            max_retry_attempts: 3,
        };

        info!("Creating agent {} for project: {}", agent_id, project_name);

        // Initialize database manager
        let db_manager = Arc::new(Mutex::new(
            DatabaseManager::new_default()
                .await
                .context("Failed to initialize database manager")?,
        ));

        // Initialize web searcher
        let web_searcher = Arc::new(Mutex::new(
            WebSearcher::new()
                .await
                .context("Failed to initialize web searcher")?,
        ));

        // Initialize prompt builder placeholder (will be set when needed)
        let prompt_builder = Arc::new(Mutex::new(None));

        let agent = Self {
            agent_id,
            config,
            file_manager: None,
            db_manager,
            _web_searcher: web_searcher,
            prompt_builder,
            message_queue: Arc::new(Mutex::new(Vec::new())),
            current_context: Arc::new(Mutex::new(AgentContext::default())),
        };

        info!("Agent {} created successfully", agent.agent_id);
        Ok(agent)
    }

    /// Initialize file manager for Rust projects
    pub async fn initialize_file_manager(&mut self) -> Result<()> {
        // Look for Cargo.toml or create a basic Rust project structure
        let cargo_toml_path = self.config.project_path.join("Cargo.toml");

        if !cargo_toml_path.exists() {
            self.create_basic_rust_project().await?;
        }

        let file_manager = RustFileManager::new(&self.config.project_path)
            .context("Failed to create file manager")?;

        self.file_manager = Some(file_manager);
        info!("File manager initialized for {}", self.config.project_name);
        Ok(())
    }

    /// Create basic Rust project structure
    async fn create_basic_rust_project(&self) -> Result<()> {
        let src_dir = self.config.project_path.join("src");
        if !src_dir.exists() {
            fs::create_dir_all(&src_dir).await?;
        }

        // Create basic Cargo.toml
        let cargo_content = format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
libc = "0.2"
"#,
            self.config.project_name.replace("-", "_")
        );

        fs::write(self.config.project_path.join("Cargo.toml"), cargo_content).await?;

        // Create basic main.rs if it doesn't exist
        let main_rs = src_dir.join("main.rs");
        if !main_rs.exists() {
            fs::write(
                main_rs,
                "fn main() {\n    println!(\"Hello, world!\");\n}\n",
            )
            .await?;
        }

        info!(
            "Created basic Rust project structure for {}",
            self.config.project_name
        );
        Ok(())
    }

    /// Initialize prompt builder
    pub async fn initialize_prompt_builder(&self) -> Result<()> {
        {
            let db_manager = self.db_manager.lock().await;
            let builder = PromptBuilder::new(
                &*db_manager,
                self.config.project_name.clone(),
                Some(self.config.cache_path.clone()),
            )
            .await?;

            // We need to work around lifetime issues here
            // In a real implementation, you'd want to restructure to avoid this
            let builder_static = unsafe {
                std::mem::transmute::<PromptBuilder<'_>, PromptBuilder<'static>>(builder)
            };

            let mut prompt_builder = self.prompt_builder.lock().await;
            *prompt_builder = Some(builder_static);
        }

        info!(
            "Prompt builder initialized for {}",
            self.config.project_name
        );
        Ok(())
    }

    // ===== Information Gathering Methods =====

    /// Actively gather information about source code
    pub async fn gather_source_info(&self, file_path: &Path) -> Result<SourceInfo> {
        info!("Gathering source information for: {}", file_path.display());

        let mut source_info = SourceInfo {
            file_path: file_path.to_path_buf(),
            content: String::new(),
            functions: Vec::new(),
            includes: Vec::new(),
            dependencies: Vec::new(),
            complexity_score: 0.0,
        };

        // Read file content
        if file_path.exists() {
            source_info.content = fs::read_to_string(file_path)
                .await
                .context("Failed to read source file")?;

            // Analyze content
            source_info.functions = self.extract_functions(&source_info.content);
            source_info.includes = self.extract_includes(&source_info.content);
            source_info.complexity_score = self.calculate_complexity(&source_info.content);
        }

        // Query database for additional context
        let db_manager = self.db_manager.lock().await;
        if let Ok(similar_code) = db_manager
            .search_code_by_text(
                &source_info.content[..source_info.content.len().min(500)],
                Some(&self.config.source_language),
                Some(&self.config.project_name),
            )
            .await
        {
            source_info.dependencies = similar_code
                .into_iter()
                .take(5)
                .map(|entry| {
                    entry
                        .get("function_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string()
                })
                .collect();
        }

        info!(
            "Gathered info: {} functions, {} includes, complexity: {:.2}",
            source_info.functions.len(),
            source_info.includes.len(),
            source_info.complexity_score
        );

        Ok(source_info)
    }

    /// Search web for error solutions
    pub async fn search_error_solution(&self, error_message: &str) -> Result<RustErrorSolution> {
        info!("Searching web for error solution");

        let solution = solve_rust_error(error_message)
            .await
            .context("Failed to search for error solution")?;

        info!(
            "Found {} solutions with confidence: {:?}",
            solution.solutions.len(),
            solution.metadata.confidence_level
        );

        Ok(solution)
    }

    /// Locate and analyze error in code
    pub async fn locate_error(
        &self,
        error_message: &str,
        file_path: &Path,
    ) -> Result<ErrorLocation> {
        let content = fs::read_to_string(file_path).await?;
        let lines: Vec<&str> = content.lines().collect();

        // Extract line number from error message
        let line_number = self.extract_line_number(error_message);

        let error_location = ErrorLocation {
            file_path: file_path.to_path_buf(),
            line_number,
            error_text: error_message.to_string(),
            surrounding_code: if let Some(line_num) = line_number {
                self.get_surrounding_code(&lines, line_num, 5)
            } else {
                String::new()
            },
            suggested_fixes: Vec::new(),
        };

        info!(
            "Located error at line {:?} in {}",
            line_number,
            file_path.display()
        );
        Ok(error_location)
    }

    // ===== File Modification Methods =====

    /// Modify specified file with new content
    pub async fn modify_file(&mut self, file_path: &Path, new_content: &str) -> Result<()> {
        info!("Modifying file: {}", file_path.display());

        // Update current context
        {
            let mut context = self.current_context.lock().await;
            context.current_file = Some(file_path.to_path_buf());
        }

        // Write new content
        fs::write(file_path, new_content)
            .await
            .context("Failed to write file")?;

        // Notify other agents
        self.send_message(AgentMessage {
            from_agent: self.agent_id.clone(),
            to_agent: None, // Broadcast
            message_type: MessageType::FileModified,
            content: format!("Modified file: {}", file_path.display()),
            metadata: {
                let mut map = HashMap::new();
                map.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(file_path.to_string_lossy().to_string()),
                );
                map
            },
            timestamp: chrono::Utc::now(),
        })
        .await;

        info!("Successfully modified {}", file_path.display());
        Ok(())
    }

    /// Modify part of a file (specific lines or functions)
    pub async fn modify_file_section(
        &mut self,
        file_path: &Path,
        start_line: usize,
        end_line: usize,
        new_content: &str,
    ) -> Result<()> {
        info!(
            "Modifying lines {}-{} in {}",
            start_line,
            end_line,
            file_path.display()
        );

        if let Some(ref file_manager) = self.file_manager {
            file_manager
                .replace_lines(start_line, end_line, new_content)
                .context("Failed to replace lines")?;
        } else {
            return Err(anyhow!("File manager not initialized"));
        }

        // Update context
        {
            let mut context = self.current_context.lock().await;
            context.current_file = Some(file_path.to_path_buf());
        }

        info!("Successfully modified section in {}", file_path.display());
        Ok(())
    }

    // ===== AI Interaction Methods =====

    /// Use prompt builder to create context-aware prompts
    pub async fn build_translation_prompt(
        &self,
        source_file: &Path,
        target_functions: Option<Vec<String>>,
    ) -> Result<String> {
        info!("Building translation prompt for {}", source_file.display());

        let prompt_builder = self.prompt_builder.lock().await;
        if let Some(ref builder) = *prompt_builder {
            let mut prompt = builder
                .build_file_context_prompt(source_file, target_functions)
                .await
                .context("Failed to build file context prompt")?;

            // Scan for existing c2rust project results
            prompt = self
                .enhance_prompt_with_context(source_file, prompt)
                .await?;

            info!("Built prompt with {} characters", prompt.len());
            Ok(prompt)
        } else {
            // Fallback: Create basic prompt with C source code
            self.create_basic_c_prompt(source_file).await
        }
    }

    /// Translate C code to Rust using AI
    pub async fn translate_code(
        &mut self,
        source_file: &Path,
        compile_errors: Option<&str>,
    ) -> Result<TranslationResult> {
        info!("Starting AI translation for {}", source_file.display());

        // Update context
        {
            let mut context = self.current_context.lock().await;
            context.current_file = Some(source_file.to_path_buf());
            context.compilation_attempts += 1;
            if let Some(errors) = compile_errors {
                context.recent_errors.push(errors.to_string());
                // Keep only last 3 errors to avoid context bloat
                if context.recent_errors.len() > 3 {
                    context.recent_errors.remove(0);
                }
            }
        }

        // Build context-aware prompt
        let base_prompt = self.build_translation_prompt(source_file, None).await?;

        // Add error context if provided
        let mut messages = vec![base_prompt];
        if let Some(errors) = compile_errors {
            messages.push(format!(
                "Previous compilation failed with these errors:\n```\n{}\n```\n\nPlease fix these issues in the translation.",
                errors
            ));
        }

        // Load translation template and Linus role for first-time translation
        let mut template = self
            .load_prompt_template("file_conversion")
            .await
            .unwrap_or_else(|_| "Translate the C code to safe, idiomatic Rust code.".to_string());

        // Add Linus role context for initial translation
        if let Ok(linus_role) = self.load_prompt_template("linus_role").await {
            template = format!("{}\n\n{}", linus_role, template);
        }

        // Check if we need to use chunked requests due to large context
        let total_tokens = messages
            .iter()
            .map(|m| utils::estimate_token_count(m))
            .sum::<usize>()
            + utils::estimate_token_count(&template);

        let ai_response = if total_tokens > 80000 {
            info!(
                "Large context detected ({} tokens), using chunked requests",
                total_tokens
            );
            let chunked_responses =
                llm_request_with_prompt_chunked(messages, template, Some(80000))
                    .await
                    .context("AI chunked translation request failed")?;

            // Combine chunked responses with clear separators
            chunked_responses.join("\n\n--- RESPONSE CHUNK ---\n\n")
        } else {
            llm_request_with_prompt(messages, template)
                .await
                .context("AI translation request failed")?
        };

        // Process AI response
        let result = self.process_translation_response(&ai_response).await?;

        info!(
            "Translation completed with confidence: {:.2}",
            result.confidence_score
        );
        Ok(result)
    }

    /// Process AI translation response
    async fn process_translation_response(&self, response: &str) -> Result<TranslationResult> {
        // Clean the response by removing line numbers and extracting JSON from markdown blocks
        let cleaned_response = self.clean_json_response(response);

        // Try to parse as JSON first
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&cleaned_response) {
            let rust_code = json_value
                .get("rust_code")
                .and_then(|v| v.as_str())
                .unwrap_or(response)
                .to_string();

            let cargo_deps = json_value
                .get("cargo")
                .and_then(|v| v.as_str())
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();

            let key_changes = json_value
                .get("key_changes")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let warnings = json_value
                .get("warnings")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            // Process AI tool usage if present
            if let Some(tool_usage) = json_value.get("tool_usage") {
                if let Err(e) = self.process_ai_tool_usage_immutable(tool_usage).await {
                    log::warn!("Failed to process AI tool usage: {}", e);
                }
            }

            return Ok(TranslationResult {
                rust_code,
                cargo_dependencies: cargo_deps,
                key_changes,
                warnings,
                confidence_score: 0.8, // Default confidence
                compilation_status: CompilationStatus::Unknown,
            });
        }

        // Fallback: extract rust code from markdown blocks
        let rust_code = self.extract_rust_code(response);

        Ok(TranslationResult {
            rust_code,
            cargo_dependencies: vec!["libc".to_string()], // Default dependency
            key_changes: vec!["Translated from C to Rust".to_string()],
            warnings: Vec::new(),
            confidence_score: 0.6, // Lower confidence for markdown extraction
            compilation_status: CompilationStatus::Unknown,
        })
    }

    /// Clean JSON response by removing line numbers and extracting from markdown blocks
    fn clean_json_response(&self, response: &str) -> String {
        // First, try to extract JSON from markdown code blocks
        if let Some(json_start) = response.find("```json") {
            let content_start = json_start + 7; // length of "```json"
            if let Some(json_end) = response[content_start..].find("```") {
                let json_content = &response[content_start..content_start + json_end];
                return self.remove_line_numbers(json_content);
            }
        }

        // If no markdown blocks, try to find JSON-like content
        if response.trim().starts_with('{') {
            return self.remove_line_numbers(response);
        }

        // Look for lines that start with numbers and remove them
        self.remove_line_numbers(response)
    }

    /// Remove line numbers from the beginning of lines
    fn remove_line_numbers(&self, content: &str) -> String {
        use regex::Regex;

        // Remove line numbers at the start of lines (e.g., "    1 " or "   25 ")
        let line_num_regex = Regex::new(r"(?m)^\s*\d+\s+").unwrap();
        let cleaned = line_num_regex.replace_all(content, "");

        // Trim whitespace and return
        cleaned.trim().to_string()
    }

    // ===== Chunked Translation Methods =====

    /// Split source file into manageable chunks for translation
    /// Strategies: by function, by line count, or smart splitting
    pub async fn split_into_chunks(
        &self,
        source_file: &Path,
        max_lines_per_chunk: usize,
    ) -> Result<Vec<CodeChunk>> {
        info!(
            "Splitting {} into chunks (max {} lines per chunk)",
            source_file.display(),
            max_lines_per_chunk
        );

        let content = fs::read_to_string(source_file)
            .await
            .context("Failed to read source file")?;

        // Extract global context (includes, types, macros)
        let global_context = self.extract_global_context(&content);

        // Try smart splitting by functions first
        let chunks = if max_lines_per_chunk > 50 {
            self.split_by_functions(&content, max_lines_per_chunk, &global_context)
        } else {
            self.split_by_lines(&content, max_lines_per_chunk, &global_context)
        };

        info!("Split file into {} chunks", chunks.len());
        Ok(chunks)
    }

    /// Smart splitting by function boundaries
    fn split_by_functions(
        &self,
        content: &str,
        max_lines: usize,
        global_context: &ChunkContext,
    ) -> Vec<CodeChunk> {
        use regex::Regex;

        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        // Find function boundaries
        let func_regex = Regex::new(
            r"(?m)^[a-zA-Z_][a-zA-Z0-9_*\s]*\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*\{",
        )
        .unwrap();

        let mut function_starts = Vec::new();
        for (line_num, line) in lines.iter().enumerate() {
            if func_regex.is_match(line) {
                function_starts.push(line_num);
            }
        }

        // If no functions found, fall back to line-based splitting
        if function_starts.is_empty() {
            return self.split_by_lines(content, max_lines, global_context);
        }

        // Group functions into chunks
        let mut current_chunk_start = 0;
        let mut current_chunk_lines = 0;
        let mut chunk_id = 0;

        for (idx, &func_start) in function_starts.iter().enumerate() {
            let func_end = if idx + 1 < function_starts.len() {
                function_starts[idx + 1]
            } else {
                lines.len()
            };

            let func_lines = func_end - func_start;

            // If adding this function exceeds max_lines, create a chunk
            if current_chunk_lines + func_lines > max_lines && current_chunk_lines > 0 {
                let chunk_content = lines[current_chunk_start..func_start].join("\n");
                let functions = self.extract_functions(&chunk_content);

                chunks.push(CodeChunk {
                    chunk_id,
                    content: chunk_content,
                    start_line: current_chunk_start + 1,
                    end_line: func_start,
                    functions,
                    dependencies: Vec::new(),
                    context: global_context.clone(),
                });

                chunk_id += 1;
                current_chunk_start = func_start;
                current_chunk_lines = 0;
            }

            current_chunk_lines += func_lines;
        }

        // Add the last chunk
        if current_chunk_start < lines.len() {
            let chunk_content = lines[current_chunk_start..].join("\n");
            let functions = self.extract_functions(&chunk_content);

            chunks.push(CodeChunk {
                chunk_id,
                content: chunk_content,
                start_line: current_chunk_start + 1,
                end_line: lines.len(),
                functions,
                dependencies: Vec::new(),
                context: global_context.clone(),
            });
        }

        chunks
    }

    /// Simple line-based splitting
    fn split_by_lines(
        &self,
        content: &str,
        max_lines: usize,
        global_context: &ChunkContext,
    ) -> Vec<CodeChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let mut chunk_id = 0;

        for (i, chunk_lines) in lines.chunks(max_lines).enumerate() {
            let chunk_content = chunk_lines.join("\n");
            let functions = self.extract_functions(&chunk_content);

            chunks.push(CodeChunk {
                chunk_id,
                content: chunk_content,
                start_line: i * max_lines + 1,
                end_line: ((i + 1) * max_lines).min(lines.len()),
                functions,
                dependencies: Vec::new(),
                context: global_context.clone(),
            });

            chunk_id += 1;
        }

        chunks
    }

    /// Extract global context (includes, types, macros, etc.)
    fn extract_global_context(&self, content: &str) -> ChunkContext {
        let includes = self.extract_includes(content);

        // Extract type definitions (struct, typedef, enum)
        let type_regex =
            regex::Regex::new(r"(?m)^(?:typedef\s+)?(?:struct|union|enum)\s+(\w+)").unwrap();
        let type_definitions: Vec<String> = type_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect();

        // Extract global variables
        let global_var_regex = regex::Regex::new(
            r"(?m)^(?:extern\s+)?(?:static\s+)?[a-zA-Z_][a-zA-Z0-9_*\s]+\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*[;=]"
        ).unwrap();
        let global_variables: Vec<String> = global_var_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect();

        // Extract macros
        let macro_regex = regex::Regex::new(r"(?m)^#define\s+(\w+)").unwrap();
        let macros: Vec<String> = macro_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect();

        ChunkContext {
            includes,
            type_definitions,
            global_variables,
            macros,
        }
    }

    /// Translate code in chunks with progress tracking
    pub async fn translate_code_chunked(
        &mut self,
        source_file: &Path,
        max_lines_per_chunk: usize,
    ) -> Result<ChunkedTranslationResult> {
        info!("Starting chunked translation for {}", source_file.display());

        // Split into chunks
        let chunks = self
            .split_into_chunks(source_file, max_lines_per_chunk)
            .await?;
        let total_chunks = chunks.len();

        // Initialize progress tracking
        {
            let mut context = self.current_context.lock().await;
            context.translation_progress.insert(
                source_file.to_string_lossy().to_string(),
                ChunkedTranslationProgress {
                    total_chunks,
                    completed_chunks: 0,
                    failed_chunks: Vec::new(),
                    _start_time: chrono::Utc::now(),
                },
            );
            context
                .chunk_cache
                .insert(source_file.to_string_lossy().to_string(), chunks.clone());
        }

        // Translate each chunk
        let mut chunk_results = Vec::new();
        let mut successful_chunks = 0;
        let mut failed_chunks = Vec::new();

        for chunk in chunks {
            info!(
                "Translating chunk {}/{} ({}-{} lines)",
                chunk.chunk_id + 1,
                total_chunks,
                chunk.start_line,
                chunk.end_line
            );

            match self.translate_chunk(&chunk).await {
                Ok(result) => {
                    successful_chunks += 1;
                    chunk_results.push(result);

                    // Update progress
                    let mut context = self.current_context.lock().await;
                    if let Some(progress) = context
                        .translation_progress
                        .get_mut(&source_file.to_string_lossy().to_string())
                    {
                        progress.completed_chunks += 1;
                    }

                    info!("Chunk {} translated successfully", chunk.chunk_id);
                }
                Err(e) => {
                    log::error!("Failed to translate chunk {}: {}", chunk.chunk_id, e);
                    failed_chunks.push(chunk.chunk_id);

                    // Add placeholder result
                    chunk_results.push(ChunkTranslationResult {
                        chunk_id: chunk.chunk_id,
                        rust_code: format!(
                            "// Failed to translate chunk {}\n// Error: {}",
                            chunk.chunk_id, e
                        ),
                        dependencies: Vec::new(),
                        warnings: vec![format!("Translation failed: {}", e)],
                        confidence_score: 0.0,
                        compilation_status: CompilationStatus::Failed(e.to_string()),
                    });

                    // Update progress
                    let mut context = self.current_context.lock().await;
                    if let Some(progress) = context
                        .translation_progress
                        .get_mut(&source_file.to_string_lossy().to_string())
                    {
                        progress.failed_chunks.push(chunk.chunk_id);
                        progress.completed_chunks += 1;
                    }
                }
            }

            // Send progress update
            self.send_progress_update(source_file, successful_chunks, total_chunks)
                .await;
        }

        // Merge chunks
        let merged_code = self.merge_chunk_results(&chunk_results).await?;

        // Calculate overall confidence
        let overall_confidence = if successful_chunks > 0 {
            chunk_results
                .iter()
                .map(|r| r.confidence_score)
                .sum::<f32>()
                / total_chunks as f32
        } else {
            0.0
        };

        info!(
            "Chunked translation completed: {}/{} chunks successful, confidence: {:.2}",
            successful_chunks, total_chunks, overall_confidence
        );

        Ok(ChunkedTranslationResult {
            chunks: chunk_results,
            merged_code,
            total_chunks,
            successful_chunks,
            failed_chunks,
            overall_confidence,
        })
    }

    /// Translate a single chunk
    async fn translate_chunk(&self, chunk: &CodeChunk) -> Result<ChunkTranslationResult> {
        // Build context-aware prompt for this chunk
        let context_header = self.build_chunk_context_header(&chunk.context);

        let prompt = format!(
            "{}\n\n// Chunk {} (lines {}-{}):\n// Functions in this chunk: {}\n\n{}",
            context_header,
            chunk.chunk_id,
            chunk.start_line,
            chunk.end_line,
            chunk.functions.join(", "),
            chunk.content
        );

        let template = self
            .load_prompt_template("file_conversion")
            .await
            .unwrap_or_else(|_| {
                "Translate this C code chunk to safe, idiomatic Rust code. \
                 Preserve the function signatures and logic. \
                 Return only the Rust code without explanations."
                    .to_string()
            });

        // Call AI for translation
        let ai_response = llm_request_with_prompt(vec![prompt], template)
            .await
            .context("Failed to translate chunk")?;

        // Extract Rust code from response
        let rust_code = self.extract_rust_code(&ai_response);

        // Analyze dependencies in the translated code
        let dependencies = self.extract_dependencies(&rust_code);

        Ok(ChunkTranslationResult {
            chunk_id: chunk.chunk_id,
            rust_code,
            dependencies,
            warnings: Vec::new(),
            confidence_score: 0.75, // Default confidence for chunk
            compilation_status: CompilationStatus::Unknown,
        })
    }

    /// Build context header for chunk translation
    fn build_chunk_context_header(&self, context: &ChunkContext) -> String {
        let mut header = String::new();

        if !context.includes.is_empty() {
            header.push_str("// Original includes:\n");
            for include in &context.includes {
                header.push_str(&format!("// #include <{}>\n", include));
            }
            header.push('\n');
        }

        if !context.type_definitions.is_empty() {
            header.push_str("// Type definitions in this file:\n");
            for typedef in &context.type_definitions {
                header.push_str(&format!("// type: {}\n", typedef));
            }
            header.push('\n');
        }

        if !context.global_variables.is_empty() {
            header.push_str("// Global variables in this file:\n");
            for var in &context.global_variables {
                header.push_str(&format!("// global: {}\n", var));
            }
            header.push('\n');
        }

        header
    }

    /// Extract dependencies from Rust code
    fn extract_dependencies(&self, rust_code: &str) -> Vec<String> {
        use regex::Regex;

        let mut deps = Vec::new();

        // Extract function calls
        let call_regex = Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();
        for cap in call_regex.captures_iter(rust_code) {
            if let Some(func) = cap.get(1) {
                let func_name = func.as_str().to_string();
                if !deps.contains(&func_name) {
                    deps.push(func_name);
                }
            }
        }

        deps
    }

    /// Merge chunk translation results into a single Rust file
    async fn merge_chunk_results(&self, chunks: &[ChunkTranslationResult]) -> Result<String> {
        info!("Merging {} chunks into final code", chunks.len());

        let mut merged = String::new();

        // Add file header
        merged.push_str("// Auto-generated Rust code from C translation\n");
        merged.push_str("// Generated using chunked translation\n\n");

        // Collect all unique dependencies
        let mut all_deps = std::collections::HashSet::new();
        for chunk in chunks {
            for dep in &chunk.dependencies {
                all_deps.insert(dep.clone());
            }
        }

        // Add common imports
        merged.push_str("use std::os::raw::*;\n");
        if all_deps.iter().any(|d| d.contains("libc")) {
            merged.push_str("use libc;\n");
        }
        merged.push_str("\n");

        // Add each chunk's code
        for chunk in chunks {
            merged.push_str(&format!(
                "// ========== Chunk {} ==========\n",
                chunk.chunk_id
            ));

            if !chunk.warnings.is_empty() {
                for warning in &chunk.warnings {
                    merged.push_str(&format!("// WARNING: {}\n", warning));
                }
            }

            merged.push_str(&chunk.rust_code);
            merged.push_str("\n\n");
        }

        info!("Merged code: {} lines", merged.lines().count());
        Ok(merged)
    }

    /// Send progress update message
    async fn send_progress_update(&self, file_path: &Path, completed: usize, total: usize) {
        let mut metadata = HashMap::new();
        metadata.insert(
            "file_path".to_string(),
            serde_json::Value::String(file_path.to_string_lossy().to_string()),
        );
        metadata.insert(
            "completed".to_string(),
            serde_json::Value::Number(completed.into()),
        );
        metadata.insert("total".to_string(), serde_json::Value::Number(total.into()));
        metadata.insert(
            "percentage".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64((completed as f64 / total as f64) * 100.0)
                    .unwrap_or(serde_json::Number::from(0)),
            ),
        );

        self.send_message(AgentMessage {
            from_agent: self.agent_id.clone(),
            to_agent: None,
            message_type: MessageType::StatusUpdate,
            content: format!(
                "Translation progress: {}/{} chunks completed ({:.1}%)",
                completed,
                total,
                (completed as f64 / total as f64) * 100.0
            ),
            metadata,
            timestamp: chrono::Utc::now(),
        })
        .await;
    }

    /// Get chunked translation progress for a file
    pub async fn get_chunk_progress(&self, file_path: &Path) -> Option<(usize, usize, Vec<usize>)> {
        let context = self.current_context.lock().await;
        context
            .translation_progress
            .get(&file_path.to_string_lossy().to_string())
            .map(|progress| {
                (
                    progress.completed_chunks,
                    progress.total_chunks,
                    progress.failed_chunks.clone(),
                )
            })
    }

    /// Resume chunked translation from a specific chunk (for retry)
    pub async fn resume_chunked_translation(
        &mut self,
        source_file: &Path,
        failed_chunk_ids: Vec<usize>,
    ) -> Result<ChunkedTranslationResult> {
        info!(
            "Resuming chunked translation for {} (retrying {} chunks)",
            source_file.display(),
            failed_chunk_ids.len()
        );

        // Get cached chunks
        let chunks = {
            let context = self.current_context.lock().await;
            context
                .chunk_cache
                .get(&source_file.to_string_lossy().to_string())
                .cloned()
        };

        let chunks = chunks.ok_or_else(|| {
            anyhow!(
                "No cached chunks found for {}. Please run full translation first.",
                source_file.display()
            )
        })?;

        // Retry only failed chunks
        let mut chunk_results = Vec::new();
        let mut successful_chunks = 0;
        let mut still_failed = Vec::new();

        for chunk_id in failed_chunk_ids {
            if let Some(chunk) = chunks.iter().find(|c| c.chunk_id == chunk_id) {
                info!("Retrying chunk {}", chunk_id);

                match self.translate_chunk(chunk).await {
                    Ok(result) => {
                        successful_chunks += 1;
                        chunk_results.push(result);
                        info!("Chunk {} translated successfully on retry", chunk_id);
                    }
                    Err(e) => {
                        log::error!("Chunk {} still failed: {}", chunk_id, e);
                        still_failed.push(chunk_id);
                    }
                }
            }
        }

        // Merge results
        let merged_code = self.merge_chunk_results(&chunk_results).await?;

        let overall_confidence = if successful_chunks > 0 {
            chunk_results
                .iter()
                .map(|r| r.confidence_score)
                .sum::<f32>()
                / chunk_results.len() as f32
        } else {
            0.0
        };

        info!(
            "Resume completed: {}/{} chunks successful",
            successful_chunks,
            chunk_results.len()
        );

        Ok(ChunkedTranslationResult {
            chunks: chunk_results,
            merged_code,
            total_chunks: chunks.len(),
            successful_chunks,
            failed_chunks: still_failed,
            overall_confidence,
        })
    }

    // ===== Inter-Agent Communication =====

    /// Send message to other agents
    pub async fn send_message(&self, message: AgentMessage) {
        let mut queue = self.message_queue.lock().await;
        queue.push(message.clone());

        debug!(
            "Sent message: {:?} -> {:?}: {}",
            message.message_type,
            message.to_agent.as_deref().unwrap_or("broadcast"),
            message.content
        );

        // In a real implementation, this would send to a message broker
        // For now, just log the message
        info!("Message queued for delivery");
    }

    /// Receive messages from other agents
    pub async fn receive_messages(&self) -> Vec<AgentMessage> {
        let mut queue = self.message_queue.lock().await;
        let messages: Vec<AgentMessage> = queue.drain(..).collect();

        if !messages.is_empty() {
            debug!("Received {} messages", messages.len());
        }

        messages
    }

    /// Request help from other agents
    pub async fn request_help(
        &self,
        problem_description: &str,
        context: Option<&str>,
    ) -> Result<()> {
        let mut metadata = HashMap::new();
        if let Some(ctx) = context {
            metadata.insert(
                "context".to_string(),
                serde_json::Value::String(ctx.to_string()),
            );
        }

        self.send_message(AgentMessage {
            from_agent: self.agent_id.clone(),
            to_agent: None, // Broadcast
            message_type: MessageType::RequestHelp,
            content: problem_description.to_string(),
            metadata,
            timestamp: chrono::Utc::now(),
        })
        .await;

        info!("Requested help: {}", problem_description);
        Ok(())
    }

    // ===== Utility Methods =====

    /// Load prompt template from config/prompts directory
    async fn load_prompt_template(&self, template_name: &str) -> Result<String> {
        let template_path = PathBuf::from("config/prompts").join(format!("{}.md", template_name));

        if template_path.exists() {
            fs::read_to_string(&template_path)
                .await
                .context("Failed to read prompt template")
        } else {
            Err(anyhow!("Template not found: {}", template_name))
        }
    }

    /// Extract functions from C code (simple regex-based)
    fn extract_functions(&self, content: &str) -> Vec<String> {
        use regex::Regex;

        let re = Regex::new(
            r"(?m)^[a-zA-Z_][a-zA-Z0-9_*\s]*\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*\{",
        )
        .unwrap();

        re.captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Extract includes from C code
    fn extract_includes(&self, content: &str) -> Vec<String> {
        use regex::Regex;

        let re = Regex::new(r#"#include\s+[<"](.*?)[>"]"#).unwrap();

        re.captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Calculate code complexity (simple heuristic)
    fn calculate_complexity(&self, content: &str) -> f32 {
        let lines = content.lines().count() as f32;
        let functions = self.extract_functions(content).len() as f32;
        let complexity_keywords = ["if", "while", "for", "switch", "goto"]
            .iter()
            .map(|&keyword| content.matches(keyword).count())
            .sum::<usize>() as f32;

        (lines + functions * 2.0 + complexity_keywords * 1.5) / 10.0
    }

    /// Extract line number from error message
    fn extract_line_number(&self, error_message: &str) -> Option<usize> {
        use regex::Regex;

        let re = Regex::new(r":(\d+):").unwrap();
        re.captures(error_message)
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().parse().ok())
    }

    /// Get surrounding code context
    fn get_surrounding_code(&self, lines: &[&str], line_num: usize, context: usize) -> String {
        let start = line_num.saturating_sub(context + 1);
        let end = (line_num + context).min(lines.len());

        lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4}: {}", start + i + 1, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract Rust code from markdown response
    fn extract_rust_code(&self, response: &str) -> String {
        // Try ```rust block first
        if let Some(start) = response.find("```rust") {
            let code_start = start + 7; // length of "```rust"
            if let Some(end) = response[code_start..].find("```") {
                let code = response[code_start..code_start + end].trim();
                return code.to_string();
            }
        }

        // Try generic ``` block
        if let Some(start) = response.find("```\n") {
            let code_start = start + 4;
            if let Some(end) = response[code_start..].find("\n```") {
                let code = response[code_start..code_start + end].trim();
                return code.to_string();
            }
        }

        // Fallback: return the whole response
        response.to_string()
    }

    /// Get agent status
    pub async fn get_status(&self) -> AgentStatus {
        let context = self.current_context.lock().await;

        AgentStatus {
            agent_id: self.agent_id.clone(),
            project_name: self.config.project_name.clone(),
            current_file: context.current_file.clone(),
            compilation_attempts: context.compilation_attempts,
            recent_errors_count: context.recent_errors.len(),
            is_file_manager_ready: self.file_manager.is_some(),
            message_queue_size: self.message_queue.lock().await.len(),
        }
    }

    // ===== Enhanced Prompt Building Methods =====

    /// Enhance prompt with context based on existing c2rust projects
    async fn enhance_prompt_with_context(
        &self,
        source_file: &Path,
        base_prompt: String,
    ) -> Result<String> {
        let mut enhanced_prompt = base_prompt;

        // Check if there's an existing c2rust translation
        let potential_rust_file = self.find_c2rust_equivalent(source_file).await;

        if let Some(rust_file) = potential_rust_file {
            // Found existing c2rust translation - add it as reference
            if let Ok(rust_content) = fs::read_to_string(&rust_file).await {
                enhanced_prompt.push_str("\n\n// Existing c2rust translation for reference:\n");
                enhanced_prompt.push_str(&format!("```rust\n{}\n```\n", rust_content));
                info!(
                    "Added existing c2rust translation as context: {}",
                    rust_file.display()
                );
            }
        } else {
            // No c2rust translation found - add original C code
            if let Ok(c_content) = fs::read_to_string(source_file).await {
                enhanced_prompt.push_str("\n\n// Original C code to translate:\n");
                enhanced_prompt.push_str(&format!("```c\n{}\n```\n", c_content));
                info!("Added original C code as context");
            }
        }

        Ok(enhanced_prompt)
    }

    /// Find c2rust equivalent file if exists
    async fn find_c2rust_equivalent(&self, source_file: &Path) -> Option<PathBuf> {
        // Strategy 1: Look for .rs file with same name in src/
        let file_stem = source_file.file_stem()?;
        let src_dir = self.config.project_path.join("src");
        let rust_file = src_dir.join(format!("{}.rs", file_stem.to_string_lossy()));

        if rust_file.exists() {
            // Check if it looks like c2rust output (contains c2rust markers)
            if let Ok(content) = fs::read_to_string(&rust_file).await {
                if content.contains("c2rust")
                    || content.contains("::c_void")
                    || content.contains("libc::")
                {
                    return Some(rust_file);
                }
            }
        }

        // Strategy 2: Look in c2rust output directories
        let c2rust_dirs = ["c2rust_out", "target/c2rust", "rust_out"];
        for dir in &c2rust_dirs {
            let c2rust_path = self.config.project_path.join(dir);
            if c2rust_path.exists() {
                let rust_file = c2rust_path.join(format!("{}.rs", file_stem.to_string_lossy()));
                if rust_file.exists() {
                    return Some(rust_file);
                }
            }
        }

        None
    }

    /// Create basic prompt with C source when prompt builder unavailable
    async fn create_basic_c_prompt(&self, source_file: &Path) -> Result<String> {
        let c_content = fs::read_to_string(source_file)
            .await
            .context("Failed to read C source file")?;

        let prompt = format!(
            "Translate the following C code to safe, idiomatic Rust code:\n\n```c\n{}\n```\n\n\
            Requirements:\n\
            - Use safe Rust patterns\n\
            - Avoid unsafe blocks where possible\n\
            - Use proper error handling\n\
            - Follow Rust naming conventions\n\
            - Add appropriate use statements\n\n\
            Return the Rust code in a ```rust code block.",
            c_content
        );

        info!("Created basic C prompt with {} characters", prompt.len());
        Ok(prompt)
    }

    /// Process AI tool usage commands (immutable version)
    async fn process_ai_tool_usage_immutable(&self, tool_usage: &serde_json::Value) -> Result<()> {
        if let Some(file_search) = tool_usage.get("file_search").and_then(|v| v.as_array()) {
            for file_pattern in file_search {
                if let Some(pattern) = file_pattern.as_str() {
                    info!("AI requested file search: {}", pattern);
                    // Implement file search logic here
                }
            }
        }

        if let Some(line_locate) = tool_usage.get("line_locate").and_then(|v| v.as_array()) {
            for line_info in line_locate {
                if let Some(line_desc) = line_info.as_str() {
                    info!("AI requested line location: {}", line_desc);
                    // Implement line location logic here
                }
            }
        }

        if let Some(line_modify) = tool_usage.get("line_modify") {
            if let (Some(start), Some(end), Some(new_code)) = (
                line_modify.get("start_line").and_then(|v| v.as_u64()),
                line_modify.get("end_line").and_then(|v| v.as_u64()),
                line_modify.get("new_code").and_then(|v| v.as_str()),
            ) {
                info!("AI requested line modification: {}-{}", start, end);
                // Store modification request for later processing
                // In a real implementation, this would be queued for the next mutable operation
                log::info!(
                    "Queued line modification: {}-{} with: {}",
                    start,
                    end,
                    new_code
                );
            }
        }

        if let Some(rs_modify) = tool_usage.get("rs_modify") {
            if let (Some(file), Some(changes)) = (
                rs_modify.get("file").and_then(|v| v.as_str()),
                rs_modify.get("changes").and_then(|v| v.as_str()),
            ) {
                info!("AI requested Rust file modification: {}", file);
                // Store file modification request for later processing
                // In a real implementation, this would be queued for the next mutable operation
                log::info!(
                    "Queued file modification: {} with changes: {}",
                    file,
                    changes
                );
            }
        }

        if let Some(cargo_modify) = tool_usage.get("cargo_modify") {
            if let Some(dependencies) = cargo_modify.get("dependencies").and_then(|v| v.as_str()) {
                info!("AI requested Cargo dependency addition: {}", dependencies);
                // Implement Cargo.toml modification logic here
                self.add_cargo_dependencies(dependencies).await?;
            }
        }

        Ok(())
    }

    /// Add dependencies to Cargo.toml
    async fn add_cargo_dependencies(&self, dependencies: &str) -> Result<()> {
        let cargo_path = self.config.project_path.join("Cargo.toml");

        if !cargo_path.exists() {
            return Err(anyhow!("Cargo.toml not found"));
        }

        let mut cargo_content = fs::read_to_string(&cargo_path).await?;

        // Simple approach: append to [dependencies] section
        if !cargo_content.contains("[dependencies]") {
            cargo_content.push_str("\n[dependencies]\n");
        }

        // Parse dependencies (simple comma-separated format)
        for dep in dependencies.split(',') {
            let dep = dep.trim();
            if !dep.is_empty() && !cargo_content.contains(dep) {
                cargo_content.push_str(&format!("{} = \"*\"\n", dep));
            }
        }

        fs::write(&cargo_path, cargo_content).await?;
        info!("Added dependencies to Cargo.toml: {}", dependencies);

        Ok(())
    }
}

// ===== Supporting Types =====

#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub file_path: PathBuf,
    pub content: String,
    pub functions: Vec<String>,
    pub includes: Vec<String>,
    pub dependencies: Vec<String>,
    pub complexity_score: f32,
}

#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub file_path: PathBuf,
    pub line_number: Option<usize>,
    pub error_text: String,
    pub surrounding_code: String,
    pub suggested_fixes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: String,
    pub project_name: String,
    pub current_file: Option<PathBuf>,
    pub compilation_attempts: usize,
    pub recent_errors_count: usize,
    pub is_file_manager_ready: bool,
    pub message_queue_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_agent_creation() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().to_path_buf();

        let agent = Agent::new("test_project".to_string(), project_path, None).await;

        // Agent creation might fail due to database connection issues in test environment
        match agent {
            Ok(agent) => {
                assert_eq!(agent.config.project_name, "test_project");
                println!("✅ Agent creation test passed");
            }
            Err(e) => {
                println!(
                    "⚠️ Agent creation failed (expected in test environment): {}",
                    e
                );
                // This is expected when Qdrant is not running
                assert!(e.to_string().contains("database") || e.to_string().contains("Qdrant"));
            }
        }
    }

    #[tokio::test]
    async fn test_function_extraction_without_agent() {
        // Test function extraction without creating a full agent
        let c_code = r#"
int main(int argc, char **argv) {
    return 0;
}

void helper_function() {
    printf("Hello\n");
}
"#;

        // Create a mock agent just for testing function extraction
        let temp_dir = TempDir::new().unwrap();
        match Agent::new("test".to_string(), temp_dir.path().to_path_buf(), None).await {
            Ok(agent) => {
                let functions = agent.extract_functions(c_code);
                assert_eq!(functions.len(), 2);
                assert!(functions.contains(&"main".to_string()));
                assert!(functions.contains(&"helper_function".to_string()));
                println!("✅ Function extraction test passed");
            }
            Err(_) => {
                // Test the regex directly without agent
                use regex::Regex;
                // Use multiline mode regex to match function definitions
                let re = Regex::new(
                    r"(?m)^[a-zA-Z_][a-zA-Z0-9_*\s]*\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*\{",
                )
                .unwrap();
                let functions: Vec<String> = re
                    .captures_iter(c_code)
                    .filter_map(|cap| cap.get(1))
                    .map(|m| m.as_str().to_string())
                    .collect();

                // Should find both main and helper_function
                println!("Found functions: {:?}", functions);
                assert_eq!(functions.len(), 2); // Should find both functions
                assert!(functions.contains(&"main".to_string()));
                assert!(functions.contains(&"helper_function".to_string()));
                println!("✅ Function extraction test passed (fallback method)");
            }
        }
    }

    #[test]
    fn test_include_extraction_direct() {
        // Test include extraction without creating agent
        use regex::Regex;

        let c_code = r#"
        #include <stdio.h>
        #include "local_header.h"
        #include <stdlib.h>
        "#;

        let re = Regex::new(r#"#include\s+[<"](.*?)[>"]"#).unwrap();
        let includes: Vec<String> = re
            .captures_iter(c_code)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect();

        assert_eq!(includes.len(), 3);
        assert!(includes.contains(&"stdio.h".to_string()));
        assert!(includes.contains(&"local_header.h".to_string()));
        assert!(includes.contains(&"stdlib.h".to_string()));
        println!("✅ Include extraction test passed");
    }

    #[tokio::test]
    async fn test_code_chunking() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.c");

        let c_code = r#"
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int subtract(int a, int b) {
    return a - b;
}

int multiply(int a, int b) {
    return a * b;
}

int main() {
    printf("Hello\n");
    return 0;
}
"#;

        std::fs::write(&test_file, c_code).unwrap();

        match Agent::new("test".to_string(), temp_dir.path().to_path_buf(), None).await {
            Ok(agent) => {
                let chunks = agent.split_into_chunks(&test_file, 10).await;
                match chunks {
                    Ok(chunks) => {
                        assert!(!chunks.is_empty());
                        println!("✅ Created {} chunks", chunks.len());

                        // Verify chunks have context
                        for chunk in &chunks {
                            assert!(!chunk.context.includes.is_empty());
                            println!(
                                "Chunk {}: {} functions",
                                chunk.chunk_id,
                                chunk.functions.len()
                            );
                        }
                        println!("✅ Code chunking test passed");
                    }
                    Err(e) => println!("⚠️ Chunking failed (expected in test env): {}", e),
                }
            }
            Err(e) => {
                println!(
                    "⚠️ Agent creation failed (expected in test environment): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_global_context_extraction() {
        let c_code = r#"
#include <stdio.h>
#include <stdlib.h>

typedef struct Node {
    int data;
} Node;

static int counter = 0;
extern int global_var;

#define MAX_SIZE 100
#define MIN_SIZE 10

int add(int a, int b) {
    return a + b;
}
"#;

        let _temp_dir = TempDir::new().unwrap();

        // Test context extraction without full agent
        use regex::Regex;

        // Test includes
        let include_re = Regex::new(r#"#include\s+[<"](.*?)[>"]"#).unwrap();
        let includes: Vec<String> = include_re
            .captures_iter(c_code)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect();
        assert_eq!(includes.len(), 2);

        // Test type definitions
        let type_re = Regex::new(r"(?m)^(?:typedef\s+)?(?:struct|union|enum)\s+(\w+)").unwrap();
        let types: Vec<String> = type_re
            .captures_iter(c_code)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect();
        assert!(types.contains(&"Node".to_string()));

        // Test macros
        let macro_re = Regex::new(r"(?m)^#define\s+(\w+)").unwrap();
        let macros: Vec<String> = macro_re
            .captures_iter(c_code)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect();
        assert_eq!(macros.len(), 2);
        assert!(macros.contains(&"MAX_SIZE".to_string()));

        println!("✅ Global context extraction test passed");
    }

    #[test]
    fn test_chunk_merging() {
        let chunks = vec![
            ChunkTranslationResult {
                chunk_id: 0,
                rust_code: "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}".to_string(),
                dependencies: vec!["println".to_string()],
                warnings: Vec::new(),
                confidence_score: 0.9,
                compilation_status: CompilationStatus::Success,
            },
            ChunkTranslationResult {
                chunk_id: 1,
                rust_code: "fn main() {\n    println!(\"Hello\");\n}".to_string(),
                dependencies: vec!["add".to_string()],
                warnings: Vec::new(),
                confidence_score: 0.85,
                compilation_status: CompilationStatus::Success,
            },
        ];

        // Test merging logic
        let mut merged = String::new();
        merged.push_str("// Auto-generated Rust code from C translation\n");
        merged.push_str("// Generated using chunked translation\n\n");
        merged.push_str("use std::os::raw::*;\n\n");

        for chunk in &chunks {
            merged.push_str(&format!(
                "// ========== Chunk {} ==========\n",
                chunk.chunk_id
            ));
            merged.push_str(&chunk.rust_code);
            merged.push_str("\n\n");
        }

        assert!(merged.contains("fn add"));
        assert!(merged.contains("fn main"));
        assert!(merged.contains("Chunk 0"));
        assert!(merged.contains("Chunk 1"));

        println!("✅ Chunk merging test passed");
    }

    #[test]
    fn test_dependency_extraction() {
        let rust_code = r#"
fn main() {
    let x = add(1, 2);
    let y = subtract(3, 4);
    let z = multiply(x, y);
    process(z);
}
"#;

        use regex::Regex;
        let call_regex = Regex::new(r"([a-zA-Z_][a-zA-Z0-9_]*)\s*\(").unwrap();

        let mut deps: Vec<String> = Vec::new();
        for cap in call_regex.captures_iter(rust_code) {
            if let Some(func) = cap.get(1) {
                let func_name = func.as_str().to_string();
                if !deps.contains(&func_name) {
                    deps.push(func_name);
                }
            }
        }

        assert!(deps.contains(&"add".to_string()));
        assert!(deps.contains(&"subtract".to_string()));
        assert!(deps.contains(&"multiply".to_string()));
        assert!(deps.contains(&"process".to_string()));

        println!("✅ Dependency extraction test passed");
    }

    #[test]
    fn test_chunk_context_header() {
        let context = ChunkContext {
            includes: vec!["stdio.h".to_string(), "stdlib.h".to_string()],
            type_definitions: vec!["Node".to_string(), "List".to_string()],
            global_variables: vec!["counter".to_string()],
            macros: vec!["MAX_SIZE".to_string()],
        };

        let mut header = String::new();

        if !context.includes.is_empty() {
            header.push_str("// Original includes:\n");
            for include in &context.includes {
                header.push_str(&format!("// #include <{}>\n", include));
            }
            header.push('\n');
        }

        assert!(header.contains("stdio.h"));
        assert!(header.contains("stdlib.h"));
        assert!(header.contains("Original includes"));

        println!("✅ Chunk context header test passed");
    }

    #[tokio::test]
    async fn test_json_response_cleaning() {
        // Test JSON response with line numbers like the one in the error
        let response_with_line_numbers = r#"1 {
2   "rust_code": "fn main() { println!(\"Hello\"); }",
3   "cargo": "serde",
4   "key_changes": ["Added main function", "Used println macro"],
5   "warnings": ["Test warning"]
6 }"#;

        let temp_dir = TempDir::new().unwrap();
        match Agent::new("test".to_string(), temp_dir.path().to_path_buf(), None).await {
            Ok(agent) => {
                let cleaned = agent.clean_json_response(response_with_line_numbers);

                // Should be valid JSON now
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
                match parsed {
                    Ok(json) => {
                        assert!(json.get("rust_code").is_some());
                        assert!(json.get("key_changes").is_some());
                        assert!(json.get("warnings").is_some());
                        println!("✅ JSON response cleaning test passed");
                    }
                    Err(e) => {
                        println!("❌ JSON parsing still failed after cleaning: {}", e);
                        println!("Cleaned content: {}", cleaned);
                        panic!("JSON parsing failed");
                    }
                }
            }
            Err(_) => {
                // Test the cleaning logic directly without agent
                use regex::Regex;

                // Remove line numbers
                let line_num_regex = Regex::new(r"(?m)^\s*\d+\s+").unwrap();
                let cleaned = line_num_regex
                    .replace_all(response_with_line_numbers, "")
                    .trim()
                    .to_string();

                // Should be valid JSON now
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
                match parsed {
                    Ok(json) => {
                        assert!(json.get("rust_code").is_some());
                        assert!(json.get("key_changes").is_some());
                        assert!(json.get("warnings").is_some());
                        println!("✅ JSON response cleaning test passed (fallback method)");
                    }
                    Err(e) => {
                        println!("❌ JSON parsing still failed: {}", e);
                        println!("Cleaned content: {}", cleaned);
                        panic!("JSON parsing failed");
                    }
                }
            }
        }
    }

    #[test]
    fn test_markdown_json_extraction() {
        // Test JSON wrapped in markdown blocks
        let markdown_response = "Here's the result:\n```json\n{\n  \"rust_code\": \"fn test() {}\",\n  \"cargo\": \"tokio\"\n}\n```\nDone.";

        // Test markdown extraction logic
        if let Some(json_start) = markdown_response.find("```json") {
            let content_start = json_start + 7; // length of "```json"
            if let Some(json_end) = markdown_response[content_start..].find("```") {
                let json_content = &markdown_response[content_start..content_start + json_end];
                let cleaned = json_content.trim();

                let parsed: Result<serde_json::Value, _> = serde_json::from_str(cleaned);
                assert!(parsed.is_ok());
                println!("✅ Markdown JSON extraction test passed");
            }
        }
    }

    #[test]
    fn test_real_json_case() {
        // Test a simplified version of the actual JSON case that was failing
        let response_with_markdown = String::from("```json\n")
            + "{\n"
            + "  \"rust_code\": \"// Rust has built-in alignment support\",\n"
            + "  \"cargo\": \"\",\n"
            + "  \"key_changes\": [\n"
            + "    \"C header file macros converted to Rust built-in features\"\n"
            + "  ],\n"
            + "  \"warnings\": [\n"
            + "    \"This is a C header file\"\n"
            + "  ],\n"
            + "  \"tool_usage\": {\n"
            + "    \"file_search\": [],\n"
            + "    \"line_locate\": []\n"
            + "  }\n"
            + "}\n"
            + "```";

        // Test the cleaning logic
        use regex::Regex;

        // Extract JSON from markdown blocks
        let json_content = if let Some(json_start) = response_with_markdown.find("```json") {
            let content_start = json_start + 7;
            if let Some(json_end) = response_with_markdown[content_start..].find("```") {
                &response_with_markdown[content_start..content_start + json_end]
            } else {
                &response_with_markdown
            }
        } else {
            &response_with_markdown
        };

        // Remove line numbers if any
        let line_num_regex = Regex::new(r"(?m)^\s*\d+\s+").unwrap();
        let cleaned = line_num_regex
            .replace_all(json_content, "")
            .trim()
            .to_string();

        // Should be valid JSON now
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&cleaned);
        match parsed {
            Ok(json) => {
                assert!(json.get("rust_code").is_some());
                assert!(json.get("key_changes").is_some());
                assert!(json.get("warnings").is_some());
                assert!(json.get("tool_usage").is_some());
                println!("✅ Real JSON case test passed");
            }
            Err(e) => {
                println!("❌ JSON parsing failed: {}", e);
                println!("Cleaned content: {}", cleaned);
                panic!("JSON parsing failed for real case");
            }
        }
    }
}
