pub mod call_relation;

use anyhow::Result;
use db_services::DatabaseManager;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// File mapping information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMapping {
    pub cached_path: PathBuf,
    pub original_path: PathBuf,
}

/// Function definition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub file_path: PathBuf,
    pub line_number: Option<i32>,
    pub return_type: Option<String>,
    pub parameters: Option<String>,
    pub signature: Option<String>,
}

/// Function call relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRelationship {
    pub caller: String,
    pub called: String,
    pub line: Option<i32>,
    pub caller_file: Option<PathBuf>,
    pub called_file: Option<PathBuf>,
}

/// File dependency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDependency {
    pub from: PathBuf,
    pub to: PathBuf,
    pub dependency_type: String,
}

/// Interface context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceContext {
    pub name: String,
    pub file_path: PathBuf,
    pub language: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Prompt builder for generating context-aware prompts based on relational data
pub struct PromptBuilder<'a> {
    db_manager: &'a DatabaseManager,
    project_name: String,
    file_mappings: HashMap<PathBuf, PathBuf>, // cached_path -> original_path
    reverse_mappings: HashMap<PathBuf, PathBuf>, // original_path -> cached_path
    error_context: Vec<String>,
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

    /// Load file mappings from indices directory
    async fn load_file_mappings(&mut self, indices_dir: &Path) -> Result<()> {
        // Try to locate mapping.json in a few likely places
        let mut mapping_json_candidates: Vec<PathBuf> = Vec::new();
        mapping_json_candidates.push(indices_dir.join("mapping.json"));
        if let Some(p) = indices_dir.parent() {
            mapping_json_candidates.push(p.join("mapping.json"));
            if let Some(pp) = p.parent() {
                mapping_json_candidates.push(pp.join("mapping.json"));
            }
        }

        let mapping_json_path = mapping_json_candidates.iter().find(|p| p.exists()).cloned();

        if let Some(path) = mapping_json_path.as_ref() {
            debug!("Found mapping.json at: {:?}", path);
        } else {
            debug!(
                "mapping.json not found in candidates: {:?}",
                mapping_json_candidates
            );
        }

        // Load mapping.json if present (format: { "mappings": [ { source_path, target_path } ] })
        if let Some(mapping_path) = mapping_json_path {
            let mapping_content = fs::read_to_string(&mapping_path).await?;
            let root: serde_json::Value = serde_json::from_str(&mapping_content)?;
            if let Some(arr) = root.get("mappings").and_then(|v| v.as_array()) {
                for item in arr {
                    let source = item.get("source_path").and_then(|v| v.as_str());
                    let target = item.get("target_path").and_then(|v| v.as_str());
                    if let (Some(source_path), Some(target_path)) = (source, target) {
                        // target_path is the cached path; source_path is the original (may be just a filename)
                        self.file_mappings
                            .insert(PathBuf::from(target_path), PathBuf::from(source_path));
                        self.reverse_mappings
                            .insert(PathBuf::from(source_path), PathBuf::from(target_path));
                    }
                }
            } else {
                warn!(
                    "mapping.json does not contain 'mappings' array: {:?}",
                    mapping_path
                );
            }
        } else {
            warn!(
                "No mapping.json found near indices directory: {:?}",
                indices_dir
            );
        }

        info!(
            "Successfully loaded {} file mappings",
            self.file_mappings.len()
        );
        Ok(())
    }

    /// Resolve input path to original path using loaded mappings; non-fatal if not found
    // input: /Users/peng/Documents/AppCode/Rust/c2rust_agent/test-projects/translate_chibicc/src_cache/individual_files/chibicc
    // mapping.json: /Users/peng/Documents/AppCode/Rust/c2rust_agent/test-projects/translate_chibicc/src_cache/mapping.json
    // {
    // "mappings": [
    //   {
    //     "category": "individual",
    //     "file_type": "source",
    //     "source_path": "unicode.c",
    //     "target_path": "/Users/peng/Documents/AppCode/Rust/c2rust_agent/test-projects/translate_chibicc/src_cache/individual_files/unicode/unicode.c"
    //   },
    fn resolve_original_path(&self, input_path: &Path) -> PathBuf {
        // If input path is a cached path, map to original path
        if let Some(original) = self.file_mappings.get(input_path) {
            return original.clone();
        }
        debug!("Started resolve file path:{:?}", &input_path);

        // If input path is already an original path, return as-is
        if self.reverse_mappings.contains_key(input_path) {
            return input_path.to_path_buf();
        }

        // Try to match by filename
        let input_module_name = input_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        for (cached_path, original_path) in &self.file_mappings {
            let cached_filename = cached_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let original_filename = original_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if cached_filename == input_module_name || original_filename == input_module_name {
                return original_path.clone();
            }
        }

        warn!(
            "Unable to resolve cached path mapping for: {}",
            input_path.display()
        );
        input_path.to_path_buf()
    }

    /// Build context prompt for a specific file
    pub async fn build_file_context_prompt(
        &self,
        file_path: &Path,
        target_functions: Option<Vec<String>>,
    ) -> Result<String> {
        let original_path = if file_path.is_dir() {
            // If input is a directory, try to find the actual file
            let dir_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let mut found_path = None;
            for (cached_path, orig_path) in &self.file_mappings {
                if let Some(cached_filename) = cached_path.file_stem().and_then(|n| n.to_str()) {
                    if cached_filename == dir_name {
                        found_path = Some(orig_path.clone());
                        break;
                    }
                }
            }
            found_path.unwrap_or_else(|| file_path.join(format!("{}.c", dir_name)))
        } else {
            self.resolve_original_path(file_path)
        };
        info!(
            "Building context prompt for file {} (original path: {})",
            file_path.display(),
            original_path.display()
        );

        let mut prompt_sections = Vec::new();

        // 1. File basic information
        if let Ok(file_info) = self.get_file_basic_info(&original_path).await {
            prompt_sections.push(self.format_file_info(&file_info));
        }

        // 2. Functions defined in the file
        if let Ok(functions) = self.get_defined_functions(&original_path).await {
            if !functions.is_empty() {
                prompt_sections.push(self.format_defined_functions(&functions));
            }
        }

        // 3. Function call relationships
        if let Ok(relationships) = self
            .get_call_relationships(&original_path, target_functions.as_ref())
            .await
        {
            if !relationships.is_empty() {
                prompt_sections.push(self.format_call_relationships(&relationships));
            }
        }

        // 4. File dependencies
        if let Ok(dependencies) = self.get_file_dependencies(&original_path).await {
            if !dependencies.is_empty() {
                prompt_sections.push(self.format_file_dependencies(&dependencies));
            }
        }

        // 5. Related interface information
        if let Ok(interfaces) = self.get_interface_context(&original_path).await {
            if !interfaces.is_empty() {
                prompt_sections.push(self.format_interface_context(&interfaces));
            }
        }

        // 6. Build complete prompt
        let display_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| file_path.to_str().unwrap_or(""));
        let full_prompt = self.build_complete_prompt(display_name, &prompt_sections);

        // println!("Built prompt: {}", full_prompt);

        info!(
            "Successfully built prompt with {} context sections",
            prompt_sections.len()
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

        let mut prompt_sections = Vec::new();

        // 1. Function definition information
        if let Ok(Some(func_def)) = self.get_function_definition(function_name).await {
            prompt_sections.push(self.format_function_definition(&func_def));
        }

        // 2. Caller information
        if include_callers {
            if let Ok(callers) = self.get_function_callers(function_name).await {
                if !callers.is_empty() {
                    prompt_sections.push(self.format_function_callers(&callers));
                }
            }
        }

        // 3. Called function information
        if include_callees {
            if let Ok(callees) = self.get_function_callees(function_name).await {
                if !callees.is_empty() {
                    prompt_sections.push(self.format_function_callees(&callees));
                }
            }
        }

        // 4. Add error context if present
        if !self.error_context.is_empty() {
            if let Some(last_error) = self.error_context.last() {
                prompt_sections.push(self.format_error_message(last_error));
            }
        }

        // 5. Build complete function prompt
        let full_prompt = self.build_function_prompt(function_name, &prompt_sections);

        info!(
            "Successfully built function prompt with {} context sections",
            prompt_sections.len()
        );
        Ok(full_prompt)
    }

    /// Add error context for debugging
    pub fn add_error_context(&mut self, error_message: String) {
        debug!("Adding error context: {}", error_message);
        self.error_context.push(error_message);
    }

    /// Get file basic information from database
    async fn get_file_basic_info(&self, file_path: &Path) -> Result<serde_json::Value> {
        debug!("Getting file basic info for: {}", file_path.display());

        let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Use existing schema: code_entries holds per-code metadata
        // Summarize entries for the given file
        let query = r#"
            SELECT file_path, language, project, COUNT(*) AS entry_count
            FROM code_entries
            WHERE (file_path = ? OR file_path LIKE ?)
            GROUP BY file_path, language, project
            ORDER BY entry_count DESC
            LIMIT 1
        "#;

        let params = vec![
            json!(file_path.to_string_lossy().to_string()),
            json!(format!("%{}", file_name)),
        ];

        match self.db_manager.execute_raw_query(query, params).await {
            Ok(results) => {
                if let Some(row) = results.first() {
                    Ok(json!({
                        "file_path": row.get("file_path").unwrap_or(&json!("unknown")),
                        "language": row.get("language").unwrap_or(&json!("c")),
                        "project_name": row.get("project").unwrap_or(&json!("unknown")),
                        "interface_count": row.get("entry_count").unwrap_or(&json!(0))
                    }))
                } else {
                    Ok(json!({
                        "file_path": file_path.to_string_lossy().to_string(),
                        "language": "c",
                        "project_name": self.project_name,
                        "interface_count": 0
                    }))
                }
            }
            Err(e) => {
                warn!("Failed to get file basic info: {}", e);
                Ok(json!({
                    "file_path": file_path.to_string_lossy().to_string(),
                    "language": "c",
                    "project_name": self.project_name,
                    "interface_count": 0
                }))
            }
        }
    }

    /// Get functions defined in the file
    async fn get_defined_functions(&self, file_path: &Path) -> Result<Vec<FunctionInfo>> {
        debug!("Getting defined functions for: {}", file_path.display());

        let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // Pull function definitions from analysis_results joined with code_entries
        let query = r#"
            SELECT ce.file_path AS file_path, ce.function_name AS function_name, ar.result AS result_json
            FROM analysis_results ar
            JOIN code_entries ce ON ce.id = ar.code_id
            WHERE ar.analysis_type = 'function_definition'
              AND (ce.file_path = ? OR ce.file_path LIKE ?)
            ORDER BY ce.updated_at DESC
        "#;

        let params = vec![
            json!(file_path.to_string_lossy().to_string()),
            json!(format!("%{}", file_name)),
        ];

        match self.db_manager.execute_raw_query(query, params).await {
            Ok(results) => {
                let mut functions: Vec<FunctionInfo> = Vec::new();
                for row in results {
                    let file_path_val = row
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let result_json = row
                        .get("result_json")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}");

                    let parsed: serde_json::Value =
                        serde_json::from_str(result_json).unwrap_or(json!({}));
                    let name = parsed.get("name").and_then(|v| v.as_str()).unwrap_or(
                        row.get("function_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown"),
                    );
                    let line = parsed
                        .get("line")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as i32);
                    let return_type = parsed
                        .get("return_type")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    // parameters is array of {name, type}
                    let parameters_str =
                        parsed
                            .get("parameters")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                let parts: Vec<String> = arr
                                    .iter()
                                    .map(|p| {
                                        let t = p
                                            .get("type")
                                            .or_else(|| p.get("r#type"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("?");
                                        let n = p
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("param");
                                        format!("{} {}", t, n)
                                    })
                                    .collect();
                                parts.join(", ")
                            });
                    let signature = Some(format!(
                        "{} {}({})",
                        return_type.as_deref().unwrap_or("void"),
                        name,
                        parameters_str.as_deref().unwrap_or("")
                    ));

                    functions.push(FunctionInfo {
                        name: name.to_string(),
                        file_path: PathBuf::from(file_path_val),
                        line_number: line,
                        return_type,
                        parameters: parameters_str,
                        signature,
                    });
                }

                debug!("Found {} defined functions", functions.len());
                Ok(functions)
            }
            Err(e) => {
                warn!("Failed to get defined functions: {}", e);
                Ok(Vec::new())
            }
        }
    }

    /// Get call relationships for the file
    async fn get_call_relationships(
        &self,
        file_path: &Path,
        _target_functions: Option<&Vec<String>>,
    ) -> Result<HashMap<String, Vec<CallRelationship>>> {
        debug!("Getting call relationships for: {}", file_path.display());
        // Return empty relationships gracefully.
        let relationships: HashMap<String, Vec<CallRelationship>> = HashMap::new();
        debug!("Call relationships not available in current DB schema");
        Ok(relationships)
    }

    /// Get file dependencies
    async fn get_file_dependencies(&self, file_path: &Path) -> Result<Vec<FileDependency>> {
        debug!("Getting file dependencies for: {}", file_path.display());

        // Not available in current DB schema; return empty
        debug!("File dependencies not available in current DB schema");
        Ok(Vec::new())
    }

    /// Get function definition
    async fn get_function_definition(&self, function_name: &str) -> Result<Option<FunctionInfo>> {
        debug!("Getting function definition for: {}", function_name);

        // Join code_entries and analysis_results for this function
        let query = r#"
            SELECT ce.file_path AS file_path, ce.function_name AS function_name, ar.result AS result_json
            FROM code_entries ce
            LEFT JOIN analysis_results ar ON ar.code_id = ce.id AND ar.analysis_type = 'function_definition'
            WHERE ce.function_name = ?
            ORDER BY ce.updated_at DESC
            LIMIT 1
        "#;

        let params = vec![json!(function_name)];

        match self.db_manager.execute_raw_query(query, params).await {
            Ok(results) => {
                if let Some(row) = results.first() {
                    let file_path_val = row
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let result_json = row
                        .get("result_json")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}");
                    let parsed: serde_json::Value =
                        serde_json::from_str(result_json).unwrap_or(json!({}));
                    let name = parsed
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(function_name);
                    let line = parsed
                        .get("line")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as i32);
                    let return_type = parsed
                        .get("return_type")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let parameters_str =
                        parsed
                            .get("parameters")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                let parts: Vec<String> = arr
                                    .iter()
                                    .map(|p| {
                                        let t = p
                                            .get("type")
                                            .or_else(|| p.get("r#type"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("?");
                                        let n = p
                                            .get("name")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("param");
                                        format!("{} {}", t, n)
                                    })
                                    .collect();
                                parts.join(", ")
                            });
                    let signature = Some(format!(
                        "{} {}({})",
                        return_type.as_deref().unwrap_or("void"),
                        name,
                        parameters_str.as_deref().unwrap_or("")
                    ));

                    Ok(Some(FunctionInfo {
                        name: name.to_string(),
                        file_path: PathBuf::from(file_path_val),
                        line_number: line,
                        return_type,
                        parameters: parameters_str,
                        signature,
                    }))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                warn!("Failed to get function definition: {}", e);
                Ok(None)
            }
        }
    }

    /// Get function callers
    async fn get_function_callers(&self, function_name: &str) -> Result<Vec<CallRelationship>> {
        debug!("Getting function callers for: {}", function_name);

        // Not available in current DB schema; return empty
        debug!("Function callers not available in current DB schema");
        Ok(Vec::new())
    }

    /// Get function callees
    async fn get_function_callees(&self, function_name: &str) -> Result<Vec<CallRelationship>> {
        debug!("Getting function callees for: {}", function_name);

        // Not available in current DB schema; return empty
        debug!("Function callees not available in current DB schema");
        Ok(Vec::new())
    }

    /// Get interface context from vector database
    async fn get_interface_context(&self, file_path: &Path) -> Result<Vec<InterfaceContext>> {
        debug!("Getting interface context for: {}", file_path.display());

        let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Search for interfaces related to this file
        let interfaces = self
            .db_manager
            .search_interfaces_by_name("", Some(&self.project_name))
            .await?;

        let mut relevant_interfaces = Vec::new();
        for interface in interfaces {
            let interface_file = Path::new(&interface.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            if file_path.to_string_lossy() == interface.file_path || file_name == interface_file {
                relevant_interfaces.push(InterfaceContext {
                    name: interface.name,
                    file_path: PathBuf::from(interface.file_path),
                    language: interface.language,
                    inputs: interface
                        .inputs
                        .into_iter()
                        .map(|input| format!("{:?}", input))
                        .collect(),
                    outputs: interface
                        .outputs
                        .into_iter()
                        .map(|output| format!("{:?}", output))
                        .collect(),
                });
            }
        }

        // Limit the number of interfaces to avoid overwhelming the prompt
        relevant_interfaces.truncate(10);

        debug!("Found {} relevant interfaces", relevant_interfaces.len());
        Ok(relevant_interfaces)
    }

    // Formatting methods
    fn format_file_info(&self, file_info: &serde_json::Value) -> String {
        format!(
            "## 文件信息\n- 文件路径: {}\n- 编程语言: {}\n- 项目名称: {}\n- 接口数量: {}\n",
            file_info
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
            file_info
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
            file_info
                .get("project_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown"),
            file_info
                .get("interface_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        )
    }

    fn format_defined_functions(&self, functions: &[FunctionInfo]) -> String {
        if functions.is_empty() {
            return String::new();
        }

        let mut section = "## 文件中定义的函数\n".to_string();
        for func in functions {
            section.push_str(&format!(
                "\n### {} (行 {})\n- 返回类型: {}\n- 函数签名: `{}`\n- 参数: {}\n",
                func.name,
                func.line_number.unwrap_or(0),
                func.return_type.as_deref().unwrap_or("unknown"),
                func.signature.as_deref().unwrap_or(&func.name),
                func.parameters.as_deref().unwrap_or("void")
            ));
        }
        section
    }

    fn format_call_relationships(
        &self,
        relationships: &HashMap<String, Vec<CallRelationship>>,
    ) -> String {
        if relationships.is_empty() {
            return String::new();
        }

        let mut section = "## 函数调用关系\n".to_string();

        if let Some(internal_calls) = relationships.get("internal_calls") {
            if !internal_calls.is_empty() {
                section.push_str("### 文件内部调用\n");
                for call in internal_calls {
                    section.push_str(&format!(
                        "- `{}` 调用 `{}` (行 {})\n",
                        call.caller,
                        call.called,
                        call.line.unwrap_or(0)
                    ));
                }
            }
        }

        if let Some(external_calls) = relationships.get("external_calls") {
            if !external_calls.is_empty() {
                section.push_str("\n### 外部文件调用\n");
                for call in external_calls {
                    let caller_file = call
                        .caller_file
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    section.push_str(&format!(
                        "- `{}:{}` 调用 `{}` (行 {})\n",
                        caller_file,
                        call.caller,
                        call.called,
                        call.line.unwrap_or(0)
                    ));
                }
            }
        }

        section
    }

    fn format_file_dependencies(&self, dependencies: &[FileDependency]) -> String {
        if dependencies.is_empty() {
            return String::new();
        }

        let mut section = "## 文件依赖关系\n".to_string();
        for dep in dependencies.iter().take(10) {
            let source_file = dep
                .from
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| dep.from.to_string_lossy().into_owned());
            let target_file = dep
                .to
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| dep.to.to_string_lossy().into_owned());

            section.push_str(&format!(
                "- `{}` → `{}` ({})\n",
                source_file, target_file, dep.dependency_type
            ));
        }
        section
    }

    fn format_interface_context(&self, interfaces: &[InterfaceContext]) -> String {
        if interfaces.is_empty() {
            return String::new();
        }

        let mut section = "## 相关接口信息\n".to_string();
        for interface in interfaces.iter().take(5) {
            let file_name = interface
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| interface.file_path.to_string_lossy().into_owned());

            section.push_str(&format!(
                "\n### {}\n- 文件: {}\n- 语言: {}\n",
                interface.name, file_name, interface.language
            ));
        }
        section
    }

    fn format_function_definition(&self, func_def: &FunctionInfo) -> String {
        let file_name = func_def
            .file_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| func_def.file_path.to_string_lossy().into_owned());

        format!(
            "## 函数定义\n- 函数名: {}\n- 文件: {}\n- 行号: {}\n- 返回类型: {}\n- 函数签名: `{}`\n- 参数: {}\n",
            func_def.name,
            file_name,
            func_def.line_number.unwrap_or(0),
            func_def.return_type.as_deref().unwrap_or("unknown"),
            func_def.signature.as_deref().unwrap_or(&func_def.name),
            func_def.parameters.as_deref().unwrap_or("void")
        )
    }

    fn format_function_callers(&self, callers: &[CallRelationship]) -> String {
        if callers.is_empty() {
            return String::new();
        }

        let mut section = "## 调用该函数的位置\n".to_string();
        for caller in callers {
            let caller_file = caller
                .caller_file
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            section.push_str(&format!(
                "- `{}:{}` (行 {})\n",
                caller_file,
                caller.caller,
                caller.line.unwrap_or(0)
            ));
        }
        section
    }

    fn format_function_callees(&self, callees: &[CallRelationship]) -> String {
        if callees.is_empty() {
            return String::new();
        }

        let mut section = "## 该函数调用的其他函数\n".to_string();
        for callee in callees {
            let called_file = callee
                .called_file
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            section.push_str(&format!(
                "- `{}` 在 `{}` (行 {})\n",
                callee.called,
                called_file,
                callee.line.unwrap_or(0)
            ));
        }
        section
    }

    fn format_error_message(&self, error_message: &str) -> String {
        format!(
            "## 错误信息\n在上一次构建中，发生错误信息: {}\n",
            error_message
        )
    }

    fn build_complete_prompt(&self, file_path: &str, sections: &[String]) -> String {
        let header = format!(
            "# C到Rust转换上下文信息\n\n正在转换文件: **{}**\n\n以下是基于项目调用关系分析得到的上下文信息，请在转换过程中参考这些信息以保持函数调用关系和接口一致性。\n\n",
            file_path
        );

        let content = sections.join("\n");

        let footer = r#"

**角色**：你是一位精通C和Rust的编译器专家，专长于将C代码转换为高效、安全的Rust代码。
**核心指令**：
1. 严格保持功能一致性，优先使用Rust原生特性（如Option/Result）代替C的指针和错误处理
2. 输出必须为JSON格式，包含以下字段：
   - `original`：原始C代码（字符串）
   - `rust_code`：转换后的完整Rust代码（字符串）
   - `key_changes`：关键修改的简要说明（字符串数组）
   - `warnings`：潜在问题警告（字符串数组）
3. 当遇到未定义的C行为时：
   - 添加`// FIXME:`注释标记
   - 在`warnings`中详细说明风险
4. 转换要求：
   - 用`Option<*mut T>`处理可能为NULL的指针
   - 将C宏转换为Rust常量/函数
   - 用`libc` crate处理系统调用
   - 显式标注`unsafe`块

**输出示例**：
```json
{
  "original": "int add(int a, int b) { return a + b; }",
  "rust_code": "fn add(a: i32, b: i32) -> i32 { a + b }",
  "key_changes": ["使用i32替代int", "移除多余分号"],
  "warnings": []
}
```
"#;

        format!("{}{}{}", header, content, footer)
    }

    fn build_function_prompt(&self, function_name: &str, sections: &[String]) -> String {
        let header = format!(
            "# 函数转换上下文信息\n\n正在转换函数: **{}**\n\n以下是该函数的调用关系和上下文信息：\n\n",
            function_name
        );

        let content = sections.join("\n");

        let footer = r#"

## 函数转换指导

请根据上述调用关系信息，确保转换后的Rust函数：
1. 保持与调用者的接口兼容性
2. 正确处理被调用函数的依赖关系
3. 使用适当的Rust类型和错误处理机制
"#;

        format!("{}{}{}", header, content, footer)
    }
}

// // Implement fallback methods for error cases
// impl<'a> PromptBuilder<'a> {
//     fn get_fallback_prompt(&self, file_path: &Path) -> String {
//         let file_name = file_path
//             .file_name()
//             .and_then(|n| n.to_str())
//             .unwrap_or_else(|| file_path.to_str().unwrap_or(""));

//         format!(
//             "# C到Rust转换\n\n正在转换文件: **{}**\n\n由于无法获取详细的上下文信息，请按照以下基本原则进行转换：\n\n1. 保持函数接口的基本结构\n2. 使用Rust标准的类型映射\n3. 添加适当的错误处理\n4. 确保内存安全\n\n请进行标准的C到Rust代码转换。\n",
//             file_name
//         )
//     }

//     fn get_fallback_function_prompt(&self, function_name: &str) -> String {
//         format!(
//             "# 函数转换\n\n正在转换函数: **{}**\n\n请按照标准的C到Rust转换原则进行转换：\n1. 保持函数签名的基本语义\n2. 使用Rust类型系统\n3. 添加错误处理\n4. 确保内存安全\n",
//             function_name
//         )
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    async fn create_test_db_manager() -> DatabaseManager {
        // Create an in-memory database for testing
        db_services::create_database_manager(None, None, None, None)
            .await
            .expect("Failed to create test database manager")
    }

    async fn create_test_file_mappings(dir: &Path) -> Result<()> {
        let mappings = json!({
            "original_file.c": {
                "cached_path": "cached_file.c"
            },
            "another_file.c": {
                "cached_path": "cached_another.c"
            }
        });

        let mappings_file = dir.join("file_mappings.json");
        let mut file = File::create(&mappings_file).await?;
        file.write_all(mappings.to_string().as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    async fn create_test_mapping_json(dir: &Path) -> Result<()> {
        // Create a mapping.json that follows the { "mappings": [ { source_path, target_path } ] } format
        let mappings = json!({
            "mappings": [
                {
                    "category": "individual",
                    "file_type": "source",
                    "source_path": "unicode.c",
                    "target_path": "/tmp/src_cache/individual_files/unicode/unicode.c"
                },
                {
                    "category": "individual",
                    "file_type": "source",
                    "source_path": "lexer.c",
                    "target_path": "/tmp/src_cache/individual_files/lexer/lexer.c"
                }
            ]
        });

        let mappings_file = dir.join("mapping.json");
        let mut file = File::create(&mappings_file).await?;
        file.write_all(mappings.to_string().as_bytes()).await?;
        file.flush().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_prompt_builder_creation() {
        let db_manager = create_test_db_manager().await;
        let builder = PromptBuilder::new(&db_manager, "test_project".to_string(), None).await;

        assert!(builder.is_ok());
        let builder = builder.unwrap();
        assert_eq!(builder.project_name, "test_project");
        assert!(builder.file_mappings.is_empty());
    }

    #[tokio::test]
    async fn test_file_mappings_loading() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        create_test_file_mappings(temp_dir.path())
            .await
            .expect("Failed to create test mappings");

        let db_manager = create_test_db_manager().await;
        let builder = PromptBuilder::new(
            &db_manager,
            "test_project".to_string(),
            Some(temp_dir.path().to_path_buf()),
        )
        .await;

        assert!(builder.is_ok());
        let builder = builder.unwrap();
        assert_eq!(builder.file_mappings.len(), 2);
        assert_eq!(
            builder.file_mappings.get(Path::new("cached_file.c")),
            Some(&PathBuf::from("original_file.c"))
        );
    }

    #[tokio::test]
    async fn test_mapping_json_loading_and_resolution() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        create_test_mapping_json(temp_dir.path())
            .await
            .expect("Failed to create mapping.json");

        let db_manager = create_test_db_manager().await;
        let builder = PromptBuilder::new(
            &db_manager,
            "test_project".to_string(),
            Some(temp_dir.path().to_path_buf()),
        )
        .await
        .expect("Failed to create builder");

        // Ensure mapping.json was loaded and used: cached target path -> original source path (possibly just a filename)
        assert_eq!(
            builder.resolve_original_path(Path::new(
                "/tmp/src_cache/individual_files/unicode/unicode.c"
            )),
            PathBuf::from("unicode.c")
        );
        assert_eq!(
            builder
                .resolve_original_path(Path::new("/tmp/src_cache/individual_files/lexer/lexer.c")),
            PathBuf::from("lexer.c")
        );

        // Original path passthrough (if given as source_path itself)
        assert_eq!(
            builder.resolve_original_path(Path::new("unicode.c")),
            PathBuf::from("unicode.c")
        );
    }

    #[tokio::test]
    async fn test_resolve_original_path() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        create_test_file_mappings(temp_dir.path())
            .await
            .expect("Failed to create test mappings");

        let db_manager = create_test_db_manager().await;
        let builder = PromptBuilder::new(
            &db_manager,
            "test_project".to_string(),
            Some(temp_dir.path().to_path_buf()),
        )
        .await
        .expect("Failed to create builder");

        // Test cached path resolution
        assert_eq!(
            builder.resolve_original_path(Path::new("cached_file.c")),
            PathBuf::from("original_file.c")
        );

        // Test original path passthrough
        assert_eq!(
            builder.resolve_original_path(Path::new("original_file.c")),
            PathBuf::from("original_file.c")
        );

        // Test unknown path
        assert_eq!(
            builder.resolve_original_path(Path::new("unknown.c")),
            PathBuf::from("unknown.c")
        );
    }

    #[tokio::test]
    async fn test_build_file_context_prompt() {
        let db_manager = create_test_db_manager().await;
        let builder = PromptBuilder::new(&db_manager, "test_project".to_string(), None)
            .await
            .expect("Failed to create builder");

        let result = builder
            .build_file_context_prompt(Path::new("test_file.c"), None)
            .await;
        assert!(result.is_ok());

        let prompt = result.unwrap();
        assert!(prompt.contains("C到Rust转换上下文信息"));
        assert!(prompt.contains("test_file.c"));
        assert!(prompt.contains("JSON格式"));
    }

    #[tokio::test]
    async fn test_build_function_context_prompt() {
        let db_manager = create_test_db_manager().await;
        let builder = PromptBuilder::new(&db_manager, "test_project".to_string(), None)
            .await
            .expect("Failed to create builder");

        let result = builder
            .build_function_context_prompt("test_function", true, true)
            .await;
        assert!(result.is_ok());

        let prompt = result.unwrap();
        assert!(prompt.contains("函数转换上下文信息"));
        assert!(prompt.contains("test_function"));
        assert!(prompt.contains("调用关系"));
    }

    #[tokio::test]
    async fn test_error_context() {
        let db_manager = create_test_db_manager().await;
        let mut builder = PromptBuilder::new(&db_manager, "test_project".to_string(), None)
            .await
            .expect("Failed to create builder");

        builder.add_error_context("Test error message".to_string());
        assert_eq!(builder.error_context.len(), 1);
        assert_eq!(builder.error_context[0], "Test error message");

        let result = builder
            .build_function_context_prompt("test_function", true, true)
            .await;
        assert!(result.is_ok());

        let prompt = result.unwrap();
        assert!(prompt.contains("错误信息"));
        assert!(prompt.contains("Test error message"));
    }

    #[tokio::test]
    async fn test_format_methods() {
        let db_manager = create_test_db_manager().await;
        let builder = PromptBuilder::new(&db_manager, "test_project".to_string(), None)
            .await
            .expect("Failed to create builder");

        // Test file info formatting
        let file_info = json!({
            "file_path": "test.c",
            "language": "c",
            "project_name": "test_project",
            "interface_count": 5
        });
        let formatted = builder.format_file_info(&file_info);
        assert!(formatted.contains("文件信息"));
        assert!(formatted.contains("test.c"));
        assert!(formatted.contains("5"));

        // Test function info formatting
        let functions = vec![FunctionInfo {
            name: "test_func".to_string(),
            file_path: PathBuf::from("test.c"),
            line_number: Some(10),
            return_type: Some("int".to_string()),
            parameters: Some("int a, int b".to_string()),
            signature: Some("int test_func(int a, int b)".to_string()),
        }];
        let formatted = builder.format_defined_functions(&functions);
        assert!(formatted.contains("文件中定义的函数"));
        assert!(formatted.contains("test_func"));
        assert!(formatted.contains("行 10"));
    }
}
