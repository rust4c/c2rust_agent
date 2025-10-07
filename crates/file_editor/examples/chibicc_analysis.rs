//! Example: Analyzing and modifying chibicc_cache projects
//!
//! This example demonstrates how to use file_editor with real C2Rust generated projects
//! like those in the chibicc_cache example directory.
//!
//! Run with: cargo run --example chibicc_analysis

use file_editor::{RustFileManager, SymbolType};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    println!("🔍 ChiLiCc Cache Project Analysis");
    println!("================================\n");

    // Path to the chibicc_cache example project
    let cache_path = "/Users/peng/Documents/Tmp/chibicc_cache";
    let example_project = Path::new(cache_path)
        .join("individual_files")
        .join("chibicc")
        .join("final-output");

    if !example_project.exists() {
        println!(
            "❌ Example project not found at: {}",
            example_project.display()
        );
        println!("   Please ensure the chibicc_cache project exists at the expected location.");
        return Ok(());
    }

    println!("📂 Analyzing project: {}", example_project.display());

    // Initialize the file manager
    let manager = RustFileManager::new(&example_project)?;

    println!("✅ Project discovered:");
    println!("   Type: {:?}", manager.project_type());
    println!("   Main source: {}", manager.main_source_path().display());
    println!("   Cargo.toml: {}", manager.cargo_toml_path().display());

    // Analyze the generated code structure
    println!("\n📊 Code Analysis");
    println!("===============");

    let total_lines = manager.line_count()?;
    println!("Total lines of code: {}", total_lines);

    // List all symbols
    let symbols = manager.list_symbols()?;
    println!("Found {} symbols:", symbols.len());

    let mut function_count = 0;
    let mut struct_count = 0;
    let mut enum_count = 0;
    let mut impl_count = 0;
    let mut trait_count = 0;

    for symbol in &symbols {
        match symbol.symbol_type {
            SymbolType::Function => function_count += 1,
            SymbolType::Struct => struct_count += 1,
            SymbolType::Enum => enum_count += 1,
            SymbolType::Impl => impl_count += 1,
            SymbolType::Trait => trait_count += 1,
            _ => {}
        }
    }

    println!("  📊 Symbol breakdown:");
    println!("     Functions: {}", function_count);
    println!("     Structs: {}", struct_count);
    println!("     Enums: {}", enum_count);
    println!("     Impl blocks: {}", impl_count);
    println!("     Traits: {}", trait_count);

    // Show largest functions (potential refactoring candidates)
    println!("\n📏 Largest Functions (potential refactoring targets):");
    let mut functions: Vec<_> = symbols
        .iter()
        .filter(|s| matches!(s.symbol_type, SymbolType::Function))
        .collect();
    functions.sort_by(|a, b| {
        let a_size = a.line_range.end - a.line_range.start;
        let b_size = b.line_range.end - b.line_range.start;
        b_size.cmp(&a_size)
    });

    for (i, func) in functions.iter().take(5).enumerate() {
        let size = func.line_range.end - func.line_range.start + 1;
        println!(
            "  {}. {} ({} lines, lines {}-{})",
            i + 1,
            func.name,
            size,
            func.line_range.start,
            func.line_range.end
        );
    }

    // Check for main function and analyze it
    if let Ok(main_fn) = manager.find_symbol("main") {
        println!("\n🎯 Main Function Analysis:");
        let main_size = main_fn.line_range.end - main_fn.line_range.start + 1;
        println!("   Size: {} lines", main_size);
        println!(
            "   Location: lines {}-{}",
            main_fn.line_range.start, main_fn.line_range.end
        );

        // Show first few lines
        let preview_lines: Vec<&str> = main_fn.content.lines().take(10).collect();
        println!("   Preview:");
        for (i, line) in preview_lines.iter().enumerate() {
            println!("     {}: {}", main_fn.line_range.start + i, line);
        }
        if main_fn.content.lines().count() > 10 {
            println!(
                "     ... ({} more lines)",
                main_fn.content.lines().count() - 10
            );
        }
    }

    // Analyze Cargo.toml dependencies
    println!("\n📦 Dependencies Analysis:");
    let cargo_content = manager.read_cargo_toml()?;

    if cargo_content.contains("[dependencies]") {
        let deps_start = cargo_content.find("[dependencies]").unwrap();
        let deps_section = &cargo_content[deps_start..];
        let deps_end = deps_section.find("\n[").unwrap_or(deps_section.len());
        let deps_content = &deps_section[..deps_end];

        let deps_lines: Vec<&str> = deps_content
            .lines()
            .skip(1)
            .filter(|line| !line.trim().is_empty())
            .collect();
        println!("   Found {} dependencies:", deps_lines.len());
        for dep in deps_lines {
            println!("     {}", dep.trim());
        }
    } else {
        println!("   No dependencies section found");
    }

    // Look for unsafe code blocks
    println!("\n⚠️  Unsafe Code Analysis:");
    let content = manager.read_main_source()?;
    let unsafe_count = content.matches("unsafe").count();
    println!("   'unsafe' keyword appears {} times", unsafe_count);

    if unsafe_count > 0 {
        println!(
            "   This is expected for C2Rust generated code, as it preserves C's unsafe operations"
        );
    }

    // Look for TODO/FIXME comments (common in generated code)
    let todo_count = content.matches("TODO").count() + content.matches("FIXME").count();
    if todo_count > 0 {
        println!("   Found {} TODO/FIXME comments", todo_count);
    }

    // Example modifications (commented out to avoid changing the actual file)
    println!("\n🛠️  Example Modifications (simulation):");
    println!("   (These would modify the actual file if uncommented)");

    println!("   1. Adding logging dependency:");
    println!("      manager.add_dependency(\"log\", \"0.4\")?;");

    println!("   2. Adding debug prints to main function:");
    println!("      // Insert at beginning of main function");
    println!("      // manager.insert_at_line(main_start + 1, \"    println!(\\\"Debug: Starting main\\\");\");");

    println!("   3. Formatting code:");
    println!("      // manager.format_code()?;");

    // Demonstrate read-only line operations
    println!("\n🔍 Sample Line Operations:");
    if total_lines >= 10 {
        let sample_lines = manager.read_lines(1, 10)?;
        println!("   First 10 lines:");
        for (i, line) in sample_lines.lines().enumerate() {
            println!("     {}: {}", i + 1, line);
        }
    }

    // Check if project compiles
    println!("\n🔨 Compilation Check:");
    print!("   Checking if project compiles... ");
    match manager.check_compilation() {
        Ok(true) => println!("✅ Project compiles successfully!"),
        Ok(false) => println!("❌ Project has compilation errors"),
        Err(e) => println!("⚠️  Could not check compilation: {}", e),
    }

    println!("\n🎉 Analysis Complete!");
    println!("   The rust_file_manager crate successfully analyzed the C2Rust generated project.");
    println!("   You can now use the manager to make targeted modifications to the code.");

    Ok(())
}
