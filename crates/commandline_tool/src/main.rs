use commandline_tool::Commands;
use commandline_tool::parse_args;
use cproject_analy::file_remanager::{CProjectPreprocessor, PreprocessConfig};
use env_checker::ai_checker;
use lsp_services::lsp_services::{
    analyze_project_with_default_database, check_function_and_class_name,
};
use std::fs;
use std::path::{Path, PathBuf};
// use env_checker::disk_inspection;
use anyhow::Result;
use db_services::DatabaseManager;
use env_checker::ai_checker::{AIConnectionStatus, ai_service_init};
use env_checker::dbdata_init;
use tokio; //添加 tokio 运行时的文件
// use main_processor::single_process::SingleProcess;
use log::{debug, error, info, warn};
use main_processor::{process_batch_paths, process_single_path};
use single_processor::single_processes::singlefile_processor;
use std::collections::HashSet;
use tracing_subscriber::filter::LevelFilter as SubLevel;
use tracing_log::LogTracer;
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
    // 初始化日志系统，使用 tracing 统一处理 log 宏与 tracing 事件
    // 将 log::log! 重定向到 tracing
    let _ = LogTracer::init();
    // 控制台简洁输出，默认 debug 级别
    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_timer(fmt::time::uptime());
    let subscriber = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(SubLevel::DEBUG);
    let _ = subscriber.try_init();

    // 初始化数据库连接
    let manager: DatabaseManager = _dbdata_create().await;

    // 检查数据库状态
    match dbdata_init(manager).await {
        Ok(status) => {
            info!("数据库状态: {:?}", status);
        }
        Err(e) => {
            error!("查询数据库状态失败: {}", e);
        }
    }

    let ai_checkers = ai_service_init().await;
    match ai_checkers {
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

    //
    let cli = parse_args();

    match &cli.command {
        Commands::Analyze { input_dir } => {
            debug!("已选择分析命令");
            info!("输入目录: {}", input_dir.display());
            let input_dir = input_dir.to_str().unwrap_or("未指定");

            // 使用带数据库支持的分析功能
            match analyze_project_with_default_database(input_dir, false).await {
                Ok(_) => info!("✅ 分析完成，结果已保存到数据库"),
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
            info!("输入目录:{}", input_dir.display());

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
            info!("输出目录: {}", output_dir.display());

            // 确保输出目录存在
            if let Err(e) = fs::create_dir_all(&output_dir) {
                error!("创建输出目录失败: {}", e);
                return Ok(());
            }

            info!("正在预处理项目...");

            let config = PreprocessConfig::default();
            let mut preprocessor = CProjectPreprocessor::new(Some(config));

            if let Err(e) = preprocessor.preprocess_project(input_dir, &output_dir) {
                error!("预处理失败: {}", e);
                return Ok(());
            }

            // 使用预处理后的目录进行分析
            info!("预处理完成，缓存目录: {}", output_dir.display());
            info!("开始分析项目...");

            // 使用带数据库支持的分析功能
            match analyze_project_with_default_database(output_dir.to_str().unwrap(), false).await {
                Ok(_) => info!("✅ 项目分析完成，结果已保存到数据库"),
                Err(e) => {
                    error!("⚠️ 数据库分析失败，尝试基础分析: {}", e);
                    let _ = check_function_and_class_name(output_dir.to_str().unwrap(), false);
                }
            }
            Ok(())
        }

        // Commands::AnalyzeRelations {
        //     input_dir,
        //     project_name,
        // } => {
        //     info!("已选择关系分析命令");
        //     info!("输入目录: {}", input_dir.display());
        //     info!("项目名称: {}", project_name.as_deref().unwrap_or("未指定"));
        //     Ok(())

        //     // input_dir.to_str().unwrap_or("未指定");
        // }

        // Commands::RelationQuery {
        //     db,
        //     project,
        //     query_type,
        //     target,
        //     keyword,
        //     limit,
        // } => {
        //     info!("已选择关系查询命令");
        //     info!("数据库: {}", db);
        //     info!("项目: {}", project.as_deref().unwrap_or("未指定"));
        //     info!("查询类型: {:?}", query_type);
        //     info!("目标: {}", target.as_deref().unwrap_or("未指定"));
        //     info!("关键词: {}", keyword.as_deref().unwrap_or("未指定"));
        //     info!("结果限制: {}", limit);
        //     // "未指定"
        //     Ok(())
        // }

        // main.rs 中 Translate 命令的修改部分
        Commands::Translate {
            input_dir,
            output_dir: _, // 暂时忽略output_dir参数
        } => {
            info!("已选择转换命令");
            info!("输入目录: {}", input_dir.display());

            let cfg = main_processor::pkg_config::get_config()?;

            if !input_dir.exists() {
                error!("错误: 输入目录不存在: {}", input_dir.display());
                return Ok(());
            }

            // 发现C项目
            info!("正在发现C项目...");
            let projects = match discover_c_projects(input_dir).await {
                Ok(projects) => projects,
                Err(e) => {
                    error!("发现C项目失败: {}", e);
                    return Ok(());
                }
            };

            if projects.is_empty() {
                warn!("在目录 {} 中没有找到C项目", input_dir.display());
                return Ok(());
            }

            info!("发现 {} 个C项目:", projects.len());
            for (i, project) in projects.iter().enumerate() {
                info!("  {}. {}", i + 1, project.display());
            }

            // 使用批量处理功能进行转换
            info!("开始批量转换...");
            match process_batch_paths(cfg, projects).await {
                Ok(()) => {
                    info!("✅ 所有C到Rust转换完成!");
                    println!("🎉 转换成功完成!");
                    println!("📁 转换结果保存在各项目目录下的 'rust-project' 文件夹中");
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
            info!("已选择测试单文件处理命令");
            info!("文件路径: {}", input_dir.display());
            let _ = singlefile_processor(input_dir).await;
            Ok(())
        }
    }
}
