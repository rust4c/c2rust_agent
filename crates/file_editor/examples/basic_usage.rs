//! Basic usage example for file_editor
//!
//! This example demonstrates the core functionality of the file_editor crate.
//! Run with: cargo run --example basic_usage

use file_editor::RustFileManager;
use std::fs;
use tempfile::TempDir;

fn main() -> anyhow::Result<()> {
    println!("🦀 Rust File Manager Demo");
    println!("========================\n");

    // Create a temporary project for demonstration
    let temp_dir = create_demo_project()?;
    let project_path = temp_dir.path().join("demo_project");

    println!("📁 Created demo project at: {}", project_path.display());

    // Initialize the file manager
    let manager = RustFileManager::new(&project_path)?;

    println!("✅ File manager initialized!");
    println!("   Project type: {:?}", manager.project_type());
    println!("   Main source: {}", manager.main_source_path().display());
    println!("   Cargo.toml: {}", manager.cargo_toml_path().display());

    // Demonstrate reading operations
    println!("\n📖 Reading Operations");
    println!("====================");

    let content = manager.read_main_source()?;
    println!("Main source file content:");
    println!("{}", content);

    let line_count = manager.line_count()?;
    println!("\nTotal lines: {}", line_count);

    // Read specific lines
    let lines_2_to_4 = manager.read_lines(2, 4)?;
    println!("\nLines 2-4:");
    println!("{}", lines_2_to_4);

    // Demonstrate symbol operations
    println!("\n🔍 Symbol Operations");
    println!("===================");

    let symbols = manager.list_symbols()?;
    println!("Found {} symbols:", symbols.len());

    for symbol in &symbols {
        println!(
            "  - {} {:?} (lines {}-{})",
            symbol.name, symbol.symbol_type, symbol.line_range.start, symbol.line_range.end
        );
    }

    // Find specific function
    if let Ok(main_fn) = manager.find_symbol("main") {
        println!("\n📍 Found main function:");
        println!(
            "Lines {}-{}",
            main_fn.line_range.start, main_fn.line_range.end
        );
        println!(
            "Content preview: {}",
            main_fn
                .content
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // Demonstrate modification operations
    println!("\n✏️  Modification Operations");
    println!("==========================");

    // Add a new function
    let new_function = r#"
/// A new helper function
pub fn helper_function(x: i32) -> i32 {
    println!("Helper called with: {}", x);
    x * 2
}"#;

    manager.insert_at_line(line_count + 1, new_function)?;
    println!("✅ Added new helper function");

    // Modify the main function to use the helper
    let _main_symbol = manager.find_symbol("main")?;
    let new_main = r#"fn main() {
    println!("Hello from modified main!");
    let result = helper_function(42);
    println!("Helper result: {}", result);

    // Call the greet function
    greet("Rust File Manager");
}"#;

    manager.replace_symbol("main", new_main)?;
    println!("✅ Modified main function");

    // Demonstrate Cargo.toml operations
    println!("\n📦 Cargo.toml Operations");
    println!("=======================");

    // Add dependencies
    manager.add_dependencies(&[("serde", "1.0"), ("tokio", "1.0"), ("anyhow", "1.0")])?;
    println!("✅ Added dependencies: serde, tokio, anyhow");

    let cargo_content = manager.read_cargo_toml()?;
    println!("\nUpdated Cargo.toml:");
    println!("{}", cargo_content);

    // Show final result
    println!("\n🎯 Final Result");
    println!("===============");

    let final_content = manager.read_main_source()?;
    println!("Modified main source:");
    println!("{}", final_content);

    let final_line_count = manager.line_count()?;
    println!(
        "\nFinal line count: {} (was {})",
        final_line_count, line_count
    );

    // List all symbols again
    let final_symbols = manager.list_symbols()?;
    println!("\nFinal symbols ({}):", final_symbols.len());
    for symbol in final_symbols {
        println!("  - {} {:?}", symbol.name, symbol.symbol_type);
    }

    println!("\n🎉 Demo completed successfully!");
    println!("   Temporary project will be cleaned up automatically.");

    Ok(())
}

/// Create a demo Rust project for testing
fn create_demo_project() -> anyhow::Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let project_root = temp_dir.path().join("demo_project");
    let src_dir = project_root.join("src");

    fs::create_dir_all(&src_dir)?;

    // Create Cargo.toml
    let cargo_content = r#"[package]
name = "demo-project"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    fs::write(project_root.join("Cargo.toml"), cargo_content)?;

    // Create main.rs
    let main_content = r#"fn main() {
    println!("Hello, world!");
    greet("Demo");
}

pub fn greet(name: &str) {
    println!("Hello, {}!", name);
}

pub struct Config {
    pub debug: bool,
    pub port: u16,
}

impl Config {
    pub fn new() -> Self {
        Self {
            debug: false,
            port: 8080,
        }
    }
}
"#;
    fs::write(src_dir.join("main.rs"), main_content)?;

    Ok(temp_dir)
}
