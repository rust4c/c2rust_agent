//! Example usage of the Project Reorganizer
//!
//! This example demonstrates how to use the ProjectReorganizer to convert
//! scattered individual Rust projects into a unified workspace.

use anyhow::Result;
use project_remanager::ProjectReorganizer;
use std::path::PathBuf;

fn main() -> Result<()> {
    // Example paths - adjust these to match your actual project structure
    let src_cache_path = PathBuf::from("../../test-projects/translate_chibicc/src_cache");
    let output_path = PathBuf::from("./reorganized_output");

    println!("🔧 Project Reorganizer Example");
    println!("===============================");
    println!();

    // Check if the source exists
    if !src_cache_path.exists() {
        println!(
            "❌ Source cache path does not exist: {}",
            src_cache_path.display()
        );
        println!("   Please run this example from the correct directory or");
        println!("   adjust the src_cache_path in the example code.");
        return Ok(());
    }

    let individual_files = src_cache_path.join("individual_files");
    if !individual_files.exists() {
        println!(
            "❌ No individual_files directory found in: {}",
            src_cache_path.display()
        );
        println!("   Make sure you have processed a C project first.");
        return Ok(());
    }

    println!("✅ Found source cache: {}", src_cache_path.display());
    println!("📂 Output will be created at: {}", output_path.display());
    println!();

    // Create the reorganizer
    let reorganizer = ProjectReorganizer::new(src_cache_path, output_path.clone());

    // Run the reorganization
    match reorganizer.reorganize() {
        Ok(()) => {
            println!();
            println!("🎉 SUCCESS! Project has been reorganized.");
            println!("📁 Check the output at: {}", output_path.display());
            println!();
            println!("Try these commands:");
            println!("  cd {}", output_path.display());
            println!("  cargo check");
            println!("  cargo build");
        }
        Err(e) => {
            println!("❌ FAILED to reorganize project: {}", e);
            println!();
            println!("Common issues:");
            println!("  - Make sure the src_cache contains processed Rust projects");
            println!("  - Check that individual_files/ directory exists");
            println!("  - Verify you have write permissions to the output directory");
        }
    }

    Ok(())
}
