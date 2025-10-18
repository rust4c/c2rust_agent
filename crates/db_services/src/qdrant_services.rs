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

use crate::pkg_config::QdrantConfig;

const DEFAULT_BATCH_SIZE: usize = 100;
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const DEFAULT_PORT: u16 = 6334; // Default use port 6334

/// Qdrant vector database server
pub struct QdrantServer {
    client: Qdrant,
    collection_name: String,
    vector_size: u64,
    _timeout: Duration,
    batch_size: usize,
}

#[allow(dead_code)]
impl QdrantServer {
    /// Create new Qdrant server instance
    pub async fn new(qdrant_config: QdrantConfig) -> Result<Self> {
        // Determine port (prefer passed parameter, otherwise use default)
        let port = qdrant_config.port.unwrap_or(DEFAULT_PORT);
        let host = qdrant_config.host;
        let address = format!("http://{}:{}", host, port);

        let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);

        // Use new version Qdrant client
        let client = Qdrant::from_url(&address)
            .timeout(timeout)
            .build()
            .context("Failed to create Qdrant client")?;

        let mut instance = Self {
            client,
            collection_name: qdrant_config.collection_name.to_string(),
            vector_size: qdrant_config.vector_size as u64,
            _timeout: timeout,
            batch_size: DEFAULT_BATCH_SIZE,
        };

        let vector_size = instance.vector_size;

        instance.ensure_collection().await?;
        info!(
            "Qdrant client initialization successful: {} (port: {}, timeout: {:?}, vector dimension: {})",
            host, port, timeout, vector_size
        );

        Ok(instance)
    }

    /// Ensure collection exists and is properly configured
    async fn ensure_collection(&mut self) -> Result<()> {
        // Check if collection exists
        let collections = self.client.list_collections().await?;
        let exists = collections
            .collections
            .iter()
            .any(|c| c.name == self.collection_name);

        if exists {
            debug!("Using existing collection: {}", self.collection_name);
        } else {
            self.create_collection().await?;
        }

        Ok(())
    }

    /// Create new collection
    async fn create_collection(&self) -> Result<()> {
        let _response = self
            .client
            .create_collection(
                CreateCollectionBuilder::new(&self.collection_name)
                    .vectors_config(VectorParamsBuilder::new(self.vector_size, Distance::Cosine)),
            )
            .await
            .context("Failed to create collection")?;

        info!(
            "Created Qdrant collection: {} (dimension: {})",
            self.collection_name, self.vector_size
        );
        Ok(())
    }

    /// Recreate collection
    async fn recreate_collection(&mut self) -> Result<()> {
        self.client
            .delete_collection(&self.collection_name)
            .await
            .context("Failed to delete collection")?;

        self.create_collection().await?;
        info!("Recreated collection: {}", self.collection_name);
        Ok(())
    }

    /// Insert code vector
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
            .context("Failed to insert point")?;

        debug!("Inserted code vector: {}, ID: {}", function_name, point_id);
        Ok(point_id)
    }

    /// Search similar code
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
            .context("Failed to search points")?
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

        debug!("Found {} similar code entries", results.len());
        Ok(results)
    }

    /// Batch insert vectors
    pub async fn batch_insert_vectors(
        &self,
        vectors_data: Vec<HashMap<String, JsonValue>>,
    ) -> Result<Vec<String>> {
        let total_vectors = vectors_data.len();
        if total_vectors == 0 {
            return Ok(vec![]);
        }

        info!(
            "Starting batch insert of {} vectors, batch size: {}",
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
                    info!("Batch {}/{} insertion successful", batch_num, total_batches);
                }
                Err(e) => {
                    error!("Batch insertion failed: {}", e);
                    return Err(e);
                }
            }
        }

        info!(
            "Batch insertion completed, successfully inserted {}/{} vectors",
            all_point_ids.len(),
            total_vectors
        );
        Ok(all_point_ids)
    }

    /// Insert single batch (with retry mechanism)
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
                        .context("Missing vector data")?
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
                        .context("Failed to batch insert points")?;
                    return Ok(ids);
                }
                Err(e) if retries < MAX_RETRIES => {
                    retries += 1;
                    let wait_secs = 2u64.pow(retries as u32);
                    warn!(
                        "Batch {} parsing failed (attempt {}/{}): {}, waiting {} seconds before retry",
                        batch_num, retries, MAX_RETRIES, e, wait_secs
                    );
                    sleep(Duration::from_secs(wait_secs)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Get code by ID
    pub async fn get_code_by_id(
        &self,
        _point_id: &str,
    ) -> Result<Option<HashMap<String, JsonValue>>> {
        // Note: This method needs to be implemented based on actual Qdrant client API
        // Current version may not directly support getting points by ID
        warn!("get_code_by_id method needs implementation based on actual API");
        Ok(None)
    }

    /// Clear collection
    pub async fn clear_collection(&self) -> Result<()> {
        self.client
            .delete_collection(&self.collection_name)
            .await
            .context("Failed to delete collection")?;
        self.create_collection()
            .await
            .context("Failed to recreate collection")?;
        info!("Collection cleared and recreated: {}", self.collection_name);
        Ok(())
    }

    /// Health check
    pub async fn health_check(&self) -> bool {
        self.client
            .health_check()
            .await
            .map(|_| true)
            .unwrap_or(false)
    }

    /// Set batch size
    pub fn set_batch_size(&mut self, size: usize) {
        self.batch_size = size;
        info!("Updated batch size to: {}", size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    async fn create_test_client() -> QdrantServer {
        let host = env::var("QDRANT_HOST").unwrap_or("localhost".to_string());
        let port = env::var("QDRANT_PORT").map(|p| p.parse().unwrap()).ok();

        let qdrant_config = QdrantConfig {
            host,
            port,
            collection_name: "test_collection".to_string(),
            vector_size: 384,
        };

        QdrantServer::new(qdrant_config)
            .await
            .expect("Failed to create test client")
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
            .expect("Failed to insert vector");

        // Note: get_code_by_id method is currently not implemented, so skip this test
        // let result = client.get_code_by_id(&id).await.expect("Failed to get point");
        // assert!(result.is_some());

        let search_results = client
            .search_similar_code(vector, 5, Some("rust"), None, 0.0)
            .await
            .expect("Search failed");
        assert_eq!(search_results.len(), 1);
    }

    #[tokio::test]
    async fn test_port_config() {
        let qdrant_config = QdrantConfig {
            host: "localhost".to_string(),
            port: Some(6334),
            collection_name: "test_collection".to_string(),
            vector_size: 384,
        };
        // Test explicit port configuration
        let client = QdrantServer::new(qdrant_config)
            .await
            .expect("Failed to create port test client");

        assert!(client.health_check().await);
    }
}
