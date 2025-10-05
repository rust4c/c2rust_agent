/// 检测当前目录下的项目类型（lib 或 package）
/// 规则：
/// - 如果有 main 函数，判定为 package
/// - 如果有 pub fn/struct/trait 且无 main，判定为 lib
/// - 只有一个文件时，优先根据内容判断
pub fn detect_rust_file_type(rust_code: &str) -> RustFileType {
    if rust_code.contains("fn main(") {
        RustFileType::Package
    } else if rust_code.contains("pub fn")
        || rust_code.contains("pub struct")
        || rust_code.contains("pub trait")
    {
        RustFileType::Lib
    } else {
        // 默认按 package 处理
        RustFileType::Package
    }
}

/// Rust 文件类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustFileType {
    Lib,
    Package,
}
use anyhow::Result;
use log::{debug, info};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// 处理文件夹中的 .c 和 .h 文件
///
/// 根据文件组合情况进行不同处理：
/// - 只有 1 个 .h 文件：创建对应的 .c 文件
/// - 1 个 .c + 1 个 .h：将 .h 内容写入 .c 文件开头
/// - 只有 1 个 .c 文件：直接返回
///
/// # 返回
/// 处理后的 C 文件路径
pub fn process_c_h_files(dir_path: &Path) -> Result<PathBuf> {
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
        info!("处理完成");
        return Ok(c_files[0].clone());
    }

    // 其他情况返回错误
    Err(anyhow::anyhow!(
        "不支持的文件组合: {}个.c文件, {}个.h文件",
        c_files.len(),
        h_files.len()
    ))
}

/// 创建 Rust 项目结构，并根据类型自动命名 src 文件
pub fn create_rust_project_structure_with_type(
    project_path: &Path,
    rust_code: &str,
) -> Result<PathBuf> {
    info!("创建Rust项目结构，路径: {:?}", project_path);

    // 创建项目目录
    fs::create_dir_all(project_path.join("src"))?;

    // 检测类型
    let file_type = detect_rust_file_type(rust_code);
    let file_name = match file_type {
        RustFileType::Package => "main.rs",
        RustFileType::Lib => "lib.rs",
    };
    let rust_file_path = project_path.join("src").join(file_name);

    // 写入 Rust 代码
    let mut rust_file = File::create(&rust_file_path)?;
    rust_file.write_all(rust_code.as_bytes())?;

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

    info!(
        "已创建 Rust 项目结构: {}，src文件: {}",
        project_path.display(),
        file_name
    );
    Ok(rust_file_path)
}
