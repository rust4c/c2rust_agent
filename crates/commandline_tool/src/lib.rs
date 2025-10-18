use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use log::{error, info, warn};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// Internal crates
use cproject_analy::PreProcessor;
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
    /// Show debug logs (disabled by default)
    #[arg(long, short = 'd', global = true, help = "show debug log")]
    pub debug: bool,
    /// Force re-execution (e.g., re-preprocess even if cached output exists)
    #[arg(
        long,
        short = 'f',
        global = true,
        help = "force re-run even if cached/output exists"
    )]
    pub force: bool,
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

        /// Preprocess cache output directory (optional). If provided, the Rust workspace will be created next to it as <cache_name>_workspace
        #[arg(long, value_name = "DIR")]
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
    /// List all available projects
    ListProjects,
    /// Show project statistics
    Stats,
    /// Generate project report
    Report,
    /// Find function definitions and calls
    FindFunc,
    /// Get function call chain
    CallChain,
    /// Analyze file relationships
    FileAnalysis,
    /// Get most frequently called functions
    TopCalled,
    /// Get most complex functions
    TopComplex,
    /// Analyze file dependencies
    DepsAnalysis,
    /// Search function usage
    Search,
    /// Get function usage summary
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
            info!("Database status: {:?}", status);
        }
        Err(e) => {
            error!("Failed to query database status: {}", e);
        }
    }

    // Initialize AI services
    match ai_service_init().await {
        Ok(status) => {
            info!("AI service status: {:?}", status);
            match status {
                AIConnectionStatus::AllConnected => info!("AI services connected"),
                AIConnectionStatus::AllDisconnected => error!("All AI services disconnected"),
                _ => warn!("Some AI services connection status unknown"),
            }
        }
        Err(e) => {
            error!("Failed to query AI service status: {}", e);
        }
    }

    Ok(())
}

/// Analyze a C project directory.
pub async fn run_analyze(input_dir: &Path) -> Result<()> {
    let input_str = input_dir.to_string_lossy();
    match analyze_project_with_default_database(&input_str, false).await {
        Ok(_) => info!("✅ Analysis completed, results saved to database"),
        Err(e) => {
            error!(
                "⚠️ Database analysis failed, attempting basic analysis: {}",
                e
            );
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

    info!("Input directory: {}", input_dir.display());
    info!("Output directory: {}", output_dir.display());

    std::fs::create_dir_all(&output_dir)?;
    info!("Preprocessing project...");
    let mut preprocessor = PreProcessor::new_default();
    match preprocessor
        .preprocess_project(input_dir, &output_dir)
        .await
    {
        Ok(_) => {}
        Err(e) => {
            error!("Preprocessing failed: {}", e);
            return Err(e);
        }
    }

    info!(
        "Preprocessing completed, cache directory: {}",
        output_dir.display()
    );
    info!("Starting project analysis...");
    let output_str = output_dir.to_string_lossy();
    match analyze_project_with_default_database(&output_str, false).await {
        Ok(_) => info!("✅ Project analysis completed, results saved to database"),
        Err(e) => {
            error!(
                "⚠️ Database analysis failed, attempting basic analysis: {}",
                e
            );
            let _ = check_function_and_class_name(&output_str, false);
        }
    }
    Ok(output_dir)
}

/// Translate a C project into Rust workspace. Performs preprocess, discover, convert, and reorganize.
pub async fn run_translate(input_dir: &Path, output_dir: Option<&Path>) -> Result<()> {
    use anyhow::anyhow;

    if !input_dir.exists() {
        return Err(anyhow!(
            "Input directory does not exist: {}",
            input_dir.display()
        ));
    }

    // Preprocess (cache dir). If output_dir is provided, use it as cache dir.
    info!("Starting preprocessing (preprocess)...");
    let cache_dir = output_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| default_cache_dir_for(input_dir));
    // Only skip preprocess when cache dir already contains .c/.h files
    fn cache_has_c_or_h(dir: &Path) -> bool {
        if !dir.exists() {
            return false;
        }
        for entry in WalkDir::new(dir)
            .max_depth(50)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension() {
                    if ext == "c" || ext == "h" {
                        return true;
                    }
                }
            }
        }
        false
    }

    if !cache_has_c_or_h(&cache_dir) {
        let mut preprocessor = PreProcessor::new_default();
        match preprocessor.preprocess_project(input_dir, &cache_dir).await {
            Ok(_) => {}
            Err(e) => {
                error!("Preprocessing failed: {}", e);
                return Err(e);
            }
        }
        info!(
            "Preprocessing completed, cache directory: {}",
            cache_dir.display()
        );
    } else {
        info!(
            "Detected existing cache directory: {}, skipping preprocessing",
            cache_dir.display()
        );
    }

    // Discover C projects
    info!("Discovering C projects...");
    let projects = discover_c_projects(&cache_dir).await?;
    if projects.is_empty() {
        warn!("No C projects found in directory {}", cache_dir.display());
        return Ok(());
    }
    info!("Found {} C projects:", projects.len());
    for (i, project) in projects.iter().enumerate() {
        info!("  {}. {}", i + 1, project.display());
    }

    // Convert batch
    info!("Starting batch conversion...");
    let processor = MainProcessor::new(main_processor::pkg_config::get_config()?);
    info!(
        "Calling MainProcessor::process_batch to process {} projects",
        projects.len()
    );
    match processor.process_batch(projects).await {
        Ok(()) => {
            info!("✅ All C to Rust conversions completed!");
            info!(
                "📁 Conversion results saved in 'rust-project' or 'rust_project' folders under each project directory"
            );

            // Reorganize workspace
            // If output_dir (cache) is provided, create workspace next to it with suffix _workspace
            let workspace_out = if let Some(p) = output_dir {
                let parent = p.parent().unwrap_or_else(|| Path::new("."));
                let dir_name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "project".to_string());
                parent.join(format!("{}_workspace", dir_name))
            } else {
                default_workspace_dir_for(input_dir)
            };
            info!(
                "Starting project reorganization: {}",
                workspace_out.display()
            );
            let reorganizer = ProjectReorganizer::new(cache_dir.clone(), workspace_out.clone());
            if let Err(e) = reorganizer.reorganize() {
                error!("Project reorganization failed: {}", e);
            } else {
                info!("📦 Workspace generated: {}", workspace_out.display());
            }
        }
        Err(e) => {
            error!("❌ Error occurred during conversion: {}", e);
            if e.to_string().contains("max_retry_attempts") {
                warn!(
                    "💡 Tip: Please create config file config/config.toml with max_retry_attempts and concurrent_limit configurations"
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
