//! High-level file manager interface
//!
//! This module provides a convenient, high-level API for common Rust project file operations.
//! Following Linus's principle: "Good interfaces hide complexity without sacrificing power."

use crate::{CodeSymbol, FileManagerError, LineRange, ProjectType, RustProject};
use anyhow::{Context, Result};
use std::path::Path;

/// High-level Rust project file manager
/// This is the main entry point for most operations
pub struct RustFileManager {
    project: RustProject,
}

impl RustFileManager {
    /// Create a new file manager for a Rust project
    /// Automatically discovers the project structure from any path within it
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let project = RustProject::discover(path)?;
        Ok(Self { project })
    }

    /// Get the project information
    pub fn project(&self) -> &RustProject {
        &self.project
    }

    /// Get the main source file path (main.rs or lib.rs)
    pub fn main_source_path(&self) -> &Path {
        &self.project.main_source
    }

    /// Get the Cargo.toml path
    pub fn cargo_toml_path(&self) -> &Path {
        &self.project.cargo_toml
    }

    /// Get project type
    pub fn project_type(&self) -> ProjectType {
        self.project.project_type
    }

    // === Content Reading Operations ===

    /// Read the main source file content
    pub fn read_main_source(&self) -> Result<String> {
        self.project.read_main_source()
    }

    /// Read Cargo.toml content
    pub fn read_cargo_toml(&self) -> Result<String> {
        self.project.read_cargo_toml()
    }

    /// Read specific lines from main source file
    pub fn read_lines(&self, start: usize, end: usize) -> Result<String> {
        let range = LineRange::new(start, end)?;
        self.project.read_lines(range)
    }

    /// Read a single line from main source file
    pub fn read_line(&self, line_number: usize) -> Result<String> {
        let range = LineRange::single_line(line_number)?;
        self.project.read_lines(range)
    }

    // === Content Writing Operations ===

    /// Write content to main source file
    pub fn write_main_source(&self, content: &str) -> Result<()> {
        self.project.write_main_source(content)
    }

    /// Write content to Cargo.toml
    pub fn write_cargo_toml(&self, content: &str) -> Result<()> {
        self.project.write_cargo_toml(content)
    }

    /// Replace content in specific line range of main source file
    pub fn replace_lines(&self, start: usize, end: usize, new_content: &str) -> Result<()> {
        let range = LineRange::new(start, end)?;
        self.project.replace_lines(range, new_content)
    }

    /// Replace a single line in main source file
    pub fn replace_line(&self, line_number: usize, new_content: &str) -> Result<()> {
        let range = LineRange::single_line(line_number)?;
        self.project.replace_lines(range, new_content)
    }

    /// Insert content at specific line in main source file
    pub fn insert_at_line(&self, line_number: usize, content: &str) -> Result<()> {
        crate::line_ops::insert_at_line(&self.project.main_source, line_number, content)
    }

    /// Delete specific line range from main source file
    pub fn delete_lines(&self, start: usize, end: usize) -> Result<()> {
        let range = LineRange::new(start, end)?;
        crate::line_ops::delete_line_range(&self.project.main_source, range)
    }

    /// Delete a single line from main source file
    pub fn delete_line(&self, line_number: usize) -> Result<()> {
        let range = LineRange::single_line(line_number)?;
        crate::line_ops::delete_line_range(&self.project.main_source, range)
    }

    // === Symbol Operations ===

    /// Find a function or struct by name
    pub fn find_symbol(&self, name: &str) -> Result<CodeSymbol> {
        self.project.find_symbol(name)
    }

    /// List all functions and structs in the project
    pub fn list_symbols(&self) -> Result<Vec<CodeSymbol>> {
        self.project.list_symbols()
    }

    /// Get the content of a specific function or struct
    pub fn get_symbol_content(&self, name: &str) -> Result<String> {
        let symbol = self.find_symbol(name)?;
        Ok(symbol.content)
    }

    /// Replace a function or struct with new content
    pub fn replace_symbol(&self, name: &str, new_content: &str) -> Result<()> {
        let symbol = self.find_symbol(name)?;
        self.replace_lines(symbol.line_range.start, symbol.line_range.end, new_content)
    }

    /// List all function names
    pub fn list_function_names(&self) -> Result<Vec<String>> {
        let symbols = self.list_symbols()?;
        Ok(symbols
            .into_iter()
            .filter(|s| matches!(s.symbol_type, crate::SymbolType::Function))
            .map(|s| s.name)
            .collect())
    }

    /// List all struct names
    pub fn list_struct_names(&self) -> Result<Vec<String>> {
        let symbols = self.list_symbols()?;
        Ok(symbols
            .into_iter()
            .filter(|s| matches!(s.symbol_type, crate::SymbolType::Struct))
            .map(|s| s.name)
            .collect())
    }

    // === Cargo.toml Manipulation ===

    /// Add a dependency to Cargo.toml
    pub fn add_dependency(&self, name: &str, version: &str) -> Result<()> {
        let content = self.read_cargo_toml()?;
        let updated = add_cargo_dependency(&content, name, version)?;
        self.write_cargo_toml(&updated)
    }

    /// Add multiple dependencies to Cargo.toml
    pub fn add_dependencies(&self, deps: &[(&str, &str)]) -> Result<()> {
        let mut content = self.read_cargo_toml()?;
        for &(name, version) in deps {
            content = add_cargo_dependency(&content, name, version)?;
        }
        self.write_cargo_toml(&content)
    }

    // === Utility Operations ===

    /// Get total line count of main source file
    pub fn line_count(&self) -> Result<usize> {
        crate::line_ops::line_count(&self.project.main_source)
    }

    /// Check if the project compiles successfully
    pub fn check_compilation(&self) -> Result<bool> {
        let output = std::process::Command::new("cargo")
            .arg("check")
            .current_dir(&self.project.root_path)
            .output()
            .context("Failed to run cargo check")?;

        Ok(output.status.success())
    }

    /// Format the main source file using rustfmt
    pub fn format_code(&self) -> Result<()> {
        let output = std::process::Command::new("rustfmt")
            .arg(&self.project.main_source)
            .output()
            .context("Failed to run rustfmt")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(
                FileManagerError::FileOperation(format!("rustfmt failed: {}", stderr)).into(),
            );
        }

        Ok(())
    }
}

/// Helper function to add a dependency to Cargo.toml content
fn add_cargo_dependency(content: &str, name: &str, version: &str) -> Result<String> {
    // Simple approach: find [dependencies] section and add the dependency
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut found_dependencies = false;
    let mut added = false;
    let dependency_line = format!("{} = \"{}\"", name, version);

    for line in lines {
        result.push(line.to_string());

        if line.trim() == "[dependencies]" {
            found_dependencies = true;
        } else if found_dependencies && line.starts_with('[') && line.ends_with(']') && !added {
            // We've moved to another section, add the dependency before this line
            result.insert(result.len() - 1, dependency_line.clone());
            added = true;
        }
    }

    // If we found [dependencies] but never added (reached end of file)
    if found_dependencies && !added {
        result.push(dependency_line.clone());
    } else if !found_dependencies {
        // No [dependencies] section found, add it
        result.push("".to_string());
        result.push("[dependencies]".to_string());
        result.push(dependency_line);
    }

    Ok(result.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_project(temp_dir: &TempDir, is_binary: bool) -> std::path::PathBuf {
        let project_root = temp_dir.path().join("test_project");
        let src_dir = project_root.join("src");

        fs::create_dir_all(&src_dir).unwrap();

        // Create Cargo.toml
        let cargo_content = r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
        fs::write(project_root.join("Cargo.toml"), cargo_content).unwrap();

        // Create source file
        if is_binary {
            let main_content = r#"fn main() {
    println!("Hello, world!");
}

pub fn helper_function() {
    println!("Helper");
}

pub struct TestStruct {
    field: i32,
}
"#;
            fs::write(src_dir.join("main.rs"), main_content).unwrap();
        } else {
            let lib_content = r#"pub fn public_function() {
    println!("Public");
}

fn private_function() {
    println!("Private");
}

pub struct LibStruct {
    value: String,
}
"#;
            fs::write(src_dir.join("lib.rs"), lib_content).unwrap();
        }

        project_root
    }

    #[test]
    fn test_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = create_test_project(&temp_dir, true);

        let manager = RustFileManager::new(&project_root).unwrap();
        assert_eq!(manager.project_type(), ProjectType::Binary);
        assert!(manager.main_source_path().ends_with("main.rs"));
    }

    #[test]
    fn test_read_write_operations() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = create_test_project(&temp_dir, true);

        let manager = RustFileManager::new(&project_root).unwrap();

        // Test reading
        let content = manager.read_main_source().unwrap();
        assert!(content.contains("fn main()"));

        // Test line operations
        let first_line = manager.read_line(1).unwrap();
        assert!(first_line.contains("fn main()"));

        // Test symbol operations
        let symbols = manager.list_symbols().unwrap();
        assert!(symbols.len() > 0);

        let main_fn = manager.find_symbol("main").unwrap();
        assert_eq!(main_fn.name, "main");
    }

    #[test]
    fn test_add_dependency() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = create_test_project(&temp_dir, true);

        let manager = RustFileManager::new(&project_root).unwrap();

        manager.add_dependency("serde", "1.0").unwrap();

        let cargo_content = manager.read_cargo_toml().unwrap();
        assert!(cargo_content.contains("serde = \"1.0\""));
    }

    #[test]
    fn test_replace_symbol() {
        let temp_dir = TempDir::new().unwrap();
        let project_root = create_test_project(&temp_dir, true);

        let manager = RustFileManager::new(&project_root).unwrap();

        // Replace the helper function
        let new_function = r#"pub fn helper_function() {
    println!("Updated helper");
    println!("With multiple lines");
}"#;

        manager
            .replace_symbol("helper_function", new_function)
            .unwrap();

        let content = manager.read_main_source().unwrap();
        assert!(content.contains("Updated helper"));
        assert!(content.contains("With multiple lines"));
    }
}
