use anyhow::{Context, Result, anyhow};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[derive(Debug, Serialize, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub file: PathBuf,
    pub return_type: String,
    pub parameters: Vec<Parameter>,
    pub line: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Parameter {
    pub name: String,
    pub r#type: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub file: PathBuf,
    pub members: Vec<Member>,
    pub line: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Member {
    pub name: String,
    pub r#type: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct VariableInfo {
    pub name: String,
    pub file: PathBuf,
    pub r#type: String,
    pub line: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct MacroInfo {
    pub name: String,
    pub file: PathBuf,
    pub value: String,
    pub line: u32,
}

pub struct ClangdAnalyzer {
    project_root: PathBuf,
    compile_commands_path: PathBuf,
    pub functions: Vec<FunctionInfo>,
    pub classes: Vec<ClassInfo>,
    pub variables: Vec<VariableInfo>,
    pub macros: Vec<MacroInfo>,
}

impl ClangdAnalyzer {
    pub fn new(project_root: &str) -> Self {
        let project_root = PathBuf::from(project_root).canonicalize().unwrap();
        let compile_commands_path = project_root.join("compile_commands.json");

        ClangdAnalyzer {
            project_root,
            compile_commands_path,
            functions: Vec::new(),
            classes: Vec::new(),
            variables: Vec::new(),
            macros: Vec::new(),
        }
    }

    pub fn generate_compile_commands(&self) -> Result<()> {
        println!("Generating compilation database using compiledb...");

        let status = Command::new("compiledb")
            .arg("-n")
            .arg("make")
            .current_dir(&self.project_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| anyhow!("Failed to run compiledb: {}", e))?;

        if !status.success() {
            return Err(anyhow!("compiledb failed with exit code: {}", status));
        }

        println!("Successfully generated compilation database");
        Ok(())
    }

    pub fn get_source_files_from_compile_commands(&self) -> Result<Vec<PathBuf>> {
        if !self.compile_commands_path.exists() {
            return Err(anyhow!("Compilation database does not exist"));
        }

        let content = fs::read_to_string(&self.compile_commands_path)?;
        let compile_commands: Vec<Value> = serde_json::from_str(&content)?;

        let mut source_files = Vec::new();
        for entry in compile_commands {
            if let Some(file) = entry.get("file").and_then(Value::as_str) {
                let mut path = PathBuf::from(file);
                if path.is_relative() {
                    path = self.project_root.join(path);
                }
                source_files.push(path);
            }
        }

        println!(
            "Found {} source files in compilation database",
            source_files.len()
        );
        Ok(source_files)
    }

    pub fn analyze_with_clang_ast(&mut self, file_path: &Path) -> Result<()> {
        let compile_command = self
            .find_compile_command_for_file(file_path)
            .unwrap_or_default();

        let mut cmd = Command::new("clang");
        cmd.arg("-Xclang")
            .arg("-ast-dump=json")
            .arg("-fsyntax-only")
            .arg("-w")
            .arg("-Wno-error")
            .arg("-ferror-limit=0");

        // Parse and add compile command arguments
        for part in compile_command.split_whitespace() {
            if part == "-o" || part == "-c" {
                continue;
            }
            if part.ends_with(".c")
                || part.ends_with(".cpp")
                || part.ends_with(".cc")
                || part.ends_with(".cxx")
            {
                continue;
            }
            cmd.arg(part);
        }

        cmd.arg(file_path);

        let output = cmd.output()?;

        if !output.status.success() {
            println!("Clang AST analysis failed for: {}", file_path.display());
            return self.fallback_parse(file_path);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(ast_data) = serde_json::from_str::<Value>(&stdout) {
            self.traverse_ast(&ast_data, file_path);
            Ok(())
        } else {
            println!("Failed to parse AST JSON for: {}", file_path.display());
            self.fallback_parse(file_path)
        }
    }

    fn find_compile_command_for_file(&self, file_path: &Path) -> Option<String> {
        if !self.compile_commands_path.exists() {
            return None;
        }

        let content = fs::read_to_string(&self.compile_commands_path).ok()?;
        let compile_commands: Vec<Value> = serde_json::from_str(&content).ok()?;

        for entry in compile_commands {
            if let Some(entry_file) = entry.get("file").and_then(Value::as_str) {
                let mut path = PathBuf::from(entry_file);
                if path.is_relative() {
                    path = self.project_root.join(path);
                }

                if path == file_path {
                    return entry
                        .get("command")
                        .and_then(Value::as_str)
                        .map(String::from);
                }
            }
        }

        None
    }

    fn fallback_parse(&mut self, file_path: &Path) -> Result<()> {
        let content = fs::read_to_string(file_path)?;
        self.extract_functions_with_regex(&content, file_path);
        self.extract_structs_with_regex(&content, file_path);
        Ok(())
    }

    fn extract_functions_with_regex(&mut self, content: &str, file_path: &Path) {
        let func_pattern = r"(?x)
            (?:static\s+)?        # optional static
            (?:inline\s+)?        # optional inline
            (\w+(?:\s*\*)*)       # return type (can include pointers)
            \s+                   # whitespace
            (\w+)                 # function name
            \s*                   # optional whitespace
            \(                    # opening parenthesis
            ([^{]*?)              # parameters (non-greedy)
            \)                    # closing parenthesis
            \s*                   # optional whitespace
            (?:                   # non-capturing group for function start
                \{                # opening brace
                | ;               # or semicolon (for declarations)
            )";

        let re = Regex::new(func_pattern).unwrap();
        let mut line_number = 1;
        let mut last_index = 0;

        for cap in re.captures_iter(content) {
            // Calculate line number
            if let Some(m) = cap.get(0) {
                let text_since_last = &content[last_index..m.start()];
                line_number += text_since_last.chars().filter(|&c| c == '\n').count() as u32;
                last_index = m.start();
            }

            let return_type = cap[1].trim().to_string();
            let func_name = cap[2].trim().to_string();
            let params_str = cap[3].trim();

            let mut parameters = Vec::new();
            if !params_str.is_empty() && params_str != "void" {
                // Split parameters while handling commas inside parentheses
                let mut param_parts = Vec::new();
                let mut current = String::new();
                let mut paren_depth = 0;

                for c in params_str.chars() {
                    match c {
                        '(' => {
                            paren_depth += 1;
                            current.push(c);
                        }
                        ')' => {
                            paren_depth -= 1;
                            current.push(c);
                        }
                        ',' if paren_depth == 0 => {
                            param_parts.push(current.trim().to_string());
                            current.clear();
                        }
                        _ => current.push(c),
                    }
                }

                if !current.is_empty() {
                    param_parts.push(current.trim().to_string());
                }

                for param in param_parts {
                    if param.is_empty() {
                        continue;
                    }

                    let parts: Vec<&str> = param.split_whitespace().collect();
                    let param_name = if parts.len() > 1 {
                        // Remove any array brackets or pointers from the name
                        parts
                            .last()
                            .unwrap()
                            .trim_matches(|c| c == '[' || c == ']' || c == '*' || c == '&')
                    } else {
                        "param"
                    }
                    .to_string();

                    let param_type = if parts.len() > 1 {
                        parts[..parts.len() - 1].join(" ")
                    } else {
                        parts[0].to_string()
                    };

                    parameters.push(Parameter {
                        name: param_name,
                        r#type: param_type,
                    });
                }
            }

            self.functions.push(FunctionInfo {
                name: func_name,
                file: file_path.to_path_buf(),
                return_type,
                parameters,
                line: line_number,
            });
        }
    }

    fn extract_structs_with_regex(&mut self, content: &str, file_path: &Path) {
        let struct_pattern = r"(?x)
            (?:typedef\s+)?       # optional typedef
            struct\s+             # struct keyword
            (\w+)?                # struct name (optional)
            \s*                   # optional whitespace
            \{                    # opening brace
            ([^}]*)               # members (non-greedy)
            \}                    # closing brace
            \s*                   # optional whitespace
            (\w+)?                # struct alias (optional)
            \s*                   # optional whitespace
            ;                     # semicolon";

        let re = Regex::new(struct_pattern).unwrap();
        let mut line_number = 1;
        let mut last_index = 0;

        for cap in re.captures_iter(content) {
            // Calculate line number
            if let Some(m) = cap.get(0) {
                let text_since_last = &content[last_index..m.start()];
                line_number += text_since_last.chars().filter(|&c| c == '\n').count() as u32;
                last_index = m.start();
            }

            let name1 = cap.get(1).map(|m| m.as_str().to_string());
            let name2 = cap.get(3).map(|m| m.as_str().to_string());
            let struct_name = name1.or(name2).unwrap_or_else(|| "anonymous".to_string());

            if struct_name == "anonymous" {
                continue;
            }

            let members_str = cap[2].trim();
            let mut members = Vec::new();

            for line in members_str.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with("//") || line.starts_with("/*") {
                    continue;
                }

                // Remove trailing semicolon if present
                let line = line.trim_end_matches(';');

                // Skip lines with just a semicolon
                if line.is_empty() {
                    continue;
                }

                // Split into parts (type and name)
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }

                let member_type = if parts.len() > 1 {
                    parts[..parts.len() - 1].join(" ")
                } else {
                    parts[0].to_string()
                };

                let member_name = if parts.len() > 1 {
                    // Clean up the name (remove any trailing semicolon, etc.)
                    parts
                        .last()
                        .unwrap()
                        .trim_matches(|c| c == ';' || c == '[' || c == ']' || c == '*' || c == '&')
                } else {
                    "unnamed"
                }
                .to_string();

                members.push(Member {
                    name: member_name,
                    r#type: member_type,
                });
            }

            self.classes.push(ClassInfo {
                name: struct_name,
                file: file_path.to_path_buf(),
                members,
                line: line_number,
            });
        }
    }

    fn traverse_ast(&mut self, node: &Value, file_path: &Path) {
        if let Some(kind) = node.get("kind").and_then(Value::as_str) {
            match kind {
                "FunctionDecl" => self.extract_function_info(node, file_path),
                "RecordDecl" | "CXXRecordDecl" => self.extract_struct_info(node, file_path),
                "VarDecl" => self.extract_variable_info(node, file_path),
                "MacroDefinition" => self.extract_macro_info(node, file_path),
                _ => (),
            }
        }

        if let Some(inner) = node.get("inner").and_then(Value::as_array) {
            for child in inner {
                self.traverse_ast(child, file_path);
            }
        }
    }

    fn extract_function_info(&mut self, node: &Value, file_path: &Path) {
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed")
            .to_string();

        let return_type = node
            .get("type")
            .and_then(|t| t.get("qualType"))
            .and_then(Value::as_str)
            .map(|s| {
                if let Some(pos) = s.find('(') {
                    s[..pos].trim().to_string()
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_else(|| "void".to_string());

        let line = node
            .get("loc")
            .and_then(|loc| loc.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        let mut parameters = Vec::new();
        if let Some(inner) = node.get("inner").and_then(Value::as_array) {
            for child in inner {
                if let Some(kind) = child.get("kind").and_then(Value::as_str) {
                    if kind == "ParmVarDecl" {
                        let param_name = child
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("unnamed")
                            .to_string();

                        let param_type = child
                            .get("type")
                            .and_then(|t| t.get("qualType"))
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string();

                        parameters.push(Parameter {
                            name: param_name,
                            r#type: param_type,
                        });
                    }
                }
            }
        }

        self.functions.push(FunctionInfo {
            name,
            file: file_path.to_path_buf(),
            return_type,
            parameters,
            line,
        });
    }

    fn extract_struct_info(&mut self, node: &Value, file_path: &Path) {
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed")
            .to_string();

        if name == "unnamed" {
            return;
        }

        let line = node
            .get("loc")
            .and_then(|loc| loc.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        let mut members = Vec::new();
        if let Some(inner) = node.get("inner").and_then(Value::as_array) {
            for child in inner {
                if let Some(kind) = child.get("kind").and_then(Value::as_str) {
                    if kind == "FieldDecl" {
                        let member_name = child
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("unnamed")
                            .to_string();

                        let member_type = child
                            .get("type")
                            .and_then(|t| t.get("qualType"))
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string();

                        members.push(Member {
                            name: member_name,
                            r#type: member_type,
                        });
                    }
                }
            }
        }

        self.classes.push(ClassInfo {
            name,
            file: file_path.to_path_buf(),
            members,
            line,
        });
    }

    fn extract_variable_info(&mut self, node: &Value, file_path: &Path) {
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed")
            .to_string();

        let var_type = node
            .get("type")
            .and_then(|t| t.get("qualType"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let line = node
            .get("loc")
            .and_then(|loc| loc.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        self.variables.push(VariableInfo {
            name,
            file: file_path.to_path_buf(),
            r#type: var_type,
            line,
        });
    }

    fn extract_macro_info(&mut self, node: &Value, file_path: &Path) {
        let name = node
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unnamed")
            .to_string();

        let value = node
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let line = node
            .get("loc")
            .and_then(|loc| loc.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;

        self.macros.push(MacroInfo {
            name,
            file: file_path.to_path_buf(),
            value,
            line,
        });
    }

    pub fn analyze_project(&mut self) -> Result<()> {
        println!("Analyzing project: {}", self.project_root.display());

        // Attempt to generate compilation database
        if let Err(e) = self.generate_compile_commands() {
            println!("⚠️ Failed to generate compilation database: {}", e);
        }

        // Get source files from compilation database
        let source_files = match self.get_source_files_from_compile_commands() {
            Ok(files) => files,
            Err(e) => {
                println!(
                    "⚠️ Failed to get source files from compilation database: {}",
                    e
                );
                println!("Falling back to directory scan...");
                self.find_source_files_in_project()?
            }
        };

        if source_files.is_empty() {
            return Err(anyhow!("No source files found"));
        }

        println!("Found {} source files to analyze", source_files.len());

        for (i, file) in source_files.iter().enumerate() {
            println!(
                "Analyzing ({}/{}): {}",
                i + 1,
                source_files.len(),
                file.display()
            );
            if let Err(e) = self.analyze_with_clang_ast(file) {
                println!("⚠️ Failed to analyze {}: {}", file.display(), e);
            }
        }

        Ok(())
    }

    fn find_source_files_in_project(&self) -> Result<Vec<PathBuf>> {
        let mut source_files = Vec::new();
        let extensions = ["c", "cpp", "cxx", "cc", "h", "hpp", "hxx"];

        for entry in walkdir::WalkDir::new(&self.project_root) {
            let entry = entry?;
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    if extensions.contains(&ext) {
                        source_files.push(entry.path().to_path_buf());
                    }
                }
            }
        }

        println!(
            "Found {} source files in project directory",
            source_files.len()
        );
        Ok(source_files)
    }

    pub fn print_analysis_results(&self, detailed: bool) {
        println!("\n{}\n代码分析结果\n{}", "=".repeat(80), "=".repeat(80));

        if detailed {
            self.print_detailed_results();
        } else {
            self.print_summary_results();
        }

        self.print_statistics();
    }

    fn print_detailed_results(&self) {
        // Functions
        println!("\n📋 函数列表 ({} 个):", self.functions.len());
        println!("{}", "-".repeat(60));

        for func in &self.functions {
            let file_rel = self.relative_path(&func.file);
            println!("🔧 {}", func.name);
            println!("   文件: {}:{}", file_rel, func.line);
            println!("   返回类型: {}", func.return_type);

            if func.parameters.is_empty() {
                println!("   参数: 无");
            } else {
                println!("   参数:");
                for param in &func.parameters {
                    println!("     - {}: {}", param.name, param.r#type);
                }
            }
            println!();
        }

        // Classes
        println!("\n📊 结构体/类列表 ({} 个):", self.classes.len());
        println!("{}", "-".repeat(60));

        for class in &self.classes {
            let file_rel = self.relative_path(&class.file);
            println!("🏗️  {}", class.name);
            println!("   文件: {}:{}", file_rel, class.line);

            if class.members.is_empty() {
                println!("   成员: 无");
            } else {
                println!("   成员:");
                for member in &class.members {
                    println!("     - {}: {}", member.name, member.r#type);
                }
            }
            println!();
        }

        // Variables
        println!("\n🌐 全局变量列表 ({} 个):", self.variables.len());
        println!("{}", "-".repeat(60));

        for var in &self.variables {
            let file_rel = self.relative_path(&var.file);
            println!("📦 {}", var.name);
            println!("   文件: {}:{}", file_rel, var.line);
            println!("   类型: {}", var.r#type);
            println!();
        }
    }

    fn print_summary_results(&self) {
        // Filter out complex functions and anonymous structs
        let important_functions: Vec<_> = self
            .functions
            .iter()
            .filter(
                |f| {
                    !f.name.starts_with("__")
                        && f.parameters.len() <= 5
                        && !f.return_type.contains("(*")
                }, // Filter function pointers
            )
            .collect();

        let count = std::cmp::min(20, important_functions.len());
        println!("\n📋 主要函数列表 (显示 {} 个):", count);
        println!("{}", "-".repeat(60));

        for func in important_functions.iter().take(count) {
            let file_rel = self.relative_path(&func.file);
            let params: Vec<String> = func
                .parameters
                .iter()
                .map(|p| format!("{}: {}", p.name, p.r#type))
                .collect();

            println!(
                "🔧 {} {}({})",
                func.return_type,
                func.name,
                params.join(", ")
            );

            println!("   文件: {}:{}", file_rel, func.line);
            println!();
        }

        // Classes
        if !self.classes.is_empty() {
            println!("\n📊 结构体/类列表 ({} 个):", self.classes.len());
            println!("{}", "-".repeat(60));

            for class in &self.classes {
                let file_rel = self.relative_path(&class.file);
                println!("🏗️  {} ({} 成员)", class.name, class.members.len());
                println!("   文件: {}:{}", file_rel, class.line);

                if !class.members.is_empty() {
                    for member in class.members.iter().take(3) {
                        println!("     - {}: {}", member.name, member.r#type);
                    }

                    if class.members.len() > 3 {
                        println!("     ... 还有 {} 个成员", class.members.len() - 3);
                    }
                }
                println!();
            }
        }
    }

    fn print_statistics(&self) {
        println!("\n📈 统计信息:");
        println!("{}", "-".repeat(30));
        println!("函数总数: {}", self.functions.len());
        println!("结构体/类总数: {}", self.classes.len());
        println!("全局变量总数: {}", self.variables.len());
        println!("宏定义总数: {}", self.macros.len());

        // File statistics
        let mut file_stats: HashMap<PathBuf, (usize, usize, usize)> = HashMap::new();

        for func in &self.functions {
            let entry = file_stats.entry(func.file.clone()).or_insert((0, 0, 0));
            entry.0 += 1;
        }

        for class in &self.classes {
            let entry = file_stats.entry(class.file.clone()).or_insert((0, 0, 0));
            entry.1 += 1;
        }

        for var in &self.variables {
            let entry = file_stats.entry(var.file.clone()).or_insert((0, 0, 0));
            entry.2 += 1;
        }

        println!("\n📁 按文件统计:");
        for (file, (funcs, classes, vars)) in &file_stats {
            let rel_path = self.relative_path(file);
            println!(
                "  {}: {}函数, {}结构体, {}变量",
                rel_path, funcs, classes, vars
            );
        }
    }

    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.project_root)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    pub fn get_structure(&self) -> (Vec<FunctionInfo>, Vec<ClassInfo>, Vec<VariableInfo>) {
        (
            self.functions.clone(),
            self.classes.clone(),
            self.variables.clone(),
        )
    }
}

pub fn check_function_and_class_name(project_path: &str, detailed: bool) -> Result<()> {
    println!("🚀 Starting C/C++ code analysis with clang...");
    println!("Project path: {}", project_path);

    let mut analyzer = ClangdAnalyzer::new(project_path);
    analyzer.analyze_project()?;
    analyzer.print_analysis_results(detailed);

    println!("\n✅ Analysis completed!");
    Ok(())
}
