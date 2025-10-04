use anyhow::Result;
use db_services::DatabaseManager;
use llm_requester::llm_request_with_prompt;
use log::{error, info, warn};
use prompt_builder::PromptBuilder;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tokio::time::{timeout, Duration};

use crate::code_splitter::{append_text_with_limit, make_messages_with_function_chunks, total_len, MAX_TOTAL_PROMPT_CHARS};

/// 当第二次附加编译结果且超过限制时，调用 LLM 进行概括压缩
async fn summarize_text_if_needed(original: &str, required_len: usize) -> Result<String> {
    let prompt = format!(
        "你是资深 Rust 构建与诊断专家。请将以下编译/链接输出进行高度概括，提炼：\n- 主要报错与根因（按模块/函数聚类）\n- 关键报错行与最短复现线索\n- 可能的修复建议（最多 5 条）\n\n请将输出限制在大约 {} 个字符以内，注意删除无关警告与重复堆栈。",
        required_len
    );

    let input = format!("原始日志如下：\n```\n{}\n```", original);
    let summary = llm_request_with_prompt(vec![input], prompt).await?;
    Ok(summary)
}

/// 处理LLM响应并提取Rust代码
fn process_llm_response(llm_response: &str, _output_dir: &Path) -> Result<String> {
    info!("处理LLM响应");

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
            let code_start = start_idx + 8;
            if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                let code_end = code_start + end_idx;
                rust_code = Some(llm_response[code_start..code_end].to_string());
                info!("成功从Rust代码块中提取代码");
            }
        } else if let Some(start_idx) = llm_response.find("```\n") {
            let code_start = start_idx + 4;
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

    rust_code.ok_or_else(|| anyhow::anyhow!("无法从LLM响应中提取Rust代码"))
}

/// AI 第二阶段翻译：优化 C2Rust 生成的代码
/// 
/// # 参数
/// * `rust_code_path` - C2Rust 生成的 Rust 代码路径
/// * `original_c_path` - 原始 C 代码路径（用于参考）
/// * `output_dir` - 输出目录
/// * `compile_errors` - 可选的编译错误信息，用于迭代修复
/// 
/// # 返回
/// 优化后的 Rust 代码
pub async fn ai_optimize_rust_code(
    rust_code_path: &Path,
    original_c_path: &Path,
    output_dir: &Path,
    compile_errors: Option<&str>,
) -> Result<String> {
    info!("开始 AI 第二阶段代码优化: {:?}", rust_code_path);

    // 读取 C2Rust 生成的代码
    let c2rust_code = fs::read_to_string(rust_code_path)?;
    info!("C2Rust 生成的代码长度: {} 字符", c2rust_code.len());

    // 读取原始 C 代码用于参考
    let original_c_code = fs::read_to_string(original_c_path)?;

    // 创建数据库管理器
    let db_manager = DatabaseManager::new_default().await?;

    // 创建 PromptBuilder
    let prompt_builder = PromptBuilder::new(
        &db_manager,
        "c_project".to_string(),
        Some(original_c_path.to_path_buf()),
    )
    .await?;

    // 构建优化提示词
    let base_prompt = prompt_builder
        .build_file_context_prompt(original_c_path, None)
        .await?;

    // 生成按函数分片的消息，避免超限
    let intro = if let Some(errors) = compile_errors {
        format!(
            "{base}\n\n--- 两阶段翻译任务（分片传输 + 编译错误修复）---\n上一次编译失败，错误如下：\n```\n{errors}\n```\n\n我将分片发送'原始 C 代码'与'C2Rust 生成的 Rust 代码'，请在接收完所有片段后：\n- 修复上述编译错误\n- 合理移除 unsafe\n- 优化所有权与错误处理\n- 使用惯用数据结构\n- 输出完整、可编译且等价的 Rust 代码\n",
            base = base_prompt,
            errors = errors
        )
    } else {
        format!(
            "{base}\n\n--- 两阶段翻译任务（分片传输）---\n我将分片发送'原始 C 代码'与'C2Rust 生成的 Rust 代码'，请在接收完所有片段后：\n- 合理移除 unsafe\n- 优化所有权与错误处理\n- 使用惯用数据结构\n- 输出完整、可编译且等价的 Rust 代码\n",
            base = base_prompt
        )
    };

    let mut messages: Vec<String> = Vec::new();
    messages.push(intro);

    // C 原始代码分片
    let mut c_msgs = make_messages_with_function_chunks(
        "",
        "原始 C 代码",
        &original_c_code,
        true,
        MAX_TOTAL_PROMPT_CHARS,
    );
    messages.append(&mut c_msgs);

    // C2Rust 生成的 Rust 分片
    let mut r_msgs = make_messages_with_function_chunks(
        "",
        "C2Rust 生成的 Rust 代码（第一阶段）",
        &c2rust_code,
        false,
        MAX_TOTAL_PROMPT_CHARS,
    );
    messages.append(&mut r_msgs);

    // 收尾说明
    messages.push(
        "当你已接收完全部片段，请一次性输出优化后的完整 Rust 代码（单个 main.rs 或必要的模块），确保可编译。".to_string()
    );

    // 如果没有传入编译错误，尝试从文件读取（向后兼容）
    if compile_errors.is_none() {
        let compile_log_path = output_dir.join("compile_output.txt");
        if compile_log_path.exists() {
            if let Ok(compile_output) = fs::read_to_string(&compile_log_path) {
                info!(
                    "检测到编译输出（{} 字符），尝试附加到优化提示中",
                    compile_output.len()
                );
                
                let remain = MAX_TOTAL_PROMPT_CHARS.saturating_sub(total_len(&messages));
                let mut body = compile_output;
                
                if body.len() + 256 > remain {
                    let target = remain.saturating_sub(256).max(2000);
                    info!("编译输出超限，调用概括以压缩到约 {} 字符", target);
                    body = summarize_text_if_needed(&body, target).await.unwrap_or_else(|_| {
                        warn!("概括失败，退化为截断");
                        if target < body.len() {
                            body[..target].to_string()
                        } else {
                            body.clone()
                        }
                    });
                }
                
                messages = append_text_with_limit(
                    messages,
                    "上一次编译输出/错误",
                    &body,
                    MAX_TOTAL_PROMPT_CHARS,
                )
                .await;
            }
        }
    }

    info!(
        "AI 优化消息条数: {}，总长度: {} 字符",
        messages.len(),
        total_len(&messages)
    );

    // 调用 LLM 进行优化
    info!("调用 LLM 进行代码优化");
    let timeout_duration = Duration::from_secs(6000); // 100分钟超时

    let llm_response = match timeout(
        timeout_duration,
        llm_request_with_prompt(
            messages,
            "你是一位 Rust 专家，擅长优化 C2Rust 生成的代码，使其更加安全、高效和符合 Rust 惯用写法".to_string(),
        ),
    ).await {
        Ok(Ok(response)) => {
            info!("AI 优化响应接收成功，长度: {} 字符", response.len());
            response
        }
        Ok(Err(e)) => {
            error!("AI 优化请求失败: {}", e);
            return Err(e);
        }
        Err(_) => {
            let error_msg = "AI 优化请求超时";
            error!("{}", error_msg);
            let timeout_path = output_dir.join("ai_optimization_timeout.txt");
            fs::write(timeout_path, error_msg)?;
            return Err(anyhow::anyhow!(error_msg));
        }
    };

    // 处理 AI 响应并提取优化后的代码
    let optimized_code = process_llm_response(&llm_response, output_dir)?;

    info!("AI 代码优化完成");
    Ok(optimized_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_llm_response_rust_block() {
        let response = r#"
Here is the optimized code:

```rust
fn main() {
    println!("Hello, world!");
}
```

This is better.
"#;
        let result = process_llm_response(response, Path::new("/tmp"));
        assert!(result.is_ok());
        let code = result.unwrap();
        assert!(code.contains("println!"));
        assert!(!code.contains("```"));
    }
}
