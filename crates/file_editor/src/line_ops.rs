//! Line-level file operations
//!
//! Precise line manipulation following Linus's principle:
//! "If you need more than 3 levels of indentation, you're screwed anyway, and should fix your program."

use crate::{FileManagerError, LineRange};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Read specific line range from a file
pub fn read_line_range<P: AsRef<Path>>(path: P, range: LineRange) -> Result<String> {
    let path = path.as_ref();
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let lines: Vec<&str> = content.lines().collect();

    if range.end > lines.len() {
        return Err(FileManagerError::InvalidLineRange {
            start: range.start,
            end: range.end,
        }
        .into());
    }

    let selected_lines = &lines[(range.start - 1)..range.end];
    Ok(selected_lines.join("\n"))
}

/// Replace content in specific line range
pub fn replace_line_range<P: AsRef<Path>>(
    path: P,
    range: LineRange,
    new_content: &str,
) -> Result<()> {
    let path = path.as_ref();
    let original_content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let mut lines: Vec<&str> = original_content.lines().collect();

    if range.end > lines.len() {
        return Err(FileManagerError::InvalidLineRange {
            start: range.start,
            end: range.end,
        }
        .into());
    }

    // Replace the specified range with new content
    let new_lines: Vec<&str> = new_content.lines().collect();
    lines.splice((range.start - 1)..range.end, new_lines);

    let new_file_content = lines.join("\n");
    if !new_file_content.is_empty() && !original_content.ends_with('\n') == false {
        // Preserve original file's trailing newline behavior
    }

    fs::write(path, new_file_content)
        .with_context(|| format!("Failed to write file: {}", path.display()))?;

    Ok(())
}

/// Insert content at specific line
pub fn insert_at_line<P: AsRef<Path>>(path: P, line_number: usize, content: &str) -> Result<()> {
    let path = path.as_ref();
    let original_content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let mut lines: Vec<&str> = original_content.lines().collect();

    if line_number > lines.len() + 1 {
        return Err(FileManagerError::InvalidLineRange {
            start: line_number,
            end: line_number,
        }
        .into());
    }

    let new_lines: Vec<&str> = content.lines().collect();
    let insert_pos = if line_number == 0 { 0 } else { line_number - 1 };

    for (i, new_line) in new_lines.into_iter().enumerate() {
        lines.insert(insert_pos + i, new_line);
    }

    let new_file_content = lines.join("\n");
    fs::write(path, new_file_content)
        .with_context(|| format!("Failed to write file: {}", path.display()))?;

    Ok(())
}

/// Delete specific line range
pub fn delete_line_range<P: AsRef<Path>>(path: P, range: LineRange) -> Result<()> {
    replace_line_range(path, range, "")
}

/// Get total line count of a file
pub fn line_count<P: AsRef<Path>>(path: P) -> Result<usize> {
    let path = path.as_ref();
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    Ok(content.lines().count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_file(temp_dir: &TempDir, filename: &str, content: &str) -> std::path::PathBuf {
        let file_path = temp_dir.path().join(filename);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    #[test]
    fn test_read_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let file_path = create_test_file(&temp_dir, "test.txt", content);

        let range = LineRange::new(2, 4).unwrap();
        let result = read_line_range(&file_path, range).unwrap();
        assert_eq!(result, "line 2\nline 3\nline 4");
    }

    #[test]
    fn test_read_single_line() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\nline 3";
        let file_path = create_test_file(&temp_dir, "test.txt", content);

        let range = LineRange::single_line(2).unwrap();
        let result = read_line_range(&file_path, range).unwrap();
        assert_eq!(result, "line 2");
    }

    #[test]
    fn test_replace_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let file_path = create_test_file(&temp_dir, "test.txt", content);

        let range = LineRange::new(2, 4).unwrap();
        replace_line_range(&file_path, range, "new line\nanother new line").unwrap();

        let new_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(new_content, "line 1\nnew line\nanother new line\nline 5");
    }

    #[test]
    fn test_insert_at_line() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\nline 3";
        let file_path = create_test_file(&temp_dir, "test.txt", content);

        insert_at_line(&file_path, 2, "inserted line").unwrap();

        let new_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(new_content, "line 1\ninserted line\nline 2\nline 3");
    }

    #[test]
    fn test_delete_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let file_path = create_test_file(&temp_dir, "test.txt", content);

        let range = LineRange::new(2, 4).unwrap();
        delete_line_range(&file_path, range).unwrap();

        let new_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(new_content, "line 1\nline 5");
    }

    #[test]
    fn test_line_count() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let file_path = create_test_file(&temp_dir, "test.txt", content);

        let count = line_count(&file_path).unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_invalid_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let content = "line 1\nline 2\nline 3";
        let file_path = create_test_file(&temp_dir, "test.txt", content);

        let range = LineRange::new(2, 10).unwrap();
        let result = read_line_range(&file_path, range);
        assert!(result.is_err());
    }
}
