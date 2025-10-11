pub mod file_remanager;
pub mod pkg_config;
use crate::pkg_config::{PreprocessorConfig, get_config};
use file_remanager::{CProjectPreprocessor, ProcessingStats};

use db_services::{DatabaseManager, create_database_manager};
use lsp_services::lsp_services::ClangdAnalyzer;

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{debug, error, info, warn};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
        let config = match get_config() {
            Ok(config) => config,
            Err(err) => {
                error!("Failed to load config: {}", err);
                PreprocessorConfig::default()
            }
        };
        Self::new(config)
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
            create_database_manager(None, self.config.qdrant_url.as_deref(), None, Some(384))
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
        info!("file_remanager complete");

        // 步骤2：并行执行 LSP 分析和数据库存储
        let mapping_path = cache_dir.join("mapping.json");
        if mapping_path.exists() {
            self.parallel_analysis_and_storage(source_dir, cache_dir) //, &mapping_path)
                .await?;
        } else {
            warn!("映射文件不存在，跳过 LSP 分析");
        }
        info!("analysis complete");

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
        debug!(
            "remanager files: {} -> {}",
            source_dir.display(),
            cache_dir.display()
        );
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
        // mapping_path: &Path,
    ) -> Result<()> {
        let main_pb = self.multi_progress.add(ProgressBar::new_spinner());
        main_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        main_pb.enable_steady_tick(Duration::from_millis(100));
        main_pb.set_message("🔄 开始并行分析和存储...");

        // // 读取映射文件
        // let mapping_content =
        //     fs::read_to_string(mapping_path).context("Failed to read mapping file")?;
        // let mapping: Value =
        //     serde_json::from_str(&mapping_content).context("Failed to parse mapping JSON")?;

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
                debug!("thread lsp analyze started");
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
                debug!("LSP analysis results saved to {}", analysis_path.display());
                Ok(())
            })
        };

        // // 启动数据库存储线程
        // let db_handle = {
        //     let db_pb = db_pb.clone();
        //     let mapping = mapping.clone();

        //     thread::spawn(move || -> Result<()> {
        //         let rt = tokio::runtime::Runtime::new().unwrap();
        //         rt.block_on(async {
        //             db_pb.set_message("💾 正在存储到数据库...");

        //             // 这里可以根据映射文件处理数据库存储逻辑
        //             // 例如：存储文件映射信息、接口信息等
        //             if let Some(mappings) = mapping.get("mappings").and_then(|m| m.as_array()) {
        //                 db_pb.set_message(format!("💾 正在存储 {} 个文件映射...", mappings.len()));

        //                 // 示例：可以在这里添加具体的数据库存储逻辑
        //                 // for mapping in mappings {
        //                 //     // 处理每个映射项的数据库存储
        //                 // }
        //             }

        //             db_pb.finish_with_message("✅ 数据库存储完成!");
        //             Ok(())
        //         })
        //     })
        // };

        // 等待两个线程完成
        let lsp_result = lsp_handle
            .join()
            .map_err(|e| anyhow::anyhow!("LSP thread panicked: {:?}", e))?;
        // let db_result = db_handle
        //     .join()
        //     .map_err(|e| anyhow::anyhow!("DB thread panicked: {:?}", e))?;

        // 检查结果
        if let Err(e) = lsp_result {
            error!("LSP 分析失败: {}", e);
        }
        // if let Err(e) = db_result {
        //     error!("数据库存储失败: {}", e);
        // }

        // 基于 FastEmbed 生成向量并批量入库
        let embed_pb = self.multi_progress.add(ProgressBar::new_spinner());
        embed_pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        embed_pb.enable_steady_tick(Duration::from_millis(100));
        embed_pb.set_message("🧠 正在生成向量并批量入库...");

        let analysis_path = cache_dir.join("lsp_analysis.json");
        if analysis_path.exists() {
            let analysis_content =
                fs::read_to_string(&analysis_path).context("Failed to read LSP analysis file")?;
            let analysis_json: Value = serde_json::from_str(&analysis_content)
                .context("Failed to parse LSP analysis JSON")?;

            if let Some(funcs) = analysis_json.get("functions").and_then(|v| v.as_array()) {
                // 准备嵌入文档与批量入库数据
                let mut documents: Vec<String> = Vec::new();
                let mut interfaces_data: Vec<HashMap<String, serde_json::Value>> = Vec::new();

                for f in funcs {
                    let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let return_type = f
                        .get("return_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("void");
                    let file_path = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
                    let line = f.get("line").and_then(|v| v.as_u64()).unwrap_or(0);

                    // 参数
                    let mut params_vec: Vec<String> = Vec::new();
                    let mut inputs_meta: Vec<HashMap<String, serde_json::Value>> = Vec::new();
                    if let Some(params) = f.get("parameters").and_then(|v| v.as_array()) {
                        for p in params {
                            let pname = p.get("name").and_then(|v| v.as_str()).unwrap_or("param");
                            let ptype = p.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
                            params_vec.push(format!("{} {}", ptype, pname));

                            let mut pin = HashMap::new();
                            pin.insert("name".to_string(), json!(pname));
                            pin.insert("type".to_string(), json!(ptype));
                            inputs_meta.push(pin);
                        }
                    }
                    let params_str = params_vec.join(", ");
                    let signature = format!("{} {}({});", return_type, name, params_str);

                    documents.push(signature.clone());

                    let mut meta = HashMap::new();
                    meta.insert("line".to_string(), json!(line));
                    meta.insert("source".to_string(), json!("lsp_analysis"));

                    let mut data = HashMap::new();
                    data.insert("code".to_string(), json!(signature));
                    data.insert("language".to_string(), json!("c"));
                    data.insert("name".to_string(), json!(name));
                    // 工程名：优先目录名，否则用完整路径
                    let project_name = source_dir
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| source_dir.to_string_lossy().to_string());
                    data.insert("project_name".to_string(), json!(project_name));
                    data.insert("file_path".to_string(), json!(file_path));
                    data.insert("inputs".to_string(), json!(inputs_meta));
                    data.insert("outputs".to_string(), json!([{"return_type": return_type}]));
                    data.insert("metadata".to_string(), json!(meta));

                    interfaces_data.push(data);
                }

                if !documents.is_empty() {
                    // 初始化 FastEmbed，AllMiniLML6V2 -> 384 维度，符合默认 Qdrant 配置
                    let mut model =
                        TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
                            .map_err(|e| {
                                anyhow::anyhow!(format!("Failed to initialize FastEmbed: {}", e))
                            })?;

                    // 执行嵌入
                    let embeddings = model.embed(documents.clone(), None).map_err(|e| {
                        anyhow::anyhow!(format!("FastEmbed embedding failed: {}", e))
                    })?;

                    // 附加向量
                    for (i, emb) in embeddings.into_iter().enumerate() {
                        if let Some(item) = interfaces_data.get_mut(i) {
                            item.insert("vector".to_string(), json!(emb));
                        }
                    }

                    // 批量入库
                    let _saved = db_manager
                        .batch_store_interfaces(interfaces_data)
                        .await
                        .context("Failed to batch store interfaces with vectors")?;

                    embed_pb.finish_with_message("✅ 向量生成与批量入库完成!");
                } else {
                    embed_pb.finish_with_message("ℹ️ 无函数需要嵌入，跳过向量入库");
                }
            } else {
                embed_pb.finish_with_message("ℹ️ LSP 分析结果未包含函数，跳过向量入库");
            }
        } else {
            embed_pb.finish_with_message("⚠️ 未找到 LSP 分析结果，跳过向量入库");
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
