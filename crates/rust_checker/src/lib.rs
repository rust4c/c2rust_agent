use log::{error, info};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RustCheckError {
    #[error("Project directory does not exist")]
    InvalidProjectDir,
    #[error("No Cargo.toml found in project directory")]
    NotCargoProject,
    #[error("Build failed with status {0}: {1}")]
    BuildFailed(i32, String),
    #[error("Command execution error: {0}")]
    CommandError(String),
}

pub struct RustCodeCheck {
    project_dir: PathBuf,
}

impl RustCodeCheck {
    pub fn new(project_dir: impl AsRef<Path>) -> Self {
        Self {
            project_dir: project_dir.as_ref().to_path_buf(),
        }
    }

    /// 检查 Rust 项目并尝试编译
    pub fn check_rust_project(&self) -> Result<(), RustCheckError> {
        info!("Checking Rust project at: {:?}", self.project_dir);

        // 验证项目目录是否存在
        if !self.project_dir.exists() {
            error!("Project directory does not exist: {:?}", self.project_dir);
            return Err(RustCheckError::InvalidProjectDir);
        }

        // 检查 Cargo.toml 是否存在
        let cargo_toml = self.project_dir.join("Cargo.toml");
        if !cargo_toml.exists() {
            error!("No Cargo.toml found in project directory");
            return Err(RustCheckError::NotCargoProject);
        }

        // 尝试编译项目
        self.run_cargo_build()
    }

    /// 执行 cargo build 命令
    fn run_cargo_build(&self) -> Result<(), RustCheckError> {
        info!("Attempting to build project with cargo...");

        let output = Command::new("cargo")
            .arg("build")
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| {
                let msg = format!("Failed to execute command: {}", e);
                error!("{}", msg);
                RustCheckError::CommandError(msg)
            })?;

        if output.status.success() {
            info!("Build succeeded");
            Ok(())
        } else {
            let status = output.status.code().unwrap_or(-1);
            let output_str = String::from_utf8_lossy(&output.stderr);
            let error_msg = output_str.trim().to_string();

            error!("Build failed with status {}:\n{}", status, error_msg);

            Err(RustCheckError::BuildFailed(status, error_msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_valid_project() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();

        // 创建 Cargo.toml
        let mut cargo_file = File::create(project_path.join("Cargo.toml")).unwrap();
        cargo_file
            .write_all(b"[package]\nname = \"test\"\nversion = \"0.1.0\"")
            .unwrap();

        // 创建有效的源文件
        let mut src_dir = project_path.join("src");
        std::fs::create_dir(&src_dir).unwrap();
        let mut lib_file = File::create(src_dir.join("main.rs")).unwrap();
        lib_file.write_all(b"fn main() {}").unwrap();

        let checker = RustCodeCheck::new(project_path);
        assert!(checker.check_rust_project().is_ok());
    }

    #[test]
    fn test_missing_cargo_toml() {
        let temp_dir = TempDir::new().unwrap();
        let checker = RustCodeCheck::new(temp_dir.path());

        match checker.check_rust_project().unwrap_err() {
            RustCheckError::NotCargoProject => (),
            _ => panic!("Expected NotCargoProject error"),
        }
    }

    #[test]
    fn test_invalid_project_dir() {
        let checker = RustCodeCheck::new("/non/existent/path");

        match checker.check_rust_project().unwrap_err() {
            RustCheckError::InvalidProjectDir => (),
            _ => panic!("Expected InvalidProjectDir error"),
        }
    }

    #[test]
    fn test_build_failure() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();

        // 创建无效的 Cargo.toml
        let mut cargo_file = File::create(project_path.join("Cargo.toml")).unwrap();
        cargo_file.write_all(b"invalid toml content").unwrap();

        let checker = RustCodeCheck::new(project_path);
        assert!(matches!(
            checker.check_rust_project().unwrap_err(),
            RustCheckError::BuildFailed(_, _)
        ));
    }
}
