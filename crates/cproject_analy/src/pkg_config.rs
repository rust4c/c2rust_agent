use config::{Config, File};
use serde::{Deserialize, Serialize};

/// 预处理配置（集中定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessConfig {
    /// 并行工作者数量 (0=自动检测)
    pub worker_count: usize,
    /// 文件配对规则 (源文件模式, 头文件模式)
    pub pairing_rules: Vec<(String, String)>,
    /// 排除文件模式
    pub exclude_patterns: Vec<String>,
    /// 头文件扩展名
    pub header_extensions: Vec<String>,
    /// 源文件扩展名
    pub source_extensions: Vec<String>,
    /// 大文件阈值 (字节)
    pub large_file_threshold: u64,
    /// 块大小 (字节)
    pub chunk_size: usize,
}

impl Default for PreprocessConfig {
    fn default() -> Self {
        Self {
            worker_count: 0,
            pairing_rules: vec![
                ("(.+)\\.c".to_string(), "\\1.h".to_string()),
                ("(.+)\\.cc".to_string(), "\\1.h".to_string()),
                ("(.+)\\.cpp".to_string(), "\\1.h".to_string()),
                ("(.+)\\.cpp".to_string(), "\\1.hpp".to_string()),
                ("(.+)\\.cxx".to_string(), "\\1.h".to_string()),
                ("(.+)\\.c\\+\\+".to_string(), "\\1.h".to_string()),
                ("src/(.+)\\.c".to_string(), "include/\\1.h".to_string()),
                ("src/(.+)\\.cpp".to_string(), "include/\\1.h".to_string()),
                ("src/(.+)\\.cpp".to_string(), "include/\\1.hpp".to_string()),
            ],
            exclude_patterns: vec![
                "*.bak".to_string(),
                "*.tmp".to_string(),
                ".git/*".to_string(),
                ".svn/*".to_string(),
                "*.o".to_string(),
                "*.obj".to_string(),
                "*.exe".to_string(),
                "*.dll".to_string(),
                "*.so".to_string(),
            ],
            header_extensions: vec![
                ".h".to_string(),
                ".hpp".to_string(),
                ".hh".to_string(),
                ".hxx".to_string(),
            ],
            source_extensions: vec![
                ".c".to_string(),
                ".cc".to_string(),
                ".cpp".to_string(),
                ".cxx".to_string(),
                ".c++".to_string(),
            ],
            large_file_threshold: 1024 * 1024,
            chunk_size: 1024 * 1024,
        }
    }
}

/// 预处理器顶层配置（集中定义）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorConfig {
    /// 数据库配置（保留占位）
    pub database_url: Option<String>,
    /// Qdrant 配置（保留占位）
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
            worker_count: 0,
            preprocess_config: Some(PreprocessConfig::default()),
        }
    }
}

/// NOTE: 本模块集中定义 PreprocessConfig/PreprocessorConfig，
/// 并将配置文件中的 [cproject.preprocess] 映射为本模块的 PreprocessConfig。
///
/// 配置文件示例（请将此片段同步到 config/config.default.toml）:
///
/// [cproject.preprocess]
/// # 0 表示自动选择（保留为 0 给运行时处理）
/// worker_count = 0
///
/// # 文件配对规则（多段）
/// [[cproject.preprocess.pairing_rules]]
/// source = "(.+)\\.c"
/// header = "\\1.h"
///
/// [[cproject.preprocess.pairing_rules]]
/// source = "(.+)\\.cpp"
/// header = "\\1.h"
///
/// [[cproject.preprocess.pairing_rules]]
/// source = "src/(.+)\\.c"
/// header = "include/\\1.h"
///
/// # 排除模式
/// cproject.preprocess.exclude_patterns = [ "*.bak", "*.tmp", ".git/*", ".svn/*", "*.o", "*.obj", "*.exe", "*.dll", "*.so" ]
///
/// # 扩展名
/// cproject.preprocess.header_extensions = [ ".h", ".hpp", ".hh", ".hxx" ]
/// cproject.preprocess.source_extensions = [ ".c", ".cc", ".cpp", ".cxx", ".c++" ]
///
/// # 大文件阈值/块大小
/// cproject.preprocess.large_file_threshold = 52428800     # 50MB
/// cproject.preprocess.chunk_size = 8388608               # 8MB
///
/// 说明：
/// - 如果缺失 [cproject.preprocess]，将返回 PreprocessConfig::default()
/// - 只对存在的字段执行覆盖，其他沿用默认值

pub fn get_config() -> Result<PreprocessorConfig, config::ConfigError> {
    // Try multiple possible paths for the config file
    let possible_paths = [
        "config/config.toml",       // From project root
        "../config/config.toml",    // From crates subdirectory
        "../../config/config.toml", // From deeper nested directories
    ];

    let mut config_builder = Config::builder();
    let mut found_config = false;

    for path in &possible_paths {
        if std::path::Path::new(path).exists() {
            config_builder = config_builder.add_source(File::with_name(path));
            found_config = true;
            break;
        }
    }

    if !found_config {
        return Err(config::ConfigError::NotFound(
            "config.toml not found in any expected location".to_string(),
        ));
    }

    let cfg = config_builder.build()?;
    let config: PreprocessorConfig = cfg.try_deserialize()?;

    Ok(config)
}
