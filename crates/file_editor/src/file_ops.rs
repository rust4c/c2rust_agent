//! Basic file operations
//!
//! Following the principle: "Simple, stupid, and it works"

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Read entire file content
/// Simple wrapper with better error context
pub fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Write content to file
/// Creates parent directories if needed
pub fn write_file<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    let path = path.as_ref();

    // Create parent directories if they don't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create parent directories: {}", parent.display())
        })?;
    }

    fs::write(path, content).with_context(|| format!("Failed to write file: {}", path.display()))
}

/// Check if file exists
pub fn file_exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
}

/// Get file size in bytes
pub fn file_size<P: AsRef<Path>>(path: P) -> Result<u64> {
    let path = path.as_ref();
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to read file metadata: {}", path.display()))?;
    Ok(metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        let content = "Hello, World!";

        // Write file
        write_file(&test_file, content).unwrap();

        // Read file
        let read_content = read_file(&test_file).unwrap();
        assert_eq!(content, read_content);
    }

    #[test]
    fn test_write_with_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let nested_file = temp_dir.path().join("nested").join("dir").join("test.txt");
        let content = "Nested content";

        // This should create the parent directories automatically
        write_file(&nested_file, content).unwrap();

        let read_content = read_file(&nested_file).unwrap();
        assert_eq!(content, read_content);
    }

    #[test]
    fn test_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("exists.txt");
        let non_existent = temp_dir.path().join("does_not_exist.txt");

        assert!(!file_exists(&test_file));

        write_file(&test_file, "content").unwrap();
        assert!(file_exists(&test_file));
        assert!(!file_exists(&non_existent));
    }

    #[test]
    fn test_file_size() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("size_test.txt");
        let content = "12345";

        write_file(&test_file, content).unwrap();
        let size = file_size(&test_file).unwrap();
        assert_eq!(size, 5);
    }
}
