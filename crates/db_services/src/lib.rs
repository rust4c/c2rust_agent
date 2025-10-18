use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

mod pkg_config;
use pkg_config::{get_config, DBConfig};
mod qdrant_services;
use qdrant_services::QdrantServer;
pub mod sqlite_services;
use sqlite_services::{CodeEntry, ConversionResult, SqliteService};

/// Interface information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceInfo {
    pub id: Option<i32>,
    pub name: String,
    pub inputs: Vec<HashMap<String, JsonValue>>,
    pub outputs: Vec<HashMap<String, JsonValue>>,
    pub file_path: String,
    pub qdrant_id: Option<String>,
    pub language: String,
    pub project_name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Project information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: Option<i32>,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// Configuration information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    pub key: String,
    pub value: JsonValue,
    pub description: Option<String>,
}

/// Search result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub interface: InterfaceInfo,
    pub vector_info: HashMap<String, JsonValue>,
    pub similarity_score: f32,
}

/// System status structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub sqlite: HashMap<String, JsonValue>,
    pub qdrant: HashMap<String, JsonValue>,
    pub overall_status: String,
}

/// Database manager - unified management of SQLite and Qdrant databases
#[derive(Clone)]
pub struct DatabaseManager {
    sqlite: Arc<Mutex<SqliteService>>,
    qdrant: Arc<Mutex<QdrantServer>>,
}

impl DatabaseManager {
    /// Create new database manager instance
    pub async fn new(database_config: DBConfig) -> Result<Self> {
        let sqlite_service = Arc::new(Mutex::new(
            SqliteService::new(database_config.sqlite.clone())
                .map_err(|e| anyhow!("Failed to create SQLite service: {}", e))?,
        ));

        let qdrant_service = Arc::new(Mutex::new(
            QdrantServer::new(database_config.qdrant.clone())
                .await
                .map_err(|e| anyhow!("Failed to create Qdrant service: {}", e))?,
        ));

        let manager = DatabaseManager {
            sqlite: sqlite_service,
            qdrant: qdrant_service,
        };

        manager.init_config().await?;
        info!("Database manager initialization completed");

        Ok(manager)
    }

    /// Create database manager with default configuration
    pub async fn new_default() -> Result<Self> {
        let config = match get_config() {
            Ok(config) => config,
            Err(e) => return Err(anyhow!("Failed to get config: {}", e)),
        };

        Self::new(config).await
    }

    /// Initialize default configuration
    async fn init_config(&self) -> Result<()> {
        let default_configs = vec![
            ("ai_source", json!("deepseek"), "AI service source"),
            (
                "auto_confirm_threshold",
                json!(0.85),
                "Auto confirmation threshold",
            ),
            ("strict_mode", json!(true), "Strict mode"),
            ("pointer_strategy", json!("box"), "Pointer strategy"),
            (
                "max_translation_attempts",
                json!(3),
                "Maximum translation attempts",
            ),
            (
                "vector_similarity_threshold",
                json!(0.7),
                "Vector similarity threshold",
            ),
        ];

        for (key, value, desc) in default_configs {
            if self.get_config(key).await.is_none() {
                self.set_config(key, value, Some(desc)).await?;
            }
        }

        Ok(())
    }

    /// Store interface and its vector representation
    pub async fn store_interface_with_vector(
        &self,
        name: &str,
        inputs: Vec<HashMap<String, JsonValue>>,
        outputs: Vec<HashMap<String, JsonValue>>,
        file_path: &str,
        code: &str,
        vector: Vec<f32>,
        language: Option<&str>,
        project_name: Option<&str>,
        metadata: Option<HashMap<String, JsonValue>>,
    ) -> Result<(String, String)> {
        let language = language.unwrap_or("c");
        let project = project_name.unwrap_or("default");

        // First store vector to Qdrant
        let qdrant_id = {
            let qdrant = self.qdrant.lock().await;
            qdrant
                .insert_code_vector(code, vector, language, name, project, file_path, metadata)
                .await?
        };

        // Then store code entry to SQLite
        let code_entry = CodeEntry {
            id: String::new(), // Will be auto-generated by SQLite
            code: code.to_string(),
            language: language.to_string(),
            function_name: name.to_string(),
            project: project.to_string(),
            file_path: file_path.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: Some(
                json!({
                    "qdrant_id": qdrant_id,
                    "inputs": inputs,
                    "outputs": outputs,
                    "project_name": project
                })
                .to_string(),
            ),
        };

        let interface_id = {
            let sqlite = self.sqlite.lock().await;
            sqlite.insert_code_entry(code_entry)?
        };

        info!(
            "Interface storage completed: {}, SQLite ID: {}, Qdrant ID: {}",
            name, interface_id, qdrant_id
        );
        Ok((interface_id, qdrant_id))
    }

    /// Search similar interfaces
    pub async fn search_similar_interfaces(
        &self,
        query_vector: Vec<f32>,
        limit: Option<usize>,
        language: Option<&str>,
        project: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let limit = limit.unwrap_or(10);
        let threshold = self
            .get_config_value("vector_similarity_threshold", 0.7)
            .await;

        // Search similar vectors from Qdrant
        let similar_vectors = {
            let qdrant = self.qdrant.lock().await;
            qdrant
                .search_similar_code(query_vector, limit, language, project, threshold)
                .await?
        };

        let mut results = Vec::new();
        let sqlite = self.sqlite.lock().await;

        // 获取对应的SQLite元数据
        for vector_result in similar_vectors {
            if let Some(qdrant_id_value) = vector_result.get("id") {
                let qdrant_id = qdrant_id_value.as_str().unwrap_or("").to_string();

                // 在SQLite中搜索对应的代码条目
                let code_entries = sqlite.search_code_entries(language, project, None, None)?;

                for entry in code_entries {
                    if let Some(metadata_str) = &entry.metadata {
                        if let Ok(metadata) = serde_json::from_str::<JsonValue>(metadata_str) {
                            if let Some(stored_qdrant_id) = metadata.get("qdrant_id") {
                                if stored_qdrant_id.as_str() == Some(&qdrant_id) {
                                    let interface = InterfaceInfo {
                                        id: None,
                                        name: entry.function_name.clone(),
                                        inputs: metadata
                                            .get("inputs")
                                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                                            .unwrap_or_default(),
                                        outputs: metadata
                                            .get("outputs")
                                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                                            .unwrap_or_default(),
                                        file_path: entry.file_path.clone(),
                                        qdrant_id: Some(qdrant_id.clone()),
                                        language: entry.language,
                                        project_name: Some(entry.project),
                                        created_at: Some(entry.created_at),
                                        updated_at: Some(entry.updated_at),
                                    };

                                    let search_result = SearchResult {
                                        interface,
                                        vector_info: vector_result.clone(),
                                        similarity_score: vector_result
                                            .get("score")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0)
                                            as f32,
                                    };
                                    results.push(search_result);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        debug!("搜索到 {} 个相似接口", results.len());
        Ok(results)
    }

    /// 获取接口及其代码
    pub async fn get_interface_with_code(
        &self,
        interface_id: &str,
    ) -> Result<Option<HashMap<String, JsonValue>>> {
        let sqlite = self.sqlite.lock().await;

        if let Some(code_entry) = sqlite.get_code_entry(interface_id)? {
            let mut result = HashMap::new();
            result.insert("id".to_string(), json!(code_entry.id));
            result.insert("code".to_string(), json!(code_entry.code));
            result.insert("language".to_string(), json!(code_entry.language));
            result.insert("function_name".to_string(), json!(code_entry.function_name));
            result.insert("project".to_string(), json!(code_entry.project));
            result.insert("file_path".to_string(), json!(code_entry.file_path));
            result.insert(
                "created_at".to_string(),
                json!(code_entry.created_at.to_rfc3339()),
            );
            result.insert(
                "updated_at".to_string(),
                json!(code_entry.updated_at.to_rfc3339()),
            );

            if let Some(metadata_str) = &code_entry.metadata {
                if let Ok(metadata) = serde_json::from_str::<JsonValue>(metadata_str) {
                    result.insert("metadata".to_string(), metadata);
                }
            }

            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    /// 添加转译记录
    pub async fn add_translation_record(
        &self,
        interface_id: &str,
        original_code: &str,
        translated_code: &str,
        translation_method: &str,
        success: bool,
        error_message: Option<&str>,
        translated_vector: Option<Vec<f32>>,
    ) -> Result<String> {
        let sqlite = self.sqlite.lock().await;

        // 获取原始接口信息
        let original_entry = sqlite
            .get_code_entry(interface_id)?
            .ok_or_else(|| anyhow!("Interface not found: {}", interface_id))?;

        let conversion_result = ConversionResult {
            id: String::new(),
            source_id: interface_id.to_string(),
            original_code: original_code.to_string(),
            converted_code: translated_code.to_string(),
            conversion_type: translation_method.to_string(),
            status: if success {
                "success".to_string()
            } else {
                "failed".to_string()
            },
            error_message: error_message.map(|s| s.to_string()),
            created_at: Utc::now(),
            metadata: Some(
                json!({
                    "translation_method": translation_method,
                    "success": success
                })
                .to_string(),
            ),
        };

        let history_id = sqlite.insert_conversion_result(conversion_result)?;

        // If translation successful and has vector, store Rust code vector
        if success {
            if let Some(vector) = translated_vector {
                let qdrant = self.qdrant.lock().await;
                let _rust_qdrant_id = qdrant
                    .insert_code_vector(
                        translated_code,
                        vector,
                        "rust",
                        &original_entry.function_name,
                        &original_entry.project,
                        &original_entry.file_path,
                        Some(HashMap::from([
                            ("original_interface_id".to_string(), json!(interface_id)),
                            ("translation_method".to_string(), json!(translation_method)),
                            ("translation_history_id".to_string(), json!(history_id)),
                        ])),
                    )
                    .await?;
            }
        }

        info!("Translation record addition completed: {}", history_id);
        Ok(history_id)
    }

    /// Search interfaces by name
    pub async fn search_interfaces_by_name(
        &self,
        name: &str,
        project: Option<&str>,
    ) -> Result<Vec<InterfaceInfo>> {
        let sqlite = self.sqlite.lock().await;
        let code_entries = sqlite.search_code_entries(None, project, Some(name), None)?;

        let mut interfaces = Vec::new();
        for entry in code_entries {
            let interface = InterfaceInfo {
                id: None,
                name: entry.function_name.clone(),
                inputs: Vec::new(),  // Need to parse from metadata
                outputs: Vec::new(), // Need to parse from metadata
                file_path: entry.file_path.clone(),
                qdrant_id: None, // Need to parse from metadata
                language: entry.language,
                project_name: Some(entry.project),
                created_at: Some(entry.created_at),
                updated_at: Some(entry.updated_at),
            };
            interfaces.push(interface);
        }

        Ok(interfaces)
    }

    /// Search code by text content
    pub async fn search_code_by_text(
        &self,
        query_text: &str,
        language: Option<&str>,
        project: Option<&str>,
    ) -> Result<Vec<HashMap<String, JsonValue>>> {
        let sqlite = self.sqlite.lock().await;
        let code_entries = sqlite.search_code_entries(language, project, None, None)?;

        let mut results = Vec::new();
        for entry in code_entries {
            let mut result = HashMap::new();
            result.insert("id".to_string(), json!(entry.id));
            result.insert("code".to_string(), json!(entry.code));
            result.insert("language".to_string(), json!(entry.language));
            // Filter by query text manually since SQLite service doesn't support text search
            if entry.code.contains(query_text) || entry.function_name.contains(query_text) {
                result.insert("function_name".to_string(), json!(entry.function_name));
                result.insert("project".to_string(), json!(entry.project));
                result.insert("file_path".to_string(), json!(entry.file_path));
            }
            results.push(result);
        }

        Ok(results)
    }

    /// Create project
    pub async fn create_project(
        &self,
        name: &str,
        path: &str,
        description: Option<&str>,
    ) -> Result<String> {
        let project_data = CodeEntry {
            id: String::new(),
            code: format!("Project: {}", name),
            language: "project".to_string(),
            function_name: name.to_string(),
            project: name.to_string(),
            file_path: path.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: Some(
                json!({
                    "type": "project",
                    "description": description,
                    "path": path
                })
                .to_string(),
            ),
        };

        let sqlite = self.sqlite.lock().await;
        let project_id = sqlite.insert_code_entry(project_data)?;
        info!("Project created: {} (ID: {})", name, project_id);
        Ok(project_id)
    }

    /// Get project list
    pub async fn get_projects(&self) -> Result<Vec<ProjectInfo>> {
        let sqlite = self.sqlite.lock().await;
        let code_entries = sqlite.search_code_entries(Some("project"), None, None, None)?;

        let mut projects = Vec::new();
        for entry in code_entries {
            if let Some(metadata_str) = &entry.metadata {
                if let Ok(metadata) = serde_json::from_str::<JsonValue>(metadata_str) {
                    if metadata.get("type").and_then(|v| v.as_str()) == Some("project") {
                        let project = ProjectInfo {
                            id: None,
                            name: entry.function_name.clone(),
                            path: metadata
                                .get("path")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            description: metadata
                                .get("description")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            created_at: Some(entry.created_at),
                        };
                        projects.push(project);
                    }
                }
            }
        }

        Ok(projects)
    }

    /// Get configuration
    pub async fn get_config(&self, key: &str) -> Option<JsonValue> {
        let sqlite = self.sqlite.lock().await;
        let code_entries = sqlite
            .search_code_entries(Some("config"), None, Some(key), None)
            .unwrap_or_default();

        for entry in code_entries {
            if let Some(metadata_str) = &entry.metadata {
                if let Ok(metadata) = serde_json::from_str::<JsonValue>(metadata_str) {
                    if let Some(value) = metadata.get("value") {
                        return Some(value.clone());
                    }
                }
            }
        }

        None
    }

    /// Get configuration value (with default)
    pub async fn get_config_value<T>(&self, key: &str, default: T) -> T
    where
        T: for<'de> Deserialize<'de> + Clone,
    {
        if let Some(config_value) = self.get_config(key).await {
            if let Ok(value) = serde_json::from_value(config_value) {
                return value;
            }
        }
        default
    }

    /// Set configuration
    pub async fn set_config(
        &self,
        key: &str,
        value: JsonValue,
        description: Option<&str>,
    ) -> Result<()> {
        let config_data = CodeEntry {
            id: String::new(),
            code: format!("Config: {}", key),
            language: "config".to_string(),
            function_name: key.to_string(),
            project: "system".to_string(),
            file_path: "config".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: Some(
                json!({
                    "type": "config",
                    "key": key,
                    "value": value,
                    "description": description
                })
                .to_string(),
            ),
        };

        let sqlite = self.sqlite.lock().await;
        sqlite.insert_code_entry(config_data)?;
        debug!("Configuration set: {} = {}", key, value);
        Ok(())
    }

    /// Get system status
    pub async fn get_system_status(&self) -> SystemStatus {
        let mut sqlite_info = HashMap::new();
        let mut qdrant_info = HashMap::new();
        let mut overall_status = "healthy".to_string();

        // SQLite status
        {
            let sqlite = self.sqlite.lock().await;
            let (total_connections, idle_connections) = sqlite.get_pool_status();
            sqlite_info.insert("status".to_string(), json!("connected"));
            sqlite_info.insert("total_connections".to_string(), json!(total_connections));
            sqlite_info.insert("idle_connections".to_string(), json!(idle_connections));

            if let Ok(stats) = sqlite.get_statistics() {
                sqlite_info.insert("statistics".to_string(), json!(stats));
            }
        }

        // Qdrant status
        {
            let qdrant = self.qdrant.lock().await;
            let health = qdrant.health_check().await;
            qdrant_info.insert(
                "health".to_string(),
                json!(if health { "healthy" } else { "unhealthy" }),
            );

            if !health {
                overall_status = "unhealthy".to_string();
            }
        }

        SystemStatus {
            sqlite: sqlite_info,
            qdrant: qdrant_info,
            overall_status,
        }
    }

    /// Batch store interfaces
    pub async fn batch_store_interfaces(
        &self,
        interfaces_data: Vec<HashMap<String, JsonValue>>,
    ) -> Result<Vec<(String, String)>> {
        let mut results = Vec::new();
        let mut vectors_data = Vec::new();

        // Prepare vector data
        for data in &interfaces_data {
            let code = data.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let vector: Vec<f32> = data
                .get("vector")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                })
                .unwrap_or_default();

            let mut vector_data = HashMap::new();
            vector_data.insert("code".to_string(), json!(code));
            vector_data.insert("vector".to_string(), json!(vector));
            vector_data.insert(
                "language".to_string(),
                data.get("language").cloned().unwrap_or(json!("c")),
            );
            vector_data.insert(
                "function_name".to_string(),
                data.get("name").cloned().unwrap_or(json!("")),
            );
            vector_data.insert(
                "project".to_string(),
                data.get("project_name")
                    .cloned()
                    .unwrap_or(json!("default")),
            );
            vector_data.insert(
                "file_path".to_string(),
                data.get("file_path").cloned().unwrap_or(json!("")),
            );
            vector_data.insert(
                "metadata".to_string(),
                data.get("metadata").cloned().unwrap_or(json!({})),
            );

            vectors_data.push(vector_data);
        }

        // Batch insert vectors
        let qdrant_ids = {
            let qdrant = self.qdrant.lock().await;
            qdrant.batch_insert_vectors(vectors_data).await?
        };

        // Insert SQLite metadata one by one
        let sqlite = self.sqlite.lock().await;
        for (i, data) in interfaces_data.iter().enumerate() {
            if i < qdrant_ids.len() {
                let code_entry = CodeEntry {
                    id: String::new(),
                    code: data
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    language: data
                        .get("language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("c")
                        .to_string(),
                    function_name: data
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    project: data
                        .get("project_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default")
                        .to_string(),
                    file_path: data
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    metadata: Some(
                        json!({
                            "qdrant_id": qdrant_ids[i],
                            "inputs": data.get("inputs").cloned().unwrap_or(json!([])),
                            "outputs": data.get("outputs").cloned().unwrap_or(json!([])),
                        })
                        .to_string(),
                    ),
                };

                let interface_id = sqlite.insert_code_entry(code_entry)?;
                results.push((interface_id, qdrant_ids[i].clone()));
            }
        }

        info!("Batch storage of {} interfaces completed", results.len());
        Ok(results)
    }

    /// Clear project data
    pub async fn clear_project_data(&self, project_name: &str) -> Result<bool> {
        let sqlite = self.sqlite.lock().await;
        let code_entries = sqlite.search_code_entries(None, Some(project_name), None, None)?;

        // Delete vectors in Qdrant
        let _qdrant = self.qdrant.lock().await;
        for entry in &code_entries {
            if let Some(metadata_str) = &entry.metadata {
                if let Ok(metadata) = serde_json::from_str::<JsonValue>(metadata_str) {
                    if let Some(qdrant_id) = metadata.get("qdrant_id").and_then(|v| v.as_str()) {
                        // Note: Current Qdrant service doesn't have method to delete single vector, needs implementation
                        debug!("Need to delete Qdrant vector: {}", qdrant_id);
                    }
                }
            }
        }

        // Delete entries in SQLite
        for entry in code_entries {
            sqlite.delete_code_entry(&entry.id)?;
        }

        info!("Project data cleared: {}", project_name);
        Ok(true)
    }

    /// Execute custom SQL query
    pub async fn execute_raw_query(
        &self,
        query: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        let sqlite = self.sqlite.lock().await;
        sqlite
            .execute_raw_query(query, params)
            .await
            .map_err(|e| anyhow!("Database query failed: {}", e))
    }

    /// Get SQLite service reference for advanced operations
    pub async fn get_sqlite_service(&self) -> tokio::sync::MutexGuard<'_, SqliteService> {
        self.sqlite.lock().await
    }

    /// Get currently used SQLite database file path (for diagnostics)
    pub async fn sqlite_db_path(&self) -> String {
        let sqlite = self.sqlite.lock().await;
        sqlite.db_path().to_string()
    }

    /// 获取SQLite统计信息（表内行数）
    pub async fn sqlite_statistics(
        &self,
    ) -> std::result::Result<std::collections::HashMap<String, i64>, String> {
        let sqlite = self.sqlite.lock().await;
        sqlite
            .get_statistics()
            .map_err(|e| format!("Failed to get SQLite statistics: {}", e))
    }

    /// 保存代码条目
    pub async fn save_code_entry(&self, entry: sqlite_services::CodeEntry) -> Result<String> {
        let sqlite = self.sqlite.lock().await;
        sqlite
            .insert_code_entry(entry)
            .map_err(|e| anyhow!("Failed to save code entry: {}", e))
    }

    /// 保存分析结果
    pub async fn save_analysis_result(
        &self,
        result: sqlite_services::AnalysisResult,
    ) -> Result<String> {
        let sqlite = self.sqlite.lock().await;
        sqlite
            .insert_analysis_result(result)
            .map_err(|e| anyhow!("Failed to save analysis result: {}", e))
    }

    /// Close database connections
    pub async fn close(&self) {
        info!("Database manager is closing");
        // SQLite connection pool will automatically manage connection closing
        // Qdrant client will also automatically handle connection cleanup
        info!("Database manager closed");
    }
}

/// Convenience function to create database manager
pub async fn create_database_manager(
    sqlite_path: Option<&str>,
    qdrant_url: Option<&str>,
    qdrant_collection: Option<&str>,
    vector_size: Option<usize>,
) -> Result<DatabaseManager> {
    let mut config = get_config()?;

    // Apply optional overrides
    if let Some(path) = sqlite_path {
        config.sqlite.path = path.to_string();
    }

    if let Some(collection) = qdrant_collection {
        config.qdrant.collection_name = collection.to_string();
    }

    if let Some(vdim) = vector_size {
        config.qdrant.vector_size = vdim;
    }

    if let Some(url) = qdrant_url {
        // Accept forms like "http://host:port", "host:port", or just "host"
        let trimmed = url
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let mut parts = trimmed.split(':');
        if let Some(host) = parts.next() {
            if !host.is_empty() {
                config.qdrant.host = host.to_string();
            }
        }
        if let Some(port_str) = parts.next() {
            if let Ok(port) = port_str.parse::<u16>() {
                config.qdrant.port = Some(port);
            }
        }
    }

    DatabaseManager::new(config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_database_manager_creation() {
        let manager = DatabaseManager::new_default().await;
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_config_operations() {
        let manager = DatabaseManager::new_default().await.unwrap();

        // Test setting and getting configuration
        manager
            .set_config("test_key", json!("test_value"), Some("Test configuration"))
            .await
            .unwrap();
        let value = manager.get_config("test_key").await;
        assert!(value.is_some());
        assert_eq!(value.unwrap().as_str(), Some("test_value"));
    }

    #[tokio::test]
    async fn test_system_status() {
        let manager = DatabaseManager::new_default().await.unwrap();
        let status = manager.get_system_status().await;
        assert!(!status.overall_status.is_empty());
    }
}
