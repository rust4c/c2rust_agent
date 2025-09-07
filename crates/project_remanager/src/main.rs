//! Project Reorganizer CLI
//!
//! Command-line interface for reorganizing C2Rust translated projects.
//!
//! Usage:
//!   project_remanager <src_cache_path> <output_path>
//!
//! Example:
//!   project_remanager ./test-projects/translate_chibicc/src_cache ./output/reorganized_project

use anyhow::{Context, Result};
use project_remanager::ProjectReorganizer;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: {} <src_cache_path> <output_path>", args[0]);
        eprintln!();
        eprintln!("Arguments:");
        eprintln!("  src_cache_path  Path to the src_cache directory containing individual_files/");
        eprintln!("  output_path     Path where the reorganized project should be created");
        eprintln!();
        eprintln!("Example:");
        eprintln!(
            "  {} ./test-projects/translate_chibicc/src_cache ./output/chibicc_rust",
            args[0]
        );
        std::process::exit(1);
    }

    let src_cache_path = PathBuf::from(&args[1]);
    let output_path = PathBuf::from(&args[2]);

    // Validate input path
    if !src_cache_path.exists() {
        return Err(anyhow::anyhow!(
            "Source cache directory does not exist: {}",
            src_cache_path.display()
        ));
    }

    let individual_files_path = src_cache_path.join("individual_files");
    if !individual_files_path.exists() {
        return Err(anyhow::anyhow!(
            "individual_files directory not found in src_cache: {}",
            individual_files_path.display()
        ));
    }

    println!("🔧 C2Rust Project Reorganizer");
    println!("Source: {}", src_cache_path.display());
    println!("Output: {}", output_path.display());
    println!();

    // Create the reorganizer and run it
    let reorganizer = ProjectReorganizer::new(src_cache_path, output_path.clone());

    reorganizer
        .reorganize()
        .context("Failed to reorganize the project")?;

    println!();
    println!("✅ Project reorganization completed successfully!");
    println!("📁 Output location: {}", output_path.display());
    println!();
    println!("Next steps:");
    println!("  cd {}", output_path.display());
    println!("  cargo check    # Check if the project compiles");
    println!("  cargo build    # Build the project");
    println!("  cargo run --bin <binary_name>  # Run a specific binary");

    Ok(())
}
