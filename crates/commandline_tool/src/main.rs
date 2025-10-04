use commandline_tool::Commands;
use commandline_tool::parse_args;
use cproject_analy::file_remanager::{CProjectPreprocessor, PreprocessConfig};
use lsp_services::lsp_services::{
    analyze_project_with_default_database, check_function_and_class_name,
};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use db_services::DatabaseManager;
use env_checker::ai_checker::{AIConnectionStatus, ai_service_init};
use env_checker::dbdata_init;
use tokio;

use chrono::{Datelike, Local, Timelike};
use log::{debug, error, info, warn};
use main_processor::process_batch_paths;
use project_remanager::ProjectReorganizer;
use rand::SeedableRng;
use rand::{Rng, rngs::StdRng};
use single_processor::single_processes::singlefile_processor;
use std::collections::HashSet;
use tracing_appender::rolling;
use tracing_log::LogTracer;
use tracing_subscriber::filter::LevelFilter as SubLevel;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

// // 翻译模块
// use main_processor::{MainProcessor, ProjectType};

// 初始化数据库管理器
async fn _dbdata_create() -> DatabaseManager {
    let manager = DatabaseManager::new_default()
        .await
        .expect("Failed to create DatabaseManager");
    manager
}

/// 发现C项目 - 简化版本
async fn discover_c_projects(dir: &PathBuf) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();
    let mut processed_dirs = HashSet::new();

    // 如果是文件，直接处理其父目录
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

    // 使用walkdir来避免递归问题
    use walkdir::WalkDir;

    for entry in WalkDir::new(dir)
        .max_depth(10) // 限制深度避免无限遍历
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
    // 先解析 CLI，读取 --debug 开关
    let cli = parse_args();

    // 初始化日志系统，使用 tracing 统一处理 log 宏与 tracing 事件
    let _ = LogTracer::init();

    // 确保 log 目录存在
    let log_dir = Path::new("log");
    if let Err(e) = fs::create_dir_all(log_dir) {
        eprintln!("创建 log 目录失败: {}", e);
    }

    // 控制台输出层（仅用于日志显示，不包含交互提示）
    let stdout_layer = fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_timer(fmt::time::uptime());

    // 将上一次运行的 latest.log 归档为日期命名的文件，当前运行始终写入 latest.log
    let latest_path = log_dir.join("latest.log");
    if latest_path.exists() {
        if let Ok(metadata) = fs::metadata(&latest_path) {
            if let Ok(modified) = metadata.modified() {
                // 生成 10 位数字：yyMMddHH + 随机两位数字
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
                // 若目标已存在则在名称后追加递增编号
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

    // 使用"never"滚动，固定写入 latest.log
    let file_appender = rolling::never(log_dir, "latest.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // 将 guard 泄漏到静态生命周期，确保程序结束前不被释放导致日志丢失
    let _guard: &'static _ = Box::leak(Box::new(guard));

    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_ansi(false)
        .with_writer(non_blocking);

    // 日志级别：终端在非 debug 仅显示 WARN/ERROR；debug 时显示所有
    // 文件日志：非 debug 记录到 INFO；debug 记录到 DEBUG
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

    let subscriber = tracing_subscriber::registry()
        .with(stdout_layer.with_filter(stdout_filter))
        .with(file_layer.with_filter(file_filter));
    let _ = subscriber.try_init();

    // 初始化数据库连接
    let manager: DatabaseManager = _dbdata_create().await;

    // 检查数据库状态
    match dbdata_init(manager).await {
        Ok(status) => {
            // 后台状态记录到日志，不主动在控制台交互输出
            info!("数据库状态: {:?}", status);
        }
        Err(e) => {
            error!("查询数据库状态失败: {}", e);
        }
    }

    let ai_checkers = ai_service_init().await;
    match ai_checkers {
        Ok(status) => {
            // 后台状态记录到日志，不主动在控制台交互输出
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

    // cli 已解析

    match &cli.command {
        Commands::Analyze { input_dir } => {
            debug!("已选择分析命令");
            println!("开始分析项目\n输入目录: {}", input_dir.display());
            let input_dir = input_dir.to_str().unwrap_or("未指定");

            // 使用带数据库支持的分析功能
            match analyze_project_with_default_database(input_dir, false).await {
                Ok(_) => println!("✅ 分析完成，结果已保存到数据库"),
                Err(e) => {
                    error!("⚠️ 数据库分析失败，尝试基础分析: {}", e);
                    let _ = check_function_and_class_name(input_dir, false);
                }
            }
            Ok(())
        }

        Commands::Preprocess {
            input_dir,
            output_dir,
        } => {
            debug!("已选择预处理命令");
            println!("开始预处理\n输入目录: {}", input_dir.display());

            // 确定输出目录
            let output_dir = output_dir.clone().unwrap_or_else(|| {
                let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
                // 获取输入目录名并添加"cache"后缀
                let dir_name = input_dir
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "project".to_string());

                let cache_dir_name = format!("{}_cache", dir_name);
                parent.join(cache_dir_name)
            });
            println!("输出目录: {}", output_dir.display());

            // 确保输出目录存在
            if let Err(e) = fs::create_dir_all(&output_dir) {
                error!("创建输出目录失败: {}", e);
                return Ok(());
            }

            println!("正在预处理项目...");

            let config = PreprocessConfig::default();
            let mut preprocessor = CProjectPreprocessor::new(Some(config));

            if let Err(e) = preprocessor.preprocess_project(input_dir, &output_dir) {
                error!("预处理失败: {}", e);
                return Ok(());
            }

            // 使用预处理后的目录进行分析
            println!("预处理完成，缓存目录: {}", output_dir.display());
            println!("开始分析项目...");

            // 使用带数据库支持的分析功能
            match analyze_project_with_default_database(output_dir.to_str().unwrap(), false).await {
                Ok(_) => println!("✅ 项目分析完成，结果已保存到数据库"),
                Err(e) => {
                    error!("⚠️ 数据库分析失败，尝试基础分析: {}", e);
                    let _ = check_function_and_class_name(output_dir.to_str().unwrap(), false);
                }
            }
            Ok(())
        }

        // main.rs 中 Translate 命令的修改部分
        Commands::Translate {
            input_dir,
            output_dir, // 若提供则用于最终重组输出
        } => {
            println!("已选择转换命令\n输入目录: {}", input_dir.display());

            let cfg = main_processor::pkg_config::get_config()?;

            if !input_dir.exists() {
                error!("错误: 输入目录不存在: {}", input_dir.display());
                println!("错误: 输入目录不存在: {}", input_dir.display());
                return Ok(());
            }

            // 第一步：预处理 -> 生成 src_cache
            println!("开始预处理 (preprocess)...");
            let cache_dir = {
                let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
                let dir_name = input_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "project".to_string());
                parent.join(format!("{}_cache", dir_name))
            };

            // 如果 cache 目录不存在才运行预处理，避免重复开销
            if !cache_dir.exists() {
                let config = PreprocessConfig::default();
                let mut preprocessor = CProjectPreprocessor::new(Some(config));
                if let Err(e) = preprocessor.preprocess_project(input_dir, &cache_dir) {
                    error!("预处理失败: {}", e);
                    println!("预处理失败: {}", e);
                    return Ok(());
                }
                println!("预处理完成，缓存目录: {}", cache_dir.display());
            } else {
                println!("检测到已有缓存目录: {}，跳过预处理", cache_dir.display());
            }

            // 第二步：发现 C 项目（基于 cache 目录）
            println!("正在发现C项目...");
            let projects = match discover_c_projects(&cache_dir).await {
                Ok(projects) => projects,
                Err(e) => {
                    error!("发现C项目失败: {}", e);
                    println!("发现C项目失败: {}", e);
                    return Ok(());
                }
            };

            if projects.is_empty() {
                warn!("在目录 {} 中没有找到C项目", input_dir.display());
                println!("在目录 {} 中没有找到C项目", input_dir.display());
                return Ok(());
            }

            println!("发现 {} 个C项目:", projects.len());
            for (i, project) in projects.iter().enumerate() {
                println!("  {}. {}", i + 1, project.display());
            }

            // 第三步：批量转换 C -> Rust
            println!("开始批量转换...");
            match process_batch_paths(cfg, projects).await {
                Ok(()) => {
                    info!("✅ 所有C到Rust转换完成!");
                    println!("🎉 转换成功完成!");
                    println!(
                        "📁 转换结果保存在各项目目录下的 'rust-project' 或 'rust_project' 文件夹中"
                    );

                    // 第四步：重组为一个 Rust 工作区
                    let workspace_out = output_dir.clone().unwrap_or_else(|| {
                        let parent = input_dir.parent().unwrap_or_else(|| Path::new("."));
                        let dir_name = input_dir
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "project".to_string());
                        parent.join(format!("{}_workspace", dir_name))
                    });
                    println!("开始重组项目: {}", workspace_out.display());
                    let reorganizer =
                        ProjectReorganizer::new(cache_dir.clone(), workspace_out.clone());
                    if let Err(e) = reorganizer.reorganize() {
                        error!("重组项目失败: {}", e);
                        println!("重组项目失败: {}", e);
                    } else {
                        println!("📦 已生成工作区: {}", workspace_out.display());
                    }
                }
                Err(e) => {
                    error!("❌ 转换过程中出现错误: {}", e);
                    println!("⚠️  转换失败，错误详情: {}", e);

                    // 提供更具体的错误信息
                    if e.to_string().contains("max_retry_attempts") {
                        println!("💡 提示: 请创建配置文件 config/config.toml");
                        println!("     内容示例:");
                        println!("     max_retry_attempts = 3");
                        println!("     concurrent_limit = 5");
                    }
                }
            }
            Ok(())
        }

        Commands::Test { input_dir } => {
            println!(
                "已选择测试单文件处理命令\n文件路径: {}",
                input_dir.display()
            );
            let _ = singlefile_processor(input_dir).await;
            Ok(())
        }
    }
}
