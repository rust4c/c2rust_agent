use db_services::DatabaseManager;
use env_checker::ai_checker::{self, AIConnectionStatus};
use env_checker::{DatabaseConnectionStatus, dbdata_init};
use llm_requester::{deepseek_provider, ollama_provider, openai_provider, xai_provider};
use prompt_builder::call_relation;
use std::path::{PathBuf, Path};
use std::fs;
use anyhow::{Context, Result};
use tokio;
use tracing::{info, warn};

pub struct SingleProcess {
    db_manager: DatabaseManager,
    dir_path: String,
    output_path: String,
}

impl SingleProcess {
    pub fn new(db_manager: DatabaseManager, dir_path: String, output_path: String) -> Self {
        Self {
            db_manager,
            dir_path,
            output_path,
        }
    }

    /// 处理目标文件，分析函数并保存到数据库，分析结果也可保存为文件
    pub async fn process_file(&self, file_path: &str) -> Result<()> {
        info!("开始处理文件: {}", file_path);

        let mut analyzer = call_relation::CallRelationAnalyzer::new(
            self.db_manager.clone(),
            PathBuf::from(&self.dir_path),
        )?;

        let project_name = Path::new(&self.dir_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown_project")
            .to_string();

        info!("项目名称: {}", project_name);

        let file_path_buf = PathBuf::from(file_path);

        // 检查文件是否存在
        if !file_path_buf.exists() {
            return Err(anyhow::anyhow!("目标文件不存在: {}", file_path));
        }

        // 只分析单个文件
        let analysis_result = analyzer
            .analyze_files_and_search(vec![file_path_buf.clone()], &project_name)
            .await
            .context("分析目标文件失败")?;

        // 如果有输出路径，将结果写入文件
        if !self.output_path.is_empty() {
            fs::write(&self.output_path, &analysis_result)
                .context("写入分析结果到文件失败")?;
            info!("分析结果已保存到: {}", self.output_path);
        }

        // 生成并打印分析报告
        let report = analyzer.generate_analysis_report(&project_name);
        info!("{}", report);

        Ok(())
    }
}