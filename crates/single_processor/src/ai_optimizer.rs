use anyhow::{Result, anyhow};
use db_services::DatabaseManager;
use llm_requester::llm_request_with_prompt;
use log::{error, info, warn};
use prompt_builder::PromptBuilder;
use serde_json::Value;
use std::path::Path;
use std::{fs, path::PathBuf};
use tokio::time::{Duration, timeout};

use crate::code_splitter::{
    MAX_TOTAL_PROMPT_CHARS, append_messages_with_budget, append_text_with_limit,
    make_messages_with_function_chunks, remaining_budget, total_len,
};

/// 优化结果（结构化），便于后续处理 Cargo 依赖与提示
#[derive(Debug, Clone)]
pub struct OptimizedResult {
    pub rust_code: String,
    pub cargo_crates: Vec<String>,
    pub key_changes: Vec<String>,
    pub warnings: Vec<String>,
}

impl OptimizedResult {}

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

/// 处理LLM响应并提取Rust代码及依赖信息
fn process_llm_response(llm_response: &str, _output_dir: &Path) -> Result<OptimizedResult> {
    info!("处理LLM响应");

    // 尝试多种方式解析响应
    let mut rust_code: Option<String> = None;
    let mut cargo_crates: Vec<String> = vec![];
    let mut key_changes: Vec<String> = vec![];
    let mut warnings_list: Vec<String> = vec![];

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

        // cargo 依赖，形如 "cargo": "xx,yy"
        if let Some(cargo_str) = json_response["cargo"].as_str() {
            cargo_crates = cargo_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        // key_changes 数组
        if let Some(arr) = json_response["key_changes"].as_array() {
            key_changes = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }

        // warnings 数组
        if let Some(arr) = json_response["warnings"].as_array() {
            warnings_list = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
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

            // 同时提取 cargo、key_changes、warnings
            if let Some(cargo_str) = json_response["cargo"].as_str() {
                cargo_crates = cargo_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            if let Some(arr) = json_response["key_changes"].as_array() {
                key_changes = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            }
            if let Some(arr) = json_response["warnings"].as_array() {
                warnings_list = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            }
        }
    }

    // 方法3: 尝试直接提取Rust代码块（多种变体）
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

    // 方法5: 如果以上方法都失败，尝试将整个响应作为代码（兜底）
    if rust_code.is_none() {
        warn!("所有提取方法均失败，将整个响应作为代码保存（兜底处理）");
        rust_code = Some(llm_response.to_string());
    }

    match rust_code {
        Some(code) => Ok(OptimizedResult {
            rust_code: code,
            cargo_crates,
            key_changes,
            warnings: warnings_list,
        }),
        None => Err(anyhow::anyhow!("无法从LLM响应中提取Rust代码")),
    }
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
    rust_code_path: Option<&PathBuf>,
    original_c_path: &Path,
    output_dir: &Path,
    compile_errors: Option<&str>,
) -> Result<OptimizedResult> {
    info!("开始 AI 第二阶段代码优化");

    // 读取 C2Rust 生成的代码
    let c2rust_code = match rust_code_path {
        Some(file_path) => fs::read_to_string(file_path)?,
        None => String::new(),
    };
    info!("C2Rust 生成的代码长度: {} 字符", c2rust_code.len());

    // 读取原始 C 代码用于参考
    let original_c_code = fs::read_to_string(original_c_path)?;

    // 创建数据库管理器
    let db_manager = DatabaseManager::new_default().await?;

    // 创建 PromptBuilder
    let mut prompt_builder = PromptBuilder::new(
        &db_manager,
        "c_project".to_string(),
        Some(original_c_path.to_path_buf()),
    )
    .await?;

    // 如果有编译错误，将其加入 PromptBuilder 的错误上下文中
    if let Some(errs) = compile_errors {
        prompt_builder.add_error_context(errs.to_string());
    }

    // 构建优化提示词
    let base_prompt = prompt_builder
        .build_file_context_prompt(original_c_path, None)
        .await?;

    // 生成按函数分片的消息，避免超限
    let use_c2rust_only = rust_code_path.is_some();
    let intro = if let Some(errors) = compile_errors {
        if use_c2rust_only {
            format!(
                "{base}\n\n--- 两阶段翻译任务（分片传输 + 编译错误修复）---\n上一次编译失败，错误如下：\n```\n{errors}\n```\n\n我将分片发送'C2Rust 生成的 Rust 代码'，请在接收完所有片段后：\n- 修复上述编译错误\n- 合理移除 unsafe\n- 优化所有权与错误处理\n- 使用惯用数据结构\n- 输出完整、可编译且等价的 Rust 代码\n",
                base = base_prompt,
                errors = errors
            )
        } else {
            format!(
                "{base}\n\n--- 两阶段翻译任务（分片传输 + 编译错误修复）---\n上一次编译失败，错误如下：\n```\n{errors}\n```\n\n我将分片发送'原始 C 代码'，请在接收完所有片段后：\n- 修复上述编译错误\n- 合理移除 unsafe\n- 优化所有权与错误处理\n- 使用惯用数据结构\n- 输出完整、可编译且等价的 Rust 代码\n",
                base = base_prompt,
                errors = errors
            )
        }
    } else {
        if use_c2rust_only {
            format!(
                "{base}\n\n--- 两阶段翻译任务（分片传输）---\n我将分片发送'C2Rust 生成的 Rust 代码'，请在接收完所有片段后：\n- 合理移除 unsafe\n- 优化所有权与错误处理\n- 使用惯用数据结构\n- 输出完整、可编译且等价的 Rust 代码\n",
                base = base_prompt
            )
        } else {
            format!(
                "{base}\n\n--- 两阶段翻译任务（分片传输）---\n我将分片发送'原始 C 代码'，请在接收完所有片段后：\n- 合理移除 unsafe\n- 优化所有权与错误处理\n- 使用惯用数据结构\n- 输出完整、可编译且等价的 Rust 代码\n",
                base = base_prompt
            )
        }
    };

    let mut messages: Vec<String> = Vec::new();
    // 确保 intro 不超出总预算
    if intro.len() <= MAX_TOTAL_PROMPT_CHARS {
        messages.push(intro);
    } else {
        let mut intro_trunc = intro.clone();
        intro_trunc.truncate(MAX_TOTAL_PROMPT_CHARS.saturating_sub(64));
        messages.push(intro_trunc);
    }

    if use_c2rust_only {
        // 仅发送 C2Rust 生成的 Rust 分片
        let r_msgs = make_messages_with_function_chunks(
            "",
            "C2Rust 生成的 Rust 代码（第一阶段）",
            &c2rust_code,
            false,
            MAX_TOTAL_PROMPT_CHARS,
        );
        // 避免超出总预算
        append_messages_with_budget(&mut messages, r_msgs, MAX_TOTAL_PROMPT_CHARS);
    } else {
        // 仅发送 原始 C 代码分片
        let c_msgs = make_messages_with_function_chunks(
            "",
            "原始 C 代码",
            &original_c_code,
            true,
            MAX_TOTAL_PROMPT_CHARS,
        );
        append_messages_with_budget(&mut messages, c_msgs, MAX_TOTAL_PROMPT_CHARS);
    }

    // 收尾说明
    let tail = "当你已接收完全部片段，请一次性输出优化后的完整 Rust 代码（单个 main.rs 或必要的模块），确保可编译。".to_string();
    if total_len(&messages) + tail.len() <= MAX_TOTAL_PROMPT_CHARS {
        messages.push(tail);
    }

    // 如果没有传入编译错误，尝试从文件读取（向后兼容）
    if compile_errors.is_none() {
        let compile_log_path = output_dir.join("compile_output.txt");
        if compile_log_path.exists() {
            if let Ok(compile_output) = fs::read_to_string(&compile_log_path) {
                info!(
                    "检测到编译输出（{} 字符），尝试附加到优化提示中",
                    compile_output.len()
                );

                let remain = remaining_budget(&messages, MAX_TOTAL_PROMPT_CHARS);
                let mut body = compile_output;

                if body.len() + 256 > remain {
                    let target = remain.saturating_sub(256).max(2000);
                    info!("编译输出超限，调用概括以压缩到约 {} 字符", target);
                    body = summarize_text_if_needed(&body, target)
                        .await
                        .unwrap_or_else(|_| {
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
    let optimized = process_llm_response(&llm_response, output_dir)?;

    info!("AI 代码优化完成");
    Ok(optimized)
}

/// 在最终验证失败后，请求 AI 分析失败原因并给出修复建议
pub async fn ai_analyze_final_failure(
    original_c_path: &Path,
    latest_rust_path: &Path,
    compile_errors: &str,
) -> Result<String> {
    info!("AI 失败诊断开始: {:?}", latest_rust_path);

    let original_c_code = fs::read_to_string(original_c_path).unwrap_or_else(|e| {
        warn!("读取原始 C 文件失败: {} - {:?}", e, original_c_path);
        String::new()
    });

    let latest_rust_code = fs::read_to_string(latest_rust_path).unwrap_or_else(|e| {
        warn!("读取最新 Rust 文件失败: {} - {:?}", e, latest_rust_path);
        String::new()
    });

    let db_manager = DatabaseManager::new_default().await?;
    let prompt_builder = PromptBuilder::new(
        &db_manager,
        "c_project".to_string(),
        Some(original_c_path.to_path_buf()),
    )
    .await?;

    let base_prompt = prompt_builder
        .build_file_context_prompt(original_c_path, None)
        .await?;

    let mut messages = Vec::new();
    messages.push(format!(
        "{base}\n\n--- 编译失败诊断任务 ---\n我们尝试将 C 代码转换为 Rust，但经过多次优化仍未通过编译。\n请阅读以下编译错误，结合原始 C 与最新 Rust 代码，输出：\n1. 导致失败的核心原因（按重要性排序）\n2. 建议的修复步骤，必要时包含关键代码片段\n3. 如果仍需更多上下文，请明确说明。\n\n编译错误如下：\n```\n{errors}\n```",
        base = base_prompt,
        errors = compile_errors
    ));

    if !original_c_code.is_empty() {
        let c_chunks = make_messages_with_function_chunks(
            "",
            "原始 C 代码",
            &original_c_code,
            true,
            MAX_TOTAL_PROMPT_CHARS,
        );
        append_messages_with_budget(&mut messages, c_chunks, MAX_TOTAL_PROMPT_CHARS);
    }

    if !latest_rust_code.is_empty() {
        let rust_chunks = make_messages_with_function_chunks(
            "",
            "当前 Rust 代码",
            &latest_rust_code,
            false,
            MAX_TOTAL_PROMPT_CHARS,
        );
        append_messages_with_budget(&mut messages, rust_chunks, MAX_TOTAL_PROMPT_CHARS);
    }

    let tail2 =
        "在阅读完所有上下文后，请输出诊断结论与建议，保持结构清晰，便于人工跟进。".to_string();
    if total_len(&messages) + tail2.len() <= MAX_TOTAL_PROMPT_CHARS {
        messages.push(tail2);
    }

    let timeout_duration = Duration::from_secs(1200);
    let response = match timeout(
        timeout_duration,
        llm_request_with_prompt(
            messages,
            "你是一位资深 Rust 构建优化专家，擅长定位和修复复杂的编译问题".to_string(),
        ),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            let error_msg = "AI 失败诊断请求超时";
            return Err(anyhow!(error_msg));
        }
    };

    info!("AI 失败诊断完成");
    Ok(response)
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
        let out = result.unwrap();
        assert!(out.rust_code.contains("println!"));
        assert!(!out.rust_code.contains("```"));
        assert!(out.cargo_crates.is_empty());
    }
}
