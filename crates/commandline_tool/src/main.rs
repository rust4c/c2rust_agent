use std::path::Path;
use commandline_tool::parse_args;
use commandline_tool::Commands;
use env_checker::ai_checker;
use lsp_services::lsp_services::check_function_and_class_name;
use cproject_analy::file_remanager::{CProjectPreprocessor, PreprocessConfig};
use std::fs;
// use env_checker::disk_inspection;
use db_services::DatabaseManager;
use anyhow::Result;
use tokio; //添加 tokio 运行时的文件
use env_checker::dbdata_init;
use env_checker::ai_checker::{ai_service_init, AIConnectionStatus};
// use main_processor::single_process::SingleProcess;
use single_processor::single_processes::singlefile_processor;
use env_logger::Env;

// // 翻译模块
// use main_processor::{MainProcessor, ProjectType};



// 初始化数据库管理器
async fn _dbdata_create() -> DatabaseManager {
    let manager = DatabaseManager::new_default().await.expect("Failed to create DatabaseManager");
    manager
}

#[tokio::main]
async fn main() -> Result<()>{
    // 初始化日志系统，调试使用
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    
    // 初始化数据库连接
    let manager: DatabaseManager = _dbdata_create().await;

    // 检查数据库状态
    match dbdata_init(manager).await {
        Ok(status) => {
            println!("数据库状态: {:?}", status);
        }
        Err(e) => {
            eprintln!("查询数据库状态失败: {}", e);
        }
    }

    let ai_checkers = ai_service_init().await;
    match ai_checkers {
        Ok(status) => {
            println!("AI 服务状态: {:?}", status);
            match status {
                AIConnectionStatus::AllConnected => println!("所有 AI 服务均已连接"),
                AIConnectionStatus::AllDisconnected => println!("所有 AI 服务均未连接"),
                _ => println!("部分 AI 服务连接状态不明"),
            }
        }
        Err(e) => {
            eprintln!("查询 AI 服务状态失败: {}", e);
        }
    }   
    
    //
    let cli = parse_args();

    
    match &cli.command {
        
        Commands::Analyze { 
            input_dir 
        } => {
            println!("已选择分析命令");
            println!("输入目录: {}", input_dir.display());
            let input_dir = input_dir.to_str().unwrap_or("未指定");
            let _ = check_function_and_class_name(input_dir, false);
            Ok(())
        }



        Commands::Preprocess{
            input_dir,
            output_dir
        } => {
            println!("已选择预处理命令");
            println!("输入目录:{}", input_dir.display());

            
            // 确定输出目录
            let output_dir = output_dir.clone().unwrap_or_else(|| {
                let parent = input_dir
                    .parent()
                    .unwrap_or_else(|| Path::new("."));
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
                eprintln!("创建输出目录失败: {}", e);
                return Ok(());
            }

            println!("正在预处理项目...");

            
            let config = PreprocessConfig::default();
            let mut preprocessor = CProjectPreprocessor::new(Some(config));
            
            if let Err(e) = preprocessor.preprocess_project(input_dir, &output_dir) {
                eprintln!("预处理失败: {}", e);
                return Ok(());
            }

           // 使用预处理后的目录进行分析
            println!("预处理完成，缓存目录: {}", output_dir.display());
            println!("开始分析项目...");
            let _ = check_function_and_class_name(output_dir.to_str().unwrap(), false);
            Ok(())
        }

        // Commands::Dbdatebase { 
        //     sqlite_path, 
        //     qdrant_collection, 
        //     qdrant_host, 
        //     qdrant_port, 
        //     vector_size 
        // } => {
        //     println!("已选择数据库配置命令");
        //     // manager.update_sqlite_path(sqlite_path.clone());
        //     // manager.update_qdrant_config(qdrant_collection.clone(), qdrant_host.clone(),
        //     //     *qdrant_port, *vector_size);

        //     Ok(())
        // }


        Commands::AnalyzeRelations { 
            input_dir, 
            project_name, 
            db 
        } => {
            println!("已选择关系分析命令");
            println!("输入目录: {}", input_dir.display());
            println!("项目名称: {}", project_name.as_deref().unwrap_or("未指定"));
            println!("数据库: {}", db);
            Ok(())

            // input_dir.to_str().unwrap_or("未指定");
        }



        Commands::RelationQuery { 
            db, 
            project, 
            query_type, 
            target,
            keyword, 
            limit 
        } => {
            println!("已选择关系查询命令");
            println!("数据库: {}", db);
            println!("项目: {}", project.as_deref().unwrap_or("未指定"));
            println!("查询类型: {:?}", query_type);
            println!("目标: {}", target.as_deref().unwrap_or("未指定"));
            println!("关键词: {}", keyword.as_deref().unwrap_or("未指定"));
            println!("结果限制: {}", limit);
            // "未指定"
            Ok(())
        }
        
        
        
        
        
        Commands::Translate { 
            input_dir, 
            output_dir 
        } => {
            println!("已选择转换命令");
            println!("输入目录: {}", input_dir.display());
            // println!(
            //     "输出目录: {}",
            //     output_dir
            //         .as_ref()
            //         .map_or("未指定", |p| p.to_str().unwrap_or("未指定"))
            // );

            // // 初始化翻译处理器
            // let processor = MainProcessor::new(input_dir.clone());

            // // 运行翻译工作流
            // match processor.run_translation_workflow().await{
            //     Ok(stats) => {
            //         println!("翻译完成");
            //         println!("成功翻译:{} 个项目", stats.successful_translations.len());
            //         println!("失败: {} 个项目", stats.failed_translations.len());

            //         // 如果有输出目录, 将翻译的结果
            //         if let Some(output_path) = output_dir{
            //             println!("将翻译结果移动到: {}", output_path.display());
            //         }
            //     }
            //     Err(e) => {
            //         eprint!("翻译失败: {}", e);
            //     }
            // }
            Ok(())

        }

        Commands::Test{
            input_dir
        } => {
            println!("已选择测试单文件处理命令");
            println!("文件路径: {}", input_dir.display());
            let _ = singlefile_processor(input_dir).await;
            Ok(())
    }
    }
}