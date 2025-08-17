use std::path::PathBuf;
use std::path::Path;
use commandline_tool::parse_args;
use commandline_tool::Commands;
use lsp_services::lsp_services::check_function_and_class_name;
use cproject_analy::file_remanager::{CProjectPreprocessor, PreprocessConfig};


pub fn main(){

    let cli = parse_args();


    match &cli.command {
        
        Commands::Analyze { 
            input_dir 
        } => {
            println!("已选择分析命令");
            println!("输入目录: {}", input_dir.display());
            let input_dir = input_dir.to_str().unwrap_or("未指定");
            let _ = check_function_and_class_name(input_dir, false);
        }

        Commands::Preprocess{
            input_dir,
            output_dir
        } => {
            println!("已选择预处理命令");
            println!("输入目录:{}", input_dir.display());
            println!(
                "输出目录: {}",
                output_dir.to_str().unwrap_or("未指定")
                );
                // let temp_cache = TempDir::new().expect("创建临时目录失败");
                let cache_path = output_dir.as_path();


                println!("正在预处理项目...");
            let config = PreprocessConfig::default();
            let mut preprocessor = CProjectPreprocessor::new(Some(config));
            
            if let Err(e) = preprocessor.preprocess_project(input_dir, cache_path) {
                eprintln!("预处理失败: {}", e);
                return;
            }
            
            // 使用预处理后的目录进行分析
            println!("预处理完成，缓存目录: {}", cache_path.display());
            println!("开始分析项目...");
            let _ = check_function_and_class_name(cache_path.to_str().unwrap(), false);
        }



        Commands::Translate { 
            input_dir, 
            output_dir 
        } => {
            println!("已选择转换命令");
            println!("输入目录: {}", input_dir.display());
            println!(
                "输出目录: {}",
                output_dir
                    .as_ref()
                    .map_or("未指定", |p| p.to_str().unwrap_or("未指定"))
            );
            // let output_dir = output_dir
            //     .as_ref()
            //     .and_then(|p| p.to_str())
            //     .unwrap_or_else(|| input_dir.to_str().unwrap_or("未指定"));
        }


        Commands::AnalyzeRelations { 
            input_dir, 
            project_name, 
            db 
        } => {
            println!("已选择关系分析命令");
            println!("输入目录: {}", input_dir.display());
            println!("项目名称: {}", project_name.as_deref().unwrap_or("未指定"));
            println!("数据库: {}", db);
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
        }
    }
}