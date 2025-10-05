use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use log::{error, info, warn};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// Internal crates
use cproject_analy::file_remanager::{CProjectPreprocessor, PreprocessConfig};
use db_services::DatabaseManager;
use env_checker::ai_checker::{AIConnectionStatus, ai_service_init};
use env_checker::dbdata_init;
use lsp_services::lsp_services::{
    analyze_project_with_default_database, check_function_and_class_name,
};
use main_processor::MainProcessor;
use project_remanager::ProjectReorganizer;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "c2rust-agent")]
#[command(version = "2.0")]
#[command(about = "C to Rust", long_about = None)]

pub struct Cli {
    /// 显示调试日志 (默认关闭)
    #[arg(long, short = 'd', global = true, help = "show debug log")]
    pub debug: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Output C project analysis and deconstruction
    Analyze {
        /// C Project Catalog (required)
        #[arg(long, short, value_name = "DIR", help = "enter path", required = true)]
        input_dir: PathBuf,
    },

    /// Output the pre-processing results of the C project
    Preprocess {
        /// C Project Catalog (required)
        #[arg(long, short, value_name = "DIR", help = "enter path", required = true)]
        input_dir: PathBuf,

        /// Pre processed output path
        #[arg(
            long,
            short,
            value_name = "DIR",
            help = "Output path (default: input_dir's parent/(input_dir_name + \"cache\")",
            required = false
        )]
        output_dir: Option<PathBuf>,
    },

    // /// Parse Call Relationships
    // AnalyzeRelations {
    //     /// C Project Catalog (required)
    //     #[arg(long, value_name = "DIR", required = true)]
    //     input_dir: PathBuf,

    //     /// Project name (optional)
    //     #[arg(long)]
    //     project_name: Option<String>,

    // },

    // /// Query and call relational database
    // RelationQuery {
    //     /// Database file path
    //     #[arg(long, default_value = "c2rust_metadata.db")]
    //     db: String,

    //     /// Project name (for specific query)
    //     #[arg(long)]
    //     project: Option<String>,

    //     /// Query Type
    //     #[arg(long, value_enum, default_value_t = QueryType::ListProjects)]
    //     query_type: QueryType,

    //     /// Target function name or file path
    //     #[arg(long)]
    //     target: Option<String>,

    //     /// Search keywords
    //     #[arg(long)]
    //     keyword: Option<String>,

    //     /// Limit on number of results
    //     #[arg(long, default_value_t = 10)]
    //     limit: usize,
    // },
    /// Converting Project C to RUST
    Translate {
        /// C Project Catalog (required)
        #[arg(long, value_name = "DIR", required = true)]
        input_dir: PathBuf,

        /// Export the Rust project catalog (optional)
        #[arg(long, value_name = "OIR")]
        output_dir: Option<PathBuf>,
    },

    /// test single file processing
    Test {
        /// C file path (required)
        #[arg(long, value_name = "FILE", help = "enter file path", required = true)]
        input_dir: PathBuf,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum QueryType {
    /// 列出所有可用项目
    ListProjects,
    /// 显示项目统计信息
    Stats,
    /// 生成项目报告
    Report,
    /// 查找函数定义和调用
    FindFunc,
    /// 获取函数调用链
    CallChain,
    /// 分析文件关系
    FileAnalysis,
    /// 获取最常调用的函数
    TopCalled,
    /// 获取最复杂的函数
    TopComplex,
    /// 分析文件依赖
    DepsAnalysis,
    /// 搜索函数使用情况
    Search,
    /// 获取函数使用摘要
    FuncUsage,
}

pub fn parse_args() -> Cli {
    Cli::parse()
}

// =============== Shared Async APIs for UI and CLI ===============

/// Initialize logging, database and AI connectivity once.
/// Idempotent enough for simple reuse.
pub async fn init_services(debug: bool) -> Result<()> {
    use tracing_log::LogTracer;
    use tracing_subscriber::filter::LevelFilter as SubLevel;
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    // Initialize tracing/log once (ignore repeated init errors)
    let _ = LogTracer::init();
    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_timer(fmt::time::uptime());
    let level = if debug {
        SubLevel::DEBUG
    } else {
        SubLevel::INFO
    };
    let subscriber = tracing_subscriber::registry().with(fmt_layer).with(level);
    let _ = subscriber.try_init();

    // Initialize database
    let manager: DatabaseManager = _dbdata_create().await;
    match dbdata_init(manager).await {
        Ok(status) => {
            info!("数据库状态: {:?}", status);
        }
        Err(e) => {
            error!("查询数据库状态失败: {}", e);
        }
    }

    // Initialize AI services
    match ai_service_init().await {
        Ok(status) => {
            info!("AI 服务状态: {:?}", status);
            match status {
                AIConnectionStatus::AllConnected => info!("AI 服务已连接"),
                AIConnectionStatus::AllDisconnected => error!("所有 AI 服务均未连接"),
                _ => warn!("部分 AI 服务连接状态不明"),
            }
        }
        Err(e) => {
            error!("查询 AI 服务状态失败: {}", e);
        }
    }

    Ok(())
}

/// Analyze a C project directory.
pub async fn run_analyze(input_dir: &Path) -> Result<()> {
    let input_str = input_dir.to_string_lossy();
    match analyze_project_with_default_database(&input_str, false).await {
        Ok(_) => info!("✅ 分析完成，结果已保存到数据库"),
        Err(e) => {
            error!("⚠️ 数据库分析失败，尝试基础分析: {}", e);
            let _ = check_function_and_class_name(&input_str, false);
        }
    }
    Ok(())
}

/// Preprocess a C project and then analyze the preprocessed output directory.
/// Returns the output cache directory used.
pub async fn run_preprocess(input_dir: &Path, output_dir: Option<&Path>) -> Result<PathBuf> {
    let output_dir = output_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_cache_dir_for(input_dir));

    info!("输入目录:{}", input_dir.display());
    info!("输出目录: {}", output_dir.display());

    std::fs::create_dir_all(&output_dir)?;
    info!("正在预处理项目...");
    let config = PreprocessConfig::default();
    let mut preprocessor = CProjectPreprocessor::new(Some(config));
    preprocessor.preprocess_project(input_dir, &output_dir)?;

    info!("预处理完成，缓存目录: {}", output_dir.display());
    info!("开始分析项目...");
    let output_str = output_dir.to_string_lossy();
    match analyze_project_with_default_database(&output_str, false).await {
        Ok(_) => info!("✅ 项目分析完成，结果已保存到数据库"),
        Err(e) => {
            error!("⚠️ 数据库分析失败，尝试基础分析: {}", e);
            let _ = check_function_and_class_name(&output_str, false);
        }
    }
    Ok(output_dir)
}

/// Translate a C project into Rust workspace. Performs preprocess, discover, convert, and reorganize.
pub async fn run_translate(input_dir: &Path, output_dir: Option<&Path>) -> Result<()> {
    use anyhow::anyhow;

    if !input_dir.exists() {
        return Err(anyhow!("输入目录不存在: {}", input_dir.display()));
    }

    // Preprocess (cache dir)
    info!("开始预处理 (preprocess)...");
    let cache_dir = default_cache_dir_for(input_dir);
    if !cache_dir.exists() {
        let config = PreprocessConfig::default();
        let mut preprocessor = CProjectPreprocessor::new(Some(config));
        preprocessor.preprocess_project(input_dir, &cache_dir)?;
        info!("预处理完成，缓存目录: {}", cache_dir.display());
    } else {
        info!("检测到已有缓存目录: {}，跳过预处理", cache_dir.display());
    }

    // Discover C projects
    info!("正在发现C项目...");
    let projects = discover_c_projects(&cache_dir).await?;
    if projects.is_empty() {
        warn!("在目录 {} 中没有找到C项目", input_dir.display());
        return Ok(());
    }
    info!("发现 {} 个C项目:", projects.len());
    for (i, project) in projects.iter().enumerate() {
        info!("  {}. {}", i + 1, project.display());
    }

    // Convert batch
    info!("开始批量转换...");
    let processor = MainProcessor::new(main_processor::pkg_config::get_config()?);
    info!(
        "调用 MainProcessor::process_batch 处理 {} 个项目",
        projects.len()
    );
    match processor.process_batch(projects).await {
        Ok(()) => {
            info!("✅ 所有C到Rust转换完成!");
            info!("📁 转换结果保存在各项目目录下的 'rust-project' 或 'rust_project' 文件夹中");

            // Reorganize workspace
            let workspace_out = output_dir
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| default_workspace_dir_for(input_dir));
            info!("开始重组项目: {}", workspace_out.display());
            let reorganizer = ProjectReorganizer::new(cache_dir.clone(), workspace_out.clone());
            if let Err(e) = reorganizer.reorganize() {
                error!("重组项目失败: {}", e);
            } else {
                info!("📦 已生成工作区: {}", workspace_out.display());
            }
        }
        Err(e) => {
            error!("❌ 转换过程中出现错误: {}", e);
            if e.to_string().contains("max_retry_attempts") {
                warn!(
                    "💡 提示: 请创建配置文件 config/config.toml，并包含 max_retry_attempts 与 concurrent_limit 配置"
                );
            }
        }
    }
    Ok(())
}

/// Helper: create default cache dir alongside input.
fn default_cache_dir_for(input_dir: &Path) -> PathBuf {
    let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
    let dir_name = input_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    parent.join(format!("{}_cache", dir_name))
}

/// Helper: default workspace output dir.
fn default_workspace_dir_for(input_dir: &Path) -> PathBuf {
    let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
    let dir_name = input_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    parent.join(format!("{}_workspace", dir_name))
}

/// Discover C projects under a directory, grouping by parent directories of .c/.h files.
async fn discover_c_projects(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();
    let mut processed_dirs = HashSet::new();

    if dir.is_file() {
        if let Some(ext) = dir.extension() {
            if (ext == "c" || ext == "h") && dir.parent().is_some() {
                let parent = dir.parent().unwrap();
                if !processed_dirs.contains(parent) {
                    projects.push(parent.to_path_buf());
                    processed_dirs.insert(parent.to_path_buf());
                }
            }
        }
        return Ok(projects);
    }

    for entry in WalkDir::new(dir)
        .max_depth(10)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "c" || ext == "h" {
                    if let Some(parent) = path.parent() {
                        if !processed_dirs.contains(parent) {
                            projects.push(parent.to_path_buf());
                            processed_dirs.insert(parent.to_path_buf());
                        }
                    }
                }
            }
        }
    }
    Ok(projects)
}

async fn _dbdata_create() -> DatabaseManager {
    let manager = DatabaseManager::new_default()
        .await
        .expect("Failed to create DatabaseManager");
    manager
}
