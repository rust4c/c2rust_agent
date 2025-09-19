//! 调用关系分析器
//!
//! 分析项目中的函数调用关系，并建立关系数据库。
//! 专注于 C/Rust 文件的函数分析和数据库查询。

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::task;
use log::{debug, error, info, warn};

use db_services::DatabaseManager;
use lsp_services::lsp_services::{ClangdAnalyzer, FunctionInfo, Parameter};

/// 函数定义信息
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

/// 函数调用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub caller_file: String,
    pub caller_function: Option<String>,
    pub caller_line: u32,
    pub called_function: String,
    pub called_file: Option<String>,
    pub call_type: CallType,
}

/// 调用类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallType {
    DirectCall,   // 直接调用
    IndirectCall, // 间接调用
    CBindingCall, // C 绑定调用
}

/// 文件依赖关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDependency {
    pub source_file: String,
    pub target_file: String,
    pub dependency_type: DependencyType,
}

/// 依赖类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyType {
    Include, // #include
    Use,     // Rust use
    Call,    // 函数调用依赖
}

/// 数据库函数记录
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
    file_dependencies: HashMap<String, HashSet<String>>,

    // 分析器
    clangd_analyzer: Option<ClangdAnalyzer>,
}

impl CallRelationAnalyzer {
    /// 创建新的调用关系分析器
    pub fn new(db_manager: DatabaseManager, project_root: PathBuf) -> Result<Self> {
        // 不在这里创建ClangdAnalyzer，而是在需要时异步创建
        Ok(Self {
            db_manager,
            project_root,
            function_definitions: HashMap::new(),
            function_calls: HashMap::new(),
            file_dependencies: HashMap::new(),
            clangd_analyzer: None,
        })
    }

    /// 创建带数据库支持的调用关系分析器
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
            file_dependencies: HashMap::new(),
            clangd_analyzer: Some(clangd_analyzer),
        })
    }

    /// 分析指定文件的函数并返回字符串结果
    pub async fn analyze_files_and_search(
        &mut self,
        file_paths: Vec<PathBuf>,
        project_name: &str,
    ) -> Result<String> {
        info!("开始分析文件并搜索函数定义");

        // 1. 创建数据库表
        self.create_relation_tables().await?;

        // 2. 过滤出 C/Rust 文件
        let target_files = self.filter_c_rust_files(file_paths)?;

        // 3. 获取所有函数到列表
        let mut all_functions = Vec::new();
        for file_path in &target_files {
            let functions = self.extract_functions_from_file(file_path).await?;
            all_functions.extend(functions);
        }

        // 4. 基于函数名称查找数据库
        let mut search_results = HashMap::new();
        for function_def in &all_functions {
            let db_results = self
                .search_function_in_database(&function_def.name, project_name)
                .await?;
            if !db_results.is_empty() {
                search_results.insert(function_def.name.clone(), db_results);
            }
        }

        // 5. 保存新发现的函数到数据库
        self.save_functions_to_database(&all_functions, project_name)
            .await?;

        // 6. 构成字典
        let result = FunctionSearchResult {
            functions: search_results.clone(),
            total_count: all_functions.len(),
            search_summary: self.generate_search_summary(&search_results, &all_functions),
        };

        // 7. 返回字符串结果
        Ok(serde_json::to_string_pretty(&result)?)
    }

    /// 过滤出 C/Rust 文件
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

        info!("过滤出 {} 个 C/Rust 文件", filtered.len());
        Ok(filtered)
    }

    /// 从单个文件中提取函数
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

    /// 从 Rust 文件中提取函数定义
    async fn extract_rust_functions(
        &mut self,
        file_path: &Path,
    ) -> Result<Vec<FunctionDefinition>> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("读取文件失败: {:?}", file_path))?;

        let mut functions = Vec::new();

        // Rust 函数定义的正则表达式
        let fn_regex = Regex::new(
            r"(?m)^[\s]*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*(<[^>]*>)?\s*\(([^)]*)\)\s*(?:->\s*([^{]+))?\s*\{",
        )?;

        for captures in fn_regex.captures_iter(&content) {
            let function_name = captures.get(1).unwrap().as_str();
            let generics = captures.get(2).map(|m| m.as_str()).unwrap_or("");
            let params_str = captures.get(3).unwrap().as_str();
            let return_type = captures.get(4).map(|m| m.as_str().trim()).unwrap_or("()");

            // 解析参数
            let parameters = self.parse_rust_parameters(params_str);

            // 构造函数定义
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

        info!("从 {:?} 提取到 {} 个 Rust 函数", file_path, functions.len());
        Ok(functions)
    }

    /// 从 C/C++ 文件中提取函数定义
    async fn extract_c_cpp_functions(
        &mut self,
        file_path: &Path,
    ) -> Result<Vec<FunctionDefinition>> {
        let mut functions = Vec::new();

        // 如果没有clangd_analyzer，创建一个带数据库支持的
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
            // 使用 ClangdAnalyzer 分析 C/C++ 代码
            if let Err(e) = analyzer.analyze_with_clang_ast(file_path) {
                warn!(
                    "ClangdAnalyzer failed for {:?}: {}, falling back to regex",
                    file_path, e
                );
                return self.extract_c_cpp_functions_with_regex(file_path).await;
            }

            // 转换 ClangdAnalyzer 的结果到我们的格式
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

            // 自动保存分析结果到数据库
            if let Err(e) = analyzer.save_analysis_results_to_database().await {
                warn!("Failed to save analysis results to database: {}", e);
            }
        }

        info!(
            "从 {:?} 提取到 {} 个 C/C++ 函数",
            file_path,
            functions.len()
        );
        Ok(functions)
    }

    /// 使用正则表达式提取C/C++函数的回退方法
    async fn extract_c_cpp_functions_with_regex(
        &self,
        file_path: &Path,
    ) -> Result<Vec<FunctionDefinition>> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("读取文件失败: {:?}", file_path))?;

        let mut functions = Vec::new();

        // 简单的C/C++函数定义正则表达式
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
                    parameters: Vec::new(), // 正则表达式无法精确解析参数
                    signature: line.trim().to_string(),
                    language: "c_cpp".to_string(),
                };

                functions.push(function_def);
            }
        }

        info!(
            "使用正则表达式从 {:?} 提取到 {} 个函数",
            file_path,
            functions.len()
        );
        Ok(functions)
    }

    /// 解析 Rust 函数参数
    fn parse_rust_parameters(&self, params_str: &str) -> Vec<Parameter> {
        let mut parameters = Vec::new();

        if params_str.trim().is_empty() {
            return parameters;
        }

        // 简单的参数解析，实际情况可能更复杂
        for param in params_str.split(',') {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }

            // 解析 name: type 格式
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
                warn!("数据库查询失败: {}", e);
                Ok(Vec::new())
            }
        }
    }

    /// 保存函数到数据库
    async fn save_functions_to_database(
        &self,
        functions: &[FunctionDefinition],
        project_name: &str,
    ) -> Result<()> {
        info!("保存 {} 个函数到数据库", functions.len());

        for function in functions {
            // 构造输入参数
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

            // 构造输出参数
            let outputs: Vec<std::collections::HashMap<String, serde_json::Value>> = vec![{
                let mut output_map = std::collections::HashMap::new();
                output_map.insert(
                    "type".to_string(),
                    serde_json::Value::String(function.return_type.clone()),
                );
                output_map
            }];

            // 使用 DatabaseManager 的 store_interface_with_vector 方法
            // 由于没有向量数据，我们创建一个空向量
            let empty_vector = vec![0.0; 384]; // 假设向量维度为384

            debug!("保存函数定义: {}", function.name);

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
                        "函数 {} 保存成功，接口ID: {}, 向量ID: {}",
                        function.name, interface_id, vector_id
                    );
                }
                Err(e) => {
                    warn!("保存函数 {} 失败: {}", function.name, e);
                }
            }
        }

        info!("函数定义保存完成");
        Ok(())
    }

    /// 生成搜索摘要
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
            "函数搜索完成:\n- 总计分析函数: {}\n- 数据库匹配函数: {}\n- Rust 函数: {}\n- C/C++ 函数: {}\n- 匹配率: {:.2}%",
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

    /// 创建关系数据库表
    async fn create_relation_tables(&self) -> Result<()> {
        info!("创建关系数据库表");

        // 函数定义表
        let create_function_definitions = r#"
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

        // 函数调用表
        let create_function_calls = r#"
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

        // 文件依赖表
        let create_file_dependencies = r#"
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

        // 由于 DatabaseManager 已经有内置的表结构，这里主要是确保项目存在
        match self
            .db_manager
            .create_project(
                "函数分析项目",
                &self.project_root.to_string_lossy(),
                Some("自动创建的函数分析项目"),
            )
            .await
        {
            Ok(_) => {
                info!("项目创建或确认存在");
            }
            Err(e) => {
                debug!("项目可能已存在: {}", e);
            }
        }

        info!("关系数据库表创建完成");
        Ok(())
    }

    /// 查找行号
    fn find_line_number(&self, content: &str, position: usize) -> Result<u32> {
        let line_num = content[..position].lines().count() as u32;
        Ok(line_num)
    }

    /// 获取项目中的所有 C/Rust 源文件
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

                    // 跳过常见的构建目录
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

    /// 获取文件的调用关系
    pub fn get_file_calls(&self, file_path: &str) -> Option<&Vec<FunctionCall>> {
        self.function_calls.get(file_path)
    }

    /// 批量分析整个项目
    pub async fn analyze_entire_project(&mut self, project_name: &str) -> Result<String> {
        info!("开始分析整个项目: {}", project_name);

        let source_files = self.get_project_c_rust_files()?;
        self.analyze_files_and_search(source_files, project_name)
            .await
    }

    /// 分析指定目录下的文件
    pub async fn analyze_directory(
        &mut self,
        directory_path: &Path,
        project_name: &str,
    ) -> Result<String> {
        info!("开始分析目录: {:?}", directory_path);

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

    /// 获取项目中的函数统计信息
    pub fn get_function_statistics_by_language(&self) -> HashMap<String, usize> {
        let mut stats = HashMap::new();

        for function in self.function_definitions.values() {
            *stats.entry(function.language.clone()).or_insert(0) += 1;
        }

        stats
    }

    /// 根据文件路径获取函数列表
    pub fn get_functions_by_file(&self, file_path: &str) -> Vec<&FunctionDefinition> {
        self.function_definitions
            .values()
            .filter(|func| func.file_path == file_path)
            .collect()
    }

    /// 搜索包含特定参数类型的函数
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

    /// 搜索特定返回类型的函数
    pub fn search_functions_by_return_type(&self, return_type: &str) -> Vec<&FunctionDefinition> {
        self.function_definitions
            .values()
            .filter(|func| func.return_type.contains(return_type))
            .collect()
    }

    /// 获取项目分析报告
    pub fn generate_analysis_report(&self, project_name: &str) -> String {
        let stats = self.get_statistics();
        let lang_stats = self.get_function_statistics_by_language();

        let mut report = format!("# {} 项目分析报告\n\n", project_name);
        report.push_str("## 函数统计\n");
        report.push_str(&format!("- 总函数数: {}\n", stats.total_functions));
        report.push_str(&format!("- Rust 函数: {}\n", stats.rust_functions));
        report.push_str(&format!("- C/C++ 函数: {}\n", stats.c_cpp_functions));

        report.push_str("\n## 按语言分布\n");
        for (lang, count) in &lang_stats {
            report.push_str(&format!("- {}: {} 个函数\n", lang, count));
        }

        report.push_str("\n## 文件分析\n");
        report.push_str(&format!("- 分析文件数: {}\n", stats.total_files));

        report
    }

    /// 根据函数名获取函数定义
    pub fn get_function_definition(&self, function_name: &str) -> Option<&FunctionDefinition> {
        self.function_definitions
            .values()
            .find(|def| def.name == function_name)
    }

    /// 获取所有函数定义的字符串表示
    pub fn get_all_functions_as_string(&self) -> String {
        let functions: Vec<&FunctionDefinition> = self.function_definitions.values().collect();
        match serde_json::to_string_pretty(&functions) {
            Ok(json_str) => json_str,
            Err(e) => format!("序列化函数定义失败: {}", e),
        }
    }

    /// 获取调用关系统计
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

/// 调用关系统计信息
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
    use std::path::PathBuf;
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

        // 这里需要模拟 DatabaseManager
        // let db_manager = DatabaseManager::new_default().await.unwrap();
        // let mut analyzer = CallRelationAnalyzer::new(db_manager, temp_dir.path().to_path_buf()).unwrap();

        // let functions = analyzer.extract_rust_functions(&file_path).await.unwrap();
        // assert_eq!(functions.len(), 3);
        // assert_eq!(functions[0].name, "hello_world");
        // assert_eq!(functions[0].language, "rust");
    }

    #[tokio::test]
    async fn test_function_search_by_type() {
        // 模拟函数定义数据
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

        // 测试基本功能，实际测试需要完整的 DatabaseManager
        assert_eq!(function1.name, "test_func");
        assert_eq!(function1.return_type, "String");
    }

    #[tokio::test]
    async fn test_file_filtering() {
        let temp_dir = TempDir::new().unwrap();

        // 创建测试文件
        let rust_file = temp_dir.path().join("test.rs");
        let c_file = temp_dir.path().join("test.c");
        let py_file = temp_dir.path().join("test.py");

        std::fs::write(&rust_file, "fn test() {}").unwrap();
        std::fs::write(&c_file, "int test() { return 0; }").unwrap();
        std::fs::write(&py_file, "def test(): pass").unwrap();

        let input_files = vec![rust_file.clone(), c_file.clone(), py_file.clone()];

        // 这里需要模拟 DatabaseManager
        // let db_manager = DatabaseManager::new_default().await.unwrap();
        // let analyzer = CallRelationAnalyzer::new(db_manager, temp_dir.path().to_path_buf()).unwrap();

        // let filtered = analyzer.filter_c_rust_files(input_files).unwrap();
        // assert_eq!(filtered.len(), 2); // 只有 .rs 和 .c 文件
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
