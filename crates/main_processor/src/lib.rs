pub mod pkg_config;
pub mod processor; // expose config module

use std::path::PathBuf;

pub use pkg_config::MainProcessorConfig;
pub use processor::{
    discover_src_cache_projects,
    process_batch_paths,
    process_single_path,
    process_with_dependency_graph,
};

/// Object-oriented entry point wrapper for maintaining configuration and context during upper-level calls
pub struct MainProcessor {
    cfg: MainProcessorConfig,
}

impl MainProcessor {
    /// Create processor with configuration (if no customization needed, can read from pkg_config::get_config)
    pub fn new(cfg: MainProcessorConfig) -> Self {
        Self { cfg }
    }

    /// Process single path (directory or file). Internally reuses async function directly.
    pub async fn process_single<P: AsRef<std::path::Path>>(&self, path: P) -> anyhow::Result<()> {
        processor::process_single_path(path.as_ref()).await
    }

    /// Concurrent batch processing of multiple paths, using progress bars and retry mechanisms
    pub async fn process_batch(&self, paths: Vec<PathBuf>) -> anyhow::Result<()> {
        processor::process_batch_paths(self.cfg.clone(), paths).await
    }

    /// Use relation_graph.json for dependency-aware batch processing
    pub async fn process_with_graph<P: AsRef<std::path::Path>>(
        &self,
        relation_graph_path: P,
        cache_root_hint: Option<P>,
    ) -> anyhow::Result<()> {
        processor::process_with_dependency_graph(
            self.cfg.clone(),
            relation_graph_path.as_ref(),
            cache_root_hint.as_ref().map(|p| p.as_ref()),
        )
        .await
    }

    /// Automatically discover processable subdirectories under individual_files based on src_cache directory structure
    pub async fn discover_src_cache_projects<P: AsRef<std::path::Path>>(
        &self,
        src_cache_root: P,
    ) -> anyhow::Result<Vec<PathBuf>> {
        processor::discover_src_cache_projects(src_cache_root.as_ref()).await
    }
}
