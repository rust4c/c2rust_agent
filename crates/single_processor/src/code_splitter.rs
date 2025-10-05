use log::warn;
use regex::Regex;

// 为了避免超出大模型上下文限制，定义合理的字符上限
pub const MAX_TOTAL_PROMPT_CHARS: usize = 120_000; // 单次请求总体字符上限
pub const MAX_SINGLE_MESSAGE_CHARS: usize = 30_000; // 单条消息建议上限

/// 统计消息总长度
pub fn total_len(msgs: &[String]) -> usize {
    msgs.iter().map(|s| s.len()).sum()
}

/// 计算在给定总上限下，当前消息列表的剩余预算
pub fn remaining_budget(current: &[String], max_total: usize) -> usize {
    max_total.saturating_sub(total_len(current))
}

/// 在不超过总上限的前提下，依次附加一批消息，超出即停止
pub fn append_messages_with_budget(dest: &mut Vec<String>, to_add: Vec<String>, max_total: usize) {
    let mut used = total_len(dest);
    for m in to_add {
        let len = m.len();
        if used + len <= max_total {
            dest.push(m);
            used += len;
        } else {
            break;
        }
    }
}

/// 简单的 C 函数切分
///
/// 使用正则找到函数起始位置，然后按大括号平衡截取
/// 注意：这是启发式实现，不能覆盖所有 C 语法边界，但对常见项目足够实用
pub fn split_c_code_by_function(code: &str) -> Vec<String> {
    let mut chunks = Vec::new();

    // 匹配 C 函数定义起始行的粗略正则（排除声明行与原型行）
    let re = Regex::new(r"(?m)^[ \t]*[A-Za-z_][A-Za-z0-9_\s\*\(\)\[\],]*?[\s\*]+[A-Za-z_][A-Za-z0-9_]*\s*\([^;]*?\)\s*\{").ok();

    let Some(re) = re else {
        return vec![code.to_string()];
    };

    let indices: Vec<usize> = re.find_iter(code).map(|m| m.start()).collect();

    if indices.is_empty() {
        return vec![code.to_string()];
    }

    // 文件头（includes、typedef、全局变量）作为前导部分
    let header_end = indices[0];
    let header = code[..header_end].trim();
    if !header.is_empty() {
        chunks.push(header.to_string());
    }

    // 逐个函数块，使用括号计数找到函数体结束
    let bytes = code.as_bytes();
    for (i, &start) in indices.iter().enumerate() {
        let end_bound = indices.get(i + 1).copied().unwrap_or(code.len());

        // 从"{"开始做括号匹配
        let mut brace_pos = None;
        for pos in start..end_bound.min(code.len()) {
            if bytes[pos] as char == '{' {
                brace_pos = Some(pos);
                break;
            }
        }

        let Some(mut pos) = brace_pos else {
            chunks.push(code[start..end_bound].to_string());
            continue;
        };

        let mut depth = 0i32;
        while pos < code.len() {
            let ch = bytes[pos] as char;
            if ch == '{' {
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    pos += 1;
                    break;
                }
            }
            pos += 1;
            if pos >= end_bound && depth <= 0 {
                break;
            }
        }

        let end = pos.min(code.len());
        chunks.push(code[start..end].to_string());
    }

    chunks
}

/// 粗略分割 Rust 代码为函数块
///
/// 以 fn 开头并带 { 的定义作为切分点
pub fn split_rust_code_by_function(code: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let re = Regex::new(r"(?m)^[ \t]*pub\s+fn\s+[A-Za-z_][A-Za-z0-9_]*\s*\(|(?m)^[ \t]*fn\s+[A-Za-z_][A-Za-z0-9_]*\s*\(").ok();

    let Some(re) = re else {
        return vec![code.to_string()];
    };

    let indices: Vec<usize> = re.find_iter(code).map(|m| m.start()).collect();
    if indices.is_empty() {
        return vec![code.to_string()];
    }

    // 头部（use、mod、struct/enum 等）
    let header_end = indices[0];
    let header = code[..header_end].trim();
    if !header.is_empty() {
        chunks.push(header.to_string());
    }

    let bytes = code.as_bytes();
    for (i, &start) in indices.iter().enumerate() {
        let end_bound = indices.get(i + 1).copied().unwrap_or(code.len());

        // 从 start 开始找第一个 "{" 进行匹配
        let mut brace_pos = None;
        for pos in start..end_bound.min(code.len()) {
            if bytes[pos] as char == '{' {
                brace_pos = Some(pos);
                break;
            }
        }

        let Some(mut pos) = brace_pos else {
            chunks.push(code[start..end_bound].to_string());
            continue;
        };

        let mut depth = 0i32;
        while pos < code.len() {
            let ch = bytes[pos] as char;
            if ch == '{' {
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    pos += 1;
                    break;
                }
            }
            pos += 1;
            if pos >= end_bound && depth <= 0 {
                break;
            }
        }

        let end = pos.min(code.len());
        chunks.push(code[start..end].to_string());
    }
    chunks
}

/// 在给定总长度限制下，把一段较长的文本按函数块拆分并构造多条消息
pub fn make_messages_with_function_chunks(
    prefix: &str,
    title: &str,
    code: &str,
    is_c: bool,
    max_total: usize,
) -> Vec<String> {
    fn push_msg(messages: &mut Vec<String>, s: String, budget: &mut usize) {
        if s.is_empty() {
            return;
        }
        let len = s.len();
        if len <= MAX_SINGLE_MESSAGE_CHARS && len <= *budget {
            *budget = budget.saturating_sub(len);
            messages.push(s);
        }
    }

    let mut messages = Vec::new();
    let mut budget = max_total;

    let intro = format!(
        "{prefix}\n\n--- {title}（分片传输）---\n说明：为避免超过上下文限制，以下内容按函数块拆分为多条消息。请在收到所有片段后再进行处理。\n",
        prefix = prefix,
        title = title,
    );
    push_msg(&mut messages, intro, &mut budget);

    let chunks = if is_c {
        split_c_code_by_function(code)
    } else {
        split_rust_code_by_function(code)
    };

    for (idx, chunk) in chunks.iter().enumerate() {
        if budget < 1024 {
            break;
        }
        let mut start = 0;
        while start < chunk.len() {
            let end = (start + MAX_SINGLE_MESSAGE_CHARS.min(budget)).min(chunk.len());
            let piece = &chunk[start..end];
            let msg = format!(
                "[片段 {}/{}]```{lang}\n{}\n```",
                idx + 1,
                chunks.len(),
                piece,
                lang = if is_c { "c" } else { "rust" }
            );
            push_msg(&mut messages, msg, &mut budget);
            if end == chunk.len() {
                break;
            }
            start = end;
        }
    }

    messages
}

/// 估算附加文本后是否会超过限制
pub fn will_exceed_limit(messages: &[String], extra: &str, max_total: usize) -> bool {
    total_len(messages) + extra.len() > max_total
}

/// 将一段文本以标题方式附加到消息列表
///
/// 若超过限制，则进行截断（调用方应在需要时先调用 summarize）
pub async fn append_text_with_limit(
    mut messages: Vec<String>,
    title: &str,
    text: &str,
    max_total: usize,
) -> Vec<String> {
    if text.trim().is_empty() {
        return messages;
    }

    let _remain = max_total.saturating_sub(total_len(&messages));
    let body = text.to_string();

    let msg = format!("--- {} ---\n```\n{}\n```", title, body);

    if total_len(&messages) + msg.len() <= max_total {
        messages.push(msg);
    } else {
        warn!("附加 '{}' 超限，执行截断", title);
        let budget = max_total
            .saturating_sub(total_len(&messages))
            .saturating_sub(64);
        let mut truncated = msg;
        if truncated.len() > budget {
            truncated.truncate(budget);
        }
        messages.push(truncated);
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_c_code() {
        let code = r#"
#include <stdio.h>

int add(int a, int b) {
    return a + b;
}

int main() {
    printf("%d\n", add(1, 2));
    return 0;
}
"#;
        let chunks = split_c_code_by_function(code);
        assert!(chunks.len() >= 2); // header + functions
    }

    #[test]
    fn test_total_len() {
        let msgs = vec!["hello".to_string(), "world".to_string()];
        assert_eq!(total_len(&msgs), 10);
    }
}
