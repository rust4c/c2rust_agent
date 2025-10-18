//
//                        _oo0oo_
//                       o8888888o
//                       88" . "88
//                       (| -_- |)
//                       0\  =  /0
//                     ___/`---"\___
//                   ." \\|     |// ".
//                  / \\|||  :  |||// \
//                 / _||||| -:- |||||- \
//                |   | \\\  -  /// |   |
//                | \_|  ""\---/""  |_/ |
//                \  .-\__  "-"  ___/-. /
//              ___". ."  /--.--\  `. ."___
//           ."" "<  `.___\_<|>_/___." >" "".
//          | | :  `- \`.;`\ _ /`;.`/ - ` : | |
//          \  \ `_.   \_ __\ /__ _/   .-` /  /
//      =====`-.____`.___ \_____/___.-`___.-"=====
//                        `=---="
//
//
//      ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
//             Cyber Buddha's light shines, programs run without worry.
//             Every word is perfect, translation results are smooth.
//

use commandline_tool::Commands;
use commandline_tool::parse_args;
use cproject_analy::PreProcessor;
use lsp_services::lsp_services::{
    analyze_project_with_default_database, check_function_and_class_name,
};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use env_checker::{AIConnectionStatus, IssueLevel, check_all};

use chrono::{Datelike, Local, Timelike};
use log::{debug, error, info, warn};
use main_processor::MainProcessor;
use project_remanager::ProjectReorganizer;
use rand::SeedableRng;
use rand::{Rng, rngs::StdRng};
use std::collections::HashSet;
use tracing_appender::rolling;
use tracing_log::LogTracer;
use tracing_subscriber::filter::LevelFilter as SubLevel;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

// // Translation module
// use main_processor::{MainProcessor, ProjectType};

/// Discover C projects - simplified version
async fn discover_c_projects(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();
    let mut processed_dirs = HashSet::new();

    // If it's a file, process its parent directory directly
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

    // Use walkdir to avoid recursion issues
    use walkdir::WalkDir;

    for entry in WalkDir::new(dir)
        .max_depth(10) // Limit depth to avoid infinite traversal
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

#[tokio::main]
async fn main() -> Result<()> {
    println!(
        r#"
        .oooooo.     .oooo.   ooooooooo.                            .
       d8P'  `Y8b  .dP""Y88b  `888   `Y88.                        .o8
      888                ]8P'  888   .d88' oooo  oooo   .oooo.o .o888oo
      888              .d8P'   888ooo88P'  `888  `888  d88(  "8   888
      888            .dP'      888`88b.     888   888  `"Y88b.    888
      `88b    ooo  .oP     .o  888  `88b.   888   888  o.  )88b   888 .
       `Y8bood8P'  8888888888 o888o  o888o  `V88V"V8P' 8""888P'   "888"
    "#
    );
    // First parse CLI, read --debug switch
    let cli = parse_args();

    // Initialize logging system, use tracing to handle both log macros and tracing events uniformly
    let _ = LogTracer::init();

    // Ensure log directory exists
    let log_dir = Path::new("log");
    if let Err(e) = fs::create_dir_all(log_dir) {
        eprintln!("Failed to create log directory: {}", e);
    }

    // Console output layer (for log display only, no interactive prompts)
    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_timer(fmt::time::uptime());

    // Archive the previous run's latest.log as a date-named file, current run always writes to latest.log
    let latest_path = log_dir.join("latest.log");
    if latest_path.exists() {
        if let Ok(metadata) = fs::metadata(&latest_path) {
            if let Ok(modified) = metadata.modified() {
                // Generate 10-digit number: yyMMddHH + random two digits
                let datetime: chrono::DateTime<Local> = modified.into();
                let mut rng = StdRng::from_entropy();
                let rnd: u8 = rng.gen_range(0..100);
                let code = format!(
                    "{:02}{:02}{:02}{:02}{:02}",
                    (datetime.year() % 100) as i32,
                    datetime.month(),
                    datetime.day(),
                    datetime.hour(),
                    rnd
                );
                let archive_path = log_dir.join(format!("{}.log", code));
                // If target already exists, append incremental number to the name
                let mut final_path = archive_path.clone();
                let mut idx = 1;
                while final_path.exists() {
                    final_path = log_dir.join(format!("{}-{}.log", code, idx));
                    idx += 1;
                }
                let _ = fs::rename(&latest_path, &final_path);
            }
        }
    }

    // Use "never" rolling, fixed write to latest.log
    let file_appender = rolling::never(log_dir, "latest.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Leak guard to static lifetime to ensure it's not released before program ends, preventing log loss
    let _guard: &'static _ = Box::leak(Box::new(guard));

    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .with_writer(non_blocking);

    // Log levels: terminal shows only WARN/ERROR in non-debug; shows all in debug
    // File log: records to INFO in non-debug; records to DEBUG in debug
    let stdout_filter = if cli.debug {
        SubLevel::DEBUG
    } else {
        SubLevel::WARN
    };
    let file_filter = if cli.debug {
        SubLevel::DEBUG
    } else {
        SubLevel::INFO
    };

    info!(
        r#"
        .oooooo.     .oooo.   ooooooooo.                            .
       d8P'  `Y8b  .dP""Y88b  `888   `Y88.                        .o8
      888                ]8P'  888   .d88' oooo  oooo   .oooo.o .o888oo
      888              .d8P'   888ooo88P'  `888  `888  d88(  "8   888
      888            .dP'      888`88b.     888   888  `"Y88b.    888
      `88b    ooo  .oP     .o  888  `88b.   888   888  o.  )88b   888 .
       `Y8bood8P'  8888888888 o888o  o888o  `V88V"V8P' 8""888P'   "888"
    "#
    );

    let subscriber = tracing_subscriber::registry()
        .with(stdout_layer.with_filter(stdout_filter))
        .with(file_layer.with_filter(file_filter));
    let _ = subscriber.try_init();

    let summary = check_all().await;

    if let Some(err) = &summary.config.error {
        error!("Failed to load config file: {}", err);
        println!("Failed to load config file: {}", err);
        return Err(anyhow!(err.clone()));
    }

    if let Some(report) = &summary.config.report {
        if !report.issues.is_empty() {
            println!("Configuration check results ({}):", report.path.display());
        }

        let mut has_error = false;
        for issue in &report.issues {
            match issue.level {
                IssueLevel::Error => {
                    has_error = true;
                    error!("Configuration error [{}]: {}", issue.field, issue.message);
                    println!("  ❌ {} -> {}", issue.field, issue.message);
                }
                IssueLevel::Warning => {
                    warn!("Configuration warning [{}]: {}", issue.field, issue.message);
                    println!("  ⚠️ {} -> {}", issue.field, issue.message);
                }
            }
        }

        if has_error {
            println!("Configuration contains fatal errors, please fix and retry.");
            return Err(anyhow!("configuration validation failed"));
        }
    }

    if let Some(err) = &summary.database.error {
        error!("Failed to query database status: {}", err);
        println!("Failed to query database status: {}", err);
        return Err(anyhow!("Database check failed: {}", err));
    }

    if let Some(status) = &summary.database.status {
        info!("Database status: {:?}", status);
    }

    match (&summary.ai.status, &summary.ai.error) {
        (Some(status), _) => {
            info!("AI service status: {:?}", status);
            match status {
                AIConnectionStatus::AllConnected => info!("AI services connected"),
                AIConnectionStatus::AllDisconnected => error!("All AI services disconnected"),
                _ => warn!("Some AI services connection status unknown"),
            }
        }
        (None, Some(err)) => {
            error!("Failed to query AI service status: {}", err);
        }
        _ => {}
    }

    // cli 已解析

    match &cli.command {
        Commands::Analyze { input_dir } => {
            debug!("Analyze command selected");
            println!(
                "Starting project analysis\nInput directory: {}",
                input_dir.display()
            );
            let input_dir = input_dir.to_str().unwrap_or("Not specified");
            if cli.force {
                println!("--force enabled: will force re-execution of analysis steps");
                info!("--force enabled: rerun analyze");
            }

            // Use database-supported analysis functionality
            match analyze_project_with_default_database(input_dir, false).await {
                Ok(_) => println!("✅ Analysis completed, results saved to database"),
                Err(e) => {
                    error!(
                        "⚠️ Database analysis failed, attempting basic analysis: {}",
                        e
                    );
                    let _ = check_function_and_class_name(input_dir, false);
                }
            }
            Ok(())
        }

        Commands::Preprocess {
            input_dir,
            output_dir,
        } => {
            debug!("Preprocess command selected");
            println!(
                "Starting preprocessing\nInput directory: {}",
                input_dir.display()
            );

            // Determine output directory
            let output_dir = output_dir.clone().unwrap_or_else(|| {
                let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
                // Get input directory name and add "cache" suffix
                let dir_name = input_dir
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "project".to_string());

                let cache_dir_name = format!("{}_cache", dir_name);
                parent.join(cache_dir_name)
            });
            println!("Output directory: {}", output_dir.display());

            // Ensure output directory exists
            if let Err(e) = fs::create_dir_all(&output_dir) {
                error!("Failed to create output directory: {}", e);
                return Ok(());
            }

            if cli.force {
                println!(
                    "--force enabled: will force re-preprocessing even if output directory exists"
                );
                info!("--force enabled: force preprocess in Preprocess command");
            } else {
                println!("Preprocessing project...");
            }

            let mut preprocessor = PreProcessor::new_default();

            if let Err(e) = preprocessor
                .preprocess_project(input_dir, &output_dir)
                .await
            {
                error!("Preprocessing failed: {}", e);
                return Ok(());
            }

            // Use preprocessed directory for analysis
            println!(
                "Preprocessing completed, cache directory: {}",
                output_dir.display()
            );
            println!("Starting project analysis...");

            // Use database-supported analysis functionality
            match analyze_project_with_default_database(output_dir.to_str().unwrap(), false).await {
                Ok(_) => println!("✅ Project analysis completed, results saved to database"),
                Err(e) => {
                    error!(
                        "⚠️ Database analysis failed, attempting basic analysis: {}",
                        e
                    );
                    let _ = check_function_and_class_name(output_dir.to_str().unwrap(), false);
                }
            }
            Ok(())
        }

        // Translate command modification in main.rs
        Commands::Translate {
            input_dir,
            output_dir, // If provided, used as preprocessing cache directory, workspace will be generated in sibling directory *_workspace
        } => {
            println!(
                "Translate command selected\nInput directory: {}",
                input_dir.display()
            );

            let processor = MainProcessor::new(main_processor::pkg_config::get_config()?);

            if !input_dir.exists() {
                error!(
                    "Error: Input directory does not exist: {}",
                    input_dir.display()
                );
                println!(
                    "Error: Input directory does not exist: {}",
                    input_dir.display()
                );
                return Ok(());
            }

            // Step 1: Preprocess -> Generate cache directory (if --output-dir is provided, use it as cache directory)
            println!("Starting preprocessing (preprocess)...");
            let cache_dir: PathBuf = if let Some(p) = output_dir.as_ref() {
                p.clone()
            } else {
                let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
                let dir_name = input_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "project".to_string());
                parent.join(format!("{}_cache", dir_name))
            };

            // If cache directory doesn't exist or contains no .c/.h files, run preprocessing
            fn cache_has_c_or_h(dir: &Path) -> bool {
                if !dir.exists() {
                    return false;
                }
                use walkdir::WalkDir;
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

            if cli.force || !cache_has_c_or_h(&cache_dir) {
                if cli.force {
                    println!("--force enabled: will re-preprocess even if cache exists");
                    info!("--force enabled: re-run preprocess even if cache exists");
                }

                let mut preprocessor = PreProcessor::new_default();
                if let Err(e) = preprocessor.preprocess_project(input_dir, &cache_dir).await {
                    error!("Preprocessing failed: {}", e);
                    println!("Preprocessing failed: {}", e);
                    return Ok(());
                }

                println!(
                    "Preprocessing completed, cache directory: {}",
                    cache_dir.display()
                );
            } else {
                println!(
                    "Detected existing cache directory: {}, skipping preprocessing",
                    cache_dir.display()
                );
            }

            // Step 2: Discover C projects (based on cache directory)
            println!("Discovering C projects...");
            let projects = match discover_c_projects(&cache_dir).await {
                Ok(projects) => projects,
                Err(e) => {
                    error!("Failed to discover C projects: {}", e);
                    println!("Failed to discover C projects: {}", e);
                    return Ok(());
                }
            };

            if projects.is_empty() {
                warn!("No C projects found in directory {}", cache_dir.display());
                println!("No C projects found in directory {}", cache_dir.display());
                return Ok(());
            }

            println!("Found {} C projects:", projects.len());
            for (i, project) in projects.iter().enumerate() {
                println!("  {}. {}", i + 1, project.display());
                info!("Found project to process: {}", project.display());
            }

            // Step 3: If dependency graph exists, use dependency-aware scheduling; otherwise use regular batch processing
            println!("Starting batch conversion...");
            // Prioritize user-specified fixed path
            let user_graph =
                PathBuf::from("/Users/peng/Documents/Tmp/chibicc_cache/relation_graph.json");
            let graph_in_cache = cache_dir.join("relation_graph.json");
            if user_graph.exists() {
                info!(
                    "Using dependency-aware scheduling (user path): {}",
                    user_graph.display()
                );
                match processor
                    .process_with_graph(&user_graph, Some(&cache_dir))
                    .await
                {
                    Ok(()) => {
                        info!("✅ Dependency-aware processing completed");
                    }
                    Err(e) => {
                        error!(
                            "Dependency-aware processing failed: {}, falling back to regular batch processing",
                            e
                        );
                        let _ = processor.process_batch(projects.clone()).await;
                    }
                }
            } else if graph_in_cache.exists() {
                info!(
                    "Using dependency-aware scheduling (relation_graph.json in cache): {}",
                    graph_in_cache.display()
                );
                match processor
                    .process_with_graph(&graph_in_cache, Some(&cache_dir))
                    .await
                {
                    Ok(()) => {
                        info!("✅ Dependency-aware processing completed");
                    }
                    Err(e) => {
                        error!(
                            "Dependency-aware processing failed: {}, falling back to regular batch processing",
                            e
                        );
                        let _ = processor.process_batch(projects.clone()).await;
                    }
                }
            } else {
                info!(
                    "No relation_graph.json found, calling MainProcessor::process_batch to process {} projects",
                    projects.len()
                );
                match processor.process_batch(projects).await {
                    Ok(()) => {
                        info!("✅ All C to Rust conversions completed!");
                        println!("🎉 Conversion completed successfully!");
                        println!(
                            "📁 Conversion results saved in 'rust-project' or 'rust_project' folders under each project directory"
                        );

                        // Step 4: Reorganize into a Rust workspace
                        // If --output-dir (cache directory) was provided, create <cache_name>_workspace in its sibling directory
                        // Otherwise create <input_name>_workspace according to input directory rules
                        let workspace_out: PathBuf = if let Some(p) = output_dir.as_ref() {
                            let parent = p.parent().unwrap_or_else(|| Path::new("."));
                            let dir_name = p
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "project".to_string());
                            parent.join(format!("{}_workspace", dir_name))
                        } else {
                            let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
                            let dir_name = input_dir
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "project".to_string());
                            parent.join(format!("{}_workspace", dir_name))
                        };
                        println!(
                            "Starting project reorganization: {}",
                            workspace_out.display()
                        );
                        let reorganizer =
                            ProjectReorganizer::new(cache_dir.clone(), workspace_out.clone());
                        if let Err(e) = reorganizer.reorganize() {
                            error!("Project reorganization failed: {}", e);
                            println!("Project reorganization failed: {}", e);
                        } else {
                            println!("📦 Workspace generated: {}", workspace_out.display());
                        }
                    }
                    Err(e) => {
                        error!("❌ Error occurred during conversion: {}", e);
                        println!("⚠️  Conversion failed, error details: {}", e);

                        // Provide more specific error information
                        if e.to_string().contains("max_retry_attempts") {
                            println!("💡 Tip: Please create config file config/config.toml");
                            println!("     Example content:");
                            println!("     max_retry_attempts = 3");
                            println!("     concurrent_limit = 5");
                        }
                    }
                }
            }
            Ok(())
        }

        Commands::Test { input_dir } => {
            println!(
                "Test single file processing command selected\nFile path: {}",
                input_dir.display()
            );
            let cfg = main_processor::pkg_config::get_config()?;
            let processor = MainProcessor::new(cfg);

            if let Err(err) = processor.process_single(input_dir).await {
                error!("❌ Single file processing failed: {}", err);
                println!("❌ Single file processing failed, details: {}", err);
            }
            Ok(())
        }
    }
}
