use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

mod pkg_config;
use pkg_config::{qdrant_config, sqlite_config, DBConfig};
mod qdrant_services;
use qdrant_services::QdrantServer;
mod sqlite_services;
use sqlite_services::{CodeEntry, ConversionResult, SqliteService};

/// 接口信息结构体
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

/// 项目信息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: Option<i32>,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

/// 配置信息结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    pub key: String,
    pub value: JsonValue,
    pub description: Option<String>,
}

/// 搜索结果结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub interface: InterfaceInfo,
    pub vector_info: HashMap<String, JsonValue>,
    pub similarity_score: f32,
}

/// 系统状态结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub sqlite: HashMap<String, JsonValue>,
    pub qdrant: HashMap<String, JsonValue>,
    pub overall_status: String,
}

/// 数据库管理器 - 统一管理 SQLite 和 Qdrant 数据库
pub struct DatabaseManager {
    sqlite: Arc<Mutex<SqliteService>>,
    qdrant: Arc<Mutex<QdrantServer>>,
}

impl DatabaseManager {
    /// 创建新的数据库管理器实例
    pub async fn new(database_config: DBConfig) -> Result<Self> {
        let sqlite_service = Arc::new(Mutex::new(
            SqliteService::new(database_config.get_sqlite_config().clone())
                .map_err(|e| anyhow!("Failed to create SQLite service: {}", e))?,
        ));

        let qdrant_service = Arc::new(Mutex::new(
            QdrantServer::new(database_config.get_qdrant_config().clone())
                .await
                .map_err(|e| anyhow!("Failed to create Qdrant service: {}", e))?,
        ));

        let manager = DatabaseManager {
            sqlite: sqlite_service,
            qdrant: qdrant_service,
        };

        manager.init_config().await?;
        info!("数据库管理器初始化完成");

        Ok(manager)
    }

    /// 创建默认配置的数据库管理器
    pub async fn new_default() -> Result<Self> {
        let sqlite_config = sqlite_config {
            path: "c2rust_metadata.db".to_string(),
        };

        let qdrant_config = qdrant_config {
            host: "localhost".to_string(),
            port: Some(6333),
            collection_name: "c2rust_vectors".to_string(),
            vector_size: 384,
        };

        let config = DBConfig::new(qdrant_config, sqlite_config);
        Self::new(config).await
    }

    /// 初始化默认配置
    async fn init_config(&self) -> Result<()> {
        let default_configs = vec![
            ("ai_source", json!("deepseek"), "AI服务来源"),
            ("auto_confirm_threshold", json!(0.85), "自动确认阈值"),
            ("strict_mode", json!(true), "严格模式"),
            ("pointer_strategy", json!("box"), "指针策略"),
            ("max_translation_attempts", json!(3), "最大转译尝试次数"),
            ("vector_similarity_threshold", json!(0.7), "向量相似度阈值"),
        ];

        for (key, value, desc) in default_configs {
            if self.get_config(key).await.is_none() {
                self.set_config(key, value, Some(desc)).await?;
            }
        }

        Ok(())
    }

    /// 存储接口及其向量表示
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

        // 首先存储向量到Qdrant
        let qdrant_id = {
            let qdrant = self.qdrant.lock().await;
            qdrant
                .insert_code_vector(code, vector, language, name, project, file_path, metadata)
                .await?
        };

        // 然后存储代码条目到SQLite
        let code_entry = CodeEntry {
            id: String::new(), // 将由SQLite自动生成
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
            "存储接口完成: {}, SQLite ID: {}, Qdrant ID: {}",
            name, interface_id, qdrant_id
        );
        Ok((interface_id, qdrant_id))
    }

    /// 搜索相似接口
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

        // 从Qdrant搜索相似向量
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

        // 如果转译成功且有向量，存储Rust代码向量
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

        info!("添加转译记录完成: {}", history_id);
        Ok(history_id)
    }

    /// 按名称搜索接口
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
                inputs: Vec::new(),  // 需要从metadata中解析
                outputs: Vec::new(), // 需要从metadata中解析
                file_path: entry.file_path.clone(),
                qdrant_id: None, // 需要从metadata中解析
                language: entry.language,
                project_name: Some(entry.project),
                created_at: Some(entry.created_at),
                updated_at: Some(entry.updated_at),
            };
            interfaces.push(interface);
        }

        Ok(interfaces)
    }

    /// 按文本内容搜索代码
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

    /// 创建项目
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
        info!("创建项目: {} (ID: {})", name, project_id);
        Ok(project_id)
    }

    /// 获取项目列表
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

    /// 获取配置
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

    /// 获取配置值（带默认值）
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

    /// 设置配置
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
        debug!("设置配置: {} = {}", key, value);
        Ok(())
    }

    /// 获取系统状态
    pub async fn get_system_status(&self) -> SystemStatus {
        let mut sqlite_info = HashMap::new();
        let mut qdrant_info = HashMap::new();
        let mut overall_status = "healthy".to_string();

        // SQLite状态
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

        // Qdrant状态
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

    /// 批量存储接口
    pub async fn batch_store_interfaces(
        &self,
        interfaces_data: Vec<HashMap<String, JsonValue>>,
    ) -> Result<Vec<(String, String)>> {
        let mut results = Vec::new();
        let mut vectors_data = Vec::new();

        // 准备向量数据
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

        // 批量插入向量
        let qdrant_ids = {
            let qdrant = self.qdrant.lock().await;
            qdrant.batch_insert_vectors(vectors_data).await?
        };

        // 逐个插入SQLite元数据
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

        info!("批量存储 {} 个接口完成", results.len());
        Ok(results)
    }

    /// 清空项目数据
    pub async fn clear_project_data(&self, project_name: &str) -> Result<bool> {
        let sqlite = self.sqlite.lock().await;
        let code_entries = sqlite.search_code_entries(None, Some(project_name), None, None)?;

        // 删除Qdrant中的向量
        let _qdrant = self.qdrant.lock().await;
        for entry in &code_entries {
            if let Some(metadata_str) = &entry.metadata {
                if let Ok(metadata) = serde_json::from_str::<JsonValue>(metadata_str) {
                    if let Some(qdrant_id) = metadata.get("qdrant_id").and_then(|v| v.as_str()) {
                        // 注意：当前Qdrant服务没有删除单个向量的方法，这里需要实现
                        debug!("需要删除Qdrant向量: {}", qdrant_id);
                    }
                }
            }
        }

        // 删除SQLite中的条目
        for entry in code_entries {
            sqlite.delete_code_entry(&entry.id)?;
        }

        info!("清空项目数据: {}", project_name);
        Ok(true)
    }

    /// 关闭数据库连接
    pub async fn close(&self) {
        info!("数据库管理器正在关闭");
        // SQLite连接池会自动管理连接的关闭
        // Qdrant客户端也会自动处理连接清理
        info!("数据库管理器已关闭");
    }
}

/// 创建数据库管理器的便捷函数
pub async fn create_database_manager(
    sqlite_path: Option<&str>,
    qdrant_url: Option<&str>,
    qdrant_collection: Option<&str>,
    vector_size: Option<usize>,
) -> Result<DatabaseManager> {
    let sqlite_config = sqlite_config {
        path: sqlite_path.unwrap_or("c2rust_metadata.db").to_string(),
    };

    // 解析Qdrant URL
    let (host, port) = if let Some(url) = qdrant_url {
        if url.starts_with("http://") {
            let without_proto = url.strip_prefix("http://").unwrap_or(url);
            if let Some(colon_pos) = without_proto.find(':') {
                let host = without_proto[..colon_pos].to_string();
                let port = without_proto[colon_pos + 1..]
                    .parse::<u16>()
                    .unwrap_or(6333);
                (host, Some(port))
            } else {
                (without_proto.to_string(), Some(6333))
            }
        } else {
            ("localhost".to_string(), Some(6333))
        }
    } else {
        ("localhost".to_string(), Some(6333))
    };

    let qdrant_config = qdrant_config {
        host,
        port,
        collection_name: qdrant_collection.unwrap_or("c2rust_vectors").to_string(),
        vector_size: vector_size.unwrap_or(384),
    };

    let config = DBConfig::new(qdrant_config, sqlite_config);
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

        // 测试设置和获取配置
        manager
            .set_config("test_key", json!("test_value"), Some("测试配置"))
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
