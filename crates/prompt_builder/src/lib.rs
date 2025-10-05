//! Prompt Builder - Context-aware prompt generation for C to Rust translation
//!
//! This crate provides functionality to build intelligent prompts based on code analysis,
//! function relationships, and project context stored in a database.
//!
//! ## Linus's Wisdom
//! "Talk is cheap. Show me the code."
//! This module is now split into clear, single-purpose parts. No bloat.

pub mod call_relation;
pub mod formatter;
pub mod prompt_loader;
pub mod query;
pub mod types;

// Re-export commonly used types
pub use types::{CallRelationship, FileDependency, FileMapping, FunctionInfo, InterfaceContext};

use anyhow::Result;
use db_services::DatabaseManager;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Prompt builder for generating context-aware prompts based on relational data
pub struct PromptBuilder<'a> {
    db_manager: &'a DatabaseManager,
    project_name: String,
    file_mappings: HashMap<PathBuf, PathBuf>, // cached_path -> original_path
    reverse_mappings: HashMap<PathBuf, PathBuf>, // original_path -> cached_path
    error_context: Vec<String>,
    prompt_loader: prompt_loader::PromptLoader,
}

impl<'a> PromptBuilder<'a> {
    /// Create a new PromptBuilder instance
    pub async fn new(
        db_manager: &'a DatabaseManager,
        project_name: String,
        indices_dir: Option<PathBuf>,
    ) -> Result<Self> {
        let mut builder = Self {
            db_manager,
            project_name,
            file_mappings: HashMap::new(),
            reverse_mappings: HashMap::new(),
            error_context: Vec::new(),
            prompt_loader: prompt_loader::PromptLoader::default()?,
        };

        if let Some(dir) = indices_dir.as_ref() {
            builder.load_file_mappings(dir).await?;
        }

        info!(
            "PromptBuilder initialized for project: {}",
            builder.project_name
        );
        Ok(builder)
    }

    /// Add error context from previous build attempts
    pub fn add_error_context(&mut self, error_message: String) {
        debug!("Added error context: {}", error_message);
        self.error_context.push(error_message);
    }

    /// Build context prompt for a specific file
    pub async fn build_file_context_prompt(
        &self,
        file_path: &Path,
        target_functions: Option<Vec<String>>,
    ) -> Result<String> {
        let original_path = self.resolve_path_for_query(file_path);
        info!(
            "Building context prompt for file {} (original: {})",
            file_path.display(),
            original_path.display()
        );

        let mut sections = Vec::new();

        // 1. File basic info
        if let Ok(file_info) =
            query::get_file_basic_info(self.db_manager, &original_path, &self.project_name).await
        {
            sections.push(formatter::format_file_info(&file_info));
        }

        // 2. Defined functions
        if let Ok(functions) = query::get_defined_functions(self.db_manager, &original_path).await {
            if !functions.is_empty() {
                sections.push(formatter::format_defined_functions(&functions));
            }
        }

        // 3. Call relationships
        if let Ok(relationships) = query::get_call_relationships(
            self.db_manager,
            &original_path,
            target_functions.as_ref(),
        )
        .await
        {
            if !relationships.is_empty() {
                sections.push(formatter::format_call_relationships(&relationships));
            }
        }

        // 4. File dependencies
        if let Ok(dependencies) =
            query::get_file_dependencies(self.db_manager, &original_path).await
        {
            if !dependencies.is_empty() {
                sections.push(formatter::format_file_dependencies(&dependencies));
            }
        }

        // 5. Interface context
        if let Ok(interfaces) =
            query::get_interface_context(self.db_manager, &original_path, &self.project_name).await
        {
            if !interfaces.is_empty() {
                sections.push(formatter::format_interface_context(&interfaces));
            }
        }

        // 6. Error context if any
        if !self.error_context.is_empty() {
            for err in &self.error_context {
                sections.push(formatter::format_error_message(err));
            }
        }

        // 7. Load conversion guide and build final prompt
        let conversion_guide = self
            .prompt_loader
            .load_file_conversion_prompt()
            .await
            .unwrap_or_else(|_| String::from("# 转换指导规则\n请按照标准C到Rust转换规则进行。"));

        let display_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| file_path.to_str().unwrap_or(""));

        let full_prompt = formatter::build_file_prompt(display_name, &sections, &conversion_guide);

        info!(
            "Successfully built prompt with {} context sections",
            sections.len()
        );
        Ok(full_prompt)
    }

    /// Build context prompt for a specific function
    pub async fn build_function_context_prompt(
        &self,
        function_name: &str,
        include_callers: bool,
        include_callees: bool,
    ) -> Result<String> {
        info!("Building context prompt for function: {}", function_name);

        let mut sections = Vec::new();

        // 1. Function definition
        if let Ok(Some(func_def)) =
            query::get_function_definition(self.db_manager, function_name).await
        {
            sections.push(formatter::format_function_definition(&func_def));
        }

        // 2. Callers
        if include_callers {
            if let Ok(callers) = query::get_function_callers(self.db_manager, function_name).await {
                if !callers.is_empty() {
                    sections.push(formatter::format_function_callers(&callers));
                }
            }
        }

        // 3. Callees
        if include_callees {
            if let Ok(callees) = query::get_function_callees(self.db_manager, function_name).await {
                if !callees.is_empty() {
                    sections.push(formatter::format_function_callees(&callees));
                }
            }
        }

        // 4. Error context
        if !self.error_context.is_empty() {
            for err in &self.error_context {
                sections.push(formatter::format_error_message(err));
            }
        }

        // 5. Load function conversion guide
        let conversion_guide = self
            .prompt_loader
            .load_function_conversion_prompt()
            .await
            .unwrap_or_else(|_| String::from("# 函数转换指导\n请按照标准转换规则进行。"));

        let full_prompt =
            formatter::build_function_prompt(function_name, &sections, &conversion_guide);

        info!(
            "Successfully built function prompt with {} sections",
            sections.len()
        );
        Ok(full_prompt)
    }

    // ===== Internal helper methods =====

    /// Load file mappings from indices directory
    async fn load_file_mappings(&mut self, indices_dir: &Path) -> Result<()> {
        let mut candidates: Vec<PathBuf> = vec![
            indices_dir.join("mapping.json"),
            indices_dir
                .parent()
                .unwrap_or(indices_dir)
                .join("mapping.json"),
        ];
        if let Some(parent) = indices_dir.parent().and_then(|p| p.parent()) {
            candidates.push(parent.join("mapping.json"));
        }

        let mapping_path = candidates.iter().find(|p| p.exists());

        if let Some(path) = mapping_path {
            debug!("Loading mappings from: {:?}", path);
            let content = fs::read_to_string(path).await?;
            let root: serde_json::Value = serde_json::from_str(&content)?;

            if let Some(arr) = root.get("mappings").and_then(|v| v.as_array()) {
                for item in arr {
                    if let (Some(source), Some(target)) = (
                        item.get("source_path").and_then(|v| v.as_str()),
                        item.get("target_path").and_then(|v| v.as_str()),
                    ) {
                        self.file_mappings
                            .insert(PathBuf::from(target), PathBuf::from(source));
                        self.reverse_mappings
                            .insert(PathBuf::from(source), PathBuf::from(target));
                    }
                }
            }
            info!("Loaded {} file mappings", self.file_mappings.len());
        } else {
            warn!("No mapping.json found near {:?}", indices_dir);
        }

        Ok(())
    }

    /// Resolve input path for database queries
    /// Handles directory inputs and cached path mappings
    fn resolve_path_for_query(&self, file_path: &Path) -> PathBuf {
        // Handle directory input
        if file_path.is_dir() {
            let dir_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            for (cached, orig) in &self.file_mappings {
                if let Some(cached_stem) = cached.file_stem().and_then(|n| n.to_str()) {
                    if cached_stem == dir_name {
                        return orig.clone();
                    }
                }
            }
            return file_path.join(format!("{}.c", dir_name));
        }

        // Direct mapping lookup
        if let Some(original) = self.file_mappings.get(file_path) {
            return original.clone();
        }

        // Already original path
        if self.reverse_mappings.contains_key(file_path) {
            return file_path.to_path_buf();
        }

        // Filename-based matching
        let input_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        for (cached, orig) in &self.file_mappings {
            let cached_name = cached.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let orig_name = orig.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if cached_name == input_name || orig_name == input_name {
                return orig.clone();
            }
        }

        warn!(
            "Could not resolve path mapping for: {}",
            file_path.display()
        );
        file_path.to_path_buf()
    }
}
