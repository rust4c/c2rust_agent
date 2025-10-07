# Rust File Manager

A focused crate for managing Rust project files and structures, designed with simplicity and robustness in mind.

## Philosophy

Following Linus Torvalds' principles:
- "Bad programmers worry about the code. Good programmers worry about data structures."
- Simple, focused API with no special cases
- Never break existing functionality
- Data structures drive the design

## Features

- **Project Discovery**: Automatically detect Rust projects from any path within them
- **File Operations**: Read and write main source files (main.rs/lib.rs) and Cargo.toml
- **Line-Level Operations**: Precise line manipulation with range support
- **Symbol Navigation**: Find and manipulate functions, structs, enums, traits, and impl blocks
- **Cargo.toml Management**: Add dependencies and modify project configuration
- **Type Safety**: Strong typing with comprehensive error handling

## Quick Start

### Basic Usage

```rust
use rust_file_manager::RustFileManager;

// Create a file manager for any path within a Rust project
let manager = RustFileManager::new("/path/to/project/src/main.rs")?;

// Read the main source file
let content = manager.read_main_source()?;
println!("Current code:\n{}", content);

// Read Cargo.toml
let cargo_content = manager.read_cargo_toml()?;
println!("Cargo.toml:\n{}", cargo_content);
```

### Line-Level Operations

```rust
use rust_file_manager::RustFileManager;

let manager = RustFileManager::new("/path/to/project")?;

// Read specific lines
let lines_2_to_5 = manager.read_lines(2, 5)?;
println!("Lines 2-5: {}", lines_2_to_5);

// Replace a single line
manager.replace_line(10, "    println!(\"Updated line\");")?;

// Replace a range of lines
manager.replace_lines(15, 20, "// This replaces lines 15-20\nlet new_code = true;")?;

// Insert content at a specific line
manager.insert_at_line(25, "// Inserted comment")?;

// Delete lines
manager.delete_lines(30, 35)?;
```

### Symbol Operations

```rust
use rust_file_manager::RustFileManager;

let manager = RustFileManager::new("/path/to/project")?;

// Find a specific function
let main_fn = manager.find_symbol("main")?;
println!("Function: {}", main_fn.name);
println!("Lines: {}-{}", main_fn.line_range.start, main_fn.line_range.end);
println!("Content:\n{}", main_fn.content);

// List all symbols
let symbols = manager.list_symbols()?;
for symbol in symbols {
    println!("{:?}: {} (lines {}-{})", 
        symbol.symbol_type, 
        symbol.name,
        symbol.line_range.start,
        symbol.line_range.end
    );
}

// Get just function names
let functions = manager.list_function_names()?;
println!("Functions: {:?}", functions);

// Replace a function
let new_function = r#"pub fn my_function() {
    println!("Updated function");
    // New implementation
}"#;
manager.replace_symbol("my_function", new_function)?;
```

### Cargo.toml Management

```rust
use rust_file_manager::RustFileManager;

let manager = RustFileManager::new("/path/to/project")?;

// Add a single dependency
manager.add_dependency("serde", "1.0")?;

// Add multiple dependencies
manager.add_dependencies(&[
    ("tokio", "1.0"),
    ("anyhow", "1.0"),
    ("log", "0.4"),
])?;

// Direct Cargo.toml manipulation
let cargo_content = manager.read_cargo_toml()?;
let updated_content = cargo_content.replace("0.1.0", "0.2.0");
manager.write_cargo_toml(&updated_content)?;
```

### Project Information

```rust
use rust_file_manager::{RustFileManager, ProjectType};

let manager = RustFileManager::new("/path/to/project")?;

// Get project information
let project = manager.project();
println!("Project root: {}", project.root_path.display());
println!("Project type: {:?}", project.project_type);
println!("Main source: {}", project.main_source.display());
println!("Cargo.toml: {}", project.cargo_toml.display());

// Check project type
match manager.project_type() {
    ProjectType::Binary => println!("This is a binary project (main.rs)"),
    ProjectType::Library => println!("This is a library project (lib.rs)"),
}

// Get file paths
println!("Main source: {}", manager.main_source_path().display());
println!("Cargo.toml: {}", manager.cargo_toml_path().display());
```

### Utility Operations

```rust
use rust_file_manager::RustFileManager;

let manager = RustFileManager::new("/path/to/project")?;

// Get line count
let total_lines = manager.line_count()?;
println!("Total lines: {}", total_lines);

// Check if project compiles
if manager.check_compilation()? {
    println!("Project compiles successfully!");
} else {
    println!("Project has compilation errors");
}

// Format code
manager.format_code()?;
println!("Code formatted with rustfmt");
```

## Low-Level API

For more control, you can use the low-level modules directly:

```rust
use rust_file_manager::{
    discover_project, read_file, write_file, 
    find_symbol, list_symbols, LineRange
};

// Direct project discovery
let project = discover_project("/path/to/project")?;

// Direct file operations
let content = read_file(&project.main_source)?;
write_file(&project.main_source, "new content")?;

// Direct symbol operations
let main_fn = find_symbol(&project.main_source, "main")?;
let all_symbols = list_symbols(&project.main_source)?;

// Line operations with ranges
use rust_file_manager::line_ops::{read_line_range, replace_line_range};

let range = LineRange::new(5, 10)?;
let lines = read_line_range(&project.main_source, range)?;
replace_line_range(&project.main_source, range, "new content")?;
```

## Error Handling

The crate uses comprehensive error handling with `anyhow::Result`:

```rust
use rust_file_manager::{RustFileManager, FileManagerError};

match RustFileManager::new("/invalid/path") {
    Ok(manager) => {
        // Use manager
    }
    Err(e) => {
        if let Some(file_err) = e.downcast_ref::<FileManagerError>() {
            match file_err {
                FileManagerError::ProjectNotFound(path) => {
                    println!("No Rust project found at: {}", path.display());
                }
                FileManagerError::InvalidProject(msg) => {
                    println!("Invalid project structure: {}", msg);
                }
                FileManagerError::SymbolNotFound { name } => {
                    println!("Symbol '{}' not found", name);
                }
                _ => println!("Other file manager error: {}", file_err),
            }
        } else {
            println!("Other error: {}", e);
        }
    }
}
```

## Examples with Real Projects

### Working with the chibicc_cache Example

```rust
use rust_file_manager::RustFileManager;

// Point to any file in a processed project
let manager = RustFileManager::new(
    "/Users/peng/Documents/Tmp/chibicc_cache/individual_files/chibicc/final-output"
)?;

// Analyze the generated code
let symbols = manager.list_symbols()?;
println!("Generated {} symbols:", symbols.len());

for symbol in symbols {
    println!("- {} {:?} ({})", 
        symbol.name, 
        symbol.symbol_type,
        symbol.line_range.end - symbol.line_range.start + 1
    );
}

// Find and modify a specific function
if let Ok(main_fn) = manager.find_symbol("main") {
    println!("Found main function at lines {}-{}", 
        main_fn.line_range.start, 
        main_fn.line_range.end
    );
    
    // Add logging to main function
    let new_main = main_fn.content.replace(
        "fn main() {", 
        "fn main() {\n    println!(\"Starting main function\");"
    );
    manager.replace_symbol("main", &new_main)?;
}

// Add dependencies that might be needed
manager.add_dependencies(&[
    ("libc", "0.2"),
    ("log", "0.4"),
])?;

// Check if it still compiles
if manager.check_compilation()? {
    println!("Modified project compiles successfully!");
}
```

## Design Principles

### Data Structure First
The core `RustProject` struct captures the essential data relationships:
- One project root
- One project type (binary or library)
- One main source file
- One Cargo.toml file

### No Special Cases
The API handles edge cases through the type system rather than conditional logic:
- `LineRange` ensures valid line numbers
- `ProjectType` enum eliminates binary/library confusion
- Automatic project discovery walks up directory tree consistently

### Simple Error Model
All operations return `Result<T>` with descriptive errors. No silent failures or complex error hierarchies.

### Composition Over Inheritance
High-level `RustFileManager` composes low-level modules rather than extending a base class.

## License

MIT OR Apache-2.0