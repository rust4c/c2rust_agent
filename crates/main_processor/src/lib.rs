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

mod pkg_config;
use single_processor::single_processes;

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
    test_mode: bool,
}

impl MainProcessor {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            multi_progress: MultiProgress::new(),
            semaphore: Arc::new(Semaphore::new(CONCURRENT_LIMIT)),
            test_mode: false,
        }
    }

    pub fn new_test_mode(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            multi_progress: MultiProgress::new(),
            semaphore: Arc::new(Semaphore::new(CONCURRENT_LIMIT)),
            test_mode: true,
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
        let single_files_dir = self.cache_dir.join("individual_files");
        let paired_files_dir = self.cache_dir.join("paired_files");
        let unrelated_files_dir = self.cache_dir.join("unrelated_files");

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
            let result = process_single_project(project.clone(), None, self.test_mode).await;

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

    /// Complete integrated translation workflow for a single project
    pub async fn translate_project_complete(
        &self,
        project: &ProjectInfo,
        db_manager: Option<&db_services::DatabaseManager>,
    ) -> Result<String> {
        debug!(
            "Starting complete translation workflow for: {}",
            project.name
        );

        // Step 1: Create Rust project structure
        create_rust_project(project).await?;

        // Step 2: Perform LLM translation with context
        let translated_code = if self.test_mode {
            perform_llm_translation_test_mode(project).await?
        } else {
            perform_llm_translation_with_retry(project, db_manager, None, false).await?
        };

        // Step 3: Write translated code to file
        write_translated_code(project, &translated_code).await?;

        // Step 4: Check compilation (optional, for validation)
        if let Err(e) = check_rust_project(project).await {
            warn!(
                "Rust project compilation check failed for {}: {:?}",
                project.name, e
            );
            // Continue anyway - some projects might have missing dependencies
        }

        info!(
            "✅ Complete translation workflow finished for: {}",
            project.name
        );
        Ok(translated_code)
    }

    /// Batch translate multiple projects with progress tracking
    pub async fn translate_projects_batch(
        &self,
        projects: Vec<ProjectInfo>,
        db_manager: Option<&db_services::DatabaseManager>,
    ) -> Result<TranslationStats> {
        let mut stats = TranslationStats {
            successful_translations: Vec::new(),
            failed_translations: HashMap::new(),
            retry_attempts: HashMap::new(),
        };

        info!("Starting batch translation for {} projects", projects.len());

        for project in projects {
            match self.translate_project_complete(&project, db_manager).await {
                Ok(_) => {
                    stats.successful_translations.push(project.name.clone());
                    info!("✅ Successfully translated: {}", project.name);
                }
                Err(e) => {
                    stats
                        .failed_translations
                        .insert(project.name.clone(), e.to_string());
                    error!("❌ Failed to translate {}: {}", project.name, e);
                }
            }
        }

        info!(
            "🎉 Batch translation completed: {} success, {} failed",
            stats.successful_translations.len(),
            stats.failed_translations.len()
        );

        Ok(stats)
    }
}

/// Process a single project through the translation workflow with intelligent retry
async fn process_single_project(
    project: ProjectInfo,
    db_manager: Option<&db_services::DatabaseManager>,
    test_mode: bool,
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
            if test_mode {
                perform_llm_translation_test_mode(&project).await
            } else {
                perform_llm_translation(&project, db_manager).await
            }
        } else {
            perform_llm_translation_with_retry(
                &project,
                db_manager,
                last_error_context.as_deref(),
                test_mode,
            )
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
