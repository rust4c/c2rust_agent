use anyhow::{Context, Result, anyhow};
use config::{Config, File};
use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_PATHS: &[&str] = &[
    "config/config.toml",
    "config/config.default.toml",
    "../config/config.toml",
    "../config/config.default.toml",
    "../../config/config.toml",
    "../../config/config.default.toml",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueLevel {
    Error,
    Warning,
}

impl fmt::Display for IssueLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueLevel::Error => write!(f, "ERROR"),
            IssueLevel::Warning => write!(f, "WARN"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigIssue {
    pub level: IssueLevel,
    pub field: String,
    pub message: String,
}

impl ConfigIssue {
    fn error(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Error,
            field: field.into(),
            message: message.into(),
        }
    }

    fn warning(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            level: IssueLevel::Warning,
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigCheckReport {
    pub path: PathBuf,
    pub issues: Vec<ConfigIssue>,
    pub validated: Option<ValidatedConfig>,
}

impl ConfigCheckReport {
    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.level == IssueLevel::Error)
    }
}

#[derive(Debug, Clone)]
pub struct ValidatedConfig {
    pub provider: ProviderKind,
    pub retry: RetryConfig,
    pub providers: ProviderConfigs,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retry_attempts: usize,
    pub concurrent_limit: usize,
}

#[derive(Debug, Clone)]
pub struct ProviderConfigs {
    pub ollama: Option<OllamaSettings>,
    pub openai: Option<ApiKeyProviderSettings>,
    pub xai: Option<ApiKeyProviderSettings>,
    pub deepseek: Option<ApiKeyProviderSettings>,
}

impl Default for ProviderConfigs {
    fn default() -> Self {
        Self {
            ollama: None,
            openai: None,
            xai: None,
            deepseek: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    DeepSeek,
    Ollama,
    OpenAI,
    XAI,
}

impl ProviderKind {
    fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "deepseek" => Some(ProviderKind::DeepSeek),
            "ollama" => Some(ProviderKind::Ollama),
            "openai" => Some(ProviderKind::OpenAI),
            "xai" => Some(ProviderKind::XAI),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub qdrant: QdrantSettings,
    pub sqlite: SqliteSettings,
}

#[derive(Debug, Clone)]
pub struct QdrantSettings {
    pub host: String,
    pub port: u16,
    pub collection_name: String,
    pub vector_size: usize,
}

#[derive(Debug, Clone)]
pub struct SqliteSettings {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct OllamaSettings {
    pub model: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApiKeyProviderSettings {
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    provider: Option<String>,
    max_retry_attempts: Option<usize>,
    concurrent_limit: Option<usize>,
    #[serde(default)]
    llm: LLMProvidersRaw,
    #[serde(default)]
    qdrant: Option<QdrantRaw>,
    #[serde(default)]
    sqlite: Option<SqliteRaw>,
}

#[derive(Debug, Deserialize, Default)]
struct LLMProvidersRaw {
    #[serde(default)]
    ollama: Option<OllamaRaw>,
    #[serde(default)]
    openai: Option<ApiKeyRaw>,
    #[serde(default)]
    xai: Option<ApiKeyRaw>,
    #[serde(default)]
    deepseek: Option<ApiKeyRaw>,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaRaw {
    model: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ApiKeyRaw {
    model: Option<String>,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QdrantRaw {
    host: Option<String>,
    port: Option<u16>,
    collection_name: Option<String>,
    vector_size: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SqliteRaw {
    path: Option<String>,
}

pub fn check_default_config() -> Result<ConfigCheckReport> {
    check_config_with_paths(DEFAULT_CONFIG_PATHS)
}

pub fn check_config_with_paths<P>(paths: impl IntoIterator<Item = P>) -> Result<ConfigCheckReport>
where
    P: AsRef<Path>,
{
    let mut attempted = Vec::new();
    let mut found_path: Option<PathBuf> = None;
    for path in paths.into_iter() {
        let candidate = path.as_ref().to_path_buf();
        attempted.push(candidate.clone());
        if candidate.exists() {
            found_path = Some(candidate);
            break;
        }
    }

    let config_path = found_path.ok_or_else(|| {
        let searched = attempted
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        anyhow!("config file not found. searched paths: [{}]", searched)
    })?;

    let mut builder = Config::builder();
    builder = builder.add_source(File::from(config_path.clone()));

    let raw_cfg: RawConfig = builder
        .build()
        .with_context(|| format!("failed to load config from {}", config_path.display()))?
        .try_deserialize()
        .with_context(|| {
            format!(
                "failed to deserialize config from {}",
                config_path.display()
            )
        })?;

    let config_abs = if config_path.is_absolute() {
        config_path.clone()
    } else {
        std::env::current_dir()
            .with_context(|| "failed to obtain current working directory".to_string())?
            .join(&config_path)
    };
    let config_dir = config_abs.parent().unwrap_or_else(|| Path::new("."));
    let project_root = config_dir.parent().unwrap_or(config_dir);

    Ok(analyze_config(
        raw_cfg,
        config_path,
        project_root.to_path_buf(),
    ))
}

fn analyze_config(raw: RawConfig, path: PathBuf, project_root: PathBuf) -> ConfigCheckReport {
    fn is_blank(value: &str) -> bool {
        value.trim().is_empty()
    }

    let RawConfig {
        provider,
        max_retry_attempts,
        concurrent_limit,
        llm,
        qdrant,
        sqlite,
    } = raw;

    let mut issues = Vec::new();

    let provider_kind = match provider.and_then(|s| ProviderKind::from_str(&s)) {
        Some(kind) => Some(kind),
        None => {
            issues.push(ConfigIssue::error(
                "provider",
                "`provider` must be one of: deepseek, ollama, openai, xai",
            ));
            None
        }
    };

    let max_retry_attempts = match max_retry_attempts {
        Some(value) if value > 0 => Some(value),
        Some(_) => {
            issues.push(ConfigIssue::error(
                "max_retry_attempts",
                "`max_retry_attempts` must be greater than zero",
            ));
            None
        }
        None => {
            issues.push(ConfigIssue::error(
                "max_retry_attempts",
                "missing `max_retry_attempts`",
            ));
            None
        }
    };

    let concurrent_limit = match concurrent_limit {
        Some(value) if value > 0 => Some(value),
        Some(_) => {
            issues.push(ConfigIssue::error(
                "concurrent_limit",
                "`concurrent_limit` must be greater than zero",
            ));
            None
        }
        None => {
            issues.push(ConfigIssue::error(
                "concurrent_limit",
                "missing `concurrent_limit`",
            ));
            None
        }
    };

    let mut providers = ProviderConfigs::default();

    if let Some(ollama_raw) = llm.ollama {
        let model = match ollama_raw.model.filter(|value| !is_blank(value)) {
            Some(model) => Some(model),
            None => {
                issues.push(ConfigIssue::error(
                    "llm.ollama.model",
                    "missing `llm.ollama.model`",
                ));
                None
            }
        };

        let base_url = match ollama_raw.base_url.filter(|value| !is_blank(value)) {
            Some(url) => Some(url),
            None => {
                issues.push(ConfigIssue::error(
                    "llm.ollama.base_url",
                    "missing `llm.ollama.base_url`",
                ));
                None
            }
        };

        let api_key = ollama_raw
            .api_key
            .and_then(|key| if is_blank(&key) { None } else { Some(key) });

        if let (Some(model), Some(base_url)) = (model, base_url) {
            providers.ollama = Some(OllamaSettings {
                model,
                base_url,
                api_key,
            });
        }
    }

    providers.openai = normalize_api_provider(llm.openai, "llm.openai", true, &mut issues);
    providers.xai = normalize_api_provider(llm.xai, "llm.xai", true, &mut issues);
    providers.deepseek = normalize_api_provider(llm.deepseek, "llm.deepseek", true, &mut issues);

    let qdrant_settings = match qdrant {
        Some(q) => {
            let host = match q.host.filter(|value| !is_blank(value)) {
                Some(host) => host,
                None => {
                    issues.push(ConfigIssue::error("qdrant.host", "missing `qdrant.host`"));
                    String::new()
                }
            };

            let port = match q.port {
                Some(port) if port > 0 => port,
                Some(_) => {
                    issues.push(ConfigIssue::error(
                        "qdrant.port",
                        "`qdrant.port` must be within 1..=65535",
                    ));
                    0
                }
                None => {
                    issues.push(ConfigIssue::error("qdrant.port", "missing `qdrant.port`"));
                    0
                }
            };

            let collection_name = match q.collection_name.filter(|value| !is_blank(value)) {
                Some(name) => name,
                None => {
                    issues.push(ConfigIssue::error(
                        "qdrant.collection_name",
                        "missing `qdrant.collection_name`",
                    ));
                    String::new()
                }
            };

            let vector_size = match q.vector_size {
                Some(size) if size > 0 => size,
                Some(_) => {
                    issues.push(ConfigIssue::error(
                        "qdrant.vector_size",
                        "`qdrant.vector_size` must be greater than zero",
                    ));
                    0
                }
                None => {
                    issues.push(ConfigIssue::error(
                        "qdrant.vector_size",
                        "missing `qdrant.vector_size`",
                    ));
                    0
                }
            };

            if !host.is_empty() && port > 0 && !collection_name.is_empty() && vector_size > 0 {
                Some(QdrantSettings {
                    host,
                    port,
                    collection_name,
                    vector_size,
                })
            } else {
                None
            }
        }
        None => {
            issues.push(ConfigIssue::error(
                "qdrant",
                "missing `[qdrant]` configuration section",
            ));
            None
        }
    };

    let sqlite_settings = match sqlite {
        Some(s) => match s.path.filter(|value| !is_blank(value)) {
            Some(path_str) => {
                let raw_path = PathBuf::from(path_str);
                let absolute_path = if raw_path.is_absolute() {
                    raw_path
                } else {
                    project_root.join(raw_path)
                };

                if !absolute_path.exists() {
                    issues.push(ConfigIssue::warning(
                        "sqlite.path",
                        format!(
                            "SQLite database file does not exist yet: {}",
                            absolute_path.display()
                        ),
                    ));
                }

                Some(SqliteSettings {
                    path: absolute_path,
                })
            }
            None => {
                issues.push(ConfigIssue::error("sqlite.path", "missing `sqlite.path`"));
                None
            }
        },
        None => {
            issues.push(ConfigIssue::error(
                "sqlite",
                "missing `[sqlite]` configuration section",
            ));
            None
        }
    };

    let provider_kind = provider_kind;

    if let Some(kind) = provider_kind.as_ref() {
        let provider_has_config = match kind {
            ProviderKind::DeepSeek => providers.deepseek.is_some(),
            ProviderKind::Ollama => providers.ollama.is_some(),
            ProviderKind::OpenAI => providers.openai.is_some(),
            ProviderKind::XAI => providers.xai.is_some(),
        };

        if !provider_has_config {
            issues.push(ConfigIssue::error(
				"provider",
				format!(
					"`{}` provider is selected but its configuration section is missing or incomplete",
					format_provider(kind)
				),
			));
        }
    }

    let validated = if !issues.iter().any(|issue| issue.level == IssueLevel::Error) {
        Some(ValidatedConfig {
            provider: provider_kind.unwrap(),
            retry: RetryConfig {
                max_retry_attempts: max_retry_attempts.unwrap(),
                concurrent_limit: concurrent_limit.unwrap(),
            },
            providers,
            database: DatabaseConfig {
                qdrant: qdrant_settings.unwrap(),
                sqlite: sqlite_settings.unwrap(),
            },
        })
    } else {
        None
    };

    ConfigCheckReport {
        path,
        issues,
        validated,
    }
}

fn normalize_api_provider(
    raw: Option<ApiKeyRaw>,
    section: &str,
    require_key: bool,
    issues: &mut Vec<ConfigIssue>,
) -> Option<ApiKeyProviderSettings> {
    fn is_blank(value: &str) -> bool {
        value.trim().is_empty()
    }

    match raw {
        Some(raw) => {
            let model = match raw.model.filter(|value| !is_blank(value)) {
                Some(model) => model,
                None => {
                    issues.push(ConfigIssue::error(
                        format!("{}{}", section, ".model"),
                        format!("missing `{}.{}`", section, "model"),
                    ));
                    return None;
                }
            };

            let api_key = match raw.api_key.filter(|value| !is_blank(value)) {
                Some(key) => key,
                None if require_key => {
                    issues.push(ConfigIssue::error(
                        format!("{}{}", section, ".api_key"),
                        format!(
                            "missing `{}` API key; set `{}`",
                            section,
                            format!("{}.api_key", section)
                        ),
                    ));
                    return None;
                }
                None => String::new(),
            };

            if !require_key && api_key.is_empty() {
                issues.push(ConfigIssue::warning(
                    format!("{}{}", section, ".api_key"),
                    format!(
                        "`{}` API key is empty; local deployments may be fine",
                        section
                    ),
                ));
            }

            Some(ApiKeyProviderSettings { model, api_key })
        }
        None => None,
    }
}

fn format_provider(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::DeepSeek => "deepseek",
        ProviderKind::Ollama => "ollama",
        ProviderKind::OpenAI => "openai",
        ProviderKind::XAI => "xai",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::path::{Path, PathBuf};

    fn write_config(dir: &Path, contents: &str) -> PathBuf {
        let config_dir = dir.join("config");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("config.toml");
        fs::write(&config_path, contents).unwrap();
        config_path
    }

    #[test]
    fn valid_config_passes() {
        let temp = tempfile::tempdir().unwrap();
        let sqlite_path = temp.path().join("data.db");
        File::create(&sqlite_path).unwrap();
        let sqlite_value = sqlite_path.to_string_lossy().replace('\\', "\\\\");

        let contents = format!(
            concat!(
                "provider = \"deepseek\"\n",
                "max_retry_attempts = 3\n",
                "concurrent_limit = 5\n\n",
                "[llm.deepseek]\n",
                "model = \"deepseek-chat\"\n",
                "api_key = \"sk-test\"\n\n",
                "[qdrant]\n",
                "host = \"localhost\"\n",
                "port = 6334\n",
                "collection_name = \"default\"\n",
                "vector_size = 1536\n\n",
                "[sqlite]\n",
                "path = \"{}\"\n"
            ),
            sqlite_value
        );

        let config_path = write_config(temp.path(), &contents);

        let report = check_config_with_paths([config_path.as_path()]).unwrap();
        assert!(
            !report.has_errors(),
            "expected no errors: {:?}",
            report.issues
        );
        let validated = report.validated.expect("validated config");
        assert_eq!(validated.provider, ProviderKind::DeepSeek);
        assert_eq!(validated.retry.max_retry_attempts, 3);
        assert_eq!(validated.database.qdrant.port, 6334);
        assert_eq!(validated.database.sqlite.path, sqlite_path);
    }

    #[test]
    fn missing_provider_is_error() {
        let temp = tempfile::tempdir().unwrap();
        let sqlite_path = temp.path().join("data.db");
        File::create(&sqlite_path).unwrap();
        let sqlite_value = sqlite_path.to_string_lossy().replace('\\', "\\\\");

        let contents = format!(
            concat!(
                "max_retry_attempts = 3\n",
                "concurrent_limit = 5\n\n",
                "[llm.deepseek]\n",
                "model = \"deepseek-chat\"\n",
                "api_key = \"sk-test\"\n\n",
                "[qdrant]\n",
                "host = \"localhost\"\n",
                "port = 6334\n",
                "collection_name = \"default\"\n",
                "vector_size = 1536\n\n",
                "[sqlite]\n",
                "path = \"{}\"\n"
            ),
            sqlite_value
        );

        let config_path = write_config(temp.path(), &contents);
        let report = check_config_with_paths([config_path.as_path()]).unwrap();
        assert!(report.has_errors());
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.level == IssueLevel::Error && issue.field == "provider")
        );
        assert!(report.validated.is_none());
    }
}
