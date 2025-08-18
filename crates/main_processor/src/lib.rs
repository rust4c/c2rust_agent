use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use llm_requester::llm_request_with_prompt;
use log::{debug, error, info, warn};
use rust_checker::{RustCheckError, RustCodeCheck};
use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::sync::Semaphore;

mod translation;

const MAX_RETRY_ATTEMPTS: usize = 3;
const CONCURRENT_LIMIT: usize = 4; // Adjust based on system capabilities

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub name: String,
    pub path: PathBuf,
    pub project_type: ProjectType,
}

#[derive(Debug, Clone)]
pub enum ProjectType {
    SingleFile,
    PairedFiles,
    UnrelatedFiles,
}

#[derive(Debug)]
pub struct TranslationProgress {
    pub total_projects: usize,
    pub completed_projects: usize,
    pub failed_projects: usize,
}

#[derive(Debug)]
pub struct TranslationStats {
    pub successful_translations: Vec<String>,
    pub failed_translations: HashMap<String, String>,
    pub retry_attempts: HashMap<String, usize>,
}

pub struct MainProcessor {
    cache_dir: PathBuf,
    multi_progress: MultiProgress,
    semaphore: Arc<Semaphore>,
}

impl MainProcessor {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            multi_progress: MultiProgress::new(),
            semaphore: Arc::new(Semaphore::new(CONCURRENT_LIMIT)),
        }
    }

    /// Main entry point for the translation workflow
    pub async fn run_translation_workflow(&self) -> Result<TranslationStats> {
        self.run_translation_workflow_with_db(None).await
    }

    /// Convenience method to run workflow with database
    pub async fn run_translation_workflow_with_database(
        &self,
        db_manager: &db_services::DatabaseManager,
    ) -> Result<TranslationStats> {
        self.run_translation_workflow_with_db(Some(db_manager))
            .await
    }

    /// Main entry point for the translation workflow with optional database
    pub async fn run_translation_workflow_with_db(
        &self,
        db_manager: Option<&db_services::DatabaseManager>,
    ) -> Result<TranslationStats> {
        info!("Starting C to Rust translation workflow");

        // Step 1: Discover all projects in cache directories
        let projects = self.discover_projects().await?;
        let total_projects = projects.len();

        if total_projects == 0 {
            warn!("No projects found in cache directory");
            return Ok(TranslationStats {
                successful_translations: Vec::new(),
                failed_translations: HashMap::new(),
                retry_attempts: HashMap::new(),
            });
        }

        info!("Found {} projects to translate", total_projects);

        // Create main progress bar
        let main_progress = self
            .multi_progress
            .add(ProgressBar::new(total_projects as u64));
        main_progress.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        main_progress.set_message("Translating C projects to Rust");

        // Step 2: Process projects concurrently
        let stats = self
            .process_projects_concurrent(projects, main_progress.clone(), db_manager)
            .await?;

        main_progress.finish_with_message("Translation workflow completed");

        // Print summary
        self.print_summary(&stats);

        Ok(stats)
    }

    /// Discover all projects in the cache directory
    pub async fn discover_projects(&self) -> Result<Vec<ProjectInfo>> {
        let mut projects = Vec::new();

        // Check for mapping.json to understand the cache structure
        let mapping_path = self.cache_dir.join("mapping.json");
        if !mapping_path.exists() {
            return Err(anyhow::anyhow!("mapping.json not found in cache directory"));
        }

        let mapping_content = fs::read_to_string(&mapping_path).await?;
        let _mapping: serde_json::Value = serde_json::from_str(&mapping_content)?;

        // Scan for different project types
        let single_files_dir = self.cache_dir.join("单独文件");
        let paired_files_dir = self.cache_dir.join("配对文件");
        let unrelated_files_dir = self.cache_dir.join("不相关文件");

        // Process single files
        if single_files_dir.exists() {
            let single_projects = self
                .scan_directory(&single_files_dir, ProjectType::SingleFile)
                .await?;
            projects.extend(single_projects);
        }

        // Process paired files
        if paired_files_dir.exists() {
            let paired_projects = self
                .scan_directory(&paired_files_dir, ProjectType::PairedFiles)
                .await?;
            projects.extend(paired_projects);
        }

        // Process unrelated files
        if unrelated_files_dir.exists() {
            let unrelated_projects = self
                .scan_directory(&unrelated_files_dir, ProjectType::UnrelatedFiles)
                .await?;
            projects.extend(unrelated_projects);
        }

        Ok(projects)
    }

    /// Scan a directory for projects
    async fn scan_directory(
        &self,
        dir: &Path,
        project_type: ProjectType,
    ) -> Result<Vec<ProjectInfo>> {
        let mut projects = Vec::new();

        if !dir.exists() {
            return Ok(projects);
        }

        let mut entries = fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed")
                    .to_string();

                projects.push(ProjectInfo {
                    name,
                    path,
                    project_type: project_type.clone(),
                });
            }
        }

        Ok(projects)
    }

    /// Process projects sequentially with progress tracking to avoid Send trait issues
    async fn process_projects_concurrent(
        &self,
        projects: Vec<ProjectInfo>,
        main_progress: ProgressBar,
        _db_manager: Option<&db_services::DatabaseManager>,
    ) -> Result<TranslationStats> {
        let mut stats = TranslationStats {
            successful_translations: Vec::new(),
            failed_translations: HashMap::new(),
            retry_attempts: HashMap::new(),
        };

        // Process projects sequentially to avoid Send trait issues with LLM clients
        for project in projects {
            // Create spinner for this project
            let spinner = self.multi_progress.add(ProgressBar::new_spinner());
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.blue} {msg}")
                    .unwrap()
                    .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
            );
            spinner.set_message(format!("Processing: {}", project.name));
            spinner.enable_steady_tick(Duration::from_millis(100));

            // Process without database context due to design limitations
            let result = process_single_project(project.clone(), None).await;

            spinner.finish_and_clear();
            main_progress.inc(1);

            match result {
                Ok(attempts) => {
                    stats.successful_translations.push(project.name.clone());
                    stats.retry_attempts.insert(project.name, attempts);
                }
                Err(error) => {
                    stats
                        .failed_translations
                        .insert(project.name, error.to_string());
                }
            }
        }

        Ok(stats)
    }

    /// Print translation summary
    fn print_summary(&self, stats: &TranslationStats) {
        println!("\n=== Translation Summary ===");
        println!(
            "Successful translations: {}",
            stats.successful_translations.len()
        );
        println!("Failed translations: {}", stats.failed_translations.len());

        if !stats.successful_translations.is_empty() {
            println!("\nSuccessful projects:");
            for project in &stats.successful_translations {
                let attempts = stats.retry_attempts.get(project).unwrap_or(&1);
                println!("  ✓ {} (attempts: {})", project, attempts);
            }
        }

        if !stats.failed_translations.is_empty() {
            println!("\nFailed projects:");
            for (project, error) in &stats.failed_translations {
                println!("  ✗ {}: {}", project, error);
            }
        }

        println!("=== End Summary ===\n");
    }
}

/// Process a single project through the translation workflow with intelligent retry
async fn process_single_project(
    project: ProjectInfo,
    db_manager: Option<&db_services::DatabaseManager>,
) -> Result<usize> {
    debug!("Starting translation for project: {}", project.name);

    // Step 1: Create Rust project
    create_rust_project(&project).await?;

    // Step 2: Translation loop with intelligent retry logic
    let mut attempts = 0;
    let mut last_error_context = None;

    loop {
        attempts += 1;

        if attempts > MAX_RETRY_ATTEMPTS {
            return Err(anyhow::anyhow!(
                "Max retry attempts ({}) exceeded for project: {}",
                MAX_RETRY_ATTEMPTS,
                project.name
            ));
        }

        // Step 2a: Use LLM to translate with retry context
        let translation_result = if attempts == 1 {
            perform_llm_translation(&project, db_manager).await
        } else {
            perform_llm_translation_with_retry(&project, db_manager, last_error_context.as_deref())
                .await
        };

        match translation_result {
            Ok(translated_code) => {
                // Write translated code to Rust project
                write_translated_code(&project, &translated_code).await?;

                // Step 3: Check with rust_checker
                match check_rust_project(&project).await {
                    Ok(_) => {
                        info!(
                            "Successfully translated and validated: {} (attempts: {})",
                            project.name, attempts
                        );
                        return Ok(attempts);
                    }
                    Err(check_error) => {
                        let error_msg = format!("{}", check_error);
                        warn!(
                            "Rust check failed for {} (attempt {}): {}",
                            project.name, attempts, error_msg
                        );

                        if attempts >= MAX_RETRY_ATTEMPTS {
                            return Err(anyhow::anyhow!(
                                "Translation failed after {} attempts. Last error: {}",
                                MAX_RETRY_ATTEMPTS,
                                error_msg
                            ));
                        }

                        // Prepare error context for next retry
                        last_error_context = Some(format!(
                            "编译失败，错误信息: {}\n请修复以下问题：\n1. 确保所有变量和函数都已正确声明\n2. 检查类型匹配和生命周期\n3. 修复语法错误\n4. 确保所有必要的use语句都已包含",
                            error_msg
                        ));
                    }
                }
            }
            Err(translation_error) => {
                let error_msg = format!("{}", translation_error);
                error!(
                    "LLM translation failed for {} (attempt {}): {}",
                    project.name, attempts, error_msg
                );

                if attempts >= MAX_RETRY_ATTEMPTS {
                    return Err(translation_error);
                }

                // Prepare error context for LLM retry
                last_error_context = Some(format!(
                    "LLM翻译失败: {}\n请确保：\n1. 输出有效的JSON格式\n2. 生成完整可编译的Rust代码\n3. 包含所有必要的依赖和use语句",
                    error_msg
                ));
            }
        }

        // Progressive delay before retry
        let delay_ms = 500 * attempts as u64;
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

/// Create a new Rust project in the project directory
async fn create_rust_project(project: &ProjectInfo) -> Result<()> {
    let rust_project_path = project.path.join("rust_project");

    // Create project directory if it doesn't exist
    if !rust_project_path.exists() {
        fs::create_dir_all(&rust_project_path).await?;
    }

    // Create Cargo.toml
    let cargo_toml_content = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
libc = "0.2"
"#,
        project.name.replace(" ", "_").replace("-", "_")
    );

    fs::write(rust_project_path.join("Cargo.toml"), cargo_toml_content).await?;

    // Create src directory
    let src_dir = rust_project_path.join("src");
    if !src_dir.exists() {
        fs::create_dir_all(&src_dir).await?;
    }

    // Read C files to analyze structure
    let c_files = read_c_source_files(project).await.unwrap_or_default();

    // Create initial main.rs or lib.rs using template generator
    let template_content = translation::generate_project_template(project, &c_files).await?;

    let main_file = if matches!(project.project_type, ProjectType::SingleFile) {
        "main.rs"
    } else {
        "lib.rs"
    };

    fs::write(src_dir.join(main_file), template_content).await?;

    debug!("Created Rust project structure for: {}", project.name);
    Ok(())
}

/// Perform LLM-based translation with structured JSON response
async fn perform_llm_translation(
    project: &ProjectInfo,
    db_manager: Option<&db_services::DatabaseManager>,
) -> Result<String> {
    perform_llm_translation_with_retry(project, db_manager, None).await
}

/// Perform LLM-based translation with retry context
async fn perform_llm_translation_with_retry(
    project: &ProjectInfo,
    db_manager: Option<&db_services::DatabaseManager>,
    retry_context: Option<&str>,
) -> Result<String> {
    debug!("Starting LLM translation for project: {}", project.name);

    // Build translation prompt
    let prompt = if let Some(db_manager) = db_manager {
        if let Some(context) = retry_context {
            translation::build_context_aware_prompt_with_retry(project, db_manager, Some(context))
                .await?
        } else {
            translation::build_context_aware_prompt(project, db_manager).await?
        }
    } else {
        translation::build_enhanced_basic_prompt(project).await?
    };

    // Read C source files
    let c_files = read_c_source_files(project).await?;

    let mut messages = vec![prompt];
    messages.extend(c_files);

    // Create enhanced system prompt for JSON output
    let system_prompt = format!(
        "你是一位精通C和Rust的编译器专家。请将提供的C代码转换为高效、安全的Rust代码。

        项目类型: {:?}

        核心要求:
        1. 严格保持功能一致性
        2. 使用Rust的所有权系统替代手动内存管理
        3. 用Result<T, E>替代错误码
        4. 优先使用Rust原生特性(Option/Result)
        5. 使用惯用的Rust模式(迭代器、模式匹配等)

        输出格式: 必须返回有效的JSON，包含以下字段:
        - \"original\": 原始C代码(字符串)
        - \"rust_code\": 完整的Rust代码(字符串)
        - \"key_changes\": 关键修改说明(字符串数组)
        - \"warnings\": 潜在问题警告(字符串数组)

        特别注意:
        - 遇到未定义行为时添加// FIXME:注释
        - 在warnings中说明所有潜在风险
        - 确保代码可编译
        {}",
        project.project_type,
        if retry_context.is_some() {
            "\n\n注意: 这是重试请求，请特别关注编译错误的修复。"
        } else {
            ""
        }
    );

    // Request translation from LLM
    let response = llm_request_with_prompt(messages, system_prompt)
        .await
        .context("Failed to get LLM translation response")?;

    // Parse structured JSON response
    let translation_result = translation::parse_llm_json_response(&response)
        .context("Failed to parse LLM JSON response")?;

    // Validate the translation
    translation_result
        .validate()
        .context("Translation validation failed")?;

    // Log warnings if any
    if !translation_result.warnings.is_empty() {
        warn!(
            "Translation warnings for {}: {:?}",
            project.name, translation_result.warnings
        );
    }

    if translation_result.has_critical_warnings() {
        warn!(
            "Critical warnings detected in translation for {}",
            project.name
        );
    }

    debug!(
        "Successfully completed LLM translation for: {}",
        project.name
    );
    Ok(translation_result.rust_code)
}

/// Read all C source files from the project directory
async fn read_c_source_files(project: &ProjectInfo) -> Result<Vec<String>> {
    let mut c_files = Vec::new();
    let mut entries = fs::read_dir(&project.path).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "c" || ext == "h" {
                    let content = fs::read_to_string(&path).await?;
                    let file_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    c_files.push(format!("// File: {}\n{}", file_name, content));
                }
            }
        }
    }

    if c_files.is_empty() {
        return Err(anyhow::anyhow!(
            "No C source files found in project: {}",
            project.name
        ));
    }

    Ok(c_files)
}

/// Write translated code to the Rust project
async fn write_translated_code(project: &ProjectInfo, translated_code: &str) -> Result<()> {
    let rust_project_path = project.path.join("rust_project");
    let src_dir = rust_project_path.join("src");

    let main_file = if matches!(project.project_type, ProjectType::SingleFile) {
        "main.rs"
    } else {
        "lib.rs"
    };

    // Clean up the translated code (remove markdown formatting if present)
    let cleaned_code = clean_llm_response(translated_code);

    fs::write(src_dir.join(main_file), cleaned_code).await?;

    debug!("Wrote translated code for: {}", project.name);
    Ok(())
}

/// Clean LLM response to extract just the Rust code
fn clean_llm_response(response: &str) -> String {
    // Remove markdown code blocks if present
    let cleaned = if response.contains("```rust") {
        response
            .split("```rust")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(response)
    } else if response.contains("```") {
        response.split("```").nth(1).unwrap_or(response)
    } else {
        response
    };

    cleaned.trim().to_string()
}

/// Check the translated Rust project using rust_checker
async fn check_rust_project(project: &ProjectInfo) -> Result<(), RustCheckError> {
    let rust_project_path = project.path.join("rust_project");
    let checker = RustCodeCheck::new(rust_project_path);
    checker.check_rust_project()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_discover_projects() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();

        // Create test structure
        let single_files_dir = cache_dir.join("单独文件");
        fs::create_dir_all(&single_files_dir).await.unwrap();
        fs::create_dir_all(single_files_dir.join("test_project"))
            .await
            .unwrap();

        // Create mapping.json
        fs::write(cache_dir.join("mapping.json"), "{}")
            .await
            .unwrap();

        let processor = MainProcessor::new(cache_dir);
        let projects = processor.discover_projects().await.unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "test_project");
    }

    #[tokio::test]
    async fn test_create_rust_project() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test_project");
        fs::create_dir_all(&project_path).await.unwrap();

        let project = ProjectInfo {
            name: "test_project".to_string(),
            path: project_path.clone(),
            project_type: ProjectType::SingleFile,
        };

        create_rust_project(&project).await.unwrap();

        assert!(project_path.join("rust_project/Cargo.toml").exists());
        assert!(project_path.join("rust_project/src/main.rs").exists());
    }

    #[tokio::test]
    async fn test_clean_llm_response() {
        let response_with_markdown = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let cleaned = clean_llm_response(response_with_markdown);
        assert_eq!(cleaned, "fn main() {\n    println!(\"Hello\");\n}");

        let response_without_markdown = "fn main() {\n    println!(\"Hello\");\n}";
        let cleaned2 = clean_llm_response(response_without_markdown);
        assert_eq!(cleaned2, response_without_markdown);
    }
}
