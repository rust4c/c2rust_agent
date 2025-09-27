use anyhow::Result;
use db_services::DatabaseManager;
use log::info;
use main_processor::{pkg_config, MainProcessor};
use std::path::PathBuf;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
    // Examples rely on the application's logging initialization.

    // Load config and create processor
    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <input_directory>", args[0]);
        eprintln!("Example: {} ./c_projects", args[0]);
        std::process::exit(1);
    }

    let input_dir = PathBuf::from(&args[1]);

    if !input_dir.exists() {
        eprintln!(
            "Error: Input directory does not exist: {}",
            input_dir.display()
        );
        std::process::exit(1);
    }

    info!("Starting C to Rust translation workflow");
    info!("Input directory: {}", input_dir.display());

    // Discover projects: if input_dir looks like a src_cache (has individual_files/),
    // use the dedicated traversal; otherwise fall back to generic discovery.
    let projects = if input_dir.join("individual_files").exists() {
        info!(
            "Detected src_cache structure at {} — using individual_files traversal",
            input_dir.display()
        );
        processor.discover_src_cache_projects(&input_dir).await?
    } else {
        discover_c_projects(&input_dir).await?
    };

    if projects.is_empty() {
        println!("No C projects found in {}", input_dir.display());
        return Ok(());
    }

    println!("Found {} C projects to translate:", projects.len());
    for (i, project) in projects.iter().enumerate() {
        println!("  {}. {}", i + 1, project.display());
    }

    // Process all projects using batch processing
    println!("\nStarting batch translation...");
    match processor.process_batch(projects).await {
        Ok(()) => {
            println!("✅ All translations completed successfully!");
        }
        Err(e) => {
            println!("❌ Some translations failed: {}", e);
        }
    }

    Ok(())
}

/// Discover C projects in the given directory
async fn discover_c_projects(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_dir() {
            // Check if this directory contains C files
            if contains_c_files(&path).await? {
                projects.push(path);
            }
        } else if path.is_file() {
            // Check if this is a standalone C file
            if let Some(ext) = path.extension() {
                if ext == "c" || ext == "h" {
                    // For standalone files, use the parent directory
                    if let Some(parent) = path.parent() {
                        if !projects.contains(&parent.to_path_buf()) {
                            projects.push(parent.to_path_buf());
                        }
                    }
                }
            }
        }
    }

    Ok(projects)
}

/// Check if a directory contains C files
async fn contains_c_files(dir: &PathBuf) -> Result<bool> {
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "c" || ext == "h" {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

// Example of how to use with database context
#[allow(dead_code)]
async fn example_with_database(input_dir: PathBuf) -> Result<()> {
    // Create database manager (requires proper configuration)
    let _db_manager = DatabaseManager::new_default().await?;

    // Discover projects
    let projects = discover_c_projects(&input_dir).await?;

    println!(
        "Processing {} projects with database context",
        projects.len()
    );

    // Create a processor with default config
    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);

    // Process each project individually for better control
    for project in projects {
        println!("Processing: {}", project.display());

        match processor.process_single(&project).await {
            Ok(()) => {
                println!("✅ Successfully processed: {}", project.display());
            }
            Err(e) => {
                println!("❌ Failed to process {}: {}", project.display(), e);
            }
        }
    }

    Ok(())
}

// Example of processing specific project types
#[allow(dead_code)]
async fn example_targeted_processing(input_dir: PathBuf) -> Result<()> {
    println!("Running targeted processing workflow");

    // Find all C files recursively
    let c_files = find_c_files_recursive(&input_dir).await?;

    println!("Found {} C files:", c_files.len());
    for file in &c_files {
        println!("  {}", file.display());
    }

    // Group files by directory (project)
    let mut projects: std::collections::HashMap<PathBuf, Vec<PathBuf>> =
        std::collections::HashMap::new();

    for file in c_files {
        if let Some(parent) = file.parent() {
            projects
                .entry(parent.to_path_buf())
                .or_insert_with(Vec::new)
                .push(file);
        }
    }

    println!("\nGrouped into {} projects:", projects.len());
    for (dir, files) in &projects {
        println!("  {} ({} files)", dir.display(), files.len());
    }

    // Process each project
    let project_dirs: Vec<PathBuf> = projects.keys().cloned().collect();
    // Use a fresh processor with default config
    let cfg = pkg_config::get_config().unwrap_or_default();
    let processor = MainProcessor::new(cfg);
    processor.process_batch(project_dirs).await?;

    Ok(())
}

/// Recursively find all C files
async fn find_c_files_recursive(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut c_files = Vec::new();
    let mut stack = vec![dir.clone()];

    while let Some(current_dir) = stack.pop() {
        let mut entries = fs::read_dir(&current_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "c" || ext == "h" {
                        c_files.push(path);
                    }
                }
            }
        }
    }

    Ok(c_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_discover_c_projects() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create test structure
        let project1 = root.join("project1");
        let project2 = root.join("project2");
        fs::create_dir_all(&project1).unwrap();
        fs::create_dir_all(&project2).unwrap();

        // Create C files
        fs::write(project1.join("main.c"), "int main() { return 0; }").unwrap();
        fs::write(
            project1.join("utils.h"),
            "#ifndef UTILS_H\n#define UTILS_H\n#endif",
        )
        .unwrap();
        fs::write(project2.join("app.c"), "int main() { return 0; }").unwrap();

        // Create non-C file (should be ignored)
        fs::write(root.join("readme.txt"), "This is a readme").unwrap();

        let projects = discover_c_projects(&root).await.unwrap();

        // Should find 2 projects
        assert_eq!(projects.len(), 2);
        assert!(projects.contains(&project1));
        assert!(projects.contains(&project2));
    }

    #[tokio::test]
    async fn test_contains_c_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Directory with C files
        let with_c = root.join("with_c");
        fs::create_dir_all(&with_c).unwrap();
        fs::write(with_c.join("test.c"), "int main() { return 0; }").unwrap();

        // Directory without C files
        let without_c = root.join("without_c");
        fs::create_dir_all(&without_c).unwrap();
        fs::write(without_c.join("readme.txt"), "No C files here").unwrap();

        assert!(contains_c_files(&with_c).await.unwrap());
        assert!(!contains_c_files(&without_c).await.unwrap());
    }

    #[tokio::test]
    async fn test_find_c_files_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create nested structure
        let level1 = root.join("level1");
        let level2 = level1.join("level2");
        fs::create_dir_all(&level2).unwrap();

        // Create C files at different levels
        fs::write(root.join("root.c"), "// root level").unwrap();
        fs::write(level1.join("level1.c"), "// level 1").unwrap();
        fs::write(level1.join("level1.h"), "// level 1 header").unwrap();
        fs::write(level2.join("level2.c"), "// level 2").unwrap();

        let c_files = find_c_files_recursive(&root).await.unwrap();

        assert_eq!(c_files.len(), 4);

        // Check that all files are found
        let file_names: Vec<String> = c_files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(file_names.contains(&"root.c".to_string()));
        assert!(file_names.contains(&"level1.c".to_string()));
        assert!(file_names.contains(&"level1.h".to_string()));
        assert!(file_names.contains(&"level2.c".to_string()));
    }
}
