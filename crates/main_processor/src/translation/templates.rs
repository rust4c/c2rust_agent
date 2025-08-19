//! Template generation for different C to Rust project types
//!
//! This module provides functions to generate appropriate Rust code templates
//! based on the structure and content of C source files.

use crate::{ProjectInfo, ProjectType};
use anyhow::Result;
use log::{debug, warn};
use std::path::Path;

/// Generate template for single file projects (typically main.c -> main.rs)
pub async fn generate_single_file_template(
    project: &ProjectInfo,
    c_files: &[String],
) -> Result<String> {
    debug!(
        "Generating single file template for project: {}",
        project.name
    );

    // Try to extract function signatures from C code
    let mut has_main = false;
    let mut functions = Vec::new();

    for c_file in c_files {
        if c_file.contains("int main(") || c_file.contains("void main(") {
            has_main = true;
        }

        // Simple function signature extraction
        for line in c_file.lines() {
            let line = line.trim();
            if line.starts_with("int ") || line.starts_with("void ") || line.starts_with("char") {
                if line.contains('(') && line.contains(')') && !line.contains("main(") {
                    if let Some(func_name) = extract_function_name(line) {
                        functions.push(func_name);
                    }
                }
            }
        }
    }

    let mut template = String::new();

    // Add common includes equivalent
    template.push_str("use std::ffi::{CStr, CString};\n");
    template.push_str("use std::ptr;\n");
    template.push_str("use std::os::raw::{c_char, c_int, c_void};\n\n");

    // Add function stubs
    for func_name in functions {
        template.push_str(&format!("// TODO: Implement {}\n", func_name));
        template.push_str(&format!("fn {}() {{\n", func_name));
        template.push_str("    unimplemented!(\"Translation pending\")\n");
        template.push_str("}\n\n");
    }

    // Add main function
    if has_main {
        template.push_str("fn main() {\n");
        template.push_str("    // TODO: Translate C main function logic\n");
        template.push_str("    println!(\"C to Rust translation - implement main logic\");\n");
        template.push_str("}\n");
    } else {
        template.push_str("fn main() {\n");
        template.push_str("    // TODO: No main function found in C code\n");
        template.push_str("    println!(\"Library code - consider making this a lib.rs\");\n");
        template.push_str("}\n");
    }

    Ok(template)
}

/// Generate template for paired files (e.g., .h/.c pairs -> lib.rs + modules)
pub async fn generate_library_template(
    project: &ProjectInfo,
    c_files: &[String],
) -> Result<String> {
    debug!("Generating library template for project: {}", project.name);

    let mut template = String::new();

    // Standard library template for paired files
    template.push_str("//! C to Rust translation of ");
    template.push_str(&project.name);
    template.push_str("\n//! Generated template - requires manual translation\n\n");

    template.push_str("use std::ffi::{CStr, CString};\n");
    template.push_str("use std::ptr;\n");
    template.push_str("use std::os::raw::{c_char, c_int, c_void};\n\n");

    // Try to extract structure and function information
    let mut structs = Vec::new();
    let mut functions = Vec::new();

    for c_file in c_files {
        // Extract struct definitions
        for line in c_file.lines() {
            let line = line.trim();
            if line.starts_with("typedef struct") || line.starts_with("struct ") {
                if let Some(struct_name) = extract_struct_name(line) {
                    structs.push(struct_name);
                }
            }

            // Extract function declarations
            if (line.starts_with("int ") || line.starts_with("void ") || line.starts_with("char"))
                && line.contains('(')
                && line.contains(')')
            {
                if let Some(func_name) = extract_function_name(line) {
                    functions.push(func_name);
                }
            }
        }
    }

    // Generate struct stubs
    for struct_name in structs {
        template.push_str(&format!("// TODO: Translate C struct {}\n", struct_name));
        template.push_str(&format!("#[repr(C)]\npub struct {} {{\n", struct_name));
        template.push_str("    // TODO: Add fields from C struct\n");
        template.push_str("}\n\n");
    }

    // Generate function stubs
    for func_name in functions {
        template.push_str(&format!("// TODO: Translate C function {}\n", func_name));
        template.push_str(&format!(
            "pub fn {}() -> Result<(), Box<dyn std::error::Error>> {{\n",
            func_name
        ));
        template.push_str("    // TODO: Implement function logic\n");
        template.push_str("    unimplemented!(\"Translation pending\")\n");
        template.push_str("}\n\n");
    }

    // Add a basic test module
    template.push_str("#[cfg(test)]\nmod tests {\n");
    template.push_str("    use super::*;\n\n");
    template.push_str("    #[test]\n");
    template.push_str("    fn test_translation_placeholder() {\n");
    template.push_str("        // TODO: Add tests after translation\n");
    template.push_str("        assert!(true, \"Translation tests pending\");\n");
    template.push_str("    }\n");
    template.push_str("}\n");

    Ok(template)
}

/// Generate template for unrelated files (multiple independent modules)
pub async fn generate_multi_module_template(
    project: &ProjectInfo,
    c_files: &[String],
) -> Result<String> {
    debug!(
        "Generating multi-module template for project: {}",
        project.name
    );

    let mut template = String::new();

    template.push_str("//! Multi-module C to Rust translation of ");
    template.push_str(&project.name);
    template.push_str("\n//! Each C file should be translated to a separate module\n\n");

    // Try to identify individual C files that should become modules
    let mut modules = Vec::new();

    for (i, c_file) in c_files.iter().enumerate() {
        if let Some(file_indicator) = c_file
            .lines()
            .find(|line| line.starts_with("// File: ") || line.starts_with("File: "))
        {
            let file_path = file_indicator
                .trim_start_matches("// File: ")
                .trim_start_matches("File: ");
            if let Some(file_name) = std::path::Path::new(file_path).file_stem() {
                modules.push(file_name.to_string_lossy().to_string());
            }
        } else {
            modules.push(format!("module_{}", i + 1));
        }
    }

    // Generate module declarations
    for module in &modules {
        template.push_str(&format!("pub mod {};\n", module));
    }
    template.push_str("\n");

    // Re-export main items
    template.push_str("// Re-export main functionality\n");
    for module in &modules {
        template.push_str(&format!("pub use {}::*;\n", module));
    }
    template.push_str("\n");

    // Generate individual module stubs (this would typically create separate files)
    template.push_str("// TODO: Create separate .rs files for each module:\n");
    for module in &modules {
        template.push_str(&format!(
            "// - {}.rs: Translate corresponding C file\n",
            module
        ));
    }
    template.push_str("\n");

    // Add a main function that ties everything together
    template.push_str("fn main() {\n");
    template.push_str("    println!(\"Multi-module translation of C project\");\n");
    template.push_str("    // TODO: Initialize and coordinate modules\n");
    template.push_str("}\n");

    Ok(template)
}

/// Generate appropriate template based on project type
pub async fn generate_project_template(
    project: &ProjectInfo,
    c_files: &[String],
) -> Result<String> {
    match project.project_type {
        ProjectType::SingleFile => generate_single_file_template(project, c_files).await,
        ProjectType::PairedFiles => generate_library_template(project, c_files).await,
        ProjectType::UnrelatedFiles => generate_multi_module_template(project, c_files).await,
    }
}

/// Extract function name from C function signature
fn extract_function_name(line: &str) -> Option<String> {
    // Simple extraction: find text between type and opening parenthesis
    if let Some(paren_pos) = line.find('(') {
        let before_paren = &line[..paren_pos].trim();
        // Split by whitespace and take the last part
        if let Some(name) = before_paren.split_whitespace().last() {
            // Remove pointer indicators
            let clean_name = name.trim_start_matches('*').trim();
            if !clean_name.is_empty() && clean_name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                return Some(clean_name.to_string());
            }
        }
    }
    None
}

/// Extract struct name from C struct definition
fn extract_struct_name(line: &str) -> Option<String> {
    if line.starts_with("typedef struct") {
        // Handle typedef struct { ... } Name;
        if let Some(name) = line.split_whitespace().last() {
            let clean_name = name.trim_end_matches(';').trim();
            if !clean_name.is_empty() {
                return Some(clean_name.to_string());
            }
        }
    } else if line.starts_with("struct ") {
        // Handle struct Name { ... };
        let after_struct = &line[7..]; // Skip "struct "
        if let Some(space_pos) = after_struct.find(' ') {
            let name = &after_struct[..space_pos];
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_name() {
        assert_eq!(
            extract_function_name("int add(int a, int b)"),
            Some("add".to_string())
        );
        assert_eq!(
            extract_function_name("void *malloc(size_t size)"),
            Some("malloc".to_string())
        );
        assert_eq!(
            extract_function_name("char* strcpy(char* dest, const char* src)"),
            Some("strcpy".to_string())
        );
        assert_eq!(extract_function_name("invalid syntax"), None);
    }

    #[test]
    fn test_extract_struct_name() {
        assert_eq!(
            extract_struct_name("typedef struct Point Point;"),
            Some("Point".to_string())
        );
        assert_eq!(
            extract_struct_name("struct Rectangle { int width; int height; };"),
            Some("Rectangle".to_string())
        );
        assert_eq!(extract_struct_name("invalid"), None);
    }
}
