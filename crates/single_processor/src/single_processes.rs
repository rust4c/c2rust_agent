use anyhow::Result;
use db_services::DatabaseManager;
use llm_requester::llm_request_with_prompt;
use log::{debug, error, info, warn};
use prompt_builder::PromptBuilder;
use serde_json::Value;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

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
                if exts
                    .iter()
                    .any(|&e| ext.eq_ignore_ascii_case(e.trim_start_matches('.')))
                {
                    result.push(path);
                }
            }
        }
    }
    info!("共找到 {} 个文件", result.len());
    Ok(result)
}

// 处理文件
pub fn process_c_project_files(file_path: &Path) -> Result<PathBuf> {
    info!("开始处理C项目文件，路径: {:?}", file_path);

    // 使用输入路径的最后一级作为文件名
    let dir_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("merged");
    let output_name = format!("{}.c", dir_name);
    let output_path = file_path.join(output_name);

    info!("输出文件路径: {:?}", output_path);

    // 递归查找所有.c和.h文件
    let files = find_files(file_path, &[".c", ".h"])?;
    info!("找到 {} 个C/H文件", files.len());

    // 创建输出文件
    let mut output_file = File::create(&output_path)?;

    for file in files {
        // 写入文件内容
        debug!("处理文件: {:?}", file);
        let content = fs::read_to_string(&file)?;
        writeln!(output_file, "// File: {}", file.display())?;
        output_file.write_all(content.as_bytes())?;
        writeln!(output_file, "\n")?;
    }

    info!("文件合并完成，输出路径: {:?}", output_path);
    Ok(output_path)
}

// 创建 Rust 项目结构
fn create_rust_project_structure(project_path: &Path) -> Result<()> {
    info!("创建Rust项目结构，路径: {:?}", project_path);

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

    info!("已创建 Rust 项目结构: {}", project_path.display());
    Ok(())
}

// 处理单个文件函数
pub async fn singlefile_processor(file_path: &Path) -> Result<()> {
    info!("开始处理文件: {:?}", file_path);

    // 处理文件 - 合并项目中的所有C文件
    let merged_file_path = process_c_project_files(file_path)?;
    info!("已合并文件到: {:?}", merged_file_path);

    // 创建数据库管理器
    info!("创建数据库管理器...");
    let db_manager = DatabaseManager::new_default().await?;

    // 创建PromptBuilder
    info!("创建PromptBuilder...");
    let prompt_builder = PromptBuilder::new(
        &db_manager,
        "c_project".to_string(),
        Some(file_path.to_path_buf()),
    )
    .await?;

    // 获取合并文件的文件名（不包含路径）
    let merged_file_name = merged_file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("merged_example.c");

    info!("使用文件名构建提示词: {}", merged_file_name);

    // 构建提示词 - 使用文件名而不是完整路径
    let prompt = prompt_builder
        .build_file_context_prompt(file_path, None)
        .await?;

    // 读取合并后的代码内容并添加到提示词中
    info!("读取合并后的代码内容...");
    let merged_code = fs::read_to_string(&merged_file_path)?;
    let enhanced_prompt = format!(
        "{}\n\n--- 以下是完整C代码 ---\n{}\n\n请将上面所有C函数、类型、宏等全部转换为Rust，输出一个可编译的main.rs文件。",
        prompt, merged_code
    );

    info!("合并代码长度: {} 字符", merged_code.len());

    // 构建增强的提示词
    let enhanced_prompt = prompt.clone();

    info!("输出提示词到文件用于调试...");
    let output_promote = file_path.join("merged_promot.json");
    let mut output_file = File::create(&output_promote)?;
    writeln!(output_file, "{}", enhanced_prompt)?;

    info!("生成的提示词长度: {} 字符", enhanced_prompt.len());

    // // 调用LLM接口
    // info!("调用LLM接口");
    // let llm_response = llm_request_with_prompt(
    //     vec![enhanced_prompt.clone()],
    //     "你是一位C到Rust代码转换专家，特别擅长文件系统和FUSE相关的代码转换".to_string(),
    // ).await?;

    // // 解析JSON响应
    // info!("解析Json响应");
    // let json_response: Value = serde_json::from_str(&llm_response)?;
    
    // // // 这里储存响应是为了测试方便正式版请注释这部分代码
    // // let output_json_test = file_path.join("llm_response.json");
    // // let mut out_json_test = File::create(&output_json_test);

    // 调用LLM接口
    info!("调用LLM接口");
    let llm_response = llm_request_with_prompt(
        vec![enhanced_prompt.clone()],
    "   你是一位C到Rust代码转换专家，特别擅长文件系统和FUSE相关的代码转换".to_string(),
    ).await?;

    // 保存原始响应便于排查
    let debug_json = file_path.join("llm_response_raw.txt");
    std::fs::write(&debug_json, &llm_response)?;

    // 预处理，去掉多余包裹
    let llm_response_clean = llm_response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    // 解析JSON响应
    info!("解析Json响应");
    let json_response: Value = serde_json::from_str(llm_response_clean)?;

    // 提取rust_code字段
    info!("提取Rust代码");
    let rust_code = json_response["rust_code"].as_str().ok_or_else(|| {
        error!("响应中缺少rust_code字段，完整响应: {}", llm_response);
        anyhow::anyhow!("响应中缺少rust_code字段")
    })?;

    // 创建 Rust 项目结构
    info!("创建Rust 项目结构");
    let rust_project_path = file_path.join("rust-project");
    create_rust_project_structure(&rust_project_path)?;

    // 输出结果到指定路径
    let output_file_path = rust_project_path.join("src").join("main.rs");
    let mut output_file = File::create(&output_file_path)?;
    write!(output_file, "{}", rust_code)?;
    info!("转换结果已保存到: {:?}", output_file_path);

    // 如果有警告信息，也打印出来
    if let Some(warnings) = json_response["warnings"].as_array() {
        if !warnings.is_empty() {
            warn!("转换警告:");
            for warning in warnings {
                if let Some(warning_text) = warning.as_str() {
                    warn!("- {}", warning_text);
                }
            }
        }
    }

    info!("文件处理完成");
    Ok(())
}
