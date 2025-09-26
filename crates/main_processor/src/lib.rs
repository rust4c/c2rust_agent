pub mod pkg_config;
pub mod processor; // expose config module

use std::path::PathBuf;

pub use pkg_config::MainProcessorConfig;
pub use processor::{discover_src_cache_projects, process_batch_paths, process_single_path};

/// 面向对象风格的入口封装，便于在上层调用时保有配置与上下文
pub struct MainProcessor {
    cfg: MainProcessorConfig,
}

impl MainProcessor {
    /// 创建处理器，传入配置（若不需要自定义，可从 pkg_config::get_config 读取）
    pub fn new(cfg: MainProcessorConfig) -> Self {
        Self { cfg }
    }

    /// 处理单个路径（目录或文件）。内部直接复用异步函数。
    pub async fn process_single<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        processor::process_single_path(path.as_ref()).await
    }

    /// 并发批处理一批路径，沿用进度条与重试机制
    pub async fn process_batch(&self, paths: Vec<PathBuf>) -> anyhow::Result<()> {
        processor::process_batch_paths(self.cfg.clone(), paths).await
    }

    /// 按 src_cache 目录结构自动发现 individual_files 下的可处理子目录
    pub async fn discover_src_cache_projects<P: AsRef<std::path::Path>>(
        &self,
        src_cache_root: P,
    ) -> anyhow::Result<Vec<PathBuf>> {
        processor::discover_src_cache_projects(src_cache_root.as_ref()).await
    }
}
