use anyhow::Result;
use log::{error, info, warn};
use rust_checker::RustCodeCheck;
use std::path::Path;

/// Compilation verification and fixing
///
/// Use rust_checker to compile project, auto-detect if it's a workspace and choose appropriate compilation method
/// Returns Ok(()) on success, returns compilation error info on failure
pub fn verify_compilation(project_path: &Path) -> Result<()> {
    info!("Starting compilation verification: {:?}", project_path);

    let checker = RustCodeCheck::new(project_path);

    // Auto-detect if it's a workspace
    let result = if checker.is_workspace() {
        info!("Detected workspace project, using workspace build");
        checker.check_workspace()
    } else {
        info!("Detected single project, using regular build");
        checker.check_rust_project()
    };

    match result {
        Ok(()) => {
            info!("✅ Compilation verification passed");
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Compilation failed: {}", e);
            warn!("❌ Compilation verification failed");
            error!("Error details: {}", error_msg);
            Err(anyhow::anyhow!(error_msg))
        }
    }
}

/// Compilation verification and fixing with retry
///
/// Compile project, if failed return error info for AI fixing, retry at most max_retries times
///
/// # Arguments
/// * `project_path` - Rust project path
/// * `max_retries` - Maximum retry attempts
///
/// # Returns
/// * `Ok(())` - Compilation successful
/// * `Err(error)` - Still failed after reaching maximum retry attempts
pub fn verify_and_fix(project_path: &Path, max_retries: u32) -> Result<()> {
    for attempt in 1..=max_retries {
        info!("Compilation attempt {}/{}", attempt, max_retries);

        match verify_compilation(project_path) {
            Ok(_) => {
                info!(
                    "🎉 Compilation successful (attempt {}/{})",
                    attempt, max_retries
                );
                return Ok(());
            }
            Err(e) => {
                if attempt < max_retries {
                    warn!(
                        "Compilation failed (attempt {}/{}), preparing to retry",
                        attempt, max_retries
                    );
                    warn!("Error details: {}", e);
                } else {
                    error!(
                        "Compilation failed, reached maximum retry attempts {}",
                        max_retries
                    );
                    return Err(anyhow::anyhow!(
                        "Compilation verification failed ({} attempts): {}",
                        max_retries,
                        e
                    ));
                }
            }
        }
    }

    Err(anyhow::anyhow!("Compilation verification failed"))
}

/// Extract key information from compilation errors
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
