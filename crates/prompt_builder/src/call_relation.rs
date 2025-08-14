//! 调用关系分析器
//!
//! 分析项目中的函数调用关系，并建立关系数据库。
//! 支持 Python 到 Rust 的调用关系分析。

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::task;
use tracing::{debug, error, info, warn};

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
    pub language: String, // "python", "rust", "c", "cpp"
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
    FfiCall,      // FFI 调用 (Python -> Rust)
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
    Import,  // Python import
    Use,     // Rust use
    Call,    // 函数调用依赖
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
        let clangd_analyzer = ClangdAnalyzer::new(&project_root.to_string_lossy());

        Ok(Self {
            db_manager,
            project_root,
            function_definitions: HashMap::new(),
            function_calls: HashMap::new(),
            file_dependencies: HashMap::new(),
            clangd_analyzer: Some(clangd_analyzer),
        })
    }

    /// 分析整个项目的调用关系
    pub async fn analyze_project_relations(&mut self, project_name: &str) -> Result<()> {
        info!("开始分析项目 {} 的调用关系", project_name);

        // 1. 创建关系表
        self.create_relation_tables().await?;

        // 2. 获取所有源文件
        let source_files = self.get_project_source_files()?;

        // 3. 分析 Rust 文件的函数定义
        self.analyze_rust_functions(&source_files).await?;

        // 4. 分析 Python 文件的函数调用
        self.analyze_python_calls(&source_files).await?;

        // 5. 分析 C/C++ 文件（如果存在）
        self.analyze_c_cpp_functions(&source_files).await?;

        // 6. 分析文件依赖关系
        self.analyze_file_dependencies(&source_files).await?;

        // 7. 分析 Python -> Rust FFI 调用
        self.analyze_python_rust_ffi(&source_files).await?;

        // 8. 保存关系到数据库
        self.save_relations_to_db(project_name).await?;

        info!("项目 {} 调用关系分析完成", project_name);
        Ok(())
    }

    /// 创建关系数据库表
    async fn create_relation_tables(&self) -> Result<()> {
        info!("创建关系数据库表");

        // 这里我们假设数据库管理器提供了执行 SQL 的方法
        // 由于 DatabaseManager 结构复杂，我们先记录需要的表结构
        // 实际实现中需要根据 DatabaseManager 的具体接口来调整

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

        // 调用关系图表
        let create_call_relationships = r#"
            CREATE TABLE IF NOT EXISTS call_relationships (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                definition_id INTEGER,
                call_id INTEGER,
                relationship_type TEXT,
                project_name TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (definition_id) REFERENCES function_definitions (id),
                FOREIGN KEY (call_id) REFERENCES function_calls (id)
            )
        "#;

        // 注意：这里需要根据实际的 DatabaseManager 接口来实现 SQL 执行
        // 目前只是示例结构

        info!("关系数据库表创建完成");
        Ok(())
    }

    /// 获取项目中的所有源文件
    fn get_project_source_files(&self) -> Result<Vec<PathBuf>> {
        let mut source_files = Vec::new();
        let source_extensions = [
            ".rs", ".py", ".c", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".hxx",
        ];

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
                        let ext_with_dot = format!(".{}", ext);
                        if extensions.contains(&ext_with_dot.as_str()) {
                            source_files.push(path);
                        }
                    }
                }
            }
            Ok(())
        }

        visit_dir(&self.project_root, &mut source_files, &source_extensions)?;
        info!("找到 {} 个源文件", source_files.len());
        Ok(source_files)
    }

    /// 分析 Rust 文件的函数定义
    async fn analyze_rust_functions(&mut self, source_files: &[PathBuf]) -> Result<()> {
        info!("分析 Rust 函数定义");

        for file_path in source_files {
            if file_path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Err(e) = self.extract_rust_functions(file_path).await {
                    warn!("分析 Rust 文件 {:?} 失败: {}", file_path, e);
                }
            }
        }

        Ok(())
    }

    /// 从 Rust 文件中提取函数定义
    async fn extract_rust_functions(&mut self, file_path: &Path) -> Result<()> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("读取文件失败: {:?}", file_path))?;

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
                    "fn {}{}{} -> {}",
                    function_name, generics, params_str, return_type
                ),
                language: "rust".to_string(),
            };

            let key = format!("{}@{}", function_name, file_path.display());
            self.function_definitions.insert(key, function_def);
        }

        Ok(())
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

    /// 分析 Python 文件的函数调用
    async fn analyze_python_calls(&mut self, source_files: &[PathBuf]) -> Result<()> {
        info!("分析 Python 函数调用");

        for file_path in source_files {
            if file_path.extension().and_then(|e| e.to_str()) == Some("py") {
                if let Err(e) = self.extract_python_calls(file_path).await {
                    warn!("分析 Python 文件 {:?} 失败: {}", file_path, e);
                }
            }
        }

        Ok(())
    }

    /// 从 Python 文件中提取函数调用
    async fn extract_python_calls(&mut self, file_path: &Path) -> Result<()> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("读取文件失败: {:?}", file_path))?;

        let file_path_str = file_path.to_string_lossy().to_string();
        let mut calls = Vec::new();

        // Python 函数调用的正则表达式
        let call_regex = Regex::new(r"(\w+)\s*\(")?;

        for (line_num, line) in content.lines().enumerate() {
            for captures in call_regex.captures_iter(line) {
                let function_name = captures.get(1).unwrap().as_str();

                // 跳过 Python 关键字和内置函数
                if [
                    "if", "for", "while", "def", "class", "import", "print", "len", "str", "int",
                    "float",
                ]
                .contains(&function_name)
                {
                    continue;
                }

                let call = FunctionCall {
                    caller_file: file_path_str.clone(),
                    caller_function: None, // TODO: 检测当前所在的函数
                    caller_line: (line_num + 1) as u32,
                    called_function: function_name.to_string(),
                    called_file: None,
                    call_type: CallType::DirectCall,
                };

                calls.push(call);
            }
        }

        self.function_calls.insert(file_path_str, calls);
        Ok(())
    }

    /// 分析 C/C++ 函数
    async fn analyze_c_cpp_functions(&mut self, source_files: &[PathBuf]) -> Result<()> {
        info!("分析 C/C++ 函数");

        if let Some(ref mut analyzer) = self.clangd_analyzer {
            // 使用 ClangdAnalyzer 分析 C/C++ 代码
            let c_cpp_files: Vec<PathBuf> = source_files
                .iter()
                .filter(|f| {
                    if let Some(ext) = f.extension().and_then(|e| e.to_str()) {
                        ["c", "cpp", "cc", "cxx", "h", "hpp", "hxx"].contains(&ext)
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();

            for file_path in c_cpp_files {
                analyzer.analyze_with_clang_ast(&file_path);
            }

            // 转换 ClangdAnalyzer 的结果到我们的格式
            for function in &analyzer.functions {
                let function_def = FunctionDefinition {
                    name: function.name.clone(),
                    file_path: function.file.to_str().unwrap_or_default().to_string(),
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

                let key = format!("{}@{:?}", function.name, function.file);
                self.function_definitions.insert(key, function_def);
            }
        }

        Ok(())
    }

    /// 分析文件依赖关系
    async fn analyze_file_dependencies(&mut self, source_files: &[PathBuf]) -> Result<()> {
        info!("分析文件依赖关系");

        for file_path in source_files {
            if let Err(e) = self.extract_file_dependencies(file_path).await {
                warn!("分析文件依赖 {:?} 失败: {}", file_path, e);
            }
        }

        Ok(())
    }

    /// 从文件中提取依赖关系
    async fn extract_file_dependencies(&mut self, file_path: &Path) -> Result<()> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("读取文件失败: {:?}", file_path))?;

        let file_path_str = file_path.to_string_lossy().to_string();
        let mut dependencies = HashSet::new();

        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "rs" => {
                // Rust use 语句
                let use_regex = Regex::new(r"use\s+([^;]+);")?;
                for captures in use_regex.captures_iter(&content) {
                    let use_path = captures.get(1).unwrap().as_str();
                    dependencies.insert(use_path.to_string());
                }
            }
            "py" => {
                // Python import 语句
                let import_regex = Regex::new(r"(?:from\s+(\S+)\s+)?import\s+([^#\n]+)")?;
                for captures in import_regex.captures_iter(&content) {
                    if let Some(module) = captures.get(1) {
                        dependencies.insert(module.as_str().to_string());
                    }
                    let imports = captures.get(2).unwrap().as_str();
                    for import in imports.split(',') {
                        dependencies.insert(import.trim().to_string());
                    }
                }
            }
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" => {
                // C/C++ include 语句
                let include_regex = Regex::new(r#"#include\s+[<"]([^>"]+)[>"]"#)?;
                for captures in include_regex.captures_iter(&content) {
                    let include_file = captures.get(1).unwrap().as_str();
                    dependencies.insert(include_file.to_string());
                }
            }
            _ => {}
        }

        self.file_dependencies.insert(file_path_str, dependencies);
        Ok(())
    }

    /// 分析 Python -> Rust FFI 调用
    async fn analyze_python_rust_ffi(&mut self, source_files: &[PathBuf]) -> Result<()> {
        info!("分析 Python -> Rust FFI 调用");

        for file_path in source_files {
            if file_path.extension().and_then(|e| e.to_str()) == Some("py") {
                if let Err(e) = self.extract_python_rust_ffi(file_path).await {
                    warn!("分析 Python FFI 调用 {:?} 失败: {}", file_path, e);
                }
            }
        }

        Ok(())
    }

    /// 从 Python 文件中提取对 Rust 的 FFI 调用
    async fn extract_python_rust_ffi(&mut self, file_path: &Path) -> Result<()> {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("读取文件失败: {:?}", file_path))?;

        let file_path_str = file_path.to_string_lossy().to_string();

        // 查找可能的 FFI 调用模式
        // 1. ctypes 调用
        let ctypes_regex = Regex::new(r"(\w+)\.(\w+)\(")?;

        // 2. PyO3 模块调用
        let pyo3_regex = Regex::new(r"import\s+(\w+).*rust")?;

        for (line_num, line) in content.lines().enumerate() {
            // 检查 ctypes 调用
            for captures in ctypes_regex.captures_iter(line) {
                let lib_name = captures.get(1).unwrap().as_str();
                let function_name = captures.get(2).unwrap().as_str();

                // 如果库名包含 rust 相关字样，认为是 FFI 调用
                if lib_name.to_lowercase().contains("rust")
                    || lib_name.to_lowercase().contains("ffi")
                {
                    let call = FunctionCall {
                        caller_file: file_path_str.clone(),
                        caller_function: None,
                        caller_line: (line_num + 1) as u32,
                        called_function: function_name.to_string(),
                        called_file: None,
                        call_type: CallType::FfiCall,
                    };

                    self.function_calls
                        .entry(file_path_str.clone())
                        .or_insert_with(Vec::new)
                        .push(call);
                }
            }
        }

        Ok(())
    }

    /// 保存关系到数据库
    async fn save_relations_to_db(&self, project_name: &str) -> Result<()> {
        info!("保存调用关系到数据库");

        // 这里需要根据实际的 DatabaseManager 接口来实现
        // 保存函数定义
        for (_, function_def) in &self.function_definitions {
            debug!("保存函数定义: {}", function_def.name);
            // TODO: 调用 db_manager 的方法保存函数定义
        }

        // 保存函数调用
        for (_, calls) in &self.function_calls {
            for call in calls {
                debug!(
                    "保存函数调用: {} -> {}",
                    call.caller_file, call.called_function
                );
                // TODO: 调用 db_manager 的方法保存函数调用
            }
        }

        // 保存文件依赖
        for (source_file, dependencies) in &self.file_dependencies {
            for dependency in dependencies {
                debug!("保存文件依赖: {} -> {}", source_file, dependency);
                // TODO: 调用 db_manager 的方法保存文件依赖
            }
        }

        info!("调用关系保存完成");
        Ok(())
    }

    /// 查找行号
    fn find_line_number(&self, content: &str, position: usize) -> Result<u32> {
        let line_num = content[..position].lines().count() as u32;
        Ok(line_num)
    }

    /// 获取调用关系统计
    pub fn get_statistics(&self) -> CallRelationStatistics {
        CallRelationStatistics {
            total_functions: self.function_definitions.len(),
            total_calls: self.function_calls.values().map(|calls| calls.len()).sum(),
            total_files: self.function_calls.len(),
            ffi_calls: self
                .function_calls
                .values()
                .flatten()
                .filter(|call| matches!(call.call_type, CallType::FfiCall))
                .count(),
            rust_functions: self
                .function_definitions
                .values()
                .filter(|func| func.language == "rust")
                .count(),
            python_files: self
                .function_calls
                .keys()
                .filter(|path| path.ends_with(".py"))
                .count(),
        }
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
}

/// 调用关系统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRelationStatistics {
    pub total_functions: usize,
    pub total_calls: usize,
    pub total_files: usize,
    pub ffi_calls: usize,
    pub rust_functions: usize,
    pub python_files: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_rust_function_extraction() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.rs");

        let rust_code = r#"
pub fn hello_world() -> String {
    "Hello, World!".to_string()
}

async fn async_function(param: i32) -> Result<String, Box<dyn std::error::Error>> {
    Ok(format!("Value: {}", param))
}
"#;

        let mut file = File::create(&file_path).unwrap();
        file.write_all(rust_code.as_bytes()).unwrap();

        // 创建测试用的 DatabaseManager
        let db_manager = DatabaseManager::new_default().await.unwrap();
        let mut analyzer =
            CallRelationAnalyzer::new(db_manager, temp_dir.path().to_path_buf()).unwrap();

        analyzer.extract_rust_functions(&file_path).await.unwrap();

        assert_eq!(analyzer.function_definitions.len(), 2);
        assert!(
            analyzer
                .function_definitions
                .contains_key(&format!("hello_world@{}", file_path.display()))
        );
        assert!(
            analyzer
                .function_definitions
                .contains_key(&format!("async_function@{}", file_path.display()))
        );
    }

    #[tokio::test]
    async fn test_python_call_extraction() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test.py");

        let python_code = r#"
import rust_lib

def main():
    result = rust_lib.hello_world()
    print(result)

    # FFI call
    lib = ctypes.CDLL('./rust_lib.so')
    lib.some_function()
"#;

        let mut file = File::create(&file_path).unwrap();
        file.write_all(python_code.as_bytes()).unwrap();

        let db_manager = DatabaseManager::new_default().await.unwrap();
        let mut analyzer =
            CallRelationAnalyzer::new(db_manager, temp_dir.path().to_path_buf()).unwrap();

        analyzer.extract_python_calls(&file_path).await.unwrap();

        let calls = analyzer
            .get_file_calls(&file_path.to_string_lossy().to_string())
            .unwrap();
        assert!(calls.len() > 0);

        // 检查是否包含 rust_lib.hello_world 调用
        assert!(calls.iter().any(|call| call.called_function == "rust_lib"));
    }
}
