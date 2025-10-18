use anyhow::Result;
use db_services::DatabaseManager;
use log::{info, warn};

/// Database connection status enum
#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseConnectionStatus {
    /// Both databases connected normally
    BothConnected,
    /// Only SQLite connected normally
    OnlySqliteConnected,
    /// Only Qdrant connected normally
    OnlyQdrantConnected,
    /// Both databases failed to connect
    BothDisconnected,
    /// Connection status unknown
    Unknown,
}

/// Initialize database and check connection status
pub async fn dbdata_init(db_manager: DatabaseManager) -> Result<DatabaseConnectionStatus> {
    info!("Starting database connection initialization...");
    // Interactive info
    println!("Checking database connections...");

    // // Try to create database manager
    // let db_manager = match DatabaseManager::new_default().await {
    //     Ok(manager) => manager,
    //     Err(e) => {
    //         warn!("Database manager initialization failed: {}", e);
    //         return Ok(DatabaseConnectionStatus::BothDisconnected);
    //     }
    // };

    // Get system status
    let status = db_manager.get_system_status().await;

    // Check SQLite connection status
    let sqlite_connected = status
        .sqlite
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "connected")
        .unwrap_or(false);

    // Check Qdrant connection status
    let qdrant_connected = status
        .qdrant
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "connected")
        .unwrap_or(false);

    // Determine connection status
    let connection_status = match (sqlite_connected, qdrant_connected) {
        (true, true) => DatabaseConnectionStatus::BothConnected,
        (true, false) => DatabaseConnectionStatus::OnlySqliteConnected,
        (false, true) => DatabaseConnectionStatus::OnlyQdrantConnected,
        (false, false) => DatabaseConnectionStatus::BothDisconnected,
    };

    // Log connection status
    match connection_status {
        DatabaseConnectionStatus::BothConnected => {
            info!("Database connection normal: Both SQLite and Qdrant are connected");
            // Interactive info
            println!("Database connection normal: Both SQLite and Qdrant are connected");
        }
        DatabaseConnectionStatus::OnlySqliteConnected => {
            warn!("Database connection partially normal: Only SQLite is connected");
            // Interactive info
            println!("Database connection partially normal: Only SQLite is connected");
        }
        DatabaseConnectionStatus::OnlyQdrantConnected => {
            warn!("Database connection partially normal: Only Qdrant is connected");
            // Interactive info
            println!("Database connection partially normal: Only Qdrant is connected");
        }
        DatabaseConnectionStatus::BothDisconnected => {
            warn!("Database connection failed: Neither SQLite nor Qdrant are connected");
            // Interactive info
            println!("Database connection failed: Neither SQLite nor Qdrant are connected");
        }
        DatabaseConnectionStatus::Unknown => {
            warn!("Database connection status unknown");
            // Interactive info
            println!("Database connection status unknown");
        }
    }

    // // Close database connection
    // db_manager.close().await;

    Ok(connection_status)
}

/// Check if databases exist (by attempting to connect)
pub async fn check_database_existence() -> Result<(bool, bool)> {
    info!("Checking if databases exist...");

    // Try to create database manager
    let db_manager = match DatabaseManager::new_default().await {
        Ok(manager) => manager,
        Err(e) => {
            warn!("Database manager initialization failed: {}", e);
            return Ok((false, false));
        }
    };

    // Get system status
    let status = db_manager.get_system_status().await;

    // Check if SQLite exists
    let sqlite_exists = status
        .sqlite
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "connected")
        .unwrap_or(false);

    // Check if Qdrant exists
    let qdrant_exists = status
        .qdrant
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "connected")
        .unwrap_or(false);

    // Close database connection
    db_manager.close().await;

    Ok((sqlite_exists, qdrant_exists))
}

/// 获取详细的数据库状态信息
pub async fn get_detailed_database_status() -> Result<String> {
    info!("Getting detailed database status information...");

    // Try to create database manager
    let db_manager = match DatabaseManager::new_default().await {
        Ok(manager) => manager,
        Err(e) => {
            return Ok(format!("Database manager initialization failed: {}", e));
        }
    };

    // Get system status
    let status = db_manager.get_system_status().await;

    // Format status information
    let status_info = format!(
        "Overall status: {}\nSQLite status: {:?}\nQdrant status: {:?}",
        status.overall_status, status.sqlite, status.qdrant
    );

    // Close database connection
    db_manager.close().await;

    Ok(status_info)
}
