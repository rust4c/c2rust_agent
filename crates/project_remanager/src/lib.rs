//! Project Reorganizer for C2Rust Agent
//!
//! This module extracts individual Rust projects from src_cache and reorganizes them
//! into a complete, compilable Rust workspace project.
//!
//! Philosophy: Keep it simple, stupid. No fancy abstractions, just move files around
//! and generate the right config files. If it works, ship it.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct FileMapping {
    pub category: String,
    pub file_type: String,
    pub source_path: String,
    pub target_path: String,
}

#[derive(Debug, Deserialize)]
pub struct MappingData {
    pub mappings: Vec<FileMapping>,
    pub timestamp: String,
    pub total_mappings: usize,
}

#[derive(Debug, Clone)]
pub struct RustProject {
    pub name: String,
    pub source_dir: PathBuf,
    pub has_main: bool,
    pub dependencies: Vec<String>,
}

/// The main project reorganizer
///
/// Does one thing: takes scattered Rust projects and makes them into
/// a proper workspace. No magic, no complexity.
pub struct ProjectReorganizer {
    src_cache_path: PathBuf,
    output_path: PathBuf,
}

impl ProjectReorganizer {
    /// Create a new reorganizer
    pub fn new(src_cache_path: PathBuf, output_path: PathBuf) -> Self {
        Self {
            src_cache_path,
            output_path,
        }
    }

    /// Main entry point - reorganize the entire project
    ///
    /// This is the only public method you need to call.
    /// It does everything: scan, analyze, reorganize.
    pub fn reorganize(&self) -> Result<()> {
        // Step 1: Clean the output directory
        self.prepare_output_dir()?;

        // Step 2: Scan for all Rust projects
        let projects = self.scan_rust_projects()?;

        if projects.is_empty() {
            return Err(anyhow::anyhow!("No Rust projects found in src_cache"));
        }

        // Step 3: Analyze project structure
        let (main_projects, lib_projects) = self.categorize_projects(projects);

        // Step 4: Create the workspace structure
        self.create_workspace_structure(&main_projects, &lib_projects)?;

        // Step 5: Copy all the code
        self.copy_project_contents(&main_projects, &lib_projects)?;

        // Step 6: Generate workspace configuration
        self.generate_workspace_config(&main_projects, &lib_projects)?;

        println!("Project reorganization completed successfully!");
        println!("Output directory: {}", self.output_path.display());

        Ok(())
    }

    /// Prepare the output directory
    fn prepare_output_dir(&self) -> Result<()> {
        if self.output_path.exists() {
            fs::remove_dir_all(&self.output_path)
                .context("Failed to clean existing output directory")?;
        }

        fs::create_dir_all(&self.output_path).context("Failed to create output directory")?;

        Ok(())
    }

    /// Scan for all Rust projects in individual_files
    fn scan_rust_projects(&self) -> Result<Vec<RustProject>> {
        let individual_files_path = self.src_cache_path.join("individual_files");

        if !individual_files_path.exists() {
            return Err(anyhow::anyhow!(
                "individual_files directory not found in src_cache"
            ));
        }

        let mut projects = Vec::new();

        for entry in fs::read_dir(&individual_files_path)? {
            let entry = entry?;
            let project_dir = entry.path();

            if !project_dir.is_dir() {
                continue;
            }

            // 支持两种命名：rust_project 与 rust-project
            let rust_project_path_underscore = project_dir.join("rust_project");
            let rust_project_path_dash = project_dir.join("rust-project");
            let rust_project_path = if rust_project_path_underscore.exists() {
                rust_project_path_underscore
            } else if rust_project_path_dash.exists() {
                rust_project_path_dash
            } else {
                continue;
            };

            let project_name = project_dir
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid project directory name"))?
                .to_string();

            let project = self.analyze_rust_project(&project_name, &rust_project_path)?;
            projects.push(project);
        }

        Ok(projects)
    }

    /// Analyze a single Rust project to extract metadata
    fn analyze_rust_project(&self, name: &str, project_path: &Path) -> Result<RustProject> {
        let cargo_toml_path = project_path.join("Cargo.toml");
        let src_path = project_path.join("src");
        let main_rs_path = src_path.join("main.rs");

        // Check if it has a main function
        let has_main = main_rs_path.exists();

        // Extract dependencies from Cargo.toml
        let dependencies = self.extract_dependencies(&cargo_toml_path)?;

        Ok(RustProject {
            name: name.to_string(),
            source_dir: project_path.to_path_buf(),
            has_main,
            dependencies,
        })
    }

    /// Extract dependencies from Cargo.toml
    fn extract_dependencies(&self, cargo_toml_path: &Path) -> Result<Vec<String>> {
        if !cargo_toml_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(cargo_toml_path)?;
        let toml: toml::Value = content.parse()?;

        let mut deps = Vec::new();

        if let Some(dependencies) = toml.get("dependencies") {
            if let Some(deps_table) = dependencies.as_table() {
                for (dep_name, _) in deps_table {
                    deps.push(dep_name.clone());
                }
            }
        }

        Ok(deps)
    }

    /// Categorize projects into main programs and libraries
    fn categorize_projects(
        &self,
        projects: Vec<RustProject>,
    ) -> (Vec<RustProject>, Vec<RustProject>) {
        let mut main_projects = Vec::new();
        let mut lib_projects = Vec::new();

        for project in projects {
            if project.has_main {
                main_projects.push(project);
            } else {
                lib_projects.push(project);
            }
        }

        (main_projects, lib_projects)
    }

    /// Create the basic workspace directory structure
    fn create_workspace_structure(
        &self,
        main_projects: &[RustProject],
        lib_projects: &[RustProject],
    ) -> Result<()> {
        // Create main structure
        fs::create_dir_all(self.output_path.join("src"))?;

        // Create bin directory for main programs
        if !main_projects.is_empty() {
            fs::create_dir_all(self.output_path.join("src/bin"))?;
        }

        // Create lib directories for libraries
        for lib_project in lib_projects {
            let lib_dir = self.output_path.join("src").join(&lib_project.name);
            fs::create_dir_all(lib_dir)?;
        }

        Ok(())
    }

    /// Copy all project contents to the new structure
    fn copy_project_contents(
        &self,
        main_projects: &[RustProject],
        lib_projects: &[RustProject],
    ) -> Result<()> {
        // Copy main programs to src/bin/
        for main_project in main_projects {
            self.copy_main_project(main_project)?;
        }

        // Copy libraries to src/lib_name/
        for lib_project in lib_projects {
            self.copy_lib_project(lib_project)?;
        }

        Ok(())
    }

    /// Copy a main program project
    fn copy_main_project(&self, project: &RustProject) -> Result<()> {
        let src_main_rs = project.source_dir.join("src/main.rs");
        let dest_main_rs = self
            .output_path
            .join("src/bin")
            .join(format!("{}.rs", project.name));

        if src_main_rs.exists() {
            fs::copy(&src_main_rs, &dest_main_rs)
                .with_context(|| format!("Failed to copy main.rs for {}", project.name))?;
        }

        // Copy other source files if they exist
        let src_dir = project.source_dir.join("src");
        if src_dir.exists() {
            for entry in fs::read_dir(&src_dir)? {
                let entry = entry?;
                let file_path = entry.path();

                if file_path.is_file() && file_path.file_name().unwrap() != "main.rs" {
                    if let Some(file_name) = file_path.file_name() {
                        let dest_path = self.output_path.join("src").join(file_name);
                        fs::copy(&file_path, &dest_path)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Copy a library project
    fn copy_lib_project(&self, project: &RustProject) -> Result<()> {
        let src_dir = project.source_dir.join("src");
        let dest_dir = self.output_path.join("src").join(&project.name);

        if !src_dir.exists() {
            return Ok(());
        }

        // Copy all source files
        for entry in fs::read_dir(&src_dir)? {
            let entry = entry?;
            let src_file = entry.path();

            if src_file.is_file() {
                if let Some(file_name) = src_file.file_name() {
                    let dest_file = dest_dir.join(file_name);
                    fs::copy(&src_file, &dest_file).with_context(|| {
                        format!(
                            "Failed to copy {} for {}",
                            file_name.to_string_lossy(),
                            project.name
                        )
                    })?;
                }
            }
        }

        // If there's no lib.rs, create one that re-exports main.rs content
        let lib_rs_path = dest_dir.join("lib.rs");
        if !lib_rs_path.exists() {
            let main_rs_path = dest_dir.join("main.rs");
            if main_rs_path.exists() {
                // Rename main.rs to lib.rs for library projects
                fs::rename(&main_rs_path, &lib_rs_path)?;
            } else {
                // Create a basic lib.rs
                fs::write(&lib_rs_path, format!("// Library: {}\n\n", project.name))?;
            }
        }

        Ok(())
    }

    /// Generate the workspace Cargo.toml
    fn generate_workspace_config(
        &self,
        main_projects: &[RustProject],
        lib_projects: &[RustProject],
    ) -> Result<()> {
        let cargo_toml_path = self.output_path.join("Cargo.toml");

        // Collect all unique dependencies
        let mut all_dependencies = HashMap::new();

        for project in main_projects.iter().chain(lib_projects.iter()) {
            for dep in &project.dependencies {
                all_dependencies.insert(dep.clone(), "0.2".to_string()); // Default version
            }
        }

        // Special handling for common dependencies
        if all_dependencies.contains_key("libc") {
            all_dependencies.insert("libc".to_string(), "0.2".to_string());
        }

        // Generate Cargo.toml content
        let mut cargo_content = String::new();
        cargo_content.push_str("[package]\n");
        cargo_content.push_str("name = \"translated_project\"\n");
        cargo_content.push_str("version = \"0.1.0\"\n");
        cargo_content.push_str("edition = \"2021\"\n\n");

        // Add dependencies
        if !all_dependencies.is_empty() {
            cargo_content.push_str("[dependencies]\n");
            for (dep_name, version) in &all_dependencies {
                cargo_content.push_str(&format!("{} = \"{}\"\n", dep_name, version));
            }
            cargo_content.push('\n');
        }

        // Add binary targets for main programs
        if !main_projects.is_empty() {
            for main_project in main_projects {
                cargo_content.push_str("[[bin]]\n");
                cargo_content.push_str(&format!("name = \"{}\"\n", main_project.name));
                cargo_content.push_str(&format!("path = \"src/bin/{}.rs\"\n\n", main_project.name));
            }
        }

        fs::write(&cargo_toml_path, cargo_content).context("Failed to write Cargo.toml")?;

        // Generate src/lib.rs if we have library modules
        if !lib_projects.is_empty() {
            self.generate_main_lib_rs(lib_projects)?;
        }

        Ok(())
    }

    /// Generate the main lib.rs that includes all library modules
    fn generate_main_lib_rs(&self, lib_projects: &[RustProject]) -> Result<()> {
        let lib_rs_path = self.output_path.join("src/lib.rs");

        let mut lib_content = String::new();
        lib_content.push_str("//! Translated C project\n");
        lib_content.push_str("//! \n");
        lib_content.push_str("//! This library contains all the translated C modules.\n\n");

        // Add module declarations
        for lib_project in lib_projects {
            lib_content.push_str(&format!("pub mod {};\n", lib_project.name));
        }

        fs::write(&lib_rs_path, lib_content).context("Failed to write src/lib.rs")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_project_reorganizer_creation() {
        let temp_dir = TempDir::new().unwrap();
        let src_cache = temp_dir.path().join("src_cache");
        let output = temp_dir.path().join("output");

        let reorganizer = ProjectReorganizer::new(src_cache, output.clone());

        assert_eq!(reorganizer.output_path, output);
    }

    #[test]
    fn test_prepare_output_dir() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        let reorganizer =
            ProjectReorganizer::new(temp_dir.path().to_path_buf(), output_path.clone());

        reorganizer.prepare_output_dir().unwrap();

        assert!(output_path.exists());
        assert!(output_path.is_dir());
    }

    #[test]
    fn test_extract_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let cargo_toml_path = temp_dir.path().join("Cargo.toml");

        let cargo_content = r#"
[package]
name = "test"
version = "0.1.0"

[dependencies]
libc = "0.2"
serde = "1.0"
"#;

        fs::write(&cargo_toml_path, cargo_content).unwrap();

        let reorganizer = ProjectReorganizer::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().join("output"),
        );

        let deps = reorganizer.extract_dependencies(&cargo_toml_path).unwrap();

        assert!(deps.contains(&"libc".to_string()));
        assert!(deps.contains(&"serde".to_string()));
    }
}
