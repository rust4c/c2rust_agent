use anyhow::Result;
use db_services::DatabaseManager;
use env_logger;
use log::info;
use main_processor::MainProcessor;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger
    env_logger::init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <cache_directory>", args[0]);
        eprintln!("Example: {} ./cache", args[0]);
        std::process::exit(1);
    }

    let cache_dir = PathBuf::from(&args[1]);

    if !cache_dir.exists() {
        eprintln!(
            "Error: Cache directory does not exist: {}",
            cache_dir.display()
        );
        std::process::exit(1);
    }

    info!("Starting C to Rust translation workflow");
    info!("Cache directory: {}", cache_dir.display());

    // Create main processor in test mode (no LLM required)
    let processor = MainProcessor::new_test_mode(cache_dir);

    // Option 1: Run without database (basic translation) in test mode
    println!("Running translation workflow in test mode (no LLM required)...");
    let stats = processor.run_translation_workflow().await?;

    // Print final statistics
    println!("\n=== Final Statistics ===");
    println!("Total successful: {}", stats.successful_translations.len());
    println!("Total failed: {}", stats.failed_translations.len());

    if !stats.failed_translations.is_empty() {
        println!("\nFailed projects details:");
        for (project, error) in &stats.failed_translations {
            println!("  {} -> {}", project, error);
        }
    }

    Ok(())
}

// Example of how to use with database context
#[allow(dead_code)]
async fn example_with_database(cache_dir: PathBuf) -> Result<()> {
    // Create database manager (requires proper configuration)
    let db_manager = DatabaseManager::new_default().await?;

    // Create processor in test mode
    let processor = MainProcessor::new_test_mode(cache_dir);

    // Run translation workflow with enhanced context
    let stats = processor
        .run_translation_workflow_with_database(&db_manager)
        .await?;

    println!("Translation completed with database context");
    println!("Successful: {}", stats.successful_translations.len());
    println!("Failed: {}", stats.failed_translations.len());

    Ok(())
}

// Example of processing specific project types
#[allow(dead_code)]
async fn example_targeted_processing(cache_dir: PathBuf) -> Result<()> {
    use main_processor::{ProjectInfo, ProjectType};

    // You can also process specific projects manually
    let processor = MainProcessor::new_test_mode(cache_dir);

    // Create a test project
    let test_project = ProjectInfo {
        name: "example_project".to_string(),
        path: PathBuf::from("./cache/单独文件/example_project"),
        project_type: ProjectType::SingleFile,
    };

    println!("Processing single project: {}", test_project.name);

    // This would normally be called internally by run_translation_workflow
    // but you can also call individual steps if needed

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_example_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let cache_dir = temp_dir.path().to_path_buf();

        // Create minimal cache structure
        let single_files = cache_dir.join("individual_files");
        fs::create_dir_all(&single_files).unwrap();
        fs::create_dir_all(single_files.join("test_project")).unwrap();

        // Create mapping.json
        fs::write(cache_dir.join("mapping.json"), "{}").unwrap();

        // Create a simple C file
        let test_c_content = r#"
#include <stdio.h>

int main() {
    printf("Hello, World!\n");
    return 0;
}
"#;
        fs::write(single_files.join("test_project/main.c"), test_c_content).unwrap();

        // Test the processor in test mode
        let processor = MainProcessor::new_test_mode(cache_dir);

        // This should discover the test project
        let projects = processor.discover_projects().await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "test_project");
    }
}
