use log::{error, info, warn};
use std::fs;
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
            .arg("--color=never")
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // 合并 stdout 和 stderr（cargo 可能在两者中都输出）
            let mut error_msg = String::new();
            if !stdout.is_empty() {
                error_msg.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !error_msg.is_empty() {
                    error_msg.push('\n');
                }
                error_msg.push_str(&stderr);
            }

            let error_msg = error_msg.trim().to_string();
            error!("Build failed with status {}:\n{}", status, error_msg);

            Err(RustCheckError::BuildFailed(status, error_msg))
        }
    }

    /// 检查是否为 workspace 项目
    pub fn is_workspace(&self) -> bool {
        let cargo_toml = self.project_dir.join("Cargo.toml");
        if let Ok(content) = fs::read_to_string(&cargo_toml) {
            content.contains("[workspace]")
        } else {
            false
        }
    }

    /// 获取 workspace 成员
    pub fn get_workspace_members(&self) -> Result<Vec<String>, RustCheckError> {
        let cargo_toml = self.project_dir.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml).map_err(|e| {
            RustCheckError::CommandError(format!("Failed to read Cargo.toml: {}", e))
        })?;

        let mut members = Vec::new();
        let mut in_workspace = false;
        let mut in_members = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed == "[workspace]" {
                in_workspace = true;
                continue;
            }

            if in_workspace && trimmed.starts_with("members") {
                in_members = true;
                // 处理单行 members = ["member1", "member2"]
                if let Some(start) = trimmed.find('[') {
                    if let Some(end) = trimmed.find(']') {
                        let members_str = &trimmed[start + 1..end];
                        for member in members_str.split(',') {
                            let member = member.trim().trim_matches('"').trim_matches('\'');
                            if !member.is_empty() {
                                members.push(member.to_string());
                            }
                        }
                        in_members = false;
                    }
                }
                continue;
            }

            if in_members {
                if trimmed.contains(']') {
                    in_members = false;
                }
                // 提取成员名称
                if let Some(member) = trimmed.trim_matches(',').trim().strip_prefix('"') {
                    if let Some(member) = member.strip_suffix('"') {
                        members.push(member.to_string());
                    }
                } else if let Some(member) = trimmed.trim_matches(',').trim().strip_prefix('\'') {
                    if let Some(member) = member.strip_suffix('\'') {
                        members.push(member.to_string());
                    }
                }
            }

            // 遇到新的 section 则退出
            if in_workspace && trimmed.starts_with('[') && trimmed != "[workspace]" {
                break;
            }
        }

        Ok(members)
    }

    /// 为 workspace 项目构建所有成员
    pub fn check_workspace(&self) -> Result<(), RustCheckError> {
        info!("Checking workspace at: {:?}", self.project_dir);

        if !self.is_workspace() {
            warn!("Not a workspace project, falling back to regular check");
            return self.check_rust_project();
        }

        let members = self.get_workspace_members()?;
        info!("Found {} workspace members: {:?}", members.len(), members);

        // 先构建整个 workspace
        info!("Building entire workspace...");
        let output = Command::new("cargo")
            .arg("build")
            .arg("--workspace")
            .arg("--color=always")
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| {
                let msg = format!("Failed to execute workspace build: {}", e);
                error!("{}", msg);
                RustCheckError::CommandError(msg)
            })?;

        if output.status.success() {
            info!("✅ Workspace build succeeded");
            Ok(())
        } else {
            let status = output.status.code().unwrap_or(-1);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            let mut error_msg = String::new();
            if !stdout.is_empty() {
                error_msg.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !error_msg.is_empty() {
                    error_msg.push('\n');
                }
                error_msg.push_str(&stderr);
            }

            let error_msg = error_msg.trim().to_string();
            error!(
                "❌ Workspace build failed with status {}:\n{}",
                status, error_msg
            );

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
            .write_all(b"[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"")
            .unwrap();

        // 创建有效的源文件
        let src_dir = project_path.join("src");
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

        // 创建 Cargo.toml
        let mut cargo_file = File::create(project_path.join("Cargo.toml")).unwrap();
        cargo_file
            .write_all(b"[package]\nname = \"test\"\nversion = \"0.1.0\"\nedition = \"2021\"")
            .unwrap();

        // 创建有语法错误的源文件
        let src_dir = project_path.join("src");
        std::fs::create_dir(&src_dir).unwrap();
        let mut lib_file = File::create(src_dir.join("main.rs")).unwrap();
        lib_file
            .write_all(b"fn main() { this_is_invalid }")
            .unwrap();

        let checker = RustCodeCheck::new(project_path);
        assert!(matches!(
            checker.check_rust_project().unwrap_err(),
            RustCheckError::BuildFailed(_, _)
        ));
    }

    #[test]
    fn test_is_workspace() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();

        // 创建 workspace Cargo.toml
        let mut cargo_file = File::create(project_path.join("Cargo.toml")).unwrap();
        cargo_file
            .write_all(b"[workspace]\nmembers = [\n    \"member1\",\n    \"member2\"\n]\n")
            .unwrap();

        let checker = RustCodeCheck::new(project_path);
        assert!(checker.is_workspace());
    }

    #[test]
    fn test_get_workspace_members() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();

        // 创建 workspace Cargo.toml
        let mut cargo_file = File::create(project_path.join("Cargo.toml")).unwrap();
        cargo_file
            .write_all(
                b"[workspace]\nmembers = [\n    \"crate1\",\n    \"crate2\",\n    \"crate3\"\n]\n",
            )
            .unwrap();

        let checker = RustCodeCheck::new(project_path);
        let members = checker.get_workspace_members().unwrap();
        assert_eq!(members.len(), 3);
        assert!(members.contains(&"crate1".to_string()));
        assert!(members.contains(&"crate2".to_string()));
        assert!(members.contains(&"crate3".to_string()));
    }

    #[test]
    fn test_workspace_members_single_line() {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path();

        // 创建单行格式的 workspace Cargo.toml
        let mut cargo_file = File::create(project_path.join("Cargo.toml")).unwrap();
        cargo_file
            .write_all(b"[workspace]\nmembers = [\"crate1\", \"crate2\"]\n")
            .unwrap();

        let checker = RustCodeCheck::new(project_path);
        let members = checker.get_workspace_members().unwrap();
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"crate1".to_string()));
        assert!(members.contains(&"crate2".to_string()));
    }
}
