/// Detect project type in current directory (lib or package)
/// Rules:
/// - If there's a main function, classify as package
/// - If there are pub fn/struct/trait but no main, classify as lib
/// - For single file, prioritize content-based classification
pub fn detect_rust_file_type(rust_code: &str) -> RustFileType {
    if rust_code.contains("fn main(") {
        RustFileType::Package
    } else if rust_code.contains("pub fn")
        || rust_code.contains("pub struct")
        || rust_code.contains("pub trait")
    {
        RustFileType::Lib
    } else {
        // Default to package handling
        RustFileType::Package
    }
}

/// Rust file type enumeration
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
use std::process::Command;

/// Process .c and .h files in folder
///
/// Handle differently based on file combinations:
/// - Only 1 .h file: create corresponding .c file
/// - 1 .c + 1 .h: write .h content to beginning of .c file
/// - Only 1 .c file: return directly
///
/// # Returns
/// Path to the processed C file
pub fn process_c_h_files(dir_path: &Path) -> Result<PathBuf> {
    info!("Starting C/H file processing, path: {:?}", dir_path);

    let mut c_files = Vec::new();
    let mut h_files = Vec::new();

    // Read files in directory
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
        "Found {} .c files and {} .h files",
        c_files.len(),
        h_files.len()
    );

    // Handle based on file situation
    if c_files.is_empty() && h_files.is_empty() {
        return Err(anyhow::anyhow!("No .c or .h files found in directory"));
    }

    // If only .h files, create corresponding .c file
    if c_files.is_empty() && h_files.len() == 1 {
        let h_file = &h_files[0];
        let c_file_path = h_file.with_extension("c");

        info!(
            "Only .h file found, creating corresponding .c file: {:?}",
            c_file_path
        );

        // Read .h file content
        let mut h_content = String::new();
        File::open(h_file)?.read_to_string(&mut h_content)?;

        // Write to .c file
        let mut c_file = File::create(&c_file_path)?;
        c_file.write_all(h_content.as_bytes())?;

        info!("Written .h file content to newly created .c file");
        return Ok(c_file_path);
    }

    // If there's one .c file and one .h file, write .h content to beginning of .c file
    if c_files.len() == 1 && h_files.len() == 1 {
        let c_file = &c_files[0];
        let h_file = &h_files[0];

        info!("Found one .c file and one .h file, writing .h content to beginning of .c file");

        // Read .h file content
        let mut h_content = String::new();
        File::open(h_file)?.read_to_string(&mut h_content)?;
        debug!("h_content: {}", h_content);

        // Read existing .c file content
        let mut c_content = String::new();
        File::open(c_file)?.read_to_string(&mut c_content)?;
        debug!("c_content: {}", c_content);

        // Write .h content to beginning of .c file
        let mut file = File::create(c_file)?;
        write!(file, "{}{}", h_content, c_content)?;

        info!("Written .h file content to beginning of .c file");
        return Ok(c_file.clone());
    }

    // If only one .c file, no processing needed
    if c_files.len() == 1 && h_files.is_empty() {
        info!("Processing completed");
        return Ok(c_files[0].clone());
    }

    // Return error for other cases
    Err(anyhow::anyhow!(
        "Unsupported file combination: {} .c files, {} .h files",
        c_files.len(),
        h_files.len()
    ))
}

/// Create Rust project structure and automatically name src files based on type
pub fn create_rust_project_structure_with_type(
    project_path: &Path,
    rust_code: &str,
) -> Result<PathBuf> {
    info!("Creating Rust project structure, path: {:?}", project_path);

    // Create project directory
    fs::create_dir_all(project_path.join("src"))?;

    // Detect type
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

/// Determine project type based on C file content:
/// - Contains `main(` considered executable (Package/bin)
/// - Otherwise considered library (Lib)
pub fn detect_project_type_from_c(c_file: &Path) -> RustFileType {
    if let Ok(mut s) = fs::read_to_string(c_file) {
        // Simple and effective: any form of main( is considered executable entry
        // Avoid over-engineering, maintain robustness
        s.make_ascii_lowercase();
        if s.contains("main(") {
            return RustFileType::Package;
        }
    }
    RustFileType::Lib
}

/// Use `cargo new` to create project skeleton (bin or lib).
/// If target directory already exists, remove first then recreate (create new then delete existing if necessary).
/// Returns path to created src main file (main.rs or lib.rs).
pub fn init_or_recreate_cargo_project(
    project_path: &Path,
    proj_type: RustFileType,
) -> Result<PathBuf> {
    if project_path.exists() {
        info!(
            "Target directory already exists, deleting and rebuilding: {}",
            project_path.display()
        );
        fs::remove_dir_all(project_path)?;
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("new");
    match proj_type {
        RustFileType::Package => {
            cmd.arg("--bin");
        }
        RustFileType::Lib => {
            cmd.arg("--lib");
        }
    }
    cmd.arg(project_path);

    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow::anyhow!("cargo new failed: {}\n{}", stderr, stdout));
    }

    // Return src main file path
    let src_file = match proj_type {
        RustFileType::Package => project_path.join("src").join("main.rs"),
        RustFileType::Lib => project_path.join("src").join("lib.rs"),
    };
    Ok(src_file)
}

/// Write given Rust code to existing cargo project (write to main.rs or lib.rs based on project type).
/// If src file doesn't exist, will create src directory and corresponding file.
pub fn write_rust_code_to_project(
    project_path: &Path,
    rust_code: &str,
    proj_type: RustFileType,
) -> Result<PathBuf> {
    fs::create_dir_all(project_path.join("src"))?;
    let target = match proj_type {
        RustFileType::Package => project_path.join("src").join("main.rs"),
        RustFileType::Lib => project_path.join("src").join("lib.rs"),
    };
    let mut f = File::create(&target)?;
    f.write_all(rust_code.as_bytes())?;
    Ok(target)
}

/// Convenience method: detect project type based on C file, initialize with cargo new, then write Rust code.
pub fn create_cargo_project_with_code_from_c(
    project_path: &Path,
    rust_code: &str,
    c_file: &Path,
) -> Result<PathBuf> {
    let proj_type = detect_project_type_from_c(c_file);
    let _ = init_or_recreate_cargo_project(project_path, proj_type)?;
    write_rust_code_to_project(project_path, rust_code, proj_type)
}
