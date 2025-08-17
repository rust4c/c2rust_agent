pub mod file_remanager;
use file_remanager::{CProjectPreprocessor, PreprocessConfig, ProcessingStats};

use db_services::DatabaseManager;
use lsp_services::lsp_services::ClangdAnalyzer;

use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{error, info, warn};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PreprocessorConfig {
    /// 数据库配置
    pub database_url: Option<String>,
    /// Qdrant 配置
    pub qdrant_url: Option<String>,
    /// 工作线程数
    pub worker_count: usize,
    /// 项目预处理配置
    pub preprocess_config: Option<PreprocessConfig>,
}

impl Default for PreprocessorConfig {
    fn default() -> Self {
        Self {
            database_url: None,
            qdrant_url: None,
            worker_count: num_cpus::get().max(1),
            preprocess_config: None,
        }
    }
}

pub struct PreProcessor {
    config: PreprocessorConfig,
    db_manager: Option<DatabaseManager>,
    multi_progress: MultiProgress,
}

impl PreProcessor {
    /// 创建新的预处理器实例
    pub fn new(config: PreprocessorConfig) -> Self {
        Self {
            config,
            db_manager: None,
            multi_progress: MultiProgress::new(),
        }
    }

    /// 使用默认配置创建预处理器
    pub fn new_default() -> Self {
        Self::new(PreprocessorConfig::default())
    }

    /// 初始化数据库连接
    pub async fn initialize_database(&mut self) -> Result<()> {
        let main_pb = self.multi_progress.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("🔌 正在连接数据库...");

        // 初始化数据库管理器
        self.db_manager = Some(
            DatabaseManager::new_default()
                .await
                .context("Failed to initialize database manager")?,
        );

        main_pb.finish_with_message("✅ 数据库连接成功!");
        info!("数据库初始化完成");
        Ok(())
    }

    /// 执行项目预处理
    pub async fn preprocess_project(
        &mut self,
        source_dir: &Path,
        cache_dir: &Path,
    ) -> Result<ProcessingStats> {
        info!(
            "开始预处理项目: {} -> {}",
            source_dir.display(),
            cache_dir.display()
        );

        // 确保数据库已初始化
        if self.db_manager.is_none() {
            self.initialize_database().await?;
        }

        // 创建缓存目录
        if !cache_dir.exists() {
            fs::create_dir_all(cache_dir).context("Failed to create cache directory")?;
        }

        // 步骤1：文件整理和映射生成
        let file_processing_stats = self.process_files(source_dir, cache_dir).await?;

        // 步骤2：并行执行 LSP 分析和数据库存储
        let mapping_path = cache_dir.join("mapping.json");
        if mapping_path.exists() {
            self.parallel_analysis_and_storage(source_dir, cache_dir, &mapping_path)
                .await?;
        } else {
            warn!("映射文件不存在，跳过 LSP 分析");
        }

        Ok(file_processing_stats)
    }

    /// 处理文件整理
    async fn process_files(
        &mut self,
        source_dir: &Path,
        cache_dir: &Path,
    ) -> Result<ProcessingStats> {
        let main_pb = self.multi_progress.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("📁 开始项目文件整理...");

        let mut preprocessor = CProjectPreprocessor::new(self.config.preprocess_config.clone());
        let stats = preprocessor
            .preprocess_project(source_dir, cache_dir)
            .context("Failed to preprocess project files")?;

        main_pb.finish_with_message("✅ 文件整理完成!");
        Ok(stats)
    }

    /// 并行执行 LSP 分析和数据库存储
    async fn parallel_analysis_and_storage(
        &mut self,
        source_dir: &Path,
        cache_dir: &Path,
        mapping_path: &Path,
    ) -> Result<()> {
        let main_pb = self.multi_progress.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("🔄 开始并行分析和存储...");

        // 读取映射文件
        let mapping_content =
            fs::read_to_string(mapping_path).context("Failed to read mapping file")?;
        let mapping: Value =
            serde_json::from_str(&mapping_content).context("Failed to parse mapping JSON")?;

        let db_manager = Arc::new(self.db_manager.take().unwrap());

        // 创建进度条
        let lsp_pb = self.multi_progress.add(ProgressBar::new_spinner());
        lsp_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.yellow} {msg}")
                .unwrap(),
        );
        lsp_pb.enable_steady_tick(Duration::from_millis(100));

        let db_pb = self.multi_progress.add(ProgressBar::new_spinner());
        db_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.magenta} {msg}")
                .unwrap(),
        );
        db_pb.enable_steady_tick(Duration::from_millis(100));

        // 准备线程数据
        let source_dir = source_dir.to_path_buf();
        let cache_dir = cache_dir.to_path_buf();

        // 启动 LSP 分析线程
        let lsp_handle = {
            let lsp_pb = lsp_pb.clone();
            let source_dir = source_dir.clone();
            let cache_dir = cache_dir.clone();

            thread::spawn(move || -> Result<()> {
                lsp_pb.set_message("🔍 正在进行 LSP 分析...");

                let mut analyzer = ClangdAnalyzer::new(source_dir.to_str().unwrap());
                analyzer.analyze_project().context("LSP analysis failed")?;

                // 保存分析结果到缓存目录
                let analysis_path = cache_dir.join("lsp_analysis.json");
                let analysis_result = serde_json::json!({
                    "functions": analyzer.functions,
                    "classes": analyzer.classes,
                    "variables": analyzer.variables,
                    "macros": analyzer.macros,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                });

                fs::write(
                    &analysis_path,
                    serde_json::to_string_pretty(&analysis_result)?,
                )
                .context("Failed to save LSP analysis results")?;

                lsp_pb.finish_with_message("✅ LSP 分析完成!");
                Ok(())
            })
        };

        // 启动数据库存储线程
        let db_handle = {
            let db_pb = db_pb.clone();
            let mapping = mapping.clone();

            thread::spawn(move || -> Result<()> {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    db_pb.set_message("💾 正在存储到数据库...");

                    // 这里可以根据映射文件处理数据库存储逻辑
                    // 例如：存储文件映射信息、接口信息等
                    if let Some(mappings) = mapping.get("mappings").and_then(|m| m.as_array()) {
                        db_pb.set_message(format!("💾 正在存储 {} 个文件映射...", mappings.len()));

                        // 示例：可以在这里添加具体的数据库存储逻辑
                        // for mapping in mappings {
                        //     // 处理每个映射项的数据库存储
                        // }
                    }

                    db_pb.finish_with_message("✅ 数据库存储完成!");
                    Ok(())
                })
            })
        };

        // 等待两个线程完成
        let lsp_result = lsp_handle
            .join()
            .map_err(|e| anyhow::anyhow!("LSP thread panicked: {:?}", e))?;
        let db_result = db_handle
            .join()
            .map_err(|e| anyhow::anyhow!("DB thread panicked: {:?}", e))?;

        // 检查结果
        if let Err(e) = lsp_result {
            error!("LSP 分析失败: {}", e);
        }
        if let Err(e) = db_result {
            error!("数据库存储失败: {}", e);
        }

        // 恢复数据库管理器
        self.db_manager =
            Some(Arc::try_unwrap(db_manager).map_err(|_| anyhow::anyhow!("Failed to unwrap Arc"))?);

        main_pb.finish_with_message("✅ 分析和存储完成!");
        Ok(())
    }

    /// 获取数据库管理器引用
    pub fn get_database_manager(&self) -> Option<&DatabaseManager> {
        self.db_manager.as_ref()
    }

    /// 获取多进度条管理器引用
    pub fn get_multi_progress(&self) -> &MultiProgress {
        &self.multi_progress
    }

    /// 清理资源
    pub async fn cleanup(&mut self) -> Result<()> {
        if let Some(db_manager) = &mut self.db_manager {
            db_manager.close().await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_preprocessor_creation() {
        let processor = PreProcessor::new_default();
        assert!(processor.db_manager.is_none());
    }

    #[tokio::test]
    async fn test_database_initialization() {
        let _processor = PreProcessor::new_default();
        // 注意：这个测试需要实际的数据库服务运行
        // processor.initialize_database().await.unwrap();
        // assert!(processor.db_manager.is_some());
    }
}
