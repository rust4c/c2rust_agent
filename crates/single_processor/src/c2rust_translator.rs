use anyhow::Result;
use log::{debug, error, info};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// C2Rust 第一阶段翻译
/// 
/// 使用 C2Rust 工具自动翻译 C 代码到 Rust
/// 
/// # 参数
/// * `dir_path` - 包含 C 源文件的目录
/// * `output_dir` - 输出目录，生成的 Rust 代码将放在这里
/// 
/// # 返回
/// 生成的 Rust 主文件路径
pub async fn c2rust_translate(dir_path: &Path, output_dir: &Path) -> Result<PathBuf> {
    info!("开始 C2Rust 第一阶段翻译: {:?}", dir_path);

    // 确保输出目录存在
    fs::create_dir_all(output_dir)?;

    // 收集目录下的 .c 与 .h 源文件（非递归）
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
            "目录中未找到可转换的 .c/.h 源文件: {}",
            dir_path.display()
        ));
    }

    info!("将转换 {} 个源文件", sources.len());

    // 运行 C2Rust 转换（简易模式：直接传入源文件列表）
    info!("执行 C2Rust 转换命令(简易模式)...");
    let mut cmd = Command::new("c2rust");
    cmd.arg("transpile")
        .arg("--output-dir")
        .arg(output_dir)
        .args(&sources)
        .current_dir(output_dir);

    let output = cmd.output();

    match output {
        Ok(result) => {
            if result.status.success() {
                info!("C2Rust 转换成功");
                debug!("C2Rust stdout: {}", String::from_utf8_lossy(&result.stdout));

                // 查找生成的 Rust 文件
                let rust_main_path = output_dir.join("src").join("main.rs");
                if rust_main_path.exists() {
                    Ok(rust_main_path)
                } else {
                    // 尝试查找其他可能的 Rust 文件
                    let src_dir = output_dir.join("src");
                    if src_dir.exists() {
                        for entry in fs::read_dir(&src_dir)? {
                            let entry = entry?;
                            let path = entry.path();
                            if path.extension().map_or(false, |ext| ext == "rs") {
                                info!("找到生成的 Rust 文件: {:?}", path);
                                return Ok(path);
                            }
                        }
                    }

                    Err(anyhow::anyhow!("C2Rust 转换完成，但未找到生成的 Rust 文件"))
                }
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                error!("C2Rust 转换失败: {}", stderr);
                Err(anyhow::anyhow!("C2Rust 转换失败: {}", stderr))
            }
        }
        Err(e) => {
            error!("执行 C2Rust 命令失败: {}", e);
            Err(anyhow::anyhow!("执行 C2Rust 命令失败: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c2rust_translate_checks_sources() {
        // 测试空目录情况
        use tempfile::tempdir;
        let temp_dir = tempdir().unwrap();
        let output_dir = temp_dir.path().join("output");
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(c2rust_translate(temp_dir.path(), &output_dir));
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("未找到可转换的"));
    }
}
