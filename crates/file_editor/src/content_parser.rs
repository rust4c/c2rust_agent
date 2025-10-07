//! Content parsing for Rust code analysis
//!
//! Simple, robust parsing that doesn't try to be too clever.
//! Following Linus's principle: "Good code has no special cases"

use crate::{CodeSymbol, FileManagerError, LineRange, SymbolType};
use anyhow::{Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;

/// Find a specific symbol (function, struct, etc.) in a Rust file
pub fn find_symbol<P: AsRef<Path>>(path: P, name: &str) -> Result<CodeSymbol> {
    let symbols = list_symbols(path)?;

    symbols
        .into_iter()
        .find(|symbol| symbol.name == name)
        .ok_or_else(|| {
            FileManagerError::SymbolNotFound {
                name: name.to_string(),
            }
            .into()
        })
}

/// List all symbols (functions, structs, etc.) in a Rust file
pub fn list_symbols<P: AsRef<Path>>(path: P) -> Result<Vec<CodeSymbol>> {
    let path = path.as_ref();
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let lines: Vec<&str> = content.lines().collect();
    let mut symbols = Vec::new();

    // Find all symbols using simple regex patterns
    symbols.extend(find_functions(&lines)?);
    symbols.extend(find_structs(&lines)?);
    symbols.extend(find_enums(&lines)?);
    symbols.extend(find_traits(&lines)?);
    symbols.extend(find_impls(&lines)?);
    symbols.extend(find_modules(&lines)?);

    // Sort by line number for consistent output
    symbols.sort_by(|a, b| a.line_range.start.cmp(&b.line_range.start));

    Ok(symbols)
}

/// Find all function definitions
fn find_functions(lines: &[&str]) -> Result<Vec<CodeSymbol>> {
    let fn_regex = Regex::new(r"^\s*(pub\s+)?(async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)")
        .expect("Invalid function regex");

    let mut functions = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(captures) = fn_regex.captures(line) {
            if let Some(name_match) = captures.get(3) {
                let name = name_match.as_str().to_string();
                let start_line = line_idx + 1;

                // Find the end of the function by matching braces
                let end_line = find_block_end(lines, line_idx)?;
                let line_range = LineRange::new(start_line, end_line)?;

                // Extract the function content
                let content_lines = &lines[(line_idx)..end_line];
                let content = content_lines.join("\n");

                functions.push(CodeSymbol {
                    name,
                    symbol_type: SymbolType::Function,
                    line_range,
                    content,
                });
            }
        }
    }

    Ok(functions)
}

/// Find all struct definitions
fn find_structs(lines: &[&str]) -> Result<Vec<CodeSymbol>> {
    let struct_regex = Regex::new(r"^\s*(pub\s+)?struct\s+([a-zA-Z_][a-zA-Z0-9_]*)")
        .expect("Invalid struct regex");

    let mut structs = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(captures) = struct_regex.captures(line) {
            if let Some(name_match) = captures.get(2) {
                let name = name_match.as_str().to_string();
                let start_line = line_idx + 1;

                let end_line = if line.contains(';') {
                    // Unit struct or tuple struct on single line
                    line_idx + 1
                } else {
                    // Struct with fields - find closing brace
                    find_block_end(lines, line_idx)?
                };

                let line_range = LineRange::new(start_line, end_line)?;
                let content_lines = &lines[line_idx..end_line];
                let content = content_lines.join("\n");

                structs.push(CodeSymbol {
                    name,
                    symbol_type: SymbolType::Struct,
                    line_range,
                    content,
                });
            }
        }
    }

    Ok(structs)
}

/// Find all enum definitions
fn find_enums(lines: &[&str]) -> Result<Vec<CodeSymbol>> {
    let enum_regex =
        Regex::new(r"^\s*(pub\s+)?enum\s+([a-zA-Z_][a-zA-Z0-9_]*)").expect("Invalid enum regex");

    let mut enums = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(captures) = enum_regex.captures(line) {
            if let Some(name_match) = captures.get(2) {
                let name = name_match.as_str().to_string();
                let start_line = line_idx + 1;
                let end_line = find_block_end(lines, line_idx)?;
                let line_range = LineRange::new(start_line, end_line)?;

                let content_lines = &lines[line_idx..end_line];
                let content = content_lines.join("\n");

                enums.push(CodeSymbol {
                    name,
                    symbol_type: SymbolType::Enum,
                    line_range,
                    content,
                });
            }
        }
    }

    Ok(enums)
}

/// Find all trait definitions
fn find_traits(lines: &[&str]) -> Result<Vec<CodeSymbol>> {
    let trait_regex =
        Regex::new(r"^\s*(pub\s+)?trait\s+([a-zA-Z_][a-zA-Z0-9_]*)").expect("Invalid trait regex");

    let mut traits = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(captures) = trait_regex.captures(line) {
            if let Some(name_match) = captures.get(2) {
                let name = name_match.as_str().to_string();
                let start_line = line_idx + 1;
                let end_line = find_block_end(lines, line_idx)?;
                let line_range = LineRange::new(start_line, end_line)?;

                let content_lines = &lines[line_idx..end_line];
                let content = content_lines.join("\n");

                traits.push(CodeSymbol {
                    name,
                    symbol_type: SymbolType::Trait,
                    line_range,
                    content,
                });
            }
        }
    }

    Ok(traits)
}

/// Find all impl blocks
fn find_impls(lines: &[&str]) -> Result<Vec<CodeSymbol>> {
    let impl_regex = Regex::new(r"^\s*impl\s+(?:<[^>]*>\s+)?([a-zA-Z_][a-zA-Z0-9_:]*)")
        .expect("Invalid impl regex");

    let mut impls = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(captures) = impl_regex.captures(line) {
            if let Some(name_match) = captures.get(1) {
                let name = format!("impl {}", name_match.as_str());
                let start_line = line_idx + 1;
                let end_line = find_block_end(lines, line_idx)?;
                let line_range = LineRange::new(start_line, end_line)?;

                let content_lines = &lines[line_idx..end_line];
                let content = content_lines.join("\n");

                impls.push(CodeSymbol {
                    name,
                    symbol_type: SymbolType::Impl,
                    line_range,
                    content,
                });
            }
        }
    }

    Ok(impls)
}

/// Find all module definitions
fn find_modules(lines: &[&str]) -> Result<Vec<CodeSymbol>> {
    let mod_regex =
        Regex::new(r"^\s*(pub\s+)?mod\s+([a-zA-Z_][a-zA-Z0-9_]*)").expect("Invalid module regex");

    let mut modules = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(captures) = mod_regex.captures(line) {
            if let Some(name_match) = captures.get(2) {
                let name = name_match.as_str().to_string();
                let start_line = line_idx + 1;

                let end_line = if line.contains(';') {
                    // Module declaration (mod foo;)
                    line_idx + 1
                } else {
                    // Inline module - find closing brace
                    find_block_end(lines, line_idx)?
                };

                let line_range = LineRange::new(start_line, end_line)?;
                let content_lines = &lines[line_idx..end_line];
                let content = content_lines.join("\n");

                modules.push(CodeSymbol {
                    name,
                    symbol_type: SymbolType::Module,
                    line_range,
                    content,
                });
            }
        }
    }

    Ok(modules)
}

/// Find the end of a code block by matching braces
/// Returns the line number (1-indexed) of the closing brace
fn find_block_end(lines: &[&str], start_idx: usize) -> Result<usize> {
    let mut brace_count = 0;
    let mut found_opening_brace = false;

    for (idx, line) in lines.iter().enumerate().skip(start_idx) {
        for ch in line.chars() {
            match ch {
                '{' => {
                    brace_count += 1;
                    found_opening_brace = true;
                }
                '}' => {
                    if found_opening_brace {
                        brace_count -= 1;
                        if brace_count == 0 {
                            return Ok(idx + 1);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // If no braces found, it might be a single-line item
    if !found_opening_brace {
        return Ok(start_idx + 1);
    }

    // If we get here, braces weren't properly matched
    Err(FileManagerError::FileOperation("Could not find matching closing brace".to_string()).into())
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
    fn test_find_functions() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
fn private_function() {
    println!("private");
}

pub fn public_function(x: i32) -> i32 {
    x + 1
}

pub async fn async_function() {
    // async code
}
"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);
        let symbols = list_symbols(&file_path).unwrap();

        let functions: Vec<_> = symbols
            .iter()
            .filter(|s| matches!(s.symbol_type, SymbolType::Function))
            .collect();

        assert_eq!(functions.len(), 3);
        assert_eq!(functions[0].name, "private_function");
        assert_eq!(functions[1].name, "public_function");
        assert_eq!(functions[2].name, "async_function");
    }

    #[test]
    fn test_find_structs() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
struct PrivateStruct {
    field: i32,
}

pub struct PublicStruct {
    pub field: String,
}

pub struct TupleStruct(i32, String);

pub struct UnitStruct;
"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);
        let symbols = list_symbols(&file_path).unwrap();

        let structs: Vec<_> = symbols
            .iter()
            .filter(|s| matches!(s.symbol_type, SymbolType::Struct))
            .collect();

        assert_eq!(structs.len(), 4);
        assert_eq!(structs[0].name, "PrivateStruct");
        assert_eq!(structs[1].name, "PublicStruct");
        assert_eq!(structs[2].name, "TupleStruct");
        assert_eq!(structs[3].name, "UnitStruct");
    }

    #[test]
    fn test_find_symbol_by_name() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"
pub fn target_function() {
    println!("found me");
}

pub struct OtherStruct;
"#;
        let file_path = create_test_file(&temp_dir, "test.rs", content);

        let symbol = find_symbol(&file_path, "target_function").unwrap();
        assert_eq!(symbol.name, "target_function");
        assert!(matches!(symbol.symbol_type, SymbolType::Function));
        assert!(symbol.content.contains("found me"));

        let not_found = find_symbol(&file_path, "nonexistent");
        assert!(not_found.is_err());
    }

    #[test]
    fn test_find_block_end() {
        let lines = vec![
            "fn test() {",
            "    if true {",
            "        println!(\"nested\");",
            "    }",
            "}",
            "// end",
        ];

        let end = find_block_end(&lines, 0).unwrap();
        assert_eq!(end, 5); // Line 5 (1-indexed) contains the closing brace
    }
}
