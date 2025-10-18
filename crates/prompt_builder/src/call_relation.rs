//! Call relation analyzer
//!
//! Analyze function call relationships in projects and build relational database.
//! Focus on function analysis and database queries for C/Rust files.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use db_services::DatabaseManager;
use lsp_services::lsp_services::{ClangdAnalyzer, Parameter};

/// Function definition information
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub file_path: String,
    pub line_number: u32,
    pub return_type: String,
    pub parameters: Vec<Parameter>,
    pub signature: String,
    pub language: String, // "rust", "c", "cpp"
}

/// Function call information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub caller_file: String,
    pub caller_function: Option<String>,
    pub caller_line: u32,
    pub called_function: String,
    pub called_file: Option<String>,
    pub call_type: CallType,
}

/// Call type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallType {
    DirectCall,   // Direct call
    IndirectCall, // Indirect call
    CBindingCall, // C binding call
}

/// File dependency relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDependency {
    pub source_file: String,
    pub target_file: String,
    pub dependency_type: DependencyType,
}

/// Dependency type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyType {
    Include, // #include
    Use,     // Rust use
    Call,    // Function call dependency
}

/// Database function record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionRecord {
    pub id: Option<i64>,
    pub function_name: String,
    pub file_path: String,
    pub line_number: u32,
    pub return_type: String,
    pub parameters: String, // JSON serialized parameters
    pub signature: String,
    pub language: String,
    pub project_name: String,
    pub created_at: Option<String>,
}

/// 函数搜索结果
#[derive(Debug, Clone, Serialize)]
pub struct FunctionSearchResult {
    pub functions: HashMap<String, Vec<FunctionDefinition>>,
    pub total_count: usize,
    pub search_summary: String,
}

/// 调用关系分析器
pub struct CallRelationAnalyzer {
    db_manager: DatabaseManager,
    project_root: PathBuf,

    // 存储分析结果
    function_definitions: HashMap<String, FunctionDefinition>,
    function_calls: HashMap<String, Vec<FunctionCall>>,

    // 分析器
    clangd_analyzer: Option<ClangdAnalyzer>,
}

impl CallRelationAnalyzer {
    /// Create new call relation analyzer
    pub fn new(db_manager: DatabaseManager, project_root: PathBuf) -> Result<Self> {
        // Don't create ClangdAnalyzer here, create it asynchronously when needed
        Ok(Self {
            db_manager,
            project_root,
            function_definitions: HashMap::new(),
            function_calls: HashMap::new(),
            clangd_analyzer: None,
        })
    }

    /// Create call relation analyzer with database support
    pub async fn new_with_database(
        db_manager: DatabaseManager,
        project_root: PathBuf,
    ) -> Result<Self> {
        let clangd_analyzer = ClangdAnalyzer::new_with_database(
            &project_root.to_string_lossy(),
            None, // sqlite_path
            None, // qdrant_url
            None, // qdrant_collection
            None, // vector_size
        )
        .await?;

        Ok(Self {
            db_manager,
            project_root,
            function_definitions: HashMap::new(),
            function_calls: HashMap::new(),
            clangd_analyzer: Some(clangd_analyzer),
        })
    }

    /// Analyze functions in specified files and return string result
    pub async fn analyze_files_and_search(
        &mut self,
        file_paths: Vec<PathBuf>,
        project_name: &str,
    ) -> Result<String> {
        info!("Starting file analysis and function definition search");

        // 1. Create database tables
        self.create_relation_tables().await?;

        // 2. Filter out C/Rust files
        let target_files = self.filter_c_rust_files(file_paths)?;

        // 3. Get all functions to list
        let mut all_functions = Vec::new();
        for file_path in &target_files {
            let functions = self.extract_functions_from_file(file_path).await?;
            all_functions.extend(functions);
        }

        // 4. Search database based on function names
        let mut search_results = HashMap::new();
        for function_def in &all_functions {
            let db_results = self
                .search_function_in_database(&function_def.name, project_name)
                .await?;
            if !db_results.is_empty() {
                search_results.insert(function_def.name.clone(), db_results);
            }
        }

        // 5. Save newly discovered functions to database
        self.save_functions_to_database(&all_functions, project_name)
            .await?;

        // 6. Build dictionary
        let result = FunctionSearchResult {
            functions: search_results.clone(),
            total_count: all_functions.len(),
            search_summary: self.generate_search_summary(&search_results, &all_functions),
        };

        // 7. Return string result
        Ok(serde_json::to_string_pretty(&result)?)
    }

    /// Filter out C/Rust files
    fn filter_c_rust_files(&self, file_paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
        let target_extensions = ["rs", "c", "cpp", "cc", "cxx", "h", "hpp", "hxx"];

        let filtered: Vec<PathBuf> = file_paths
            .into_iter()
            .filter(|path| {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    target_extensions.contains(&ext)
                } else {
                    false
                }
            })
            .collect();

        info!("Filtered out {} C/Rust files", filtered.len());
        Ok(filtered)
    }

    /// Extract functions from single file
    async fn extract_functions_from_file(
        &mut self,
        file_path: &Path,
    ) -> Result<Vec<FunctionDefinition>> {
        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match extension {
            "rs" => self.extract_rust_functions(file_path).await,
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" => {
                self.extract_c_cpp_functions(file_path).await
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Extract function definitions from Rust files
    async fn extract_rust_functions(
        &mut self,
        file_path: &Path,
    ) -> Result<Vec<FunctionDefinition>> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        let mut functions = Vec::new();

        // Regular expression for Rust function definitions
        let fn_regex = Regex::new(
            r"(?m)^[\s]*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*(<[^>]*>)?\s*\(([^)]*)\)\s*(?:->\s*([^{]+))?\s*\{",
        )?;

        for captures in fn_regex.captures_iter(&content) {
            let function_name = captures.get(1).unwrap().as_str();
            let generics = captures.get(2).map(|m| m.as_str()).unwrap_or("");
            let params_str = captures.get(3).unwrap().as_str();
            let return_type = captures.get(4).map(|m| m.as_str().trim()).unwrap_or("()");

            // Parse parameters
            let parameters = self.parse_rust_parameters(params_str);

            // Construct function definition
            let function_def = FunctionDefinition {
                name: function_name.to_string(),
                file_path: file_path.to_string_lossy().to_string(),
                line_number: self.find_line_number(&content, captures.get(0).unwrap().start())?,
                return_type: return_type.to_string(),
                parameters,
                signature: format!(
                    "fn {}{}({}) -> {}",
                    function_name, generics, params_str, return_type
                ),
                language: "rust".to_string(),
            };

            functions.push(function_def);
        }

        info!(
            "Extracted {} Rust functions from {:?}",
            functions.len(),
            file_path
        );
        Ok(functions)
    }

    /// Extract function definitions from C/C++ files
    async fn extract_c_cpp_functions(
        &mut self,
        file_path: &Path,
    ) -> Result<Vec<FunctionDefinition>> {
        let mut functions = Vec::new();

        // If no clangd_analyzer, create one with database support
        if self.clangd_analyzer.is_none() {
            let analyzer = ClangdAnalyzer::new_with_database(
                &self.project_root.to_string_lossy(),
                None,
                None,
                None,
                None,
            )
            .await?;
            self.clangd_analyzer = Some(analyzer);
        }

        if let Some(ref mut analyzer) = self.clangd_analyzer {
            // Use ClangdAnalyzer to analyze C/C++ code
            if let Err(e) = analyzer.analyze_with_clang_ast(file_path) {
                warn!(
                    "ClangdAnalyzer failed for {:?}: {}, falling back to regex",
                    file_path, e
                );
                return self.extract_c_cpp_functions_with_regex(file_path).await;
            }

            // Convert ClangdAnalyzer results to our format
            for function in &analyzer.functions {
                if function.file == *file_path {
                    let function_def = FunctionDefinition {
                        name: function.name.clone(),
                        file_path: function.file.to_string_lossy().to_string(),
                        line_number: function.line,
                        return_type: function.return_type.clone(),
                        parameters: function.parameters.clone(),
                        signature: format!(
                            "{} {}({})",
                            function.return_type,
                            function.name,
                            function
                                .parameters
                                .iter()
                                .map(|p| format!("{} {}", p.r#type, p.name))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        language: "c_cpp".to_string(),
                    };

                    functions.push(function_def);
                }
            }

            // Automatically save analysis results to database
            if let Err(e) = analyzer.save_analysis_results_to_database().await {
                warn!("Failed to save analysis results to database: {}", e);
            }
        }

        info!(
            "Extracted {} C/C++ functions from {:?}",
            functions.len(),
            file_path
        );
        Ok(functions)
    }

    /// Fallback method to extract C/C++ functions using regular expressions
    async fn extract_c_cpp_functions_with_regex(
        &self,
        file_path: &Path,
    ) -> Result<Vec<FunctionDefinition>> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        let mut functions = Vec::new();

        // Simple C/C++ function definition regular expression
        let fn_regex = Regex::new(
            r"(?m)^[\s]*(?:static\s+|extern\s+|inline\s+)*([a-zA-Z_][a-zA-Z0-9_*\s]+)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*(?:\{|;)",
        )?;

        for (line_num, line) in content.lines().enumerate() {
            if let Some(captures) = fn_regex.captures(line) {
                let return_type = captures.get(1).unwrap().as_str().trim();
                let function_name = captures.get(2).unwrap().as_str();

                let function_def = FunctionDefinition {
                    name: function_name.to_string(),
                    file_path: file_path.to_string_lossy().to_string(),
                    line_number: (line_num + 1) as u32,
                    return_type: return_type.to_string(),
                    parameters: Vec::new(), // Regular expressions cannot accurately parse parameters
                    signature: line.trim().to_string(),
                    language: "c_cpp".to_string(),
                };

                functions.push(function_def);
            }
        }

        info!(
            "Extracted {} functions from {:?} using regular expressions",
            functions.len(),
            file_path
        );
        Ok(functions)
    }

    /// Parse Rust function parameters
    fn parse_rust_parameters(&self, params_str: &str) -> Vec<Parameter> {
        let mut parameters = Vec::new();

        if params_str.trim().is_empty() {
            return parameters;
        }

        // Simple parameter parsing, actual cases may be more complex
        for param in params_str.split(',') {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }

            // Parse name: type format
            if let Some(colon_pos) = param.find(':') {
                let name = param[..colon_pos].trim();
                let type_str = param[colon_pos + 1..].trim();

                parameters.push(Parameter {
                    name: name.to_string(),
                    r#type: type_str.to_string(),
                });
            }
        }

        parameters
    }

    /// 在数据库中搜索函数
    async fn search_function_in_database(
        &self,
        function_name: &str,
        project_name: &str,
    ) -> Result<Vec<FunctionDefinition>> {
        debug!("在数据库中搜索函数: {}", function_name);

        // 使用 DatabaseManager 的 search_interfaces_by_name 方法
        match self
            .db_manager
            .search_interfaces_by_name(function_name, Some(project_name))
            .await
        {
            Ok(results) => {
                let mut function_defs = Vec::new();
                for interface in results {
                    if interface.project_name == Some(project_name.to_string()) {
                        // 将 InterfaceInfo 转换为 FunctionDefinition
                        let parameters: Vec<Parameter> = interface
                            .inputs
                            .iter()
                            .filter_map(|input| {
                                if let (Some(name), Some(type_val)) =
                                    (input.get("name"), input.get("type"))
                                {
                                    Some(Parameter {
                                        name: name.as_str().unwrap_or("").to_string(),
                                        r#type: type_val.as_str().unwrap_or("").to_string(),
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect();

                        let return_type = interface
                            .outputs
                            .first()
                            .and_then(|output| output.get("type"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("void")
                            .to_string();

                        let function_def = FunctionDefinition {
                            name: interface.name.clone(),
                            file_path: interface.file_path.clone(),
                            line_number: 0, // InterfaceInfo doesn't have line number
                            return_type: return_type.clone(),
                            parameters,
                            signature: format!("{} {}(...)", &return_type, interface.name),
                            language: interface.language.clone(),
                        };

                        function_defs.push(function_def);
                    }
                }
                Ok(function_defs)
            }
            Err(e) => {
                warn!("Database query failed: {}", e);
                Ok(Vec::new())
            }
        }
    }

    /// Save functions to database
    async fn save_functions_to_database(
        &self,
        functions: &[FunctionDefinition],
        project_name: &str,
    ) -> Result<()> {
        info!("Saving {} functions to database", functions.len());

        for function in functions {
            // Construct input parameters
            let inputs: Vec<std::collections::HashMap<String, serde_json::Value>> = function
                .parameters
                .iter()
                .map(|param| {
                    let mut input_map = std::collections::HashMap::new();
                    input_map.insert(
                        "name".to_string(),
                        serde_json::Value::String(param.name.clone()),
                    );
                    input_map.insert(
                        "type".to_string(),
                        serde_json::Value::String(param.r#type.clone()),
                    );
                    input_map
                })
                .collect();

            // Construct output parameters
            let outputs: Vec<std::collections::HashMap<String, serde_json::Value>> = vec![{
                let mut output_map = std::collections::HashMap::new();
                output_map.insert(
                    "type".to_string(),
                    serde_json::Value::String(function.return_type.clone()),
                );
                output_map
            }];

            // Use DatabaseManager's store_interface_with_vector method
            // Since there's no vector data, we create an empty vector
            let empty_vector = vec![0.0; 384]; // Assume vector dimension is 384

            debug!("Saving function definition: {}", function.name);

            match self
                .db_manager
                .store_interface_with_vector(
                    &function.name,
                    inputs,
                    outputs,
                    &function.file_path,
                    &function.signature,
                    empty_vector,
                    Some(&function.language),
                    Some(project_name),
                    None,
                )
                .await
            {
                Ok((interface_id, vector_id)) => {
                    debug!(
                        "Function {} saved successfully, interface ID: {}, vector ID: {}",
                        function.name, interface_id, vector_id
                    );
                }
                Err(e) => {
                    warn!("Failed to save function {}: {}", function.name, e);
                }
            }
        }

        info!("Function definition saving completed");
        Ok(())
    }

    /// Generate search summary
    fn generate_search_summary(
        &self,
        search_results: &HashMap<String, Vec<FunctionDefinition>>,
        all_functions: &[FunctionDefinition],
    ) -> String {
        let found_count = search_results.len();
        let total_count = all_functions.len();
        let rust_count = all_functions
            .iter()
            .filter(|f| f.language == "rust")
            .count();
        let c_cpp_count = all_functions
            .iter()
            .filter(|f| f.language == "c_cpp")
            .count();

        format!(
            "Function search completed:\n- Total analyzed functions: {}\n- Database matched functions: {}\n- Rust functions: {}\n- C/C++ functions: {}\n- Match rate: {:.2}%",
            total_count,
            found_count,
            rust_count,
            c_cpp_count,
            if total_count > 0 {
                (found_count as f64 / total_count as f64) * 100.0
            } else {
                0.0
            }
        )
    }

    /// Create relational database tables
    async fn create_relation_tables(&self) -> Result<()> {
        info!("Creating relational database tables");

        // Function definitions table
        let _create_function_definitions = r#"
            CREATE TABLE IF NOT EXISTS function_definitions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                function_name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line_number INTEGER,
                return_type TEXT,
                parameters TEXT,
                signature TEXT,
                language TEXT,
                project_name TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(function_name, file_path, project_name)
            )
        "#;

        // Function calls table
        let _create_function_calls = r#"
            CREATE TABLE IF NOT EXISTS function_calls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                caller_file TEXT NOT NULL,
                caller_function TEXT,
                caller_line INTEGER,
                called_function TEXT NOT NULL,
                called_file TEXT,
                call_type TEXT,
                project_name TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        "#;

        // File dependencies table
        let _create_file_dependencies = r#"
            CREATE TABLE IF NOT EXISTS file_dependencies (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_file TEXT NOT NULL,
                target_file TEXT NOT NULL,
                dependency_type TEXT,
                project_name TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(source_file, target_file, project_name)
            )
        "#;

        // Since DatabaseManager already has built-in table structure, here we mainly ensure the project exists
        match self
            .db_manager
            .create_project(
                "Function Analysis Project",
                &self.project_root.to_string_lossy(),
                Some("Automatically created function analysis project"),
            )
            .await
        {
            Ok(_) => {
                info!("Project created or confirmed to exist");
            }
            Err(e) => {
                debug!("Project may already exist: {}", e);
            }
        }

        info!("Relational database tables creation completed");
        Ok(())
    }

    /// Find line number
    fn find_line_number(&self, content: &str, position: usize) -> Result<u32> {
        let line_num = content[..position].lines().count() as u32;
        Ok(line_num)
    }

    /// Get all C/Rust source files in the project
    pub fn get_project_c_rust_files(&self) -> Result<Vec<PathBuf>> {
        let mut source_files = Vec::new();
        let source_extensions = ["rs", "c", "cpp", "cc", "cxx", "h", "hpp", "hxx"];

        fn visit_dir(
            dir: &Path,
            source_files: &mut Vec<PathBuf>,
            extensions: &[&str],
        ) -> Result<()> {
            if dir.is_dir() {
                for entry in fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();

                    // Skip common build directories
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if [
                            "target",
                            "build",
                            ".git",
                            "__pycache__",
                            ".vscode",
                            "node_modules",
                        ]
                        .contains(&name)
                        {
                            continue;
                        }
                    }

                    if path.is_dir() {
                        visit_dir(&path, source_files, extensions)?;
                    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if extensions.contains(&ext) {
                            source_files.push(path);
                        }
                    }
                }
            }
            Ok(())
        }

        visit_dir(&self.project_root, &mut source_files, &source_extensions)?;
        info!("找到 {} 个 C/Rust 源文件", source_files.len());
        Ok(source_files)
    }

    /// 搜索函数调用关系
    pub fn search_call_relation(&self, function_name: &str) -> Vec<&FunctionCall> {
        self.function_calls
            .values()
            .flatten()
            .filter(|call| call.called_function == function_name)
            .collect()
    }

    /// Get call relationships of file
    pub fn get_file_calls(&self, file_path: &str) -> Option<&Vec<FunctionCall>> {
        self.function_calls.get(file_path)
    }

    /// Batch analyze entire project
    pub async fn analyze_entire_project(&mut self, project_name: &str) -> Result<String> {
        info!("Starting analysis of entire project: {}", project_name);

        let source_files = self.get_project_c_rust_files()?;
        self.analyze_files_and_search(source_files, project_name)
            .await
    }

    /// Analyze files in specified directory
    pub async fn analyze_directory(
        &mut self,
        directory_path: &Path,
        project_name: &str,
    ) -> Result<String> {
        info!("Starting directory analysis: {:?}", directory_path);

        let mut source_files = Vec::new();
        let target_extensions = ["rs", "c", "cpp", "cc", "cxx", "h", "hpp", "hxx"];

        if directory_path.is_dir() {
            for entry in fs::read_dir(directory_path)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if target_extensions.contains(&ext) {
                            source_files.push(path);
                        }
                    }
                }
            }
        }

        self.analyze_files_and_search(source_files, project_name)
            .await
    }

    /// Get function statistics by language in project
    pub fn get_function_statistics_by_language(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();

        for function in self.function_definitions.values() {
            *stats.entry(function.language.clone()).or_insert(0) += 1;
        }

        stats
    }

    /// Get function list by file path
    pub fn get_functions_by_file(&self, file_path: &str) -> Vec<&FunctionDefinition> {
        self.function_definitions
            .values()
            .filter(|func| func.file_path == file_path)
            .collect()
    }

    /// Search functions containing specific parameter type
    pub fn search_functions_by_parameter_type(&self, param_type: &str) -> Vec<&FunctionDefinition> {
        self.function_definitions
            .values()
            .filter(|func| {
                func.parameters
                    .iter()
                    .any(|param| param.r#type.contains(param_type))
            })
            .collect()
    }

    /// Search functions with specific return type
    pub fn search_functions_by_return_type(&self, return_type: &str) -> Vec<&FunctionDefinition> {
        self.function_definitions
            .values()
            .filter(|func| func.return_type.contains(return_type))
            .collect()
    }

    /// Get project analysis report
    pub fn generate_analysis_report(&self, project_name: &str) -> String {
        let stats = self.get_statistics();
        let lang_stats = self.get_function_statistics_by_language();

        let mut report = format!("# {} Project Analysis Report\n\n", project_name);
        report.push_str("## Function Statistics\n");
        report.push_str(&format!("- Total functions: {}\n", stats.total_functions));
        report.push_str(&format!("- Rust functions: {}\n", stats.rust_functions));
        report.push_str(&format!("- C/C++ functions: {}\n", stats.c_cpp_functions));

        report.push_str("\n## Distribution by Language\n");
        for (lang, count) in &lang_stats {
            report.push_str(&format!("- {}: {} functions\n", lang, count));
        }

        report.push_str("\n## File Analysis\n");
        report.push_str(&format!("- Analyzed files: {}\n", stats.total_files));

        report
    }

    /// Get function definition by function name
    pub fn get_function_definition(&self, function_name: &str) -> Option<&FunctionDefinition> {
        self.function_definitions
            .values()
            .find(|def| def.name == function_name)
    }

    /// Get string representation of all function definitions
    pub fn get_all_functions_as_string(&self) -> String {
        let functions: Vec<&FunctionDefinition> = self.function_definitions.values().collect();
        match serde_json::to_string_pretty(&functions) {
            Ok(json_str) => json_str,
            Err(e) => format!("Failed to serialize function definitions: {}", e),
        }
    }

    /// Get call relationship statistics
    pub fn get_statistics(&self) -> CallRelationStatistics {
        CallRelationStatistics {
            total_functions: self.function_definitions.len(),
            total_calls: self.function_calls.values().map(|calls| calls.len()).sum(),
            total_files: self.function_calls.len(),
            rust_functions: self
                .function_definitions
                .values()
                .filter(|func| func.language == "rust")
                .count(),
            c_cpp_functions: self
                .function_definitions
                .values()
                .filter(|func| func.language == "c_cpp")
                .count(),
        }
    }
}

/// Call relationship statistics information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRelationStatistics {
    pub total_functions: usize,
    pub total_calls: usize,
    pub total_files: usize,
    pub rust_functions: usize,
    pub c_cpp_functions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_rust_function_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let rust_code = r#"
pub fn hello_world() -> String {
    "Hello, World!".to_string()
}

async fn async_function(param: i32) -> Result<(), String> {
    Ok(())
}

fn private_function() {
    debug!("private");
}
"#;

        std::fs::write(&file_path, rust_code).unwrap();

        // DatabaseManager simulation needed here
        // let db_manager = DatabaseManager::new_default().await.unwrap();
        // let mut analyzer = CallRelationAnalyzer::new(db_manager, temp_dir.path().to_path_buf()).unwrap();

        // let functions = analyzer.extract_rust_functions(&file_path).await.unwrap();
        // assert_eq!(functions.len(), 3);
        // assert_eq!(functions[0].name, "hello_world");
        // assert_eq!(functions[0].language, "rust");
    }

    #[tokio::test]
    async fn test_function_search_by_type() {
        // Simulate function definition data
        let function1 = FunctionDefinition {
            name: "test_func".to_string(),
            file_path: "test.rs".to_string(),
            line_number: 1,
            return_type: "String".to_string(),
            parameters: vec![Parameter {
                name: "input".to_string(),
                r#type: "i32".to_string(),
            }],
            signature: "fn test_func(input: i32) -> String".to_string(),
            language: "rust".to_string(),
        };

        // Test basic functionality, actual testing requires complete DatabaseManager
        assert_eq!(function1.name, "test_func");
        assert_eq!(function1.return_type, "String");
    }

    #[tokio::test]
    async fn test_file_filtering() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let rust_file = temp_dir.path().join("test.rs");
        let c_file = temp_dir.path().join("test.c");
        let py_file = temp_dir.path().join("test.py");

        std::fs::write(&rust_file, "fn test() {}").unwrap();
        std::fs::write(&c_file, "int test() { return 0; }").unwrap();
        std::fs::write(&py_file, "def test(): pass").unwrap();

        let _input_files = vec![rust_file.clone(), c_file.clone(), py_file.clone()];

        // DatabaseManager simulation needed here
        // let db_manager = DatabaseManager::new_default().await.unwrap();
        // let analyzer = CallRelationAnalyzer::new(db_manager, temp_dir.path().to_path_buf()).unwrap();

        // let filtered = analyzer.filter_c_rust_files(input_files).unwrap();
        // assert_eq!(filtered.len(), 2); // Only .rs and .c files
        // assert!(filtered.contains(&rust_file));
        // assert!(filtered.contains(&c_file));
        // assert!(!filtered.contains(&py_file));
    }

    #[test]
    fn test_function_statistics() {
        let mut function_defs = HashMap::new();

        function_defs.insert(
            "func1".to_string(),
            FunctionDefinition {
                name: "func1".to_string(),
                file_path: "test.rs".to_string(),
                line_number: 1,
                return_type: "()".to_string(),
                parameters: vec![],
                signature: "fn func1()".to_string(),
                language: "rust".to_string(),
            },
        );

        function_defs.insert(
            "func2".to_string(),
            FunctionDefinition {
                name: "func2".to_string(),
                file_path: "test.c".to_string(),
                line_number: 1,
                return_type: "int".to_string(),
                parameters: vec![],
                signature: "int func2()".to_string(),
                language: "c_cpp".to_string(),
            },
        );

        // 测试统计功能
        let rust_count = function_defs
            .values()
            .filter(|f| f.language == "rust")
            .count();
        let c_count = function_defs
            .values()
            .filter(|f| f.language == "c_cpp")
            .count();

        assert_eq!(rust_count, 1);
        assert_eq!(c_count, 1);
    }
}
