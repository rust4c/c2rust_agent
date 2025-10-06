use anyhow::{Context, Result};
use glob::Pattern;
use indicatif::{MultiProgress, ProgressBar, ProgressIterator, ProgressStyle};
use log::{error, info};
use rayon::prelude::*;
use relation_analy::generate_c_dependency_graph;
use serde::{Deserialize, Serialize};
use serde_json;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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

/// 预处理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessConfig {
    /// 并行工作者数量 (0=自动检测)
    pub worker_count: usize,
    /// 文件配对规则 (源文件模式, 头文件模式)
    pub pairing_rules: Vec<(String, String)>,
    /// 排除文件模式
    pub exclude_patterns: Vec<String>,
    /// 头文件扩展名
    pub header_extensions: Vec<String>,
    /// 源文件扩展名
    pub source_extensions: Vec<String>,
    /// 大文件阈值 (字节)
    pub large_file_threshold: u64,
    /// 块大小 (字节)
    pub chunk_size: usize,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        PreprocessConfig {
            worker_count: 0,
            pairing_rules: vec![
                (r"(.+)\.c".to_string(), r"\1.h".to_string()),
                (r"(.+)\.cpp".to_string(), r"\1.h".to_string()),
                (r"(.+)\.cc".to_string(), r"\1.hpp".to_string()),
                (r"src/(.+)\.c".to_string(), r"include/\1.h".to_string()),
            ],
            exclude_patterns: vec![
                "*.bak".to_string(),
                "*.tmp".to_string(),
                "__pycache__/*".to_string(),
                "*.pyc".to_string(),
                ".git/*".to_string(),
                ".svn/*".to_string(),
                "*.o".to_string(),
                "*.obj".to_string(),
                "*.exe".to_string(),
                "*.dll".to_string(),
                "*.so".to_string(),
            ],
            header_extensions: vec![
                ".h".to_string(),
                ".hpp".to_string(),
                ".hh".to_string(),
                ".hxx".to_string(),
            ],
            source_extensions: vec![
                ".c".to_string(),
                ".cc".to_string(),
                ".cpp".to_string(),
                ".cxx".to_string(),
                ".c++".to_string(),
            ],
            large_file_threshold: 50 * 1024 * 1024, // 50MB
            chunk_size: 8 * 1024 * 1024,            // 8MB
        }
    }
}

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

/// 文件预处理器
pub struct CProjectPreprocessor {
    config: PreprocessConfig,
    stats: ProcessingStats,
    file_mappings: Vec<FileMapping>,
}

impl CProjectPreprocessor {
    /// 创建新的预处理器
    pub fn new(config: Option<PreprocessConfig>) -> Self {
        let mut config = config.unwrap_or_default();
        if config.worker_count == 0 {
            config.worker_count = num_cpus::get().max(1);
        }
        CProjectPreprocessor {
            config,
            stats: ProcessingStats::default(),
            file_mappings: Vec::new(),
        }
    }

    /// 预处理项目文件
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
        main_pb.set_message("🚀 开始预处理项目文件...");

        // 验证目录
        if !source_dir.exists() || !source_dir.is_dir() {
            return Err(anyhow::anyhow!("源目录不存在或不是目录"));
        }

        // 创建输出目录结构
        main_pb.set_message("📁 创建输出目录结构...");
        self.create_output_structure(output_dir)?;

        // 扫描并分类文件
        main_pb.set_message("🔍 扫描项目文件...");
        let all_files = self.scan_files(source_dir, &m)?;

        main_pb.set_message("📋 分类文件中...");
        let categorized_files = self.categorize_files(&all_files, &m)?;

        // 生成映射文件
        main_pb.set_message("🗺️  生成文件映射...");
        self.generate_mapping(&categorized_files, source_dir, output_dir)?;
        self.save_mapping(output_dir)?;

        // 处理分类后的文件
        main_pb.set_message("📦 处理分类文件...");
        self.process_categorized_files(&categorized_files, output_dir, &m)?;

        // 关系分析
        main_pb.set_message("🔗 进行关系分析...");
        self.relation_analysis(output_dir)?;

        // 生成报告
        main_pb.set_message("📊 生成处理报告...");
        self.generate_report(output_dir)?;

        // 记录处理时间
        self.stats.processing_time = start_time.elapsed().as_secs_f64();
        main_pb.finish_with_message("✅ 预处理完成!");

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

    /// 创建输出目录结构
    fn create_output_structure(&self, output_dir: &Path) -> Result<()> {
        let dirs = [
            output_dir.join("paired_files"),     // 配对文件
            output_dir.join("individual_files"), // 单独文件
            output_dir.join("unrelated_files"),  // 不相关文件
        ];

        for dir in &dirs {
            fs::create_dir_all(dir).with_context(|| format!("无法创建目录: {:?}", dir))?;
        }
        Ok(())
    }

    /// 扫描文件
    fn scan_files(&mut self, source_dir: &Path, m: &MultiProgress) -> Result<Vec<PathBuf>> {
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message("🔍 扫描文件中...");

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
        scan_pb.set_message("扫描中");

        for entry in entries.iter().progress_with(scan_pb.clone()) {
            let path = entry.path();
            let relative_path = path.strip_prefix(source_dir).unwrap_or(path);

            // 检查排除模式
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
                    .push(format!("无法访问文件: {}", path.display()));
                self.stats.skipped_files += 1;
            }
        }

        self.stats.total_files = files.len();
        pb.finish_with_message(format!("✅ 扫描完成，发现 {} 个文件", files.len()));
        Ok(files)
    }

    /// 文件分类
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
        pb.set_message("文件分类中");

        let mut categorized = Vec::new();
        let mut processed_files = HashSet::new();

        // 分离源文件和头文件
        let source_files: Vec<_> = files.iter().filter(|f| self.is_source_file(f)).collect();
        let header_files: Vec<_> = files.iter().filter(|f| self.is_header_file(f)).collect();

        pb.set_message(format!(
            "找到 {} 个源文件，{} 个头文件",
            source_files.len(),
            header_files.len()
        ));

        // 寻找配对文件
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

        // 处理单独的源文件和头文件
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

        // 处理不相关文件
        for file in files.iter().progress_with(pb.clone()) {
            if !processed_files.contains(file) {
                categorized.push(FileCategory::Unrelated(file.clone()));
                self.stats.unrelated_files += 1;
            }
        }

        pb.finish_with_message("✅ 文件分类完成");
        Ok(categorized)
    }

    /// 处理分类后的文件
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
        pb.set_message("处理文件中");

        let errors = Arc::new(Mutex::new(Vec::new()));

        categorized_files.par_iter().for_each(|category| {
            match category {
                FileCategory::Paired { source, header } => {
                    let pair_name = self.get_pair_name(source);
                    let target_dir = output_dir.join("paired_files").join(&pair_name);

                    if let Err(e) = self.process_paired_files(source, header, &target_dir) {
                        errors
                            .lock()
                            .unwrap()
                            .push(format!("处理配对文件失败 {}: {}", pair_name, e));
                    }
                }
                FileCategory::Individual(file) => {
                    let file_name = self.get_file_name(file);
                    let target_dir = output_dir.join("individual_files").join(&file_name);

                    if let Err(e) = self.process_individual_file(file, &target_dir) {
                        errors
                            .lock()
                            .unwrap()
                            .push(format!("处理单独文件失败 {}: {}", file_name, e));
                    }
                }
                FileCategory::Unrelated(file) => {
                    let target_dir = output_dir.join("unrelated_files");

                    if let Err(e) = self.process_unrelated_file(file, &target_dir) {
                        errors.lock().unwrap().push(format!(
                            "处理不相关文件失败 {}: {}",
                            file.display(),
                            e
                        ));
                    }
                }
            }
            pb.inc(1);
        });

        // 收集错误
        self.stats.errors.extend(errors.lock().unwrap().drain(..));
        pb.finish_with_message("✅ 文件处理完成");
        Ok(())
    }

    /// 处理配对文件
    fn process_paired_files(&self, source: &Path, header: &Path, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;

        let source_target = target_dir.join(source.file_name().unwrap());
        let header_target = target_dir.join(header.file_name().unwrap());

        self.copy_file(source, &source_target)?;
        self.copy_file(header, &header_target)?;

        Ok(())
    }

    /// 处理单独文件
    fn process_individual_file(&self, file: &Path, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;
        let target_file = target_dir.join(file.file_name().unwrap());
        self.copy_file(file, &target_file)?;
        Ok(())
    }

    /// 处理不相关文件
    fn process_unrelated_file(&self, file: &Path, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;
        let target_file = target_dir.join(file.file_name().unwrap());
        self.copy_file(file, &target_file)?;
        Ok(())
    }

    /// 复制文件
    fn copy_file(&self, src: &Path, dst: &Path) -> Result<()> {
        let metadata = fs::metadata(src)?;

        if metadata.len() > self.config.large_file_threshold {
            self.copy_large_file(src, dst)?;
        } else {
            fs::copy(src, dst)?;
        }

        Ok(())
    }

    /// 复制大文件（分块）
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

    /// 寻找匹配的头文件
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

                    // 替换捕获组
                    for i in 0..captures.len() {
                        if let Some(cap) = captures.get(i) {
                            expected_header =
                                expected_header.replace(&format!("\\{}", i), cap.as_str());
                        }
                    }

                    // 寻找匹配的头文件
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

    /// 获取配对名称
    fn get_pair_name(&self, source_file: &Path) -> String {
        source_file
            .file_stem()
            .unwrap_or(source_file.file_name().unwrap())
            .to_string_lossy()
            .to_string()
    }

    /// 获取文件名称（不含扩展名）
    fn get_file_name(&self, file: &Path) -> String {
        file.file_stem()
            .unwrap_or(file.file_name().unwrap())
            .to_string_lossy()
            .to_string()
    }

    /// 判断是否为源文件
    fn is_source_file(&self, file: &Path) -> bool {
        if let Some(ext) = file.extension() {
            let ext_str = format!(".{}", ext.to_string_lossy());
            self.config.source_extensions.contains(&ext_str)
        } else {
            false
        }
    }

    /// 判断是否为头文件
    fn is_header_file(&self, file: &Path) -> bool {
        if let Some(ext) = file.extension() {
            let ext_str = format!(".{}", ext.to_string_lossy());
            self.config.header_extensions.contains(&ext_str)
        } else {
            false
        }
    }

    /// 生成文件映射
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

    /// 保存映射文件
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

    /// 生成处理报告
    fn generate_report(&self, output_dir: &Path) -> Result<()> {
        let report_path = output_dir.join("processing_report.json");
        let text_report_path = output_dir.join("processing_log.txt");

        // JSON报告
        let json_report = serde_json::json!({
            "statistics": &self.stats,
            "config": &self.config,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        fs::write(&report_path, serde_json::to_string_pretty(&json_report)?)?;

        // 文本报告
        let mut text_report = String::new();
        text_report.push_str("C项目文件预处理报告\n");
        text_report.push_str("===================\n\n");
        text_report.push_str(&format!("总文件数: {}\n", self.stats.total_files));
        text_report.push_str(&format!("配对文件数: {}\n", self.stats.paired_files));
        text_report.push_str(&format!("单独文件数: {}\n", self.stats.individual_files));
        text_report.push_str(&format!("不相关文件数: {}\n", self.stats.unrelated_files));
        text_report.push_str(&format!("跳过文件数: {}\n", self.stats.skipped_files));
        text_report.push_str(&format!("文件映射数: {}\n", self.stats.mapping_count));
        text_report.push_str(&format!("处理时间: {:.2} 秒\n", self.stats.processing_time));
        text_report.push_str(&format!(
            "总大小: {}\n\n",
            format_size(self.stats.total_size)
        ));

        // 终端输出处理报告
        println!("\n🎯 C项目文件预处理报告");
        println!("═══════════════════════════════════════");
        println!("📊 处理统计:");
        println!("   • 总文件数量: {}", self.stats.total_files);
        println!(
            "   • 配对文件数: {} ({}个配对)",
            self.stats.paired_files,
            self.stats.paired_files / 2
        );
        println!("   • 单独文件数: {}", self.stats.individual_files);
        println!("   • 不相关文件: {}", self.stats.unrelated_files);
        println!("   • 跳过文件数: {}", self.stats.skipped_files);
        println!("   • 文件映射数: {}", self.stats.mapping_count);

        println!("\n🔗 关系分析:");
        match generate_c_dependency_graph(output_dir) {
            Ok(rel) => {
                let include_edges: usize = rel
                    .files
                    .values()
                    .map(|n| n.local_includes.len() + n.system_includes.len())
                    .sum();
                println!("   • 文件节点数: {}", rel.files.len());
                println!("   • 包含关系数: {}", include_edges);
                println!("   • 包含目录数: {}", rel.build.include_dirs.len());
                println!("   • 链接库数量: {}", rel.build.link_libs.len());
                println!("   • 链接目录数: {}", rel.build.link_dirs.len());
                println!("   • 依赖图生成: ✅ 成功");
            }
            Err(_) => {
                println!("   • 依赖图生成: ❌ 失败");
            }
        }

        println!("\n⏱️  性能指标:");
        println!("   • 处理时间: {:.2} 秒", self.stats.processing_time);
        println!("   • 总数据量: {}", format_size(self.stats.total_size));
        println!(
            "   • 平均速度: {}/秒",
            format_size((self.stats.total_size as f64 / self.stats.processing_time) as u64)
        );
        println!("\n⚙️  配置参数:");
        println!("   • 工作线程数: {}", self.config.worker_count);
        println!(
            "   • 大文件阈值: {}",
            format_size(self.config.large_file_threshold)
        );
        println!(
            "   • 块处理大小: {}",
            format_size(self.config.chunk_size as u64)
        );

        if !self.stats.errors.is_empty() {
            println!("\n❌ 错误信息 ({} 项):", self.stats.errors.len());
            for (i, error) in self.stats.errors.iter().enumerate().take(5) {
                println!("   {}. {}", i + 1, error);
            }
            if self.stats.errors.len() > 5 {
                println!(
                    "   ... 还有 {} 个错误 (详见日志文件)",
                    self.stats.errors.len() - 5
                );
            }
        } else {
            println!("\n✅ 处理完成，无错误发生");
        }
        println!("═══════════════════════════════════════\n");

        if !self.stats.errors.is_empty() {
            text_report.push_str("错误信息:\n");
            for error in &self.stats.errors {
                text_report.push_str(&format!("- {}\n", error));
            }
        }

        fs::write(text_report_path, text_report)?;
        Ok(())
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> &ProcessingStats {
        &self.stats
    }
}

/// 格式化文件大小
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

        // 测试源文件识别
        assert!(preprocessor.is_source_file(Path::new("test.c")));
        assert!(preprocessor.is_source_file(Path::new("test.cpp")));
        assert!(!preprocessor.is_source_file(Path::new("test.txt")));

        // 测试头文件识别
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
