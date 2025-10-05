//! 测试 JSON 分割和代码提取的边界情况

#[cfg(test)]
mod tests {
    use anyhow::Result;

    // 模拟提取函数（从 single_processes.rs 复制）
    fn extract_rust_code_from_test_response(llm_response: &str) -> Result<String> {
        use log::{info, warn};
        use serde_json::Value;

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

        // 方法2: 处理被代码块包裹的JSON
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

        // 方法3: 提取Rust代码块
        if rust_code.is_none() {
            if let Some(start_idx) = llm_response.find("```rust") {
                let code_start = if llm_response[start_idx..].starts_with("```rust\n") {
                    start_idx + 8
                } else {
                    start_idx + 7
                };

                if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                    let code_end = code_start + end_idx;
                    info!("成功从```rust代码块中提取代码");
                    rust_code = Some(llm_response[code_start..code_end].to_string());
                } else if let Some(end_idx) = llm_response[code_start..].find("```") {
                    let code_end = code_start + end_idx;
                    warn!("从```rust代码块中提取代码（无结束换行符）");
                    rust_code = Some(llm_response[code_start..code_end].to_string());
                }
            } else if let Some(start_idx) = llm_response.find("```\n") {
                let code_start = start_idx + 4;
                if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                    let code_end = code_start + end_idx;
                    info!("成功从通用代码块中提取代码");
                    rust_code = Some(llm_response[code_start..code_end].to_string());
                }
            }
        }

        // 方法4: 从不完整JSON中提取rust_code
        if rust_code.is_none() {
            if let Some(start_pos) = llm_response.find(r#""rust_code""#) {
                if let Some(colon_pos) = llm_response[start_pos..].find(':') {
                    let value_start = start_pos + colon_pos + 1;
                    let remaining = &llm_response[value_start..].trim_start();

                    if remaining.starts_with('"') {
                        let content_start =
                            value_start + (llm_response[value_start..].len() - remaining.len()) + 1;
                        let mut pos = content_start;
                        let bytes = llm_response.as_bytes();
                        let mut escaped = false;

                        while pos < bytes.len() {
                            if escaped {
                                escaped = false;
                            } else if bytes[pos] == b'\\' {
                                escaped = true;
                            } else if bytes[pos] == b'"' {
                                if let Ok(json_str) =
                                    String::from_utf8(bytes[content_start..pos].to_vec())
                                {
                                    let unescaped = json_str
                                        .replace(r"\n", "\n")
                                        .replace(r"\t", "\t")
                                        .replace(r#"\""#, "\"")
                                        .replace(r"\\", "\\");
                                    info!("从不完整JSON中成功提取rust_code字段");
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

    #[test]
    fn test_complete_json() {
        let response = r#"{"rust_code": "fn main() {\n    println!(\"Hello\");\n}"}"#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "fn main() {\n    println!(\"Hello\");\n}");
    }

    #[test]
    fn test_json_with_markdown_wrapper() {
        let response = r#"```json
{"rust_code": "fn main() {\n    println!(\"Hello\");\n}"}
```"#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "fn main() {\n    println!(\"Hello\");\n}");
    }

    #[test]
    fn test_rust_code_block() {
        let response = r#"```rust
fn main() {
    println!("Hello");
}
```"#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("fn main()"));
    }

    #[test]
    fn test_rust_code_block_no_newline() {
        let response = r#"```rustfn main() {
    println!("Hello");
}```"#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("fn main()"));
    }

    #[test]
    fn test_incomplete_json() {
        // 模拟JSON在字符串中间被截断的情况
        let response = r#"{"rust_code": "fn main() {\n    println!(\"Hello, World!\");\n}", "key_changes": ["将C宏转换为Rust常量""#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("fn main()"));
    }

    #[test]
    fn test_partial_json_field() {
        // 只有rust_code字段的片段
        let response = r#""rust_code": "pub const FLT_MAX: f32 = f32::MAX;""#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("FLT_MAX"));
    }

    #[test]
    fn test_openai_format() {
        let response =
            r#"{"choices": [{"message": {"content": "fn main() {\n    println!(\"test\");\n}"}}]}"#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("fn main()"));
    }

    #[test]
    fn test_plain_text_fallback() {
        let response = "fn main() { println!(\"直接文本\"); }";
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), response);
    }

    #[test]
    fn test_generic_code_block() {
        let response = r#"```
fn test() {
    // code
}
```"#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("fn test()"));
    }

    #[test]
    fn test_escaped_quotes_in_json() {
        let response =
            r#"{"rust_code": "fn test() {\n    let s = \"escaped \\\"quotes\\\"\";\n}"}"#;
        let result = extract_rust_code_from_test_response(response);
        assert!(result.is_ok());
        let code = result.unwrap();
        assert!(code.contains("escaped"));
    }
}
