use crate::pkg_config::PreprocessConfig;
use anyhow::{Context, Result, anyhow};
use glob::Pattern;
use indicatif::{MultiProgress, ProgressBar, ProgressIterator, ProgressStyle};
use log::{error, info, warn};
use rayon::prelude::*;
use relation_analy::generate_c_dependency_graph;
use serde::Serialize;
use serde_json;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use module_installer::CompiledbInstaller;

/// 文件分类类型
#[derive(Debug, Clone, PartialEq)]
pub enum FileCategory {
    /// 配对文件（包含源文件和头文件）
    Paired { source: PathBuf, header: PathBuf },
    /// 单独文件（独立的源文件或头文件）
    Individual(PathBuf),
    /// 不相关文件（非源码文件）
    Unrelated(PathBuf),
}

/// 文件映射信息
#[derive(Debug, Clone, Serialize)]
pub struct FileMapping {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub file_type: String,
    pub category: String,
}

/// 使用 pkg_config::PreprocessConfig

/// 处理统计信息
#[derive(Debug, Default, Serialize)]
pub struct ProcessingStats {
    pub total_files: usize,
    pub paired_files: usize,
    pub individual_files: usize,
    pub unrelated_files: usize,
    pub skipped_files: usize,
    pub processing_time: f64,
    pub total_size: u64,
    pub errors: Vec<String>,
    pub mapping_count: usize,
}

/// File preprocessor
pub struct CProjectPreprocessor {
    config: PreprocessConfig,
    stats: ProcessingStats,
    file_mappings: Vec<FileMapping>,
}

impl CProjectPreprocessor {
    /// Create new preprocessor
    pub fn new(config: Option<PreprocessConfig>) -> Self {
        let config = match config {
            Some(config) => config,
            None => PreprocessConfig::default(),
        };
        CProjectPreprocessor {
            config,
            stats: ProcessingStats::default(),
            file_mappings: Vec::new(),
        }
    }

    /// Preprocess project files
    pub fn preprocess_project(
        &mut self,
        source_dir: &Path,
        output_dir: &Path,
    ) -> Result<ProcessingStats> {
        let start_time = Instant::now();
        let m = MultiProgress::new();
        let main_pb = m.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("🚀 Starting project file preprocessing...");

        // Validate directory
        if !source_dir.exists() || !source_dir.is_dir() {
            return Err(anyhow::anyhow!(
                "Source directory does not exist or is not a directory"
            ));
        }

        // Create output directory structure
        main_pb.set_message("📁 Creating output directory structure...");
        self.create_output_structure(output_dir)?;

        // Generate compile_commands.json
        main_pb.set_message("⚙️  Generating compile_commands.json...");
        self.generate_compiledb(source_dir, output_dir)?;

        // Scan and categorize files
        main_pb.set_message("🔍 Scanning project files...");
        let all_files = self.scan_files(source_dir, &m)?;

        main_pb.set_message("📋 Categorizing files...");
        let categorized_files = self.categorize_files(&all_files, &m)?;

        // Generate mapping files
        main_pb.set_message("🗺️  Generating file mappings...");
        self.generate_mapping(&categorized_files, source_dir, output_dir)?;
        self.save_mapping(output_dir)?;

        // Process categorized files
        main_pb.set_message("📦 Processing categorized files...");
        self.process_categorized_files(&categorized_files, output_dir, &m)?;

        // Relationship analysis
        main_pb.set_message("🔗 Performing relationship analysis...");
        self.relation_analysis(output_dir)?;

        // Generate report
        main_pb.set_message("📊 Generating processing report...");
        self.generate_report(output_dir)?;

        // Record processing time
        self.stats.processing_time = start_time.elapsed().as_secs_f64();
        main_pb.finish_with_message("✅ Preprocessing completed!");

        Ok(std::mem::take(&mut self.stats))
    }

    fn relation_analysis(&self, source_dir: &Path) -> Result<()> {
        match generate_c_dependency_graph(source_dir) {
            Ok(rel) => {
                info!("Relation graph: {:#?}", rel);
                let _include_edges: usize = rel
                    .files
                    .values()
                    .map(|n| n.local_includes.len() + n.system_includes.len())
                    .sum();
                info!(
                    "Include dirs: {} | Link libs: {} | Link dirs: {}",
                    rel.build.include_dirs.len(),
                    rel.build.link_libs.len(),
                    rel.build.link_dirs.len()
                );
            }
            Err(e) => {
                eprintln!("❌ Failed to generate relation graph: {e}");
                error!("Failed to generate relation graph: {e}");
            }
        }
        Ok(())
    }

    /// Generate compile_commands.json
    fn generate_compiledb(&self, source_dir: &Path, output_dir: &Path) -> Result<()> {
        // Create uv virtual environment and install compiledb
        let installer = CompiledbInstaller::new()
            .with_uv_command(&self.config.uv_command)
            .with_mirrors(self.config.uv_mirror_sources());
        let compiledb_venv = self
            .prepare_compiledb_environment(&installer, output_dir)
            .context("Unable to prepare compiledb virtual environment")?;
        info!("compiledb virtual environment path: {:?}", compiledb_venv);
        info!("Generating compilation database using compiledb...");

        if let Err(err) = std::env::set_current_dir(source_dir) {
            error!("Directory change failed: {}", err);
        }

        let status = Command::new(output_dir.join(".compiledb-venv/bin/compiledb"))
            // .arg("-n")
            .arg("make")
            .status()
            .map_err(|e| anyhow!("Failed to run compiledb: {}", e))?;

        fs::copy(
            source_dir.join("compile_commands.json"),
            output_dir.join("compile_commands.json"),
        )?;

        if !status.success() {
            error!("compiledb failed with exit code: {}", status);
            return Err(anyhow!("compiledb failed with exit code: {}", status));
        }

        if let Err(err) = std::env::set_current_dir(output_dir) {
            error!("Directory change failed: {}", err);
        }

        Ok(())
    }

    /// Create output directory structure
    fn create_output_structure(&self, output_dir: &Path) -> Result<()> {
        let dirs = [
            output_dir.join("paired_files"),     // Paired files
            output_dir.join("individual_files"), // Individual files
            output_dir.join("unrelated_files"),  // Unrelated files
        ];

        for dir in &dirs {
            fs::create_dir_all(dir)
                .with_context(|| format!("Unable to create directory: {:?}", dir))?;
        }
        Ok(())
    }

    /// Scan files
    fn scan_files(&mut self, source_dir: &Path, m: &MultiProgress) -> Result<Vec<PathBuf>> {
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message("🔍 Scanning files...");

        let mut files = Vec::new();
        let exclude_patterns: Vec<_> = self
            .config
            .exclude_patterns
            .iter()
            .map(|p| Pattern::new(p).unwrap())
            .collect();

        let entries: Vec<_> = walkdir::WalkDir::new(source_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();

        let scan_pb = m.add(ProgressBar::new(entries.len() as u64));
        scan_pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:30.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap(),
        );
        scan_pb.set_message("Scanning");

        for entry in entries.iter().progress_with(scan_pb.clone()) {
            let path = entry.path();
            let relative_path = path.strip_prefix(source_dir).unwrap_or(path);

            // Check exclude patterns
            if exclude_patterns
                .iter()
                .any(|p| p.matches_path(relative_path))
            {
                self.stats.skipped_files += 1;
                continue;
            }

            if let Ok(metadata) = fs::metadata(path) {
                files.push(path.to_path_buf());
                self.stats.total_size += metadata.len();
            } else {
                self.stats
                    .errors
                    .push(format!("Unable to access file: {}", path.display()));
                self.stats.skipped_files += 1;
            }
        }

        self.stats.total_files = files.len();
        pb.finish_with_message(format!(
            "✅ Scanning completed, found {} files",
            files.len()
        ));
        Ok(files)
    }

    /// File categorization
    fn categorize_files(
        &mut self,
        files: &[PathBuf],
        m: &MultiProgress,
    ) -> Result<Vec<FileCategory>> {
        let pb = m.add(ProgressBar::new(files.len() as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.yellow} [{elapsed_precise}] [{bar:30.yellow/blue}] {pos}/{len} 📋 {msg}")
                .unwrap()
        );
        pb.set_message("Categorizing files");

        let mut categorized = Vec::new();
        let mut processed_files = HashSet::new();

        // Separate source files and header files
        let source_files: Vec<_> = files.iter().filter(|f| self.is_source_file(f)).collect();
        let header_files: Vec<_> = files.iter().filter(|f| self.is_header_file(f)).collect();

        pb.set_message(format!(
            "Found {} source files, {} header files",
            source_files.len(),
            header_files.len()
        ));

        // Find paired files
        for source_file in source_files.iter().progress_with(pb.clone()) {
            if processed_files.contains(*source_file) {
                continue;
            }

            if let Some(header_file) = self.find_matching_header(source_file, &header_files) {
                categorized.push(FileCategory::Paired {
                    source: (*source_file).clone(),
                    header: header_file.clone(),
                });
                processed_files.insert((*source_file).clone());
                processed_files.insert(header_file);
                self.stats.paired_files += 2;
            }
        }

        // Process individual source files and header files
        for file in files.iter().progress_with(pb.clone()) {
            if processed_files.contains(file) {
                continue;
            }

            if self.is_source_file(file) || self.is_header_file(file) {
                categorized.push(FileCategory::Individual(file.clone()));
                processed_files.insert(file.clone());
                self.stats.individual_files += 1;
            }
        }

        // Process unrelated files
        for file in files.iter().progress_with(pb.clone()) {
            if !processed_files.contains(file) {
                categorized.push(FileCategory::Unrelated(file.clone()));
                self.stats.unrelated_files += 1;
            }
        }

        pb.finish_with_message("✅ File categorization completed");
        Ok(categorized)
    }

    /// Process categorized files
    fn process_categorized_files(
        &mut self,
        categorized_files: &[FileCategory],
        output_dir: &Path,
        m: &MultiProgress,
    ) -> Result<()> {
        let pb = m.add(ProgressBar::new(categorized_files.len() as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:30.cyan/blue}] {pos}/{len} 📦 {msg}")
                .unwrap(),
        );
        pb.set_message("Processing files");

        let errors = Arc::new(Mutex::new(Vec::new()));

        categorized_files.par_iter().for_each(|category| {
            match category {
                FileCategory::Paired { source, header } => {
                    let pair_name = self.get_pair_name(source);
                    let target_dir = output_dir.join("paired_files").join(&pair_name);

                    if let Err(e) = self.process_paired_files(source, header, &target_dir) {
                        errors.lock().unwrap().push(format!(
                            "Failed to process paired file {}: {}",
                            pair_name, e
                        ));
                    }
                }
                FileCategory::Individual(file) => {
                    let file_name = self.get_file_name(file);
                    let target_dir = output_dir.join("individual_files").join(&file_name);

                    if let Err(e) = self.process_individual_file(file, &target_dir) {
                        errors.lock().unwrap().push(format!(
                            "Failed to process individual file {}: {}",
                            file_name, e
                        ));
                    }
                }
                FileCategory::Unrelated(file) => {
                    let target_dir = output_dir.join("unrelated_files");

                    if let Err(e) = self.process_unrelated_file(file, &target_dir) {
                        errors.lock().unwrap().push(format!(
                            "Failed to process unrelated file {}: {}",
                            file.display(),
                            e
                        ));
                    }
                }
            }
            pb.inc(1);
        });

        // Collect errors
        self.stats.errors.extend(errors.lock().unwrap().drain(..));
        pb.finish_with_message("✅ File processing completed");
        Ok(())
    }

    /// Process paired files
    fn process_paired_files(&self, source: &Path, header: &Path, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;

        let source_target = target_dir.join(source.file_name().unwrap());
        let header_target = target_dir.join(header.file_name().unwrap());

        self.copy_file(source, &source_target)?;
        self.copy_file(header, &header_target)?;

        Ok(())
    }

    /// Process individual file
    fn process_individual_file(&self, file: &Path, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;
        let target_file = target_dir.join(file.file_name().unwrap());
        self.copy_file(file, &target_file)?;
        Ok(())
    }

    /// Process unrelated file
    fn process_unrelated_file(&self, file: &Path, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;
        let target_file = target_dir.join(file.file_name().unwrap());
        self.copy_file(file, &target_file)?;
        Ok(())
    }

    /// Copy file
    fn copy_file(&self, src: &Path, dst: &Path) -> Result<()> {
        let metadata = fs::metadata(src)?;

        if metadata.len() > self.config.large_file_threshold {
            self.copy_large_file(src, dst)?;
        } else {
            fs::copy(src, dst)?;
        }

        Ok(())
    }

    /// Copy large file (in chunks)
    fn copy_large_file(&self, src: &Path, dst: &Path) -> Result<()> {
        let mut src_file = BufReader::with_capacity(self.config.chunk_size, File::open(src)?);
        let mut dst_file = BufWriter::with_capacity(self.config.chunk_size, File::create(dst)?);
        let mut buffer = vec![0u8; self.config.chunk_size];

        loop {
            let bytes_read = src_file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            dst_file.write_all(&buffer[..bytes_read])?;
        }

        dst_file.flush()?;
        Ok(())
    }

    /// Find matching header file
    fn find_matching_header(
        &self,
        source_file: &Path,
        header_files: &[&PathBuf],
    ) -> Option<PathBuf> {
        for (source_pattern, header_pattern) in &self.config.pairing_rules {
            if let Ok(regex) = regex::Regex::new(source_pattern) {
                let source_str = source_file.to_string_lossy();
                if let Some(captures) = regex.captures(&source_str) {
                    let mut expected_header = header_pattern.clone();

                    // Replace capture groups
                    for i in 0..captures.len() {
                        if let Some(cap) = captures.get(i) {
                            expected_header =
                                expected_header.replace(&format!("\\{}", i), cap.as_str());
                        }
                    }

                    // Find matching header file
                    for header_file in header_files {
                        let header_str = header_file.to_string_lossy();
                        if header_str.contains(&expected_header)
                            || header_file.file_name() == Path::new(&expected_header).file_name()
                        {
                            return Some((*header_file).clone());
                        }
                    }
                }
            }
        }
        None
    }

    /// Get pair name
    fn get_pair_name(&self, source_file: &Path) -> String {
        source_file
            .file_stem()
            .unwrap_or(source_file.file_name().unwrap())
            .to_string_lossy()
            .to_string()
    }

    /// Get file name (without extension)
    fn get_file_name(&self, file: &Path) -> String {
        file.file_stem()
            .unwrap_or(file.file_name().unwrap())
            .to_string_lossy()
            .to_string()
    }

    /// Check if it's a source file
    fn is_source_file(&self, file: &Path) -> bool {
        if let Some(ext) = file.extension() {
            let ext_l = ext.to_string_lossy().to_string().to_ascii_lowercase();
            let with_dot = format!(".{}", ext_l);
            self.config.source_extensions.iter().any(|e| {
                let el = e.to_ascii_lowercase();
                el == with_dot || el == ext_l
            })
        } else {
            false
        }
    }

    /// Check if it's a header file
    fn is_header_file(&self, file: &Path) -> bool {
        if let Some(ext) = file.extension() {
            let ext_l = ext.to_string_lossy().to_string().to_ascii_lowercase();
            let with_dot = format!(".{}", ext_l);
            self.config.header_extensions.iter().any(|e| {
                let el = e.to_ascii_lowercase();
                el == with_dot || el == ext_l
            })
        } else {
            false
        }
    }

    /// Generate file mapping
    // TODO: Implement file mapping generation logic
    fn generate_mapping(
        &mut self,
        categorized_files: &[FileCategory],
        source_dir: &Path,
        output_dir: &Path,
    ) -> Result<()> {
        self.file_mappings.clear();

        for category in categorized_files {
            match category {
                FileCategory::Paired { source, header } => {
                    let pair_name = self.get_pair_name(source);
                    let target_dir = output_dir.join("paired_files").join(&pair_name);

                    let source_mapping = FileMapping {
                        source_path: source
                            .strip_prefix(source_dir)
                            .unwrap_or(source)
                            .to_path_buf(),
                        target_path: target_dir.join(source.file_name().unwrap()),
                        file_type: "source".to_string(),
                        category: "paired".to_string(),
                    };

                    let header_mapping = FileMapping {
                        source_path: header
                            .strip_prefix(source_dir)
                            .unwrap_or(header)
                            .to_path_buf(),
                        target_path: target_dir.join(header.file_name().unwrap()),
                        file_type: "header".to_string(),
                        category: "paired".to_string(),
                    };

                    self.file_mappings.push(source_mapping);
                    self.file_mappings.push(header_mapping);
                }
                FileCategory::Individual(file) => {
                    let file_name = self.get_file_name(file);
                    let target_dir = output_dir.join("individual_files").join(&file_name);

                    // TODO: Change this
                    let file_type = if self.is_source_file(file) {
                        "source"
                    } else if self.is_header_file(file) {
                        "header"
                    } else {
                        "unknown"
                    };

                    let mapping = FileMapping {
                        source_path: file.strip_prefix(source_dir).unwrap_or(file).to_path_buf(),
                        target_path: target_dir.join(file.file_name().unwrap()),
                        file_type: file_type.to_string(),
                        category: "individual".to_string(),
                    };

                    self.file_mappings.push(mapping);
                }
                FileCategory::Unrelated(file) => {
                    let target_dir = output_dir.join("unrelated_files");

                    let mapping = FileMapping {
                        source_path: file.strip_prefix(source_dir).unwrap_or(file).to_path_buf(),
                        target_path: target_dir.join(file.file_name().unwrap()),
                        file_type: "unrelated".to_string(),
                        category: "unrelated".to_string(),
                    };

                    self.file_mappings.push(mapping);
                }
            }
        }

        self.stats.mapping_count = self.file_mappings.len();
        Ok(())
    }

    /// Save mapping file
    fn save_mapping(&self, output_dir: &Path) -> Result<()> {
        let mapping_path = output_dir.join("mapping.json");
        let mapping_json = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "total_mappings": self.file_mappings.len(),
            "mappings": &self.file_mappings
        });

        fs::write(&mapping_path, serde_json::to_string_pretty(&mapping_json)?)?;
        Ok(())
    }

    /// Generate processing report
    fn generate_report(&self, output_dir: &Path) -> Result<()> {
        let report_path = output_dir.join("processing_report.json");
        let text_report_path = output_dir.join("processing_log.txt");

        // JSON report
        let json_report = serde_json::json!({
            "statistics": &self.stats,
            "config": &self.config,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        fs::write(&report_path, serde_json::to_string_pretty(&json_report)?)?;

        // Text report
        let mut text_report = String::new();
        text_report.push_str("C Project File Preprocessing Report\n");
        text_report.push_str("==================================\n\n");
        text_report.push_str(&format!("Total files: {}\n", self.stats.total_files));
        text_report.push_str(&format!("Paired files: {}\n", self.stats.paired_files));
        text_report.push_str(&format!(
            "Individual files: {}\n",
            self.stats.individual_files
        ));
        text_report.push_str(&format!(
            "Unrelated files: {}\n",
            self.stats.unrelated_files
        ));
        text_report.push_str(&format!("Skipped files: {}\n", self.stats.skipped_files));
        text_report.push_str(&format!("File mappings: {}\n", self.stats.mapping_count));
        text_report.push_str(&format!(
            "Processing time: {:.2} seconds\n",
            self.stats.processing_time
        ));
        text_report.push_str(&format!(
            "Total size: {}\n\n",
            format_size(self.stats.total_size)
        ));

        // Terminal output processing report
        println!("\n🎯 C Project File Preprocessing Report");
        println!("═══════════════════════════════════════");
        println!("📊 Processing Statistics:");
        println!("   • Total files: {}", self.stats.total_files);
        println!(
            "   • Paired files: {} ({} pairs)",
            self.stats.paired_files,
            self.stats.paired_files / 2
        );
        println!("   • Individual files: {}", self.stats.individual_files);
        println!("   • Unrelated files: {}", self.stats.unrelated_files);
        println!("   • Skipped files: {}", self.stats.skipped_files);
        println!("   • File mappings: {}", self.stats.mapping_count);

        println!("\n🔗 Relationship Analysis:");
        match generate_c_dependency_graph(output_dir) {
            Ok(rel) => {
                let include_edges: usize = rel
                    .files
                    .values()
                    .map(|n| n.local_includes.len() + n.system_includes.len())
                    .sum();
                println!("   • File nodes: {}", rel.files.len());
                println!("   • Include relationships: {}", include_edges);
                println!("   • Include directories: {}", rel.build.include_dirs.len());
                println!("   • Link libraries: {}", rel.build.link_libs.len());
                println!("   • Link directories: {}", rel.build.link_dirs.len());
                println!("   • Dependency graph generation: ✅ Success");
            }
            Err(_) => {
                println!("   • Dependency graph generation: ❌ Failed");
            }
        }

        println!("\n⏱️  Performance Metrics:");
        println!(
            "   • Processing time: {:.2} seconds",
            self.stats.processing_time
        );
        println!(
            "   • Total data volume: {}",
            format_size(self.stats.total_size)
        );
        let avg_speed_bytes_per_sec = if self.stats.processing_time > 0.0 {
            (self.stats.total_size as f64 / self.stats.processing_time) as u64
        } else {
            0
        };
        println!(
            "   • Average speed: {}/sec",
            format_size(avg_speed_bytes_per_sec)
        );
        println!("\n⚙️  Configuration Parameters:");
        println!("   • Worker threads: {}", self.config.worker_count);
        println!(
            "   • Large file threshold: {}",
            format_size(self.config.large_file_threshold)
        );
        println!(
            "   • Chunk processing size: {}",
            format_size(self.config.chunk_size as u64)
        );

        if !self.stats.errors.is_empty() {
            println!(
                "\n❌ Error Information ({} items):",
                self.stats.errors.len()
            );
            for (i, error) in self.stats.errors.iter().enumerate().take(5) {
                println!("   {}. {}", i + 1, error);
            }
            if self.stats.errors.len() > 5 {
                println!(
                    "   ... {} more errors (see log file for details)",
                    self.stats.errors.len() - 5
                );
            }
        } else {
            println!("\n✅ Processing completed, no errors occurred");
        }
        println!("═══════════════════════════════════════\n");

        if !self.stats.errors.is_empty() {
            text_report.push_str("Error Information:\n");
            for error in &self.stats.errors {
                text_report.push_str(&format!("- {}\n", error));
            }
        }

        fs::write(text_report_path, text_report)?;
        Ok(())
    }

    /// Get statistics
    pub fn get_stats(&self) -> &ProcessingStats {
        &self.stats
    }

    fn prepare_compiledb_environment(
        &self,
        installer: &CompiledbInstaller,
        output_dir: &Path,
    ) -> Result<PathBuf> {
        let venv_path = self.resolve_uv_venv_path(output_dir);
        self.ensure_uv_virtualenv(&venv_path)?;
        installer
            .ensure_installed(&venv_path)
            .with_context(|| format!("Unable to install compiledb in {:?}", venv_path))?;
        Ok(venv_path)
    }

    fn resolve_uv_venv_path(&self, output_dir: &Path) -> PathBuf {
        if let Some(ref configured) = self.config.uv_venv_path {
            let candidate = PathBuf::from(configured);
            if candidate.is_absolute() {
                candidate
            } else {
                output_dir.join(candidate)
            }
        } else {
            output_dir.join(".compiledb-venv")
        }
    }

    fn ensure_uv_virtualenv(&self, venv_path: &Path) -> Result<()> {
        if self.venv_has_python(venv_path) {
            return Ok(());
        }

        if let Some(parent) = venv_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Unable to create virtual environment parent directory: {:?}",
                    parent
                )
            })?;
        }

        info!("Creating uv virtual environment: {:?}", venv_path);
        match self.try_create_uv_venv(venv_path) {
            Ok(()) => {}
            Err(err) => {
                // Check if it's an error of uv command not found
                if self.is_uv_not_found_error(&err) {
                    warn!("Detected uv command not found, attempting to auto-install uv...");

                    // Try to install uv
                    self.try_install_uv()?;

                    // Retry creating virtual environment
                    info!("uv installation completed, retrying virtual environment creation...");
                    self.try_create_uv_venv(venv_path)?;
                } else {
                    return Err(err);
                }
            }
        }

        if !self.venv_has_python(venv_path) {
            warn!(
                "Python interpreter not found after creating uv virtual environment {:?}, creation may have failed",
                venv_path
            );
        }

        Ok(())
    }

    fn try_create_uv_venv(&self, venv_path: &Path) -> Result<()> {
        let status = Command::new(&self.config.uv_command)
            .arg("venv")
            .arg(venv_path)
            .status()
            .with_context(|| {
                format!(
                    "Unable to execute uv venv to create virtual environment: {:?}",
                    venv_path
                )
            })?;

        if !status.success() {
            return Err(anyhow::anyhow!(
                "uv venv failed to create virtual environment, exit code {:?}",
                status.code()
            ));
        }

        Ok(())
    }

    fn try_install_uv(&self) -> Result<()> {
        info!("Attempting to install uv tool via pip...");

        let mut command = Command::new("pip");
        command
            .arg("install")
            .arg("uv")
            .arg("--break-system-packages");

        let output = command
            .output()
            .with_context(|| "Unable to execute pip command to install uv")?;

        if output.status.success() {
            info!("uv tool installation successful");
            return Ok(());
        }

        warn!("pip installation of uv failed, attempting to use mirror sources...");

        // Try to install uv using mirror sources
        for mirror in self.config.uv_mirror_sources() {
            if let Some(index_url) = &mirror.index_url {
                let mut mirror_command = Command::new("pip");
                mirror_command
                    .arg("install")
                    .arg("uv")
                    .arg("--break-system-packages")
                    .arg("--index-url")
                    .arg(index_url);

                match mirror_command.output() {
                    Ok(output) if output.status.success() => {
                        info!(
                            "Successfully installed uv tool via {} mirror source",
                            mirror.name
                        );
                        return Ok(());
                    }
                    Ok(output) => {
                        warn!(
                            "Failed to install uv via {} mirror source: {}",
                            mirror.name,
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    Err(err) => {
                        warn!(
                            "Failed to execute pip command ({} mirror source): {}",
                            mirror.name, err
                        );
                    }
                }
            }
        }

        Err(anyhow::anyhow!(
            "Unable to install uv tool via pip, please install manually"
        ))
    }

    fn is_uv_not_found_error(&self, err: &anyhow::Error) -> bool {
        err.chain().any(|cause| {
            if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                return io_err.kind() == std::io::ErrorKind::NotFound;
            }
            false
        })
    }

    fn venv_has_python(&self, venv_path: &Path) -> bool {
        let unix_candidate = venv_path.join("bin/python");
        let windows_candidate = venv_path.join("Scripts/python.exe");
        unix_candidate.exists() || windows_candidate.exists()
    }
}

/// Format file size
fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_categorization() {
        let config = PreprocessConfig::default();
        let preprocessor = CProjectPreprocessor::new(Some(config));

        // Test source file recognition
        assert!(preprocessor.is_source_file(Path::new("test.c")));
        assert!(preprocessor.is_source_file(Path::new("test.cpp")));
        assert!(!preprocessor.is_source_file(Path::new("test.txt")));

        // Test header file recognition
        assert!(preprocessor.is_header_file(Path::new("test.h")));
        assert!(preprocessor.is_header_file(Path::new("test.hpp")));
        assert!(!preprocessor.is_header_file(Path::new("test.txt")));
    }

    #[test]
    fn test_pair_name_generation() {
        let config = PreprocessConfig::default();
        let preprocessor = CProjectPreprocessor::new(Some(config));

        let pair_name = preprocessor.get_pair_name(Path::new("test.c"));
        assert_eq!(pair_name, "test");

        let pair_name = preprocessor.get_pair_name(Path::new("path/to/example.cpp"));
        assert_eq!(pair_name, "example");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1536), "1.50 KB");
    }
}
