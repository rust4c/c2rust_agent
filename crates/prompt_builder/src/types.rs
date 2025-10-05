//! Data structure definitions for prompt builder
//!
//! This module contains all the core data structures used in the prompt builder system.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// File mapping information between cached and original paths
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMapping {
    pub cached_path: PathBuf,
    pub original_path: PathBuf,
}

/// Function definition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub file_path: PathBuf,
    pub line_number: Option<i32>,
    pub return_type: Option<String>,
    pub parameters: Option<String>,
    pub signature: Option<String>,
}

/// Function call relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRelationship {
    pub caller: String,
    pub called: String,
    pub line: Option<i32>,
    pub caller_file: Option<PathBuf>,
    pub called_file: Option<PathBuf>,
}

/// File dependency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDependency {
    pub from: PathBuf,
    pub to: PathBuf,
    pub dependency_type: String,
}

/// Interface context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceContext {
    pub name: String,
    pub file_path: PathBuf,
    pub language: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}
