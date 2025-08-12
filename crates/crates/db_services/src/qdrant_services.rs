use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use qdrant_client::qdrant::{
    Condition, CreateCollectionBuilder, Distance, Filter, PointStruct, SearchPointsBuilder,
    UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::{Payload, Qdrant};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

const DEFAULT_BATCH_SIZE: usize = 100;
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_PORT: u16 = 6334; // 默认使用 6334 端口

/// Qdrant 向量数据库服务器
pub struct QdrantServer {
    client: Qdrant,
    collection_name: String,
    vector_size: u64,
    timeout: Duration,
    batch_size: usize,
}

impl QdrantServer {
    /// 创建新的 Qdrant 服务器实例
    pub async fn new(
        host: &str,
        port: Option<u16>,
        collection_name: &str,
        vector_size: usize,
    ) -> Result<Self> {
        // 确定端口（优先使用传入参数，否则使用默认）
        let port = port.unwrap_or(DEFAULT_PORT);
        let address = format!("http://{}:{}", host, port);

        let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);

        // 使用新版 Qdrant 客户端
        let client = Qdrant::from_url(&address)
            .timeout(timeout)
            .build()
            .context("创建 Qdrant 客户端失败")?;

        let mut instance = Self {
            client,
            collection_name: collection_name.to_string(),
            vector_size: vector_size as u64,
            timeout,
            batch_size: DEFAULT_BATCH_SIZE,
        };

        instance.ensure_collection().await?;
        info!(
            "Qdrant 客户端初始化成功: {} (端口: {}, 超时: {:?}, 向量维度: {})",
            host, port, timeout, vector_size
        );

        Ok(instance)
    }

    /// 确保集合存在并正确配置
    async fn ensure_collection(&mut self) -> Result<()> {
        // 检查集合是否存在
        let collections = self.client.list_collections().await?;
        let exists = collections
            .collections
            .iter()
            .any(|c| c.name == self.collection_name);

        if exists {
            debug!("使用现有集合: {}", self.collection_name);
        } else {
            self.create_collection().await?;
        }

        Ok(())
    }

    /// 创建新集合
    async fn create_collection(&self) -> Result<()> {
        let _response = self
            .client
            .create_collection(
                CreateCollectionBuilder::new(&self.collection_name)
                    .vectors_config(VectorParamsBuilder::new(self.vector_size, Distance::Cosine)),
            )
            .await
            .context("创建集合失败")?;

        info!(
            "创建 Qdrant 集合: {} (维度: {})",
            self.collection_name, self.vector_size
        );
        Ok(())
    }

    /// 重新创建集合
    async fn recreate_collection(&mut self) -> Result<()> {
        self.client
            .delete_collection(&self.collection_name)
            .await
            .context("删除集合失败")?;

        self.create_collection().await?;
        info!("重新创建集合: {}", self.collection_name);
        Ok(())
    }

    /// 插入代码向量
    pub async fn insert_code_vector(
        &self,
        code: &str,
        vector: Vec<f32>,
        language: &str,
        function_name: &str,
        project: &str,
        file_path: &str,
        metadata: Option<HashMap<String, JsonValue>>,
    ) -> Result<String> {
        let point_id = Uuid::new_v4().to_string();
        let mut payload = Payload::new();

        payload.insert("code", code);
        payload.insert("language", language);
        payload.insert("function_name", function_name);
        payload.insert("project", project);
        payload.insert("file_path", file_path);
        payload.insert("timestamp", chrono::Utc::now().to_rfc3339());

        if let Some(meta) = metadata {
            for (key, value) in meta {
                match value {
                    JsonValue::String(s) => payload.insert(key, s),
                    JsonValue::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            payload.insert(key, i);
                        } else if let Some(f) = n.as_f64() {
                            payload.insert(key, f);
                        }
                    }
                    JsonValue::Bool(b) => payload.insert(key, b),
                    _ => payload.insert(key, value.to_string()),
                };
            }
        }

        let point = PointStruct::new(point_id.clone(), vector, payload);

        let _response = self
            .client
            .upsert_points(UpsertPointsBuilder::new(&self.collection_name, vec![point]))
            .await
            .context("插入点失败")?;

        debug!("插入代码向量: {}, ID: {}", function_name, point_id);
        Ok(point_id)
    }

    /// 搜索相似代码
    pub async fn search_similar_code(
        &self,
        query_vector: Vec<f32>,
        limit: usize,
        language: Option<&str>,
        project: Option<&str>,
        score_threshold: f32,
    ) -> Result<Vec<HashMap<String, JsonValue>>> {
        let mut filter_conditions = Vec::new();

        if let Some(lang) = language {
            filter_conditions.push(Condition::matches("language", lang.to_string()));
        }

        if let Some(proj) = project {
            filter_conditions.push(Condition::matches("project", proj.to_string()));
        }

        let mut search_builder =
            SearchPointsBuilder::new(&self.collection_name, query_vector, limit as u64)
                .with_payload(true)
                .score_threshold(score_threshold);

        if !filter_conditions.is_empty() {
            search_builder = search_builder.filter(Filter::all(filter_conditions));
        }

        let search_result = self
            .client
            .search_points(search_builder)
            .await
            .context("搜索点失败")?
            .result;

        let results: Vec<HashMap<String, JsonValue>> = search_result
            .into_iter()
            .map(|point| {
                let mut map = HashMap::new();
                if let Some(id) = point.id {
                    map.insert("id".to_string(), JsonValue::String(format!("{:?}", id)));
                }
                map.insert(
                    "score".to_string(),
                    JsonValue::Number(
                        serde_json::Number::from_f64(point.score as f64)
                            .unwrap_or_else(|| serde_json::Number::from(0)),
                    ),
                );
                map.insert(
                    "payload".to_string(),
                    serde_json::to_value(point.payload).unwrap_or_default(),
                );
                map
            })
            .collect();

        debug!("搜索到 {} 个相似代码", results.len());
        Ok(results)
    }

    /// 批量插入向量
    pub async fn batch_insert_vectors(
        &self,
        vectors_data: Vec<HashMap<String, JsonValue>>,
    ) -> Result<Vec<String>> {
        let total_vectors = vectors_data.len();
        if total_vectors == 0 {
            return Ok(vec![]);
        }

        info!(
            "开始批量插入 {} 个向量，批次大小: {}",
            total_vectors, self.batch_size
        );

        let mut all_point_ids = Vec::new();
        let mut processed = 0;
        let total_batches = (total_vectors + self.batch_size - 1) / self.batch_size;

        while processed < total_vectors {
            let batch_end = std::cmp::min(processed + self.batch_size, total_vectors);
            let batch_data = &vectors_data[processed..batch_end];
            let batch_num = (processed / self.batch_size) + 1;

            match self.insert_batch(batch_data.to_vec(), batch_num).await {
                Ok(ids) => {
                    all_point_ids.extend(ids);
                    processed = batch_end;
                    info!("批次 {}/{} 插入成功", batch_num, total_batches);
                }
                Err(e) => {
                    error!("批次插入失败: {}", e);
                    return Err(e);
                }
            }
        }

        info!(
            "批量插入完成，成功插入 {}/{} 个向量",
            all_point_ids.len(),
            total_vectors
        );
        Ok(all_point_ids)
    }

    /// 插入单一批次（带重试机制）
    async fn insert_batch(
        &self,
        batch_data: Vec<HashMap<String, JsonValue>>,
        batch_num: usize,
    ) -> Result<Vec<String>> {
        const MAX_RETRIES: usize = 3;
        let mut retries = 0;

        loop {
            let points: Result<Vec<PointStruct>> = batch_data
                .iter()
                .map(|data| {
                    let point_id = Uuid::new_v4().to_string();
                    let vector: Vec<f32> = data
                        .get("vector")
                        .and_then(|v| v.as_array())
                        .context("缺少向量数据")?
                        .iter()
                        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                        .collect();

                    let mut payload = Payload::new();
                    for (key, value) in data {
                        if key != "vector" {
                            match value {
                                JsonValue::String(s) => payload.insert(key.clone(), s.clone()),
                                JsonValue::Number(n) => {
                                    if let Some(i) = n.as_i64() {
                                        payload.insert(key.clone(), i);
                                    } else if let Some(f) = n.as_f64() {
                                        payload.insert(key.clone(), f);
                                    }
                                }
                                JsonValue::Bool(b) => payload.insert(key.clone(), *b),
                                _ => payload.insert(key.clone(), value.to_string()),
                            };
                        }
                    }

                    payload.insert("timestamp", chrono::Utc::now().to_rfc3339());
                    payload.insert("batch_num", batch_num as i64);

                    Ok(PointStruct::new(point_id.clone(), vector, payload))
                })
                .collect();

            match points {
                Ok(points) => {
                    let ids: Vec<String> = points
                        .iter()
                        .filter_map(|p| p.id.as_ref().map(|id| format!("{:?}", id)))
                        .collect();

                    self.client
                        .upsert_points(UpsertPointsBuilder::new(&self.collection_name, points))
                        .await
                        .context("批量插入点失败")?;
                    return Ok(ids);
                }
                Err(e) if retries < MAX_RETRIES => {
                    retries += 1;
                    let wait_secs = 2u64.pow(retries as u32);
                    warn!(
                        "批次 {} 解析失败 (尝试 {}/{}): {}，等待 {} 秒后重试",
                        batch_num, retries, MAX_RETRIES, e, wait_secs
                    );
                    sleep(Duration::from_secs(wait_secs)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 根据 ID 获取代码
    pub async fn get_code_by_id(
        &self,
        _point_id: &str,
    ) -> Result<Option<HashMap<String, JsonValue>>> {
        // 注意：这个方法需要根据实际的 Qdrant 客户端 API 来实现
        // 当前版本可能不直接支持按 ID 获取点
        warn!("get_code_by_id 方法需要根据实际 API 实现");
        Ok(None)
    }

    /// 清空集合
    pub async fn clear_collection(&self) -> Result<()> {
        self.client
            .delete_collection(&self.collection_name)
            .await
            .context("删除集合失败")?;
        self.create_collection().await.context("重新创建集合失败")?;
        info!("集合已清空并重新创建: {}", self.collection_name);
        Ok(())
    }

    /// 健康检查
    pub async fn health_check(&self) -> bool {
        self.client
            .health_check()
            .await
            .map(|_| true)
            .unwrap_or(false)
    }

    /// 设置批量大小
    pub fn set_batch_size(&mut self, size: usize) {
        self.batch_size = size;
        info!("更新批量大小为: {}", size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    async fn create_test_client() -> QdrantServer {
        let host = env::var("QDRANT_HOST").unwrap_or("localhost".to_string());
        let port = env::var("QDRANT_PORT").map(|p| p.parse().unwrap()).ok();

        QdrantServer::new(&host, port, "test_collection", 384)
            .await
            .expect("创建测试客户端失败")
    }

    #[tokio::test]
    async fn test_create_and_search() {
        let client = create_test_client().await;
        client.clear_collection().await.expect("清空集合失败");

        let vector = vec![0.5; 384];
        let _id = client
            .insert_code_vector(
                "fn test() { println!(\"Hello\"); }",
                vector.clone(),
                "rust",
                "test_function",
                "test_project",
                "/path/to/file.rs",
                None,
            )
            .await
            .expect("插入向量失败");

        // 注意：get_code_by_id 方法目前未实现，所以跳过这个测试
        // let result = client.get_code_by_id(&id).await.expect("获取点失败");
        // assert!(result.is_some());

        let search_results = client
            .search_similar_code(vector, 5, Some("rust"), None, 0.0)
            .await
            .expect("搜索失败");
        assert_eq!(search_results.len(), 1);
    }

    #[tokio::test]
    async fn test_port_config() {
        // 测试显式端口配置
        let client = QdrantServer::new("localhost", Some(6334), "port_test", 128)
            .await
            .expect("创建端口测试客户端失败");

        assert!(client.health_check().await);
    }
}
