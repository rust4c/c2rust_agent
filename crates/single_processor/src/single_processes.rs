use llm_requester::llm_request_with_prompt;
use prompt_builder::PromptBuilder;
use db_services::DatabaseManager;
use std::path::{Path, PathBuf};
use std::fs;
use std::fs::File;
use std::io::Write;
use anyhow::{Result, Context};
use serde_json::Value;

// 递归查找指定扩展名的文件
fn find_files(dir: &Path, exts: &[&str]) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                result.extend(find_files(&path, exts)?);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if exts.iter().any(|&e| ext.eq_ignore_ascii_case(e.trim_start_matches('.'))) {
                    result.push(path);
                }
            }
        }
    }
    Ok(result)
}

// 处理文件
pub fn process_c_project_files(file_path: &Path) -> Result<PathBuf> {
    // 输出路径
    let output_path = file_path.join("merged_example.c");

    // 递归查找所有.c和.h文件
    let files = find_files(file_path, &[".c", ".h"])?;

    // 创建输出文件
    let mut output_file = File::create(&output_path)?;

    for file in files {
        // 写入文件内容
        let content = fs::read_to_string(&file)?;
        writeln!(output_file, "// File: {}", file.display())?;
        output_file.write_all(content.as_bytes())?;
        writeln!(output_file, "\n")?;
    }
    
    Ok(output_path)
}

// 创建 Rust 项目结构
fn create_rust_project_structure(project_path: &Path) -> Result<()> {
    // 创建项目目录
    fs::create_dir_all(project_path.join("src"))?;
    
    // 创建 Cargo.toml 文件
    let cargo_toml_content = r#"[package]
name = "converted-project"
version = "0.1.0"
edition = "2021"

[dependencies]
libc = "0.2"
"#;
    
    let mut cargo_file = File::create(project_path.join("Cargo.toml"))?;
    write!(cargo_file, "{}", cargo_toml_content)?;
    
    println!("已创建 Rust 项目结构: {}", project_path.display());
    Ok(())
}

// 处理单个文件函数
pub async fn singlefile_processor(file_path: &Path) -> Result<()> {
    println!("正在处理文件: {}", file_path.display());

    // 处理文件 - 合并项目中的所有C文件
    let merged_file_path = process_c_project_files(file_path)?;
    println!("已合并文件到: {}", merged_file_path.display());

    // 创建数据库管理器
    let db_manager = DatabaseManager::new_default().await?;

    // 创建PromptBuilder
    let prompt_builder = PromptBuilder::new(&db_manager, "c_project".to_string(), None).await?;

    // 构建提示词
    let prompt = prompt_builder
        .build_file_context_prompt(merged_file_path.to_str().unwrap(), None)
        .await?;

    println!("生成的提示词长度: {} 字符", prompt.len());

    // 调用LLM接口 - 使用新的函数签名
    let llm_response = llm_request_with_prompt(
        vec![prompt.clone()], // 将提示词作为消息
        "你是一位C到Rust代码转换专家".to_string() // 系统提示
    ).await?;

    println!("LLM响应长度: {} 字符", llm_response.len());
    
    // 解析JSON响应
    let json_response: Value = serde_json::from_str(&llm_response)?;
    
    // 提取rust_code字段
    let rust_code = json_response["rust_code"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("响应中缺少rust_code字段"))?;
    
    println!("提取的Rust代码长度: {} 字符", rust_code.len());

    // 创建 Rust 项目结构
    let rust_project_path = file_path.join("rust-project");
    create_rust_project_structure(&rust_project_path)?;

    // 输出结果到指定路径
    let output_file_path = rust_project_path.join("src").join("main.rs");
    
    let mut output_file = File::create(&output_file_path)?;
    write!(output_file, "{}", rust_code)?;
    println!("转换结果已保存到: {}", output_file_path.display());

    // 如果有警告信息，也打印出来
    if let Some(warnings) = json_response["warnings"].as_array() {
        if !warnings.is_empty() {
            println!("转换警告:");
            for warning in warnings {
                if let Some(warning_text) = warning.as_str() {
                    println!("- {}", warning_text);
                }
            }
        }
    }

    Ok(())
}