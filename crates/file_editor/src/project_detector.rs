//! Project detection and discovery
//!
//! Simple, robust project detection following Linus's principle:
//! "Good code has no special cases"

use crate::{FileManagerError, ProjectType, RustProject};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Discover a Rust project from any path within it
/// Walks up the directory tree to find Cargo.toml
pub fn discover_project(start_path: &Path) -> Result<RustProject> {
    let project_root = find_project_root(start_path)?;
    let cargo_toml = project_root.join("Cargo.toml");

    if !cargo_toml.exists() {
        return Err(FileManagerError::ProjectNotFound(project_root).into());
    }

    let project_type = detect_project_type(&project_root)?;
    let main_source = get_main_source_path(&project_root, project_type);

    if !main_source.exists() {
        return Err(FileManagerError::InvalidProject(format!(
            "Main source file not found: {}",
            main_source.display()
        ))
        .into());
    }

    Ok(RustProject {
        root_path: project_root,
        project_type,
        main_source,
        cargo_toml,
    })
}

/// Find the root directory containing Cargo.toml
/// Walks up the directory tree from the given path
fn find_project_root(start_path: &Path) -> Result<PathBuf> {
    let mut current = start_path.to_path_buf();

    // If start_path is a file, start from its parent directory
    if current.is_file() {
        current = current
            .parent()
            .ok_or_else(|| FileManagerError::ProjectNotFound(current.clone()))?
            .to_path_buf();
    }

    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            return Ok(current);
        }

        // Move up one directory level
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => break,
        }
    }

    Err(FileManagerError::ProjectNotFound(start_path.to_path_buf()).into())
}

/// Detect project type by reading Cargo.toml
/// Simple logic: if [[bin]] section exists or default main.rs exists -> Binary
/// Otherwise -> Library
fn detect_project_type(project_root: &Path) -> Result<ProjectType> {
    let cargo_toml_path = project_root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("Failed to read Cargo.toml: {}", cargo_toml_path.display()))?;

    // Check for explicit [[bin]] sections
    if content.contains("[[bin]]") {
        return Ok(ProjectType::Binary);
    }

    // Check for main.rs existence
    let main_rs = project_root.join("src").join("main.rs");
    if main_rs.exists() {
        return Ok(ProjectType::Binary);
    }

    // Default to library
    Ok(ProjectType::Library)
}

/// Get the main source file path based on project type
fn get_main_source_path(project_root: &Path, project_type: ProjectType) -> PathBuf {
    match project_type {
        ProjectType::Binary => project_root.join("src").join("main.rs"),
        ProjectType::Library => project_root.join("src").join("lib.rs"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_project(temp_dir: &TempDir, is_binary: bool) -> PathBuf {
        let project_root = temp_dir.path().join("test_project");
        let src_dir = project_root.join("src");

        fs::create_dir_all(&src_dir).unwrap();

        // Create Cargo.toml
        let cargo_content = r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#;
        fs::write(project_root.join("Cargo.toml"), cargo_content).unwrap();

        // Create appropriate main file
        if is_binary {
            fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
        } else {
            fs::write(src_dir.join("lib.rs"), "// lib").unwrap();
        }

        project_root
    }

    #[test]
    fn test_discover_binary_project() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = create_test_project(&temp_dir, true);

        let project = discover_project(&project_root).unwrap();

        assert_eq!(project.project_type, ProjectType::Binary);
        assert_eq!(project.root_path, project_root);
        assert!(project.main_source.ends_with("main.rs"));
    }

    #[test]
    fn test_discover_library_project() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = create_test_project(&temp_dir, false);

        let project = discover_project(&project_root).unwrap();

        assert_eq!(project.project_type, ProjectType::Library);
        assert_eq!(project.root_path, project_root);
        assert!(project.main_source.ends_with("lib.rs"));
    }

    #[test]
    fn test_discover_from_subdirectory() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = create_test_project(&temp_dir, true);
        let src_dir = project_root.join("src");

        let project = discover_project(&src_dir).unwrap();

        assert_eq!(project.root_path, project_root);
        assert_eq!(project.project_type, ProjectType::Binary);
    }

    #[test]
    fn test_project_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let non_project_dir = temp_dir.path().join("not_a_project");
        fs::create_dir(&non_project_dir).unwrap();

        let result = discover_project(&non_project_dir);
        assert!(result.is_err());
    }
}
