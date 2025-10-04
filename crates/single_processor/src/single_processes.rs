use anyhow::Result;
use db_services::DatabaseManager;
use llm_requester::llm_request_with_prompt;
use log::{info, warn};
use prompt_builder::PromptBuilder;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::time::{Duration, timeout};

// 导入各模块
use crate::ai_optimizer::ai_optimize_rust_code;
use crate::c2rust_translator::c2rust_translate;
use crate::code_splitter::{MAX_TOTAL_PROMPT_CHARS, make_messages_with_function_chunks, total_len};
use crate::file_processor::{create_rust_project_structure, process_c_h_files};
use crate::rust_verifier::{extract_key_errors, verify_compilation};

/// 提取 LLM 响应中的 Rust 代码
fn extract_rust_code_from_response(llm_response: &str) -> Result<String> {
    // 尝试多种方式解析响应
    let mut rust_code = None;

    // 方法1: 直接JSON格式
    if let Ok(json_response) = serde_json::from_str::<Value>(llm_response) {
        if let Some(code) = json_response["rust_code"].as_str() {
            info!("成功从JSON响应中提取rust_code字段");
            rust_code = Some(code.to_string());
        } else if let Some(choices) = json_response["choices"].as_array() {
            if let Some(first_choice) = choices.first() {
                if let Some(message) = first_choice["message"].as_object() {
                    if let Some(content) = message["content"].as_str() {
                        info!("成功从OpenAI格式响应中提取内容");
                        rust_code = Some(content.to_string());
                    }
                }
            }
        }
    }

    // 方法2: 处理被代码块包裹的JSON（可能被markdown包裹）
    if rust_code.is_none() {
        let cleaned_response = llm_response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        if let Ok(json_response) = serde_json::from_str::<Value>(cleaned_response) {
            if let Some(code) = json_response["rust_code"].as_str() {
                info!("成功从清理后的JSON响应中提取rust_code字段");
                rust_code = Some(code.to_string());
            }
        }
    }

    // 方法3: 提取Rust代码块（多种变体）
    if rust_code.is_none() {
        // 尝试 ```rust 代码块
        if let Some(start_idx) = llm_response.find("```rust") {
            let code_start = if llm_response[start_idx..].starts_with("```rust\n") {
                start_idx + 8
            } else {
                // 处理 ```rust 后面可能有空格的情况
                start_idx + 7
            };

            if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                let code_end = code_start + end_idx;
                info!("成功从```rust代码块中提取代码");
                rust_code = Some(llm_response[code_start..code_end].to_string());
            } else if let Some(end_idx) = llm_response[code_start..].find("```") {
                // 兜底：```后面没有换行符
                let code_end = code_start + end_idx;
                warn!("从```rust代码块中提取代码（无结束换行符）");
                rust_code = Some(llm_response[code_start..code_end].to_string());
            }
        }
        // 尝试通用代码块 ```
        else if let Some(start_idx) = llm_response.find("```\n") {
            let code_start = start_idx + 4;
            if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                let code_end = code_start + end_idx;
                info!("成功从通用代码块中提取代码");
                rust_code = Some(llm_response[code_start..code_end].to_string());
            }
        }
    }

    // 方法4: 尝试从不完整的JSON中提取rust_code（处理分割失效）
    if rust_code.is_none() {
        // 使用字符串搜索提取 "rust_code": "..." 的内容
        if let Some(start_pos) = llm_response.find(r#""rust_code""#) {
            if let Some(colon_pos) = llm_response[start_pos..].find(':') {
                let value_start = start_pos + colon_pos + 1;
                let remaining = &llm_response[value_start..].trim_start();

                // 跳过前导引号
                if remaining.starts_with('"') {
                    let content_start =
                        value_start + (llm_response[value_start..].len() - remaining.len()) + 1;
                    let bytes = llm_response.as_bytes();
                    let mut pos = content_start;
                    let mut escaped = false;

                    // 查找下一个未转义的引号
                    while pos < bytes.len() {
                        if escaped {
                            escaped = false;
                        } else if bytes[pos] == b'\\' {
                            escaped = true;
                        } else if bytes[pos] == b'"' {
                            // 找到结束引号，处理转义序列
                            if let Ok(json_str) =
                                String::from_utf8(bytes[content_start..pos].to_vec())
                            {
                                // 手动处理常见的JSON转义序列
                                let unescaped = json_str
                                    .replace(r"\n", "\n")
                                    .replace(r"\t", "\t")
                                    .replace(r#"\""#, "\"")
                                    .replace(r"\\", "\\");
                                info!("从不完整JSON中成功提取并解码rust_code字段");
                                rust_code = Some(unescaped);
                                break;
                            }
                        }
                        pos += 1;
                    }
                }
            }
        }
    }

    // 方法5: 整个响应作为兜底
    if rust_code.is_none() {
        warn!("所有提取方法均失败，将整个响应作为代码保存（兜底处理）");
        rust_code = Some(llm_response.to_string());
    }

    rust_code.ok_or_else(|| anyhow::anyhow!("无法从LLM响应中提取Rust代码"))
}

/// 处理单个文件函数 - 纯 AI 翻译模式
///
/// 直接使用 LLM 将 C 代码转换为 Rust
pub async fn singlefile_processor(file_path: &Path) -> Result<()> {
    info!("开始处理文件: {:?}", file_path);

    let db_manager = DatabaseManager::new_default().await?;
    let prompt_builder = PromptBuilder::new(
        &db_manager,
        "c_project".to_string(),
        Some(file_path.to_path_buf()),
    )
    .await?;

    let prompt = prompt_builder
        .build_file_context_prompt(file_path, None)
        .await?;

    let processed_file = process_c_h_files(file_path)?;
    let content = fs::read_to_string(&processed_file)?;
    let enhanced_prompt = format!(
        "{}\n\n请将下面传输的 C 代码片段整体转换为一个可编译的 Rust main.rs（保持功能等价、可编译）。当你收到所有片段后再开始输出最终结果。",
        prompt
    );
    let messages = make_messages_with_function_chunks(
        &enhanced_prompt,
        "以下是处理后的 C 代码",
        &content,
        true,
        MAX_TOTAL_PROMPT_CHARS,
    );

    info!(
        "生成的消息条数: {}，总长度: {} 字符",
        messages.len(),
        total_len(&messages)
    );

    let timeout_duration = Duration::from_secs(6000);
    let llm_response = match timeout(
        timeout_duration,
        llm_request_with_prompt(
            messages,
            "你是一位C到Rust代码转换专家，特别擅长文件系统和FUSE相关的代码转换".to_string(),
        ),
    )
    .await
    {
        Ok(Ok(response)) => {
            info!("LLM响应接收成功，长度: {} 字符", response.len());
            response
        }
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            let error_msg = "LLM请求超时，未能在100分钟内获取响应";
            let timeout_path = file_path.join("llm_request_timeout.txt");
            fs::write(timeout_path, error_msg)?;
            return Err(anyhow::anyhow!(error_msg));
        }
    };

    let rust_code = extract_rust_code_from_response(&llm_response)?;
    let rust_project_path = file_path.join("rust-project");
    create_rust_project_structure(&rust_project_path)?;

    let output_file_path = rust_project_path.join("src").join("main.rs");
    let mut output_file = File::create(&output_file_path)?;
    write!(output_file, "{}", rust_code)?;
    info!("转换结果已保存到: {:?}", output_file_path);
    info!("文件处理完成");
    Ok(())
}

/// 阶段状态回调类型
pub type StageCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// 两阶段翻译主函数（带进度回调）
///
/// 第一阶段：C2Rust 自动翻译
/// 第二阶段：AI 优化并集成编译验证（最多重试 3 次）
pub async fn two_stage_processor_with_callback(
    file_path: &Path,
    callback: Option<StageCallback>,
) -> Result<()> {
    let notify = |msg: &str| {
        if let Some(ref cb) = callback {
            cb(msg);
        }
    };

    info!("开始两阶段翻译处理: {:?}", file_path);
    notify("📋 准备处理C文件");

    notify("📋 准备处理C文件");

    let processed_c_file = process_c_h_files(file_path)?;
    info!("要翻译的C文件: {:?}", processed_c_file);
    notify("✓ C文件预处理完成");

    let work_dir = file_path.join("two-stage-translation");
    let c2rust_dir = work_dir.join("c2rust-output");
    let final_dir = work_dir.join("final-output");

    fs::create_dir_all(&work_dir)?;
    fs::create_dir_all(&c2rust_dir)?;
    fs::create_dir_all(&final_dir)?;

    notify("🔄 阶段1/2: C2Rust自动翻译");
    info!("🔄 第一阶段：C2Rust 自动翻译");
    let c2rust_output = match c2rust_translate(&processed_c_file, &c2rust_dir).await {
        Ok(path) => {
            info!("✅ C2Rust 翻译成功: {:?}", path);
            notify("✓ C2Rust翻译完成");
            path
        }
        Err(e) => {
            warn!("⚠️  C2Rust 翻译失败: {}，将跳过第一阶段直接使用AI翻译", e);
            notify("⚠️ C2Rust失败，切换纯AI模式");
            return singlefile_processor(file_path).await;
        }
    };

    notify("🔄 阶段2/2: AI优化+编译验证");
    info!("🔄 第二阶段：AI 代码优化 + 编译验证");
    create_rust_project_structure(&final_dir)?;

    let final_output_path = final_dir.join("src").join("main.rs");
    let max_retries = 3;
    let mut compile_errors: Option<String> = None;

    for attempt in 1..=max_retries {
        notify(&format!("🔄 AI优化 (尝试 {}/{})", attempt, max_retries));
        info!("🔄 AI优化尝试 {}/{}", attempt, max_retries);

        let optimized_code = ai_optimize_rust_code(
            &c2rust_output,
            &processed_c_file,
            &final_dir,
            compile_errors.as_deref(),
        )
        .await?;

        fs::write(&final_output_path, &optimized_code)?;
        info!("✅ AI优化代码已保存: {:?}", final_output_path);
        notify("✓ AI优化完成，准备编译");

        notify(&format!("🔍 编译验证 (尝试 {}/{})", attempt, max_retries));
        info!("🔍 开始编译验证（尝试 {}/{}）", attempt, max_retries);
        match verify_compilation(&final_dir) {
            Ok(_) => {
                info!("🎉 编译验证通过！两阶段翻译成功完成");
                notify("🎉 编译通过！");

                let c2rust_backup_path = final_dir.join("c2rust_original.rs");
                if let Ok(c2rust_content) = fs::read_to_string(&c2rust_output) {
                    fs::write(&c2rust_backup_path, &c2rust_content)?;
                    info!("📄 C2Rust 原始输出已备份到: {:?}", c2rust_backup_path);
                }

                info!("✅ 两阶段翻译处理完成，最终结果: {:?}", final_output_path);
                notify("✅ 全部完成");
                return Ok(());
            }
            Err(e) => {
                if attempt < max_retries {
                    warn!("❌ 编译失败（尝试 {}/{}），准备重试", attempt, max_retries);
                    notify(&format!(
                        "❌ 编译失败，准备重试 ({}/{})",
                        attempt, max_retries
                    ));

                    let error_msg = e.to_string();
                    let key_errors = extract_key_errors(&error_msg);
                    info!("关键错误信息：\n{}", key_errors);

                    compile_errors = Some(key_errors);
                } else {
                    warn!("❌ 编译验证失败，已达最大重试次数 {}", max_retries);
                    warn!("最后的编译错误: {}", e);
                    notify("❌ 编译失败，已达重试上限");

                    let error_log_path = final_dir.join("final_compile_errors.txt");
                    fs::write(&error_log_path, e.to_string())?;
                    info!("编译错误已保存到: {:?}", error_log_path);

                    return Err(anyhow::anyhow!(
                        "两阶段翻译失败：AI优化后的代码经过 {} 次尝试仍无法编译通过。\n最后错误: {}",
                        max_retries,
                        e
                    ));
                }
            }
        }
    }

    Err(anyhow::anyhow!("两阶段翻译失败：未知错误"))
}

/// 两阶段翻译主函数（无回调版本，向后兼容）
pub async fn two_stage_processor(file_path: &Path) -> Result<()> {
    two_stage_processor_with_callback(file_path, None).await
}
