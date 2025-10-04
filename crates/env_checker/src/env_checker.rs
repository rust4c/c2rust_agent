use anyhow::Result;
use db_services::DatabaseManager;
use log::{info, warn};

/// 数据库连接状态枚举
#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseConnectionStatus {
    /// 两个数据库都连接正常
    BothConnected,
    /// 只有 SQLite 连接正常
    OnlySqliteConnected,
    /// 只有 Qdrant 连接正常
    OnlyQdrantConnected,
    /// 两个数据库都连接失败
    BothDisconnected,
    /// 连接状态未知
    Unknown,
}

/// 初始化数据库并检查连接状态
pub async fn dbdata_init(db_manager: DatabaseManager) -> Result<DatabaseConnectionStatus> {
    info!("开始初始化数据库连接...");
    // 交互信息
    println!("正在检查数据库连接...");

    // // 尝试创建数据库管理器
    // let db_manager = match DatabaseManager::new_default().await {
    //     Ok(manager) => manager,
    //     Err(e) => {
    //         warn!("数据库管理器初始化失败: {}", e);
    //         return Ok(DatabaseConnectionStatus::BothDisconnected);
    //     }
    // };

    // 获取系统状态
    let status = db_manager.get_system_status().await;

    // 检查 SQLite 连接状态
    let sqlite_connected = status
        .sqlite
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "connected")
        .unwrap_or(false);

    // 检查 Qdrant 连接状态
    let qdrant_connected = status
        .qdrant
        .get("health")
        .and_then(|v| v.as_str())
        .map(|s| s == "healthy")
        .unwrap_or(false);

    // 确定连接状态
    let connection_status = match (sqlite_connected, qdrant_connected) {
        (true, true) => DatabaseConnectionStatus::BothConnected,
        (true, false) => DatabaseConnectionStatus::OnlySqliteConnected,
        (false, true) => DatabaseConnectionStatus::OnlyQdrantConnected,
        (false, false) => DatabaseConnectionStatus::BothDisconnected,
    };

    // 记录连接状态
    match connection_status {
        DatabaseConnectionStatus::BothConnected => {
            info!("数据库连接正常: SQLite 和 Qdrant 都已连接");
            // 交互信息
            println!("数据库连接正常: SQLite 和 Qdrant 都已连接");
        }
        DatabaseConnectionStatus::OnlySqliteConnected => {
            warn!("数据库连接部分正常: 只有 SQLite 已连接");
            // 交互信息
            println!("数据库连接部分正常: 只有 SQLite 已连接");
        }
        DatabaseConnectionStatus::OnlyQdrantConnected => {
            warn!("数据库连接部分正常: 只有 Qdrant 已连接");
            // 交互信息
            println!("数据库连接部分正常: 只有 Qdrant 已连接");
        }
        DatabaseConnectionStatus::BothDisconnected => {
            warn!("数据库连接失败: SQLite 和 Qdrant 都未连接");
            // 交互信息
            println!("数据库连接失败: SQLite 和 Qdrant 都未连接");
        }
        DatabaseConnectionStatus::Unknown => {
            warn!("数据库连接状态未知");
            // 交互信息
            println!("数据库连接状态未知");
        }
    }

    // // 关闭数据库连接
    // db_manager.close().await;

    Ok(connection_status)
}

/// 检查数据库是否存在（通过尝试连接）
pub async fn check_database_existence() -> Result<(bool, bool)> {
    info!("检查数据库是否存在...");

    // 尝试创建数据库管理器
    let db_manager = match DatabaseManager::new_default().await {
        Ok(manager) => manager,
        Err(e) => {
            warn!("数据库管理器初始化失败: {}", e);
            return Ok((false, false));
        }
    };

    // 获取系统状态
    let status = db_manager.get_system_status().await;

    // 检查 SQLite 是否存在
    let sqlite_exists = status
        .sqlite
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "connected")
        .unwrap_or(false);

    // 检查 Qdrant 是否存在
    let qdrant_exists = status
        .qdrant
        .get("health")
        .and_then(|v| v.as_str())
        .map(|s| s == "healthy")
        .unwrap_or(false);

    // 关闭数据库连接
    db_manager.close().await;

    Ok((sqlite_exists, qdrant_exists))
}

/// 获取详细的数据库状态信息
pub async fn get_detailed_database_status() -> Result<String> {
    info!("获取详细的数据库状态信息...");

    // 尝试创建数据库管理器
    let db_manager = match DatabaseManager::new_default().await {
        Ok(manager) => manager,
        Err(e) => {
            return Ok(format!("数据库管理器初始化失败: {}", e));
        }
    };

    // 获取系统状态
    let status = db_manager.get_system_status().await;

    // 格式化状态信息
    let status_info = format!(
        "整体状态: {}\nSQLite 状态: {:?}\nQdrant 状态: {:?}",
        status.overall_status, status.sqlite, status.qdrant
    );

    // 关闭数据库连接
    db_manager.close().await;

    Ok(status_info)
}
