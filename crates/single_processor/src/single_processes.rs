use anyhow::Result;
use db_services::DatabaseManager;
use llm_requester::llm_request_with_prompt;
use log::{debug, error, info, warn};
use prompt_builder::PromptBuilder;
use serde_json::Value;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tokio::time::{Duration, timeout};

// 处理文件夹中的.c和.h文件
fn process_c_h_files(dir_path: &Path) -> Result<PathBuf> {
    info!("开始处理C/H文件，路径: {:?}", dir_path);

    let mut c_files = Vec::new();
    let mut h_files = Vec::new();

    // 读取目录中的文件
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                match ext.to_str().unwrap() {
                    "c" => c_files.push(path),
                    "h" => h_files.push(path),
                    _ => continue,
                }
            }
        }
    }

    info!(
        "找到 {} 个.c文件 和 {} 个.h文件",
        c_files.len(),
        h_files.len()
    );

    // 根据文件情况处理
    if c_files.is_empty() && h_files.is_empty() {
        return Err(anyhow::anyhow!("目录中没有找到.c或.h文件"));
    }

    // 如果只有.h文件，创建对应的.c文件
    if c_files.is_empty() && h_files.len() == 1 {
        let h_file = &h_files[0];
        let c_file_path = h_file.with_extension("c");

        info!("只有.h文件，创建对应的.c文件: {:?}", c_file_path);

        // 读取.h文件内容
        let mut h_content = String::new();
        File::open(h_file)?.read_to_string(&mut h_content)?;

        // 写入.c文件
        let mut c_file = File::create(&c_file_path)?;
        c_file.write_all(h_content.as_bytes())?;

        info!("已将.h文件内容写入新创建的.c文件");
        return Ok(c_file_path);
    }

    // 如果只有一个.c文件和一个.h文件，将.h内容写入.c文件开头
    if c_files.len() == 1 && h_files.len() == 1 {
        let c_file = &c_files[0];
        let h_file = &h_files[0];

        info!("有一个.c文件和一个.h文件，将.h内容写入.c文件开头");

        // 读取.h文件内容
        let mut h_content = String::new();
        File::open(h_file)?.read_to_string(&mut h_content)?;
        debug!("h_content: {}", h_content);

        // 读取现有.c文件内容
        let mut c_content = String::new();
        File::open(c_file)?.read_to_string(&mut c_content)?;
        debug!("c_content: {}", c_content);

        // 将.h内容写入.c文件开头
        let mut file = File::create(c_file)?;
        write!(file, "{}{}", h_content, c_content)?;

        info!("已将.h文件内容写入.c文件开头");
        return Ok(c_file.clone());
    }

    // 如果只有一个.c文件，不做任何处理
    if c_files.len() == 1 && h_files.is_empty() {
        info!("只有一个.c文件，不做任何处理");
        return Ok(c_files[0].clone());
    }

    // 其他情况返回错误
    Err(anyhow::anyhow!(
        "不支持的文件组合: {}个.c文件, {}个.h文件",
        c_files.len(),
        h_files.len()
    ))
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

// 处理LLM响应并提取Rust代码
fn process_llm_response(llm_response: &str, _output_dir: &Path) -> Result<String> {
    info!("处理LLM响应");

    debug!("LLM Response: {}", llm_response);

    // 尝试多种方式解析响应
    let mut rust_code = None;

    // 方法1: 尝试直接解析为JSON
    if let Ok(json_response) = serde_json::from_str::<Value>(llm_response) {
        if let Some(code) = json_response["rust_code"].as_str() {
            rust_code = Some(code.to_string());
            info!("成功从JSON响应中提取Rust代码");
        } else if let Some(choices) = json_response["choices"].as_array() {
            if let Some(first_choice) = choices.first() {
                if let Some(message) = first_choice["message"].as_object() {
                    if let Some(content) = message["content"].as_str() {
                        rust_code = Some(content.to_string());
                        info!("成功从OpenAI格式响应中提取内容");
                    }
                }
            }
        }
    }

    // 方法2: 尝试处理被代码块包裹的JSON
    if rust_code.is_none() {
        let cleaned_response = llm_response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        if let Ok(json_response) = serde_json::from_str::<Value>(cleaned_response) {
            if let Some(code) = json_response["rust_code"].as_str() {
                rust_code = Some(code.to_string());
                info!("成功从清理后的JSON响应中提取Rust代码");
            }
        }
    }

    // 方法3: 尝试直接提取Rust代码块
    if rust_code.is_none() {
        if let Some(start_idx) = llm_response.find("```rust\n") {
            let code_start = start_idx + 8; // 跳过 ```rust\n
            if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                let code_end = code_start + end_idx;
                rust_code = Some(llm_response[code_start..code_end].to_string());
                info!("成功从Rust代码块中提取代码");
            }
        } else if let Some(start_idx) = llm_response.find("```\n") {
            let code_start = start_idx + 4; // 跳过 ```\n
            if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                let code_end = code_start + end_idx;
                rust_code = Some(llm_response[code_start..code_end].to_string());
                info!("成功从通用代码块中提取代码");
            }
        }
    }

    // 方法4: 如果以上方法都失败，尝试将整个响应作为代码
    if rust_code.is_none() {
        warn!("无法从响应中提取结构化代码，将整个响应作为代码保存");
        rust_code = Some(llm_response.to_string());
    }

    debug!("Output Rust Code: {:?}", &rust_code.as_ref());

    rust_code.ok_or_else(|| anyhow::anyhow!("无法从LLM响应中提取Rust代码"))
}

// 构建包含源文件内容的提示词
fn build_prompt_with_source_files(prompt: &str, file_path: &Path) -> Result<String> {
    info!("构建包含源文件内容的提示词");

    // 处理C/H文件
    let processed_file = process_c_h_files(file_path)?;

    // 读取处理后的文件内容
    let content = fs::read_to_string(&processed_file)?;

    // 将源文件内容添加到原始提示词中
    let enhanced_prompt = format!(
        "{}\n\n--- 以下是处理后的C代码 ---\n{}\n\n请将上面的C代码转换为Rust，输出一个可编译的main.rs文件。",
        prompt, content
    );

    Ok(enhanced_prompt)
}

// 处理单个文件函数
pub async fn singlefile_processor(file_path: &Path) -> Result<()> {
    info!("开始处理文件: {:?}", file_path);

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

    // 构建提示词
    let prompt = prompt_builder
        .build_file_context_prompt(file_path, None)
        .await?;

    // 构建包含所有源文件内容的提示词
    let enhanced_prompt = build_prompt_with_source_files(&prompt, file_path)?;

    info!("生成的提示词长度: {} 字符", enhanced_prompt.len());

    debug!("Prompt Output: {}", enhanced_prompt);

    // 调用LLM接口，添加超时处理
    info!("调用LLM接口");

    // 设置超时时间为100分钟
    let timeout_duration = Duration::from_secs(6000);

    let llm_response = match timeout(
        timeout_duration,
        llm_request_with_prompt(
            vec![enhanced_prompt],
            "你是一位C到Rust代码转换专家，特别擅长文件系统和FUSE相关的代码转换".to_string(),
        ),
    )
    .await
    {
        Ok(Ok(response)) => {
            info!("LLM响应接收成功，长度: {} 字符", response.len());
            response
        }
        Ok(Err(e)) => {
            error!("LLM请求失败: {}", e);
            return Err(e);
        }
        Err(_) => {
            let error_msg = "LLM请求超时，未能在10分钟内获取响应";
            error!("{}", error_msg);

            // 保存超时信息
            let timeout_path = file_path.join("llm_request_timeout.txt");
            fs::write(timeout_path, error_msg)?;

            return Err(anyhow::anyhow!(error_msg));
        }
    };

    // 处理LLM响应并提取Rust代码
    let rust_code = process_llm_response(&llm_response, file_path)?;

    // 创建 Rust 项目结构
    info!("创建Rust 项目结构");
    let rust_project_path = file_path.join("rust-project");
    create_rust_project_structure(&rust_project_path)?;

    // 输出结果到指定路径
    let output_file_path = rust_project_path.join("src").join("main.rs");
    let mut output_file = File::create(&output_file_path)?;
    write!(output_file, "{}", rust_code)?;
    info!("转换结果已保存到: {:?}", output_file_path);

    info!("文件处理完成");
    Ok(())
}
