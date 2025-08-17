use anyhow::{Context, Result, anyhow};
use glob::Pattern;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use libc::{statvfs, c_char};
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

// 预处理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessConfig {
    pub worker_count: usize,                  // 并行工作者数量 (0=自动检测)
    pub pairing_rules: Vec<(String, String)>, // 文件配对规则
    pub exclude_patterns: Vec<String>,        // 排除文件模式
    pub header_extensions: Vec<String>,       // 头文件扩展名
    pub source_extensions: Vec<String>,       // 源文件扩展名
    pub large_file_threshold: u64,            // 大文件阈值 (字节)
    pub chunk_size: usize,                    // 块大小 (字节)
    pub min_disk_space: u64,                  // 最小磁盘空间要求 (字节)
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        PreprocessConfig {
            worker_count: 0,
            pairing_rules: vec![
                (r"(.*)\.c".to_string(), r"\1.h".to_string()), // 默认规则
                (
                    r"src/(.*)_impl\.c".to_string(),
                    r"include/\1\.h".to_string(),
                ), // 自定义规则
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
            ],
            large_file_threshold: 100 * 1024 * 1024, // 100MB
            chunk_size: 16 * 1024 * 1024,            // 16MB
            min_disk_space: 1024 * 1024 * 1024,      // 1GB
        }
    }
}

// 文件信息
#[derive(Debug, Clone)]
struct FileInfo {
    path: PathBuf,
    size: u64,
    is_large: bool,
}

impl FileInfo {
    fn new(path: PathBuf) -> Result<Self> {
        let metadata = fs::metadata(&path)?;
        let size = metadata.len();
        Ok(FileInfo {
            path,
            size,
            is_large: size > 100 * 1024 * 1024, // 100MB
        })
    }
}

// 处理统计信息
#[derive(Debug, Default, Serialize)]
pub struct ProcessingStats {
    total_files: usize,
    processed_pairs: usize,
    header_only: usize,
    source_only: usize,
    misc_files: usize,
    skipped_files: usize,
    processing_time: f64,
    total_size: u64,
    errors: Vec<String>,
}

// C工程预处理器
pub struct CProjectPreprocessor {
    config: PreprocessConfig,
    stats: ProcessingStats,
}

impl CProjectPreprocessor {
    pub fn new(config: Option<PreprocessConfig>) -> Self {
        let mut config = config.unwrap_or_default();
        if config.worker_count == 0 {
            config.worker_count = num_cpus::get().max(1);
        }
        CProjectPreprocessor {
            config,
            stats: ProcessingStats::default(),
        }
    }

    // 主处理函数
    pub fn preprocess_project(
        &mut self,
        source_dir: &Path,
        cache_dir: &Path,
    ) -> Result<ProcessingStats> {
        let start_time = Instant::now();
        let m = MultiProgress::new();
        let main_pb = m.add(ProgressBar::new_spinner());
        main_pb.set_message("Starting preprocessing...");

        // 验证目录
        if !source_dir.exists() || !source_dir.is_dir() {
            return Err(anyhow!(
                "Source directory does not exist or is not a directory"
            ));
        }

        // 检查磁盘空间
        self.check_disk_space(cache_dir)?;

        // 创建缓存目录结构
        self.create_cache_structure(cache_dir)?;

        // 扫描源文件
        let all_files = self.scan_source_files(source_dir, &m)?;
        self.stats.total_files = all_files.len();
        self.stats.total_size = all_files.iter().map(|f| f.size).sum();
        main_pb.println(format!(
            "Found {} files, total size: {}",
            self.stats.total_files,
            format_size(self.stats.total_size)
        ));

        // 查找文件配对
        let file_pairs = self.find_file_pairs(&all_files);
        self.stats.processed_pairs = file_pairs.len();
        main_pb.println(format!("Found {} file pairs", file_pairs.len()));

        // 处理配对文件
        let paired_files = self.process_paired_files(&file_pairs, cache_dir, &m)?;
        let remaining_files: Vec<_> = all_files
            .into_iter()
            .filter(|f| !paired_files.contains(&f.path))
            .collect();

        // 处理剩余文件
        self.process_remaining_files(&remaining_files, cache_dir, &m)?;

        // 生成处理报告
        self.generate_processing_report(cache_dir)?;

        // 记录处理时间
        self.stats.processing_time = start_time.elapsed().as_secs_f64();
        main_pb.println(format!(
            "Preprocessing completed in {:.2} seconds",
            self.stats.processing_time
        ));
        main_pb.finish_with_message("Done!");

        Ok(std::mem::take(&mut self.stats))
    }

    // 检查磁盘空间
    fn check_disk_space(&self, path: &Path) -> Result<()> {
    #[cfg(target_family = "unix")]
    {
        let c_path = CString::new(path.to_str().unwrap()).unwrap();
        let mut s: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
        let ret = unsafe { statvfs(c_path.as_ptr() as *const c_char, s.as_mut_ptr()) };
        if ret != 0 {
            return Err(anyhow!("statvfs failed"));
        }
        let s = unsafe { s.assume_init() };
        let free_space = s.f_bavail as u64 * s.f_frsize as u64;
        if free_space < self.config.min_disk_space {
            return Err(anyhow!(
                "Insufficient disk space: available {}, required {}",
                format_size(free_space),
                format_size(self.config.min_disk_space)
            ));
        }
    }
    #[cfg(not(target_family = "unix"))]
    {
        println!("Disk space check skipped (only supported on Unix)");
    }
    Ok(())
    }

    // 创建缓存目录结构
    fn create_cache_structure(&self, cache_dir: &Path) -> Result<()> {
        let dirs = vec![
            cache_dir.join("paired_files"),
            cache_dir.join("individual_files/header_only"),
            cache_dir.join("individual_files/source_only"),
            cache_dir.join("individual_files/misc_files"),
        ];

        for dir in dirs {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create directory {:?}", dir))?;
        }
        Ok(())
    }

    // 扫描源文件
    fn scan_source_files(&mut self, source_dir: &Path, m: &MultiProgress) -> Result<Vec<FileInfo>> {
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_message("Scanning source files...");

        let mut all_files = Vec::new();
        let exclude_patterns: Vec<_> = self
            .config
            .exclude_patterns
            .iter()
            .map(|p| Pattern::new(p).unwrap())
            .collect();

        for entry in walkdir::WalkDir::new(source_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
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

                match FileInfo::new(path.to_path_buf()) {
                    Ok(file_info) => all_files.push(file_info),
                    Err(e) => {
                        self.stats.errors.push(format!(
                            "Failed to access file {}: {}",
                            path.display(),
                            e
                        ));
                        self.stats.skipped_files += 1;
                    }
                }
            }
        }

        pb.finish_with_message(format!("Scanned {} files", all_files.len()));
        Ok(all_files)
    }

    // 查找文件配对
    fn find_file_pairs(&self, files: &[FileInfo]) -> HashMap<String, (FileInfo, FileInfo)> {
        let mut pairs = HashMap::new();

        // 分类文件
        let c_files: Vec<_> = files
            .iter()
            .filter(|f| {
                self.config
                    .source_extensions
                    .iter()
                    .any(|ext| f.path.extension() == Some(OsStr::new(ext)))
            })
            .cloned()
            .collect();

        let h_files: Vec<_> = files
            .iter()
            .filter(|f| {
                self.config
                    .header_extensions
                    .iter()
                    .any(|ext| f.path.extension() == Some(OsStr::new(ext)))
            })
            .cloned()
            .collect();

        for c_file in &c_files {
            for (pattern, replacement) in &self.config.pairing_rules {
                let c_path = c_file.path.to_string_lossy();
                if let Some(captures) = regex::Regex::new(pattern).unwrap().captures(&c_path) {
                    let mut expected_h_name = replacement.clone();
                    for i in 0..captures.len() {
                        if let Some(cap) = captures.get(i) {
                            expected_h_name =
                                expected_h_name.replace(&format!("\\{}", i), cap.as_str());
                        }
                    }

                    // 查找匹配的头文件
                    if let Some(h_file) = h_files.iter().find(|h| {
                        h.path.to_string_lossy().ends_with(&expected_h_name)
                            || h.path.file_name().unwrap()
                                == Path::new(&expected_h_name).file_name().unwrap()
                    }) {
                        let base_name = c_file
                            .path
                            .file_stem()
                            .unwrap()
                            .to_string_lossy()
                            .to_string();
                        let unique_base = self.make_unique_basename(&base_name, &pairs);
                        pairs.insert(unique_base, (c_file.clone(), h_file.clone()));
                        break;
                    }
                }
            }
        }

        pairs
    }

    // 生成唯一的基础名称
    fn make_unique_basename(
        &self,
        base: &str,
        pairs: &HashMap<String, (FileInfo, FileInfo)>,
    ) -> String {
        if !pairs.contains_key(base) {
            return base.to_string();
        }

        let mut counter = 1;
        loop {
            let candidate = format!("{}_{}", base, counter);
            if !pairs.contains_key(&candidate) {
                return candidate;
            }
            counter += 1;
        }
    }

    // 处理配对文件
    fn process_paired_files(
        &mut self,
        file_pairs: &HashMap<String, (FileInfo, FileInfo)>,
        cache_dir: &Path,
        m: &MultiProgress,
    ) -> Result<HashSet<PathBuf>> {
        let pb = m.add(ProgressBar::new(file_pairs.len() as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                .unwrap(),
        );
        pb.set_message("Copying paired files...");

        let paired_files = Arc::new(Mutex::new(HashSet::new()));
        let errors = Arc::new(Mutex::new(Vec::new()));

        // 并行处理文件对
        let _ = file_pairs
            .par_iter()
            .try_for_each(|(base_name, (c_file, h_file))| {
                let target_dir = cache_dir.join("paired_files").join(base_name);
                if let Err(e) = fs::create_dir_all(&target_dir) {
                    errors.lock().unwrap().push(format!(
                        "Failed to create directory {}: {}",
                        target_dir.display(),
                        e
                    ));
                    return Ok::<(), ()>(());
                }

                let c_target = target_dir.join(c_file.path.file_name().unwrap());
                let h_target = target_dir.join(h_file.path.file_name().unwrap());

                if let Err(e) = self.safe_copy_file(&c_file.path, &c_target) {
                    errors.lock().unwrap().push(format!(
                        "Failed to copy {} to {}: {}",
                        c_file.path.display(),
                        c_target.display(),
                        e
                    ));
                }

                if let Err(e) = self.safe_copy_file(&h_file.path, &h_target) {
                    errors.lock().unwrap().push(format!(
                        "Failed to copy {} to {}: {}",
                        h_file.path.display(),
                        h_target.display(),
                        e
                    ));
                }

                {
                    let mut pf = paired_files.lock().unwrap();
                    pf.insert(c_file.path.clone());
                    pf.insert(h_file.path.clone());
                }

                pb.inc(1);
                Ok(())
            });

        // 收集错误
        self.stats.errors.extend(errors.lock().unwrap().drain(..));

        pb.finish_with_message("Paired files processed");
        Ok(Arc::try_unwrap(paired_files).unwrap().into_inner().unwrap())
    }

    // 处理剩余文件
    fn process_remaining_files(
        &mut self,
        remaining_files: &[FileInfo],
        cache_dir: &Path,
        m: &MultiProgress,
    ) -> Result<()> {
        if remaining_files.is_empty() {
            return Ok(());
        }

        let pb = m.add(ProgressBar::new(remaining_files.len() as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                .unwrap(),
        );
        pb.set_message("Processing remaining files...");

        // 分类文件
        let mut header_only = Vec::new();
        let mut source_only = Vec::new();
        let mut misc_files = Vec::new();

        for file in remaining_files {
            if self
                .config
                .header_extensions
                .iter()
                .any(|ext| file.path.extension() == Some(OsStr::new(ext)))
            {
                header_only.push(file);
                self.stats.header_only += 1;
            } else if self
                .config
                .source_extensions
                .iter()
                .any(|ext| file.path.extension() == Some(OsStr::new(ext)))
            {
                source_only.push(file);
                self.stats.source_only += 1;
            } else {
                misc_files.push(file);
                self.stats.misc_files += 1;
            }
        }

        let errors = Arc::new(Mutex::new(Vec::new()));

        // 处理头文件
        header_only.par_iter().for_each(|file| {
            let target_dir = cache_dir.join("individual_files/header_only");
            if let Err(e) = self.copy_individual_file(file, &target_dir) {
                errors.lock().unwrap().push(format!(
                    "Failed to copy header {}: {}",
                    file.path.display(),
                    e
                ));
            }
            pb.inc(1);
        });

        // 处理源文件
        source_only.par_iter().for_each(|file| {
            let target_dir = cache_dir.join("individual_files/source_only");
            if let Err(e) = self.copy_individual_file(file, &target_dir) {
                errors.lock().unwrap().push(format!(
                    "Failed to copy source {}: {}",
                    file.path.display(),
                    e
                ));
            }
            pb.inc(1);
        });

        // 处理其他文件
        misc_files.par_iter().for_each(|file| {
            let target_dir = cache_dir.join("individual_files/misc_files");
            if let Err(e) = self.copy_individual_file(file, &target_dir) {
                errors.lock().unwrap().push(format!(
                    "Failed to copy misc file {}: {}",
                    file.path.display(),
                    e
                ));
            }
            pb.inc(1);
        });

        // 收集错误
        self.stats.errors.extend(errors.lock().unwrap().drain(..));

        pb.finish_with_message("Remaining files processed");
        Ok(())
    }

    // 拷贝单个文件
    fn copy_individual_file(&self, file: &FileInfo, target_dir: &Path) -> Result<()> {
        fs::create_dir_all(target_dir)?;
        let target_path = target_dir.join(file.path.file_name().unwrap());
        self.safe_copy_file(&file.path, &target_path)?;
        Ok(())
    }

    // 安全拷贝文件
    fn safe_copy_file(&self, src: &Path, dst: &Path) -> Result<u64> {
        // 处理大文件
        if fs::metadata(src)?.len() > self.config.large_file_threshold {
            self.copy_large_file(src, dst)
        } else {
            self.copy_small_file(src, dst)
        }
    }

    // 拷贝小文件
    fn copy_small_file(&self, src: &Path, dst: &Path) -> Result<u64> {
        fs::copy(src, dst).map_err(|e| e.into())
    }

    // 拷贝大文件（分块）
    fn copy_large_file(&self, src: &Path, dst: &Path) -> Result<u64> {
        let mut src_file = BufReader::with_capacity(self.config.chunk_size, File::open(src)?);
        let mut dst_file = BufWriter::with_capacity(self.config.chunk_size, File::create(dst)?);

        let mut buffer = vec![0u8; self.config.chunk_size];
        let mut total_copied = 0;

        loop {
            let bytes_read = src_file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            dst_file.write_all(&buffer[..bytes_read])?;
            total_copied += bytes_read as u64;
        }

        dst_file.flush()?;
        Ok(total_copied)
    }

    // 生成处理报告
    fn generate_processing_report(&self, cache_dir: &Path) -> Result<()> {
        let report_path = cache_dir.join("processing_report.json");
        let text_report_path = cache_dir.join("processing_log.txt");

        // JSON报告
        let json_report = serde_json::json!({
            "statistics": &self.stats,
            "config": &self.config,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        fs::write(&report_path, serde_json::to_string_pretty(&json_report)?)?;

        // 文本报告
        let mut text_report = String::new();
        text_report.push_str("C Project Preprocessing Report\n");
        text_report.push_str("==============================\n\n");
        text_report.push_str(&format!("Total files: {}\n", self.stats.total_files));
        text_report.push_str(&format!(
            "File pairs processed: {}\n",
            self.stats.processed_pairs
        ));
        text_report.push_str(&format!("Header-only files: {}\n", self.stats.header_only));
        text_report.push_str(&format!("Source-only files: {}\n", self.stats.source_only));
        text_report.push_str(&format!("Misc files: {}\n", self.stats.misc_files));
        text_report.push_str(&format!("Skipped files: {}\n", self.stats.skipped_files));
        text_report.push_str(&format!(
            "Processing time: {:.2} seconds\n",
            self.stats.processing_time
        ));
        text_report.push_str(&format!(
            "Total size: {}\n\n",
            format_size(self.stats.total_size)
        ));

        if !self.stats.errors.is_empty() {
            text_report.push_str("Errors:\n");
            for error in &self.stats.errors {
                text_report.push_str(&format!("- {}\n", error));
            }
        }

        fs::write(text_report_path, text_report)?;
        Ok(())
    }
}

// 格式化文件大小
fn format_size(size: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_idx = 0;

    while size > 1024.0 && unit_idx < units.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, units[unit_idx])
}

// fn main() -> Result<()> {
//     // 示例配置
//     let config = PreprocessConfig {
//         worker_count: 4,
//         pairing_rules: vec![
//             (r"(.*)\.c".to_string(), r"\1.h".to_string()),
//             (
//                 r"src/(.*)_impl\.c".to_string(),
//                 r"include/\1\.h".to_string(),
//             ),
//         ],
//         exclude_patterns: vec![
//             "*.bak".to_string(),
//             "*.tmp".to_string(),
//             "__pycache__/*".to_string(),
//             ".git/*".to_string(),
//         ],
//         ..Default::default()
//     };

//     let mut preprocessor = CProjectPreprocessor::new(Some(config));

//     let source_dir = Path::new("/path/to/source");
//     let cache_dir = Path::new("/path/to/cache");

//     match preprocessor.preprocess_project(source_dir, cache_dir) {
//         Ok(stats) => {
//             println!("Preprocessing completed successfully!");
//             println!("Statistics: {:?}", stats);
//         }
//         Err(e) => {
//             eprintln!("Preprocessing failed: {}", e);
//         }
//     }

//     Ok(())
// }
