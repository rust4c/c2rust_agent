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
        // Build a robust candidate list:
        // - mapping.json in the provided path
        // - mapping.json in the parent path(s)
        // - walk up ancestors until root (covers src_cache/mapping.json when indices_dir is src_cache/individual_files/stddef)
        let start = if indices_dir.is_dir() {
            indices_dir.to_path_buf()
        } else {
            indices_dir.parent().unwrap_or(indices_dir).to_path_buf()
        };
        info!("Loading file mappings starting from: {:?}", start);

        let mut candidates: Vec<PathBuf> = vec![
            start.join("mapping.json"),
            start
                .parent()
                .unwrap_or(start.as_path())
                .join("mapping.json"),
        ];

        // Walk up ancestors and add mapping.json at each level
        let mut current = start.as_path();
        let mut safety_guard = 0;
        while let Some(parent) = current.parent() {
            candidates.push(parent.join("mapping.json"));
            current = parent;
            safety_guard += 1;
            if safety_guard > 10 {
                // avoid excessive loops in corner cases
                break;
            }
        }

        // Deduplicate candidates while preserving order
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|p| seen.insert(p.clone()));

        // Try candidates in order
        let mut mapping_path: Option<PathBuf> = None;
        for cand in &candidates {
            if cand.exists() {
                mapping_path = Some(cand.clone());
                break;
            }
        }
        debug!("Mapping file candidates: {:?}", candidates);

        if let Some(path) = mapping_path {
            debug!("Loading mappings from: {:?}", path);
            let content = fs::read_to_string(&path).await?;
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
            warn!(
                "mapping.json not found. Searched candidates near {:?}: {:?}",
                indices_dir, candidates
            );
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

        // 1) Filename-based exact match as a quick fallback
        let input_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        for (cached, orig) in &self.file_mappings {
            let cached_name = cached.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let orig_name = orig.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if cached_name == input_name || orig_name == input_name {
                return orig.clone();
            }
        }

        // 2) Stem-based robust matching to bridge individual_files/paired_files and .c/.h variants
        let input_stem = file_path.file_stem().and_then(|n| n.to_str()).unwrap_or("");
        let input_ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        // Collect candidates whose cached path stem equals the input stem, ignoring the intermediate folder
        let mut stem_matches: Vec<(&PathBuf, &PathBuf)> = Vec::new();
        for (cached, orig) in &self.file_mappings {
            if let Some(stem) = cached.file_stem().and_then(|n| n.to_str()) {
                if stem == input_stem {
                    stem_matches.push((cached, orig));
                }
            }
        }

        // Prefer same-extension match if available
        if !stem_matches.is_empty() {
            if !input_ext.is_empty() {
                if let Some((_, orig)) = stem_matches.iter().find(|(cached, _)| {
                    cached
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case(&input_ext))
                        .unwrap_or(false)
                }) {
                    return (*orig).clone();
                }
            }
            // Otherwise return the first stem match
            let (_, orig) = stem_matches[0];
            return orig.clone();
        }

        // 3) If still unresolved, try swapping extension on any name-equal original
        //    e.g., input is foo.h but only foo.c exists in mapping -> return original with swapped ext
        if !input_stem.is_empty() && !input_ext.is_empty() {
            for (_cached, orig) in &self.file_mappings {
                let orig_stem = orig.file_stem().and_then(|n| n.to_str()).unwrap_or("");
                if orig_stem == input_stem {
                    let mut candidate = orig.clone();
                    // Set to desired extension if different
                    if let Some(_) = candidate.extension() {
                        candidate.set_extension(&input_ext);
                    }
                    return candidate;
                }
            }
        }

        warn!(
            "Could not resolve path mapping for: {}",
            file_path.display()
        );
        // Fall back to using the input file name only; DB queries use LIKE on filename so this still helps
        if !input_name.is_empty() {
            return PathBuf::from(input_name);
        }
        file_path.to_path_buf()
    }
}
