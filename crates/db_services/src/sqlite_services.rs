use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Result as SqliteResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::pkg_config::sqlite_config;

/// Type alias for the connection pool
type SqlitePool = Pool<SqliteConnectionManager>;
type PooledSqliteConnection = PooledConnection<SqliteConnectionManager>;

/// Custom error type for database operations
#[derive(Debug)]
pub enum DatabaseError {
    SqliteError(rusqlite::Error),
    PoolError(r2d2::Error),
    Other(String),
}

impl From<rusqlite::Error> for DatabaseError {
    fn from(err: rusqlite::Error) -> Self {
        DatabaseError::SqliteError(err)
    }
}

impl From<r2d2::Error> for DatabaseError {
    fn from(err: r2d2::Error) -> Self {
        DatabaseError::PoolError(err)
    }
}

impl std::fmt::Display for DatabaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseError::SqliteError(e) => write!(f, "SQLite error: {}", e),
            DatabaseError::PoolError(e) => write!(f, "Connection pool error: {}", e),
            DatabaseError::Other(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for DatabaseError {}

/// Result type for database operations
pub type Result<T> = std::result::Result<T, DatabaseError>;

/// SQLite database service for C2Rust agent
/// Provides storage and retrieval of code metadata, analysis results, and conversion history
/// Now with r2d2 connection pooling for multi-threading support
#[derive(Debug, Clone)]
pub struct SqliteService {
    pool: SqlitePool,
    db_path: String,
}

/// Represents a code entry in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEntry {
    pub id: String,
    pub code: String,
    pub language: String,
    pub function_name: String,
    pub project: String,
    pub file_path: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: Option<String>, // JSON string for additional metadata
}

/// Represents a conversion result in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub id: String,
    pub source_id: String,
    pub original_code: String,
    pub converted_code: String,
    pub conversion_type: String, // e.g., "c_to_rust", "cpp_to_rust"
    pub status: String,          // e.g., "success", "failed", "partial"
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<String>,
}

/// Represents an analysis result in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub id: String,
    pub code_id: String,
    pub analysis_type: String, // e.g., "complexity", "memory_safety", "performance"
    pub result: String,        // JSON string containing analysis results
    pub score: Option<f64>,
    pub created_at: DateTime<Utc>,
}

impl SqliteService {
    /// Create a new SQLite service instance with connection pooling
    pub fn new(sqlite_config: sqlite_config) -> Result<Self> {
        let db_path = sqlite_config.path;
        let manager = SqliteConnectionManager::file(&db_path);
        let pool = Pool::builder()
            .max_size(15) // Maximum number of connections in the pool
            .min_idle(Some(5)) // Minimum number of idle connections
            .build(manager)?;

        let service = SqliteService {
            pool,
            db_path: db_path.to_string(),
        };

        service.initialize_tables()?;
        info!(
            "SQLite service initialized at: {} with connection pooling",
            db_path
        );
        Ok(service)
    }

    /// Create an in-memory database for testing with connection pooling
    pub fn new_in_memory() -> Result<Self> {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder()
            .max_size(10) // Smaller pool for in-memory testing
            .min_idle(Some(2))
            .build(manager)?;

        let service = SqliteService {
            pool,
            db_path: ":memory:".to_string(),
        };

        service.initialize_tables()?;
        debug!("In-memory SQLite service initialized with connection pooling");
        Ok(service)
    }

    /// Get a connection from the pool
    fn get_connection(&self) -> Result<PooledSqliteConnection> {
        self.pool.get().map_err(DatabaseError::from)
    }

    /// Initialize database tables
    fn initialize_tables(&self) -> Result<()> {
        let conn = self.get_connection()?;

        // Create code_entries table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS code_entries (
                id TEXT PRIMARY KEY,
                code TEXT NOT NULL,
                language TEXT NOT NULL,
                function_name TEXT,
                project TEXT,
                file_path TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                metadata TEXT
            )",
            [],
        )?;

        // Create conversion_results table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversion_results (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                original_code TEXT NOT NULL,
                converted_code TEXT NOT NULL,
                conversion_type TEXT NOT NULL,
                status TEXT NOT NULL,
                error_message TEXT,
                created_at TEXT NOT NULL,
                metadata TEXT,
                FOREIGN KEY(source_id) REFERENCES code_entries(id)
            )",
            [],
        )?;

        // Create analysis_results table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS analysis_results (
                id TEXT PRIMARY KEY,
                code_id TEXT NOT NULL,
                analysis_type TEXT NOT NULL,
                result TEXT NOT NULL,
                score REAL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(code_id) REFERENCES code_entries(id)
            )",
            [],
        )?;

        // Create indexes for better performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_code_entries_language ON code_entries(language)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_code_entries_project ON code_entries(project)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_conversion_results_source_id ON conversion_results(source_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_analysis_results_code_id ON analysis_results(code_id)",
            [],
        )?;

        info!("Database tables initialized successfully");
        Ok(())
    }

    /// Insert a new code entry
    pub fn insert_code_entry(&self, mut entry: CodeEntry) -> Result<String> {
        if entry.id.is_empty() {
            entry.id = Uuid::new_v4().to_string();
        }

        let now = Utc::now();
        entry.created_at = now;
        entry.updated_at = now;

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO code_entries (id, code, language, function_name, project, file_path, created_at, updated_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &entry.id,
                &entry.code,
                &entry.language,
                &entry.function_name,
                &entry.project,
                &entry.file_path,
                entry.created_at.to_rfc3339(),
                entry.updated_at.to_rfc3339(),
                &entry.metadata
            ],
        )?;

        debug!("Inserted code entry with ID: {}", entry.id);
        Ok(entry.id)
    }

    /// Get a code entry by ID
    pub fn get_code_entry(&self, id: &str) -> Result<Option<CodeEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, code, language, function_name, project, file_path, created_at, updated_at, metadata
             FROM code_entries WHERE id = ?1",
        )?;

        let entry_iter = stmt.query_map([id], |row| self.row_to_code_entry(row))?;

        for entry in entry_iter {
            return Ok(Some(entry?));
        }

        Ok(None)
    }

    /// Search code entries with optional filters
    pub fn search_code_entries(
        &self,
        language: Option<&str>,
        project: Option<&str>,
        function_name: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<CodeEntry>> {
        let conn = self.get_connection()?;

        // Build query and collect owned parameters
        let mut query = "SELECT id, code, language, function_name, project, file_path, created_at, updated_at, metadata FROM code_entries WHERE 1=1".to_string();
        let mut params = Vec::new();

        if let Some(lang) = language {
            query.push_str(" AND language = ?");
            params.push(lang.to_string());
        }

        if let Some(proj) = project {
            query.push_str(" AND project = ?");
            params.push(proj.to_string());
        }

        if let Some(func) = function_name {
            query.push_str(" AND function_name = ?");
            params.push(func.to_string());
        }

        query.push_str(" ORDER BY updated_at DESC");

        if let Some(limit_val) = limit {
            query.push_str(&format!(" LIMIT {}", limit_val));
        }

        let mut stmt = conn.prepare(&query)?;
        let param_refs: Vec<&str> = params.iter().map(AsRef::as_ref).collect();
        let entry_iter = stmt.query_map(rusqlite::params_from_iter(param_refs), |row| {
            self.row_to_code_entry(row)
        })?;

        let mut entries = Vec::new();
        for entry in entry_iter {
            entries.push(entry?);
        }

        debug!("Found {} code entries", entries.len());
        Ok(entries)
    }

    /// Insert a conversion result
    pub fn insert_conversion_result(&self, mut result: ConversionResult) -> Result<String> {
        if result.id.is_empty() {
            result.id = Uuid::new_v4().to_string();
        }

        result.created_at = Utc::now();

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO conversion_results (id, source_id, original_code, converted_code, conversion_type, status, error_message, created_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                &result.id,
                &result.source_id,
                &result.original_code,
                &result.converted_code,
                &result.conversion_type,
                &result.status,
                &result.error_message,
                result.created_at.to_rfc3339(),
                &result.metadata
            ],
        )?;

        debug!("Inserted conversion result with ID: {}", result.id);
        Ok(result.id)
    }

    /// Get conversion results for a source code entry
    pub fn get_conversion_results(&self, source_id: &str) -> Result<Vec<ConversionResult>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_id, original_code, converted_code, conversion_type, status, error_message, created_at, metadata
             FROM conversion_results WHERE source_id = ?1 ORDER BY created_at DESC",
        )?;

        let result_iter = stmt.query_map([source_id], |row| self.row_to_conversion_result(row))?;

        let mut results = Vec::new();
        for result in result_iter {
            results.push(result?);
        }

        debug!(
            "Found {} conversion results for source_id: {}",
            results.len(),
            source_id
        );
        Ok(results)
    }

    /// Insert an analysis result
    pub fn insert_analysis_result(&self, mut result: AnalysisResult) -> Result<String> {
        if result.id.is_empty() {
            result.id = Uuid::new_v4().to_string();
        }

        result.created_at = Utc::now();

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO analysis_results (id, code_id, analysis_type, result, score, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &result.id,
                &result.code_id,
                &result.analysis_type,
                &result.result,
                result.score,
                result.created_at.to_rfc3339()
            ],
        )?;

        debug!("Inserted analysis result with ID: {}", result.id);
        Ok(result.id)
    }

    /// Get analysis results for a code entry
    pub fn get_analysis_results(&self, code_id: &str) -> Result<Vec<AnalysisResult>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, code_id, analysis_type, result, score, created_at
             FROM analysis_results WHERE code_id = ?1 ORDER BY created_at DESC",
        )?;

        let result_iter = stmt.query_map([code_id], |row| self.row_to_analysis_result(row))?;

        let mut results = Vec::new();
        for result in result_iter {
            results.push(result?);
        }

        debug!(
            "Found {} analysis results for code_id: {}",
            results.len(),
            code_id
        );
        Ok(results)
    }

    /// Update an existing code entry
    pub fn update_code_entry(&self, entry: &CodeEntry) -> Result<()> {
        let updated_at = Utc::now();

        let conn = self.get_connection()?;
        let rows_affected = conn.execute(
            "UPDATE code_entries SET code = ?1, language = ?2, function_name = ?3, project = ?4, file_path = ?5, updated_at = ?6, metadata = ?7
             WHERE id = ?8",
            params![
                &entry.code,
                &entry.language,
                &entry.function_name,
                &entry.project,
                &entry.file_path,
                updated_at.to_rfc3339(),
                &entry.metadata,
                &entry.id
            ],
        )?;

        if rows_affected == 0 {
            warn!("No code entry found with ID: {}", entry.id);
        } else {
            debug!("Updated code entry with ID: {}", entry.id);
        }

        Ok(())
    }

    /// Delete a code entry and its related records
    pub fn delete_code_entry(&self, id: &str) -> Result<()> {
        let conn = self.get_connection()?;

        // Use a transaction for consistency
        let tx = conn.unchecked_transaction()?;

        // Delete analysis results first (due to foreign key constraint)
        tx.execute("DELETE FROM analysis_results WHERE code_id = ?1", [id])?;

        // Delete conversion results
        tx.execute("DELETE FROM conversion_results WHERE source_id = ?1", [id])?;

        // Delete the code entry
        let rows_affected = tx.execute("DELETE FROM code_entries WHERE id = ?1", [id])?;

        tx.commit()?;

        if rows_affected == 0 {
            warn!("No code entry found with ID: {}", id);
        } else {
            debug!("Deleted code entry and related records for ID: {}", id);
        }

        Ok(())
    }

    /// Get database statistics
    pub fn get_statistics(&self) -> Result<HashMap<String, i64>> {
        let conn = self.get_connection()?;
        let mut stats = HashMap::new();

        // Count code entries
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM code_entries")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        stats.insert("code_entries".to_string(), count);

        // Count conversion results
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM conversion_results")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        stats.insert("conversion_results".to_string(), count);

        // Count analysis results
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM analysis_results")?;
        let count: i64 = stmt.query_row([], |row| row.get(0))?;
        stats.insert("analysis_results".to_string(), count);

        debug!("Database statistics retrieved");
        Ok(stats)
    }

    /// Vacuum the database to reclaim space
    pub fn vacuum(&self) -> Result<()> {
        let conn = self.get_connection()?;
        conn.execute("VACUUM", [])?;
        info!("Database vacuumed successfully");
        Ok(())
    }

    /// Get connection pool status
    pub fn get_pool_status(&self) -> (u32, u32) {
        let state = self.pool.state();
        (state.connections, state.idle_connections)
    }

    /// Helper function to convert a database row to CodeEntry
    fn row_to_code_entry(&self, row: &rusqlite::Row) -> SqliteResult<CodeEntry> {
        let created_at_str: String = row.get(6)?;
        let updated_at_str: String = row.get(7)?;

        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc);

        Ok(CodeEntry {
            id: row.get(0)?,
            code: row.get(1)?,
            language: row.get(2)?,
            function_name: row.get(3)?,
            project: row.get(4)?,
            file_path: row.get(5)?,
            created_at,
            updated_at,
            metadata: row.get(8)?,
        })
    }

    /// Helper function to convert a database row to ConversionResult
    fn row_to_conversion_result(&self, row: &rusqlite::Row) -> SqliteResult<ConversionResult> {
        let created_at_str: String = row.get(7)?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc);

        Ok(ConversionResult {
            id: row.get(0)?,
            source_id: row.get(1)?,
            original_code: row.get(2)?,
            converted_code: row.get(3)?,
            conversion_type: row.get(4)?,
            status: row.get(5)?,
            error_message: row.get(6)?,
            created_at,
            metadata: row.get(8)?,
        })
    }

    /// Helper function to convert a database row to AnalysisResult
    fn row_to_analysis_result(&self, row: &rusqlite::Row) -> SqliteResult<AnalysisResult> {
        let created_at_str: String = row.get(5)?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?
            .with_timezone(&Utc);

        Ok(AnalysisResult {
            id: row.get(0)?,
            code_id: row.get(1)?,
            analysis_type: row.get(2)?,
            result: row.get(3)?,
            score: row.get(4)?,
            created_at,
        })
    }

    /// Execute raw SQL query with parameters
    pub async fn execute_raw_query(
        &self,
        query: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<HashMap<String, serde_json::Value>>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(query)?;

        // Convert JSON values to owned rusqlite parameters
        let rusqlite_params: Vec<Box<dyn rusqlite::ToSql + Send + Sync>> = params
            .into_iter()
            .map(|v| match v {
                serde_json::Value::String(s) => {
                    Box::new(s) as Box<dyn rusqlite::ToSql + Send + Sync>
                }
                serde_json::Value::Number(n) if n.is_i64() => {
                    Box::new(n.as_i64().unwrap()) as Box<dyn rusqlite::ToSql + Send + Sync>
                }
                serde_json::Value::Number(n) if n.is_f64() => {
                    Box::new(n.as_f64().unwrap()) as Box<dyn rusqlite::ToSql + Send + Sync>
                }
                serde_json::Value::Bool(b) => Box::new(b) as Box<dyn rusqlite::ToSql + Send + Sync>,
                serde_json::Value::Null => {
                    Box::new(None::<String>) as Box<dyn rusqlite::ToSql + Send + Sync>
                }
                _ => Box::new(v.to_string()) as Box<dyn rusqlite::ToSql + Send + Sync>,
            })
            .collect();

        let param_refs: Vec<&dyn rusqlite::ToSql> = rusqlite_params
            .iter()
            .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
            .collect();

        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let column_count = row.as_ref().column_count();
            let mut result = HashMap::new();

            for i in 0..column_count {
                let column_name = row.as_ref().column_name(i).unwrap_or("unknown").to_string();
                let value: serde_json::Value = match row.get_ref(i)? {
                    rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                    rusqlite::types::ValueRef::Integer(i) => {
                        serde_json::Value::Number(serde_json::Number::from(i))
                    }
                    rusqlite::types::ValueRef::Real(f) => serde_json::Value::Number(
                        serde_json::Number::from_f64(f)
                            .unwrap_or_else(|| serde_json::Number::from(0)),
                    ),
                    rusqlite::types::ValueRef::Text(s) => {
                        serde_json::Value::String(String::from_utf8_lossy(s).to_string())
                    }
                    rusqlite::types::ValueRef::Blob(b) => {
                        use base64::prelude::*;
                        serde_json::Value::String(BASE64_STANDARD.encode(b))
                    }
                };
                result.insert(column_name, value);
            }
            Ok(result)
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }
}

// Make SqliteService thread-safe
unsafe impl Send for SqliteService {}
unsafe impl Sync for SqliteService {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_service_creation() {
        let service = SqliteService::new_in_memory().unwrap();
        let (total_connections, idle_connections) = service.get_pool_status();
        assert!(total_connections >= idle_connections);
        assert!(idle_connections >= 2); // We set min_idle to 2 for in-memory
    }

    #[tokio::test]
    async fn test_code_entry_operations() {
        let service = SqliteService::new_in_memory().unwrap();

        let entry = CodeEntry {
            id: "".to_string(),
            code: "fn main() { println!(\"Hello, world!\"); }".to_string(),
            language: "rust".to_string(),
            function_name: "main".to_string(),
            project: "test_project".to_string(),
            file_path: "src/main.rs".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: Some("{\"test\": true}".to_string()),
        };

        let id = service.insert_code_entry(entry.clone()).unwrap();
        assert!(!id.is_empty());

        let retrieved = service.get_code_entry(&id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.code, entry.code);
        assert_eq!(retrieved.language, entry.language);
    }

    #[tokio::test]
    async fn test_search_code_entries() {
        let service = SqliteService::new_in_memory().unwrap();

        // Insert multiple entries
        let entries = vec![
            CodeEntry {
                id: "".to_string(),
                code: "fn test1() {}".to_string(),
                language: "rust".to_string(),
                function_name: "test1".to_string(),
                project: "project1".to_string(),
                file_path: "src/test1.rs".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                metadata: None,
            },
            CodeEntry {
                id: "".to_string(),
                code: "int test2() {}".to_string(),
                language: "c".to_string(),
                function_name: "test2".to_string(),
                project: "project1".to_string(),
                file_path: "src/test2.c".to_string(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                metadata: None,
            },
        ];

        for entry in entries {
            service.insert_code_entry(entry).unwrap();
        }

        // Search by language
        let rust_entries = service
            .search_code_entries(Some("rust"), None, None, None)
            .unwrap();
        assert_eq!(rust_entries.len(), 1);
        assert_eq!(rust_entries[0].language, "rust");

        // Search by project
        let project_entries = service
            .search_code_entries(None, Some("project1"), None, None)
            .unwrap();
        assert_eq!(project_entries.len(), 2);

        // Test limit
        let limited_entries = service
            .search_code_entries(None, Some("project1"), None, Some(1))
            .unwrap();
        assert_eq!(limited_entries.len(), 1);
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        use std::sync::Arc;
        use tokio::task;

        let service = Arc::new(SqliteService::new_in_memory().unwrap());
        let mut handles = Vec::new();

        // Spawn multiple concurrent tasks
        for i in 0..10 {
            let service_clone = Arc::clone(&service);
            let handle = task::spawn(async move {
                let entry = CodeEntry {
                    id: "".to_string(),
                    code: format!("fn test{}() {{}}", i),
                    language: "rust".to_string(),
                    function_name: format!("test{}", i),
                    project: "concurrent_test".to_string(),
                    file_path: format!("src/test{}.rs", i),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    metadata: None,
                };

                service_clone.insert_code_entry(entry).unwrap()
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        let mut ids = Vec::new();
        for handle in handles {
            let id = handle.await.unwrap();
            ids.push(id);
        }

        // Verify all entries were inserted
        assert_eq!(ids.len(), 10);
        let entries = service
            .search_code_entries(None, Some("concurrent_test"), None, None)
            .unwrap();
        assert_eq!(entries.len(), 10);
    }
}
