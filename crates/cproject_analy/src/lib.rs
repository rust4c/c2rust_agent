pub mod file_remanager;
pub mod pkg_config;
use crate::pkg_config::{PreprocessorConfig, get_config};
use file_remanager::{CProjectPreprocessor, ProcessingStats};

use db_services::{DatabaseManager, create_database_manager};
use lsp_services::lsp_services::ClangdAnalyzer;

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{debug, error, info, warn};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct PreProcessor {
    config: PreprocessorConfig,
    db_manager: Option<DatabaseManager>,
    multi_progress: MultiProgress,
}

impl PreProcessor {
    /// Create new preprocessor instance
    pub fn new(config: PreprocessorConfig) -> Self {
        Self {
            config,
            db_manager: None,
            multi_progress: MultiProgress::new(),
        }
    }

    /// Create preprocessor with default configuration
    pub fn new_default() -> Self {
        let config = match get_config() {
            Ok(config) => config,
            Err(err) => {
                error!("Failed to load config: {}", err);
                PreprocessorConfig::default()
            }
        };
        Self::new(config)
    }

    /// Initialize database connection
    pub async fn initialize_database(&mut self) -> Result<()> {
        let main_pb = self.multi_progress.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("🔌 Connecting to database...");

        // Initialize database manager
        self.db_manager = Some(
            create_database_manager(None, self.config.qdrant_url.as_deref(), None, Some(384))
                .await
                .context("Failed to initialize database manager")?,
        );

        main_pb.finish_with_message("✅ Database connection successful!");
        info!("Database initialization completed");
        Ok(())
    }

    /// Execute project preprocessing
    pub async fn preprocess_project(
        &mut self,
        source_dir: &Path,
        cache_dir: &Path,
    ) -> Result<ProcessingStats> {
        info!(
            "Starting project preprocessing: {} -> {}",
            source_dir.display(),
            cache_dir.display()
        );

        // Ensure database is initialized
        if self.db_manager.is_none() {
            self.initialize_database().await?;
        }

        // Create cache directory
        if !cache_dir.exists() {
            fs::create_dir_all(cache_dir).context("Failed to create cache directory")?;
        }

        // Step 1: File organization and mapping generation
        let file_processing_stats = self.process_files(source_dir, cache_dir).await?;
        info!("file_remanager complete");

        // Step 2: Parallel LSP analysis and database storage
        let mapping_path = cache_dir.join("mapping.json");
        if mapping_path.exists() {
            self.parallel_analysis_and_storage(source_dir, cache_dir) //, &mapping_path)
                .await?;
        } else {
            warn!("Mapping file does not exist, skipping LSP analysis");
        }
        info!("analysis complete");

        Ok(file_processing_stats)
    }

    /// Process file organization
    async fn process_files(
        &mut self,
        source_dir: &Path,
        cache_dir: &Path,
    ) -> Result<ProcessingStats> {
        let main_pb = self.multi_progress.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("📁 Starting project file organization...");

        let mut preprocessor = CProjectPreprocessor::new(self.config.preprocess_config.clone());
        debug!(
            "remanager files: {} -> {}",
            source_dir.display(),
            cache_dir.display()
        );
        let stats = preprocessor
            .preprocess_project(source_dir, cache_dir)
            .context("Failed to preprocess project files")?;

        main_pb.finish_with_message("✅ File organization completed!");
        Ok(stats)
    }

    /// Execute LSP analysis and database storage in parallel
    async fn parallel_analysis_and_storage(
        &mut self,
        source_dir: &Path,
        cache_dir: &Path,
        // mapping_path: &Path,
    ) -> Result<()> {
        let main_pb = self.multi_progress.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("🔄 Starting parallel analysis and storage...");

        let db_manager = Arc::new(self.db_manager.take().unwrap());

        // Create progress bars
        let lsp_pb = self.multi_progress.add(ProgressBar::new_spinner());
        lsp_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.yellow} {msg}")
                .unwrap(),
        );
        lsp_pb.enable_steady_tick(Duration::from_millis(100));

        let db_pb = self.multi_progress.add(ProgressBar::new_spinner());
        db_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.magenta} {msg}")
                .unwrap(),
        );
        db_pb.enable_steady_tick(Duration::from_millis(100));

        // Prepare thread data
        let source_dir = source_dir.to_path_buf();
        let cache_dir = cache_dir.to_path_buf();

        // Start LSP analysis thread
        let lsp_handle = {
            let lsp_pb = lsp_pb.clone();
            let source_dir = source_dir.clone();
            let cache_dir = cache_dir.clone();

            thread::spawn(move || -> Result<()> {
                debug!("thread lsp analyze started");
                lsp_pb.set_message("🔍 Performing LSP analysis...");

                let mut analyzer = ClangdAnalyzer::new(source_dir.to_str().unwrap());
                analyzer.analyze_project().context("LSP analysis failed")?;

                // Save analysis results to cache directory
                let analysis_path = cache_dir.join("lsp_analysis.json");
                let analysis_result = serde_json::json!({
                    "functions": analyzer.functions,
                    "classes": analyzer.classes,
                    "variables": analyzer.variables,
                    "macros": analyzer.macros,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });

                fs::write(
                    &analysis_path,
                    serde_json::to_string_pretty(&analysis_result)?,
                )
                .context("Failed to save LSP analysis results")?;

                lsp_pb.finish_with_message("✅ LSP analysis completed!");
                debug!("LSP analysis results saved to {}", analysis_path.display());
                Ok(())
            })
        };

        // Wait for thread completion
        let lsp_result = lsp_handle
            .join()
            .map_err(|e| anyhow::anyhow!("LSP thread panicked: {:?}", e))?;

        // Check results
        if let Err(e) = lsp_result {
            error!("LSP analysis failed: {}", e);
        }

        // Generate vectors based on FastEmbed and batch insert to database
        let embed_pb = self.multi_progress.add(ProgressBar::new_spinner());
        embed_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        embed_pb.enable_steady_tick(Duration::from_millis(100));
        embed_pb.set_message("🧠 Generating vectors and batch inserting to database...");

        let analysis_path = cache_dir.join("lsp_analysis.json");
        if analysis_path.exists() {
            let analysis_content =
                fs::read_to_string(&analysis_path).context("Failed to read LSP analysis file")?;
            let analysis_json: Value = serde_json::from_str(&analysis_content)
                .context("Failed to parse LSP analysis JSON")?;

            if let Some(funcs) = analysis_json.get("functions").and_then(|v| v.as_array()) {
                // Prepare embedding documents and batch insert data
                let mut documents: Vec<String> = Vec::new();
                let mut interfaces_data: Vec<HashMap<String, serde_json::Value>> = Vec::new();

                for f in funcs {
                    let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let return_type = f
                        .get("return_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("void");
                    let file_path = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
                    let line = f.get("line").and_then(|v| v.as_u64()).unwrap_or(0);

                    // Parameters
                    let mut params_vec: Vec<String> = Vec::new();
                    let mut inputs_meta: Vec<HashMap<String, serde_json::Value>> = Vec::new();
                    if let Some(params) = f.get("parameters").and_then(|v| v.as_array()) {
                        for p in params {
                            let pname = p.get("name").and_then(|v| v.as_str()).unwrap_or("param");
                            let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
                            params_vec.push(format!("{} {}", ptype, pname));

                            let mut pin = HashMap::new();
                            pin.insert("name".to_string(), json!(pname));
                            pin.insert("type".to_string(), json!(ptype));
                            inputs_meta.push(pin);
                        }
                    }
                    let params_str = params_vec.join(", ");
                    let signature = format!("{} {}({});", return_type, name, params_str);

                    documents.push(signature.clone());

                    let mut meta = HashMap::new();
                    meta.insert("line".to_string(), json!(line));
                    meta.insert("source".to_string(), json!("lsp_analysis"));

                    let mut data = HashMap::new();
                    data.insert("code".to_string(), json!(signature));
                    data.insert("language".to_string(), json!("c"));
                    data.insert("name".to_string(), json!(name));
                    // Project name: prefer directory name, otherwise use full path
                    let project_name = source_dir
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| source_dir.to_string_lossy().to_string());
                    data.insert("project_name".to_string(), json!(project_name));
                    data.insert("file_path".to_string(), json!(file_path));
                    data.insert("inputs".to_string(), json!(inputs_meta));
                    data.insert("outputs".to_string(), json!([{"return_type": return_type}]));
                    data.insert("metadata".to_string(), json!(meta));

                    interfaces_data.push(data);
                }

                if !documents.is_empty() {
                    // Initialize FastEmbed, BGELargeENV15 -> 1024 dimensions, compatible with default Qdrant configuration
                    let mut model =
                        TextEmbedding::try_new(InitOptions::new(EmbeddingModel::BGELargeENV15))
                            .map_err(|e| {
                                anyhow::anyhow!(format!("Failed to initialize FastEmbed: {}", e))
                            })?;

                    // Execute embedding
                    let embeddings = model.embed(documents.clone(), None).map_err(|e| {
                        anyhow::anyhow!(format!("FastEmbed embedding failed: {}", e))
                    })?;

                    // Attach vectors
                    for (i, emb) in embeddings.into_iter().enumerate() {
                        if let Some(item) = interfaces_data.get_mut(i) {
                            item.insert("vector".to_string(), json!(emb));
                        }
                    }

                    // Batch insert to database
                    let _saved = db_manager
                        .batch_store_interfaces(interfaces_data)
                        .await
                        .context("Failed to batch store interfaces with vectors")?;

                    embed_pb
                        .finish_with_message("✅ Vector generation and batch insertion completed!");
                } else {
                    embed_pb.finish_with_message(
                        "ℹ️ No functions need embedding, skipping vector insertion",
                    );
                }
            } else {
                embed_pb.finish_with_message(
                    "ℹ️ LSP analysis results contain no functions, skipping vector insertion",
                );
            }
        } else {
            embed_pb.finish_with_message(
                "⚠️ LSP analysis results not found, skipping vector insertion",
            );
        }

        // Restore database manager
        self.db_manager =
            Some(Arc::try_unwrap(db_manager).map_err(|_| anyhow::anyhow!("Failed to unwrap Arc"))?);

        main_pb.finish_with_message("✅ Analysis and storage completed!");
        Ok(())
    }

    /// Get database manager reference
    pub fn get_database_manager(&self) -> Option<&DatabaseManager> {
        self.db_manager.as_ref()
    }

    /// Get multi-progress manager reference
    pub fn get_multi_progress(&self) -> &MultiProgress {
        &self.multi_progress
    }

    /// Cleanup resources
    pub async fn cleanup(&mut self) -> Result<()> {
        if let Some(db_manager) = &mut self.db_manager {
            db_manager.close().await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_preprocessor_creation() {
        let processor = PreProcessor::new_default();
        assert!(processor.db_manager.is_none());
    }

    #[tokio::test]
    async fn test_database_initialization() {
        let _processor = PreProcessor::new_default();
        // Note: This test requires actual database service running
        // processor.initialize_database().await.unwrap();
        // assert!(processor.db_manager.is_some());
    }
}
