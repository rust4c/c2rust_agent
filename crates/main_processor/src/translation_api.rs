//! High-level API for C to Rust translation
//!
//! This module provides convenient functions for translating C projects to Rust,
//! with proper error handling, progress tracking, and formatting.

use crate::{MainProcessor, ProjectInfo, ProjectType, TranslationStats};
use anyhow::{Context, Result};
use db_services::DatabaseManager;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// High-level API for C to Rust translation
pub struct TranslationAPI {
    processor: MainProcessor,
    db_manager: Option<DatabaseManager>,
}

/// Configuration for translation process
#[derive(Debug, Clone)]
pub struct TranslationConfig {
    /// Enable test mode (no actual LLM calls)
    pub test_mode: bool,
    /// Maximum retry attempts for failed translations
    pub max_retries: usize,
    /// Enable verbose logging
    pub verbose: bool,
    /// Custom cache directory
    pub cache_dir: Option<String>,
    /// Enable database context (requires db_manager)
    pub use_database: bool,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            test_mode: false,
            max_retries: 3,
            verbose: false,
            cache_dir: None,
            use_database: true,
        }
    }
}

/// Result of a single project translation
#[derive(Debug, Clone)]
pub struct ProjectTranslationResult {
    pub project_name: String,
    pub success: bool,
    pub rust_code: Option<String>,
    pub error_message: Option<String>,
    pub warnings: Vec<String>,
}

impl TranslationAPI {
    /// Create new translation API instance
    pub fn new(config: TranslationConfig) -> Self {
        let cache_dir = PathBuf::from(config.cache_dir.unwrap_or_else(|| "cache".to_string()));
        let processor = if config.test_mode {
            MainProcessor::new_test_mode(cache_dir)
        } else {
            MainProcessor::new(cache_dir)
        };

        Self {
            processor,
            db_manager: None,
        }
    }

    /// Create new translation API with database support
    pub async fn new_with_database(
        config: TranslationConfig,
        db_config_path: Option<&str>,
    ) -> Result<Self> {
        let cache_dir = PathBuf::from(config.cache_dir.unwrap_or_else(|| "cache".to_string()));
        let processor = if config.test_mode {
            MainProcessor::new_test_mode(cache_dir)
        } else {
            MainProcessor::new(cache_dir)
        };

        let db_manager = if config.use_database {
            Some(DatabaseManager::new_default().await?)
        } else {
            None
        };

        Ok(Self {
            processor,
            db_manager,
        })
    }

    /// Translate a single C file to Rust
    pub async fn translate_single_file<P: AsRef<Path>>(
        &self,
        c_file_path: P,
        output_dir: Option<P>,
    ) -> Result<ProjectTranslationResult> {
        let c_file_path = c_file_path.as_ref();
        let project_name = c_file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let project_path = output_dir
            .map(|p| p.as_ref().to_path_buf())
            .unwrap_or_else(|| {
                c_file_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(format!("{}_rust", project_name))
            });

        let project = ProjectInfo {
            name: project_name.clone(),
            path: project_path,
            project_type: ProjectType::SingleFile,
        };

        self.translate_project_internal(&project).await
    }

    /// Translate a C project (directory) to Rust
    pub async fn translate_project<P: AsRef<Path>>(
        &self,
        project_dir: P,
        output_dir: Option<P>,
    ) -> Result<ProjectTranslationResult> {
        let project_dir = project_dir.as_ref();
        let project_name = project_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let project_path = output_dir
            .map(|p| p.as_ref().to_path_buf())
            .unwrap_or_else(|| {
                project_dir
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(format!("{}_rust", project_name))
            });

        // Determine project type based on directory structure
        let project_type = self.determine_project_type(project_dir).await?;

        let project = ProjectInfo {
            name: project_name.clone(),
            path: project_path,
            project_type,
        };

        self.translate_project_internal(&project).await
    }

    /// Translate multiple projects in batch
    pub async fn translate_batch<P: AsRef<Path>>(
        &self,
        project_paths: Vec<P>,
    ) -> Result<Vec<ProjectTranslationResult>> {
        info!(
            "Starting batch translation of {} projects",
            project_paths.len()
        );

        let mut results = Vec::new();

        for (index, project_path) in project_paths.into_iter().enumerate() {
            info!("Processing project {}/{}", index + 1, results.capacity());

            let result = self.translate_project(project_path, None).await;
            match result {
                Ok(translation_result) => {
                    results.push(translation_result);
                }
                Err(e) => {
                    warn!("Failed to process project {}: {}", index + 1, e);
                    results.push(ProjectTranslationResult {
                        project_name: format!("project_{}", index + 1),
                        success: false,
                        rust_code: None,
                        error_message: Some(e.to_string()),
                        warnings: vec![],
                    });
                }
            }
        }

        info!("Batch translation completed");
        Ok(results)
    }

    /// Auto-discover and translate C projects in a directory
    pub async fn auto_discover_and_translate<P: AsRef<Path>>(
        &self,
        search_dir: P,
    ) -> Result<Vec<ProjectTranslationResult>> {
        debug!(
            "Auto-discovering projects in: {}",
            search_dir.as_ref().display()
        );

        // Use the processor's discovery functionality
        let discovered_projects = self.processor.discover_projects().await?;

        info!("Discovered {} projects", discovered_projects.len());

        let mut results = Vec::new();
        for project in discovered_projects {
            let result = self.translate_project_internal(&project).await;
            results.push(result?);
        }

        Ok(results)
    }

    /// Get translation statistics summary
    pub fn get_translation_summary(results: &[ProjectTranslationResult]) -> TranslationStats {
        let mut successful = Vec::new();
        let mut failed = HashMap::new();

        for result in results {
            if result.success {
                successful.push(result.project_name.clone());
            } else {
                failed.insert(
                    result.project_name.clone(),
                    result
                        .error_message
                        .clone()
                        .unwrap_or_else(|| "Unknown error".to_string()),
                );
            }
        }

        TranslationStats {
            successful_translations: successful,
            failed_translations: failed,
            retry_attempts: HashMap::new(), // This would need to be tracked separately
        }
    }

    /// Internal method to translate a project
    async fn translate_project_internal(
        &self,
        project: &ProjectInfo,
    ) -> Result<ProjectTranslationResult> {
        debug!("Translating project: {}", project.name);

        match self
            .processor
            .translate_project_complete(project, self.db_manager.as_ref())
            .await
        {
            Ok(rust_code) => Ok(ProjectTranslationResult {
                project_name: project.name.clone(),
                success: true,
                rust_code: Some(rust_code),
                error_message: None,
                warnings: vec![], // Could be enhanced to capture warnings
            }),
            Err(e) => {
                warn!("Translation failed for {}: {}", project.name, e);
                Ok(ProjectTranslationResult {
                    project_name: project.name.clone(),
                    success: false,
                    rust_code: None,
                    error_message: Some(e.to_string()),
                    warnings: vec![],
                })
            }
        }
    }

    /// Determine project type based on directory analysis
    async fn determine_project_type<P: AsRef<Path>>(&self, project_dir: P) -> Result<ProjectType> {
        use tokio::fs;

        let project_dir = project_dir.as_ref();
        let mut c_files = Vec::new();

        let mut entries = fs::read_dir(project_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "c" {
                    c_files.push(path);
                }
            }
        }

        match c_files.len() {
            0 => Err(anyhow::anyhow!("No C files found in project directory")),
            1 => Ok(ProjectType::SingleFile),
            2 => {
                // Check if files are related (same name with .c/.h)
                if c_files.len() == 2 {
                    let names: Vec<_> = c_files
                        .iter()
                        .map(|p| p.file_stem().and_then(|s| s.to_str()).unwrap_or(""))
                        .collect();
                    if names[0] == names[1] {
                        Ok(ProjectType::PairedFiles)
                    } else {
                        Ok(ProjectType::UnrelatedFiles)
                    }
                } else {
                    Ok(ProjectType::UnrelatedFiles)
                }
            }
            _ => Ok(ProjectType::UnrelatedFiles),
        }
    }
}

/// Convenience function for quick single file translation
pub async fn translate_c_file<P: AsRef<Path>>(
    c_file_path: P,
    output_dir: Option<P>,
) -> Result<String> {
    let config = TranslationConfig::default();
    let api = TranslationAPI::new(config);

    let result = api.translate_single_file(c_file_path, output_dir).await?;

    if result.success {
        result
            .rust_code
            .ok_or_else(|| anyhow::anyhow!("Translation succeeded but no Rust code returned"))
    } else {
        Err(anyhow::anyhow!(
            "Translation failed: {}",
            result
                .error_message
                .unwrap_or_else(|| "Unknown error".to_string())
        ))
    }
}

/// Convenience function for quick project translation
pub async fn translate_c_project<P: AsRef<Path>>(
    project_dir: P,
    output_dir: Option<P>,
) -> Result<String> {
    let config = TranslationConfig::default();
    let api = TranslationAPI::new(config);

    let result = api.translate_project(project_dir, output_dir).await?;

    if result.success {
        result
            .rust_code
            .ok_or_else(|| anyhow::anyhow!("Translation succeeded but no Rust code returned"))
    } else {
        Err(anyhow::anyhow!(
            "Translation failed: {}",
            result
                .error_message
                .unwrap_or_else(|| "Unknown error".to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    async fn create_test_c_file(dir: &Path, filename: &str, content: &str) -> Result<()> {
        fs::write(dir.join(filename), content).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_translation_config_default() {
        let config = TranslationConfig::default();
        assert!(!config.test_mode);
        assert_eq!(config.max_retries, 3);
        assert!(config.use_database);
    }

    #[tokio::test]
    async fn test_translation_api_creation() {
        let config = TranslationConfig {
            test_mode: true,
            ..Default::default()
        };
        let api = TranslationAPI::new(config);
        // Should not panic
        assert_eq!(api.db_manager.is_none(), true);
    }

    #[tokio::test]
    async fn test_determine_project_type_single_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let dir_path = temp_dir.path();

        create_test_c_file(dir_path, "main.c", "int main() { return 0; }").await?;

        let config = TranslationConfig {
            test_mode: true,
            ..Default::default()
        };
        let api = TranslationAPI::new(config);

        let project_type = api.determine_project_type(dir_path).await?;
        assert!(matches!(project_type, ProjectType::SingleFile));

        Ok(())
    }

    #[tokio::test]
    async fn test_determine_project_type_paired_files() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let dir_path = temp_dir.path();

        create_test_c_file(
            dir_path,
            "utils.c",
            "int add(int a, int b) { return a + b; }",
        )
        .await?;
        create_test_c_file(dir_path, "utils.h", "int add(int a, int b);").await?;

        let config = TranslationConfig {
            test_mode: true,
            ..Default::default()
        };
        let api = TranslationAPI::new(config);

        let project_type = api.determine_project_type(dir_path).await?;
        assert!(matches!(project_type, ProjectType::PairedFiles));

        Ok(())
    }

    #[tokio::test]
    async fn test_convenience_function() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let c_file = temp_dir.path().join("test.c");

        fs::write(&c_file, "int main() { return 0; }").await?;

        // This should work in test mode
        let result = translate_c_file(&c_file, Some(&temp_dir.path().to_path_buf())).await;

        // In test mode, this should succeed with mock output
        assert!(result.is_ok());

        Ok(())
    }
}
