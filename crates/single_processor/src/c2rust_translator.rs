use anyhow::Result;
use log::{debug, error, info};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// C2Rust first stage translation
///
/// Use C2Rust tool to automatically translate C code to Rust
///
/// # Arguments
/// * `dir_path` - Directory containing C source files
/// * `output_dir` - Output directory where generated Rust code will be placed
///
/// # Returns
/// Path to the generated main Rust file
pub async fn c2rust_translate(dir_path: &Path, output_dir: &Path) -> Result<PathBuf> {
    info!("Starting C2Rust first stage translation: {:?}", dir_path);
    debug!("dir_path: {:?}, output_dir: {:?}", dir_path, output_dir);

    // Ensure output directory exists
    fs::create_dir_all(output_dir)?;

    // Collect .c and .h source files in directory (non-recursive)
    let mut sources: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension() {
                match ext.to_str() {
                    Some("c") | Some("h") => sources.push(p),
                    _ => {}
                }
            }
        }
    }

    if sources.is_empty() {
        return Err(anyhow::anyhow!(
            "No convertible .c/.h source files found in directory: {}",
            dir_path.display()
        ));
    }

    info!("Will convert {} source files", sources.len());

    // Run C2Rust conversion (simple mode: directly pass source file list)
    info!("Executing C2Rust conversion command (simple mode)...");
    let mut cmd = Command::new("c2rust");
    cmd.arg("transpile")
        .arg("*.c")
        .arg("--output-dir")
        .arg(output_dir)
        .args(&sources)
        .current_dir(output_dir);

    let output = cmd.output();

    match output {
        Ok(result) => {
            if result.status.success() {
                info!("C2Rust conversion successful");
                debug!("C2Rust stdout: {}", String::from_utf8_lossy(&result.stdout));

                // Find generated Rust files
                let rust_main_path = output_dir.join("src").join("main.rs");
                if rust_main_path.exists() {
                    Ok(rust_main_path)
                } else {
                    // Try to find other possible Rust files
                    let src_dir = output_dir.join("src");
                    if src_dir.exists() {
                        for entry in fs::read_dir(&src_dir)? {
                            let entry = entry?;
                            let path = entry.path();
                            if path.extension().map_or(false, |ext| ext == "rs") {
                                info!("Found generated Rust file: {:?}", path);
                                return Ok(path);
                            }
                        }
                    }

                    Err(anyhow::anyhow!(
                        "C2Rust conversion completed, but no generated Rust files found"
                    ))
                }
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                error!("C2Rust conversion failed: {}", stderr);
                Err(anyhow::anyhow!("C2Rust conversion failed: {}", stderr))
            }
        }
        Err(e) => {
            error!("Failed to execute C2Rust command: {}", e);
            Err(anyhow::anyhow!("Failed to execute C2Rust command: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c2rust_translate_checks_sources() {
        // Test empty directory case
        use tempfile::tempdir;
        let temp_dir = tempdir().unwrap();
        let output_dir = temp_dir.path().join("output");

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(c2rust_translate(temp_dir.path(), &output_dir));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No convertible"));
    }
}
