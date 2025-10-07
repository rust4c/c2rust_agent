//! # Rust File Manager
//!
//! A focused crate for managing Rust project files and structures.
//! Following Linus's philosophy: "Bad programmers worry about the code. Good programmers worry about data structures."
//!
//! ## Core Philosophy
//! - Simple, focused API with no special cases
//! - Never break existing functionality
//! - Data structures drive the design, not the other way around

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod content_parser;
pub mod file_ops;
pub mod line_ops;
pub mod manager;
pub mod project_detector;

/// Core errors for file management operations
#[derive(Error, Debug)]
pub enum FileManagerError {
    #[error("Project not found at path: {0}")]
    ProjectNotFound(PathBuf),

    #[error("Invalid Rust project structure: {0}")]
    InvalidProject(String),

    #[error("File operation failed: {0}")]
    FileOperation(String),

    #[error("Line range invalid: {start}-{end}")]
    InvalidLineRange { start: usize, end: usize },

    #[error("Function or struct not found: {name}")]
    SymbolNotFound { name: String },
}

/// The fundamental data structure - a Rust project representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustProject {
    /// Root directory of the project
    pub root_path: PathBuf,
    /// Type of the project (bin vs lib)
    pub project_type: ProjectType,
    /// Main source file (main.rs or lib.rs)
    pub main_source: PathBuf,
    /// Cargo.toml path
    pub cargo_toml: PathBuf,
}

/// Project type - simple enum, no special cases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectType {
    Binary,
    Library,
}

/// Line range for precise operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

impl LineRange {
    pub fn new(start: usize, end: usize) -> Result<Self> {
        if start == 0 || end == 0 || start > end {
            return Err(FileManagerError::InvalidLineRange { start, end }.into());
        }
        Ok(Self { start, end })
    }

    pub fn single_line(line: usize) -> Result<Self> {
        Self::new(line, line)
    }
}

/// Function or struct information for code navigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSymbol {
    pub name: String,
    pub symbol_type: SymbolType,
    pub line_range: LineRange,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SymbolType {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
    Module,
}

impl RustProject {
    /// Discover a Rust project from any path within it
    /// This is the primary entry point - simple and robust
    pub fn discover<P: AsRef<Path>>(path: P) -> Result<Self> {
        project_detector::discover_project(path.as_ref())
    }

    /// Read the main source file content
    pub fn read_main_source(&self) -> Result<String> {
        file_ops::read_file(&self.main_source)
    }

    /// Read Cargo.toml content
    pub fn read_cargo_toml(&self) -> Result<String> {
        file_ops::read_file(&self.cargo_toml)
    }

    /// Write content to main source file
    pub fn write_main_source(&self, content: &str) -> Result<()> {
        file_ops::write_file(&self.main_source, content)
    }

    /// Write content to Cargo.toml
    pub fn write_cargo_toml(&self, content: &str) -> Result<()> {
        file_ops::write_file(&self.cargo_toml, content)
    }

    /// Read specific line range from main source
    pub fn read_lines(&self, range: LineRange) -> Result<String> {
        line_ops::read_line_range(&self.main_source, range)
    }

    /// Replace content in specific line range
    pub fn replace_lines(&self, range: LineRange, new_content: &str) -> Result<()> {
        line_ops::replace_line_range(&self.main_source, range, new_content)
    }

    /// Find a function or struct and return its content and location
    pub fn find_symbol(&self, name: &str) -> Result<CodeSymbol> {
        content_parser::find_symbol(&self.main_source, name)
    }

    /// List all functions and structs in the project
    pub fn list_symbols(&self) -> Result<Vec<CodeSymbol>> {
        content_parser::list_symbols(&self.main_source)
    }
}

// Re-export commonly used types for convenience
pub use content_parser::{find_symbol, list_symbols};
pub use file_ops::{read_file, write_file};
pub use line_ops::{read_line_range, replace_line_range};
pub use manager::RustFileManager;
pub use project_detector::discover_project;
