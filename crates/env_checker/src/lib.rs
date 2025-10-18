pub mod ai_checker;
pub mod cfg_checker;
pub mod env_checker;

use db_services::DatabaseManager;

// env_checker exports enum types and functions
pub use env_checker::{
    DatabaseConnectionStatus, check_database_existence, dbdata_init, get_detailed_database_status,
};

// ai_checker exports enum types and functions
pub use ai_checker::{
    AIConnectionStatus, ai_service_init, check_all_ai_services, get_detailed_ai_status,
};

pub use cfg_checker::{
    ConfigCheckReport, ConfigIssue, IssueLevel, ProviderKind, ValidatedConfig,
    check_config_with_paths, check_default_config,
};

#[derive(Debug, Clone)]
pub struct ConfigCheckResult {
    pub report: Option<ConfigCheckReport>,
    pub error: Option<String>,
}

impl ConfigCheckResult {
    pub fn has_errors(&self) -> bool {
        if self.error.is_some() {
            return true;
        }

        match &self.report {
            Some(report) => report.has_errors(),
            None => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceCheckResult<T> {
    pub status: Option<T>,
    pub error: Option<String>,
}

impl<T> ServiceCheckResult<T> {
    pub fn has_errors(&self) -> bool {
        self.error.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct EnvironmentCheckSummary {
    pub config: ConfigCheckResult,
    pub database: ServiceCheckResult<DatabaseConnectionStatus>,
    pub ai: ServiceCheckResult<AIConnectionStatus>,
}

impl EnvironmentCheckSummary {
    pub fn has_errors(&self) -> bool {
        self.config.has_errors() || self.database.has_errors() || self.ai.has_errors()
    }
}

pub async fn check_all() -> EnvironmentCheckSummary {
    let config = match check_default_config() {
        Ok(report) => ConfigCheckResult {
            report: Some(report),
            error: None,
        },
        Err(err) => ConfigCheckResult {
            report: None,
            error: Some(err.to_string()),
        },
    };

    let database = match DatabaseManager::new_default().await {
        Ok(manager) => {
            let status = dbdata_init(manager.clone()).await;
            manager.close().await;

            match status {
                Ok(status) => ServiceCheckResult {
                    status: Some(status),
                    error: None,
                },
                Err(err) => ServiceCheckResult {
                    status: None,
                    error: Some(err.to_string()),
                },
            }
        }
        Err(err) => ServiceCheckResult {
            status: None,
            error: Some(err.to_string()),
        },
    };

    let ai = match ai_service_init().await {
        Ok(status) => ServiceCheckResult {
            status: Some(status),
            error: None,
        },
        Err(err) => ServiceCheckResult {
            status: None,
            error: Some(err.to_string()),
        },
    };

    EnvironmentCheckSummary {
        config,
        database,
        ai,
    }
}
