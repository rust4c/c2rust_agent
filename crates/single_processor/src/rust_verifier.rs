use anyhow::Result;
use log::{error, info, warn};
use rust_checker::RustCodeCheck;
use std::path::Path;

/// 编译验证和修复
///
/// 使用 rust_checker 编译项目，自动检测是否为 workspace 并选择合适的编译方式
/// 成功返回 Ok(())，失败返回编译错误信息
pub fn verify_compilation(project_path: &Path) -> Result<()> {
    info!("开始编译验证: {:?}", project_path);

    let checker = RustCodeCheck::new(project_path);

    // 自动检测是否为 workspace
    let result = if checker.is_workspace() {
        info!("检测到 workspace 项目，使用 workspace 构建");
        checker.check_workspace()
    } else {
        info!("检测到单项目，使用常规构建");
        checker.check_rust_project()
    };

    match result {
        Ok(()) => {
            info!("✅ 编译验证通过");
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("编译失败: {}", e);
            warn!("❌ 编译验证失败");
            error!("错误详情: {}", error_msg);
            Err(anyhow::anyhow!(error_msg))
        }
    }
}

/// 带重试的编译验证和修复
///
/// 编译项目，如果失败则返回错误信息供 AI 修复，最多重试 max_retries 次
///
/// # 参数
/// * `project_path` - Rust 项目路径
/// * `max_retries` - 最大重试次数
///
/// # 返回
/// * `Ok(())` - 编译成功
/// * `Err(error)` - 达到最大重试次数仍失败
pub fn verify_and_fix(project_path: &Path, max_retries: u32) -> Result<()> {
    for attempt in 1..=max_retries {
        info!("第 {}/{} 次编译尝试", attempt, max_retries);

        match verify_compilation(project_path) {
            Ok(_) => {
                info!("🎉 编译成功（尝试 {}/{}）", attempt, max_retries);
                return Ok(());
            }
            Err(e) => {
                if attempt < max_retries {
                    warn!("编译失败（尝试 {}/{}），准备重试", attempt, max_retries);
                    warn!("错误详情: {}", e);
                } else {
                    error!("编译失败，已达最大重试次数 {}", max_retries);
                    return Err(anyhow::anyhow!(
                        "编译验证失败（{} 次尝试）: {}",
                        max_retries,
                        e
                    ));
                }
            }
        }
    }

    Err(anyhow::anyhow!("编译验证失败"))
}

/// 提取编译错误的关键信息
///
/// 从编译输出中提取最重要的错误信息，过滤掉重复和无关信息
pub fn extract_key_errors(error_output: &str) -> String {
    let lines: Vec<&str> = error_output.lines().collect();
    let mut key_errors = Vec::new();

    for line in lines {
        // 提取 error[E0xxx] 类型的错误
        if line.contains("error[E") || line.contains("error:") {
            key_errors.push(line);
        }
        // 提取具体的错误位置和提示
        else if line.trim().starts_with("-->") || line.trim().starts_with("|") {
            if let Some(last) = key_errors.last() {
                if !last.is_empty() {
                    key_errors.push(line);
                }
            }
        }
    }

    if key_errors.is_empty() {
        error_output.to_string()
    } else {
        key_errors.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_key_errors() {
        let output = r#"
   Compiling test-project v0.1.0
warning: unused variable: `x`
error[E0425]: cannot find value `undefined_var` in this scope
  --> src/main.rs:10:5
   |
10 |     undefined_var
   |     ^^^^^^^^^^^^^ not found in this scope

error: aborting due to previous error
"#;
        let extracted = extract_key_errors(output);
        assert!(extracted.contains("error[E0425]"));
        assert!(extracted.contains("undefined_var"));
    }
}
