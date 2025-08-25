pub mod env_checker;

// env_checker导出枚举类型和函数
pub use env_checker::{
    DatabaseConnectionStatus, 
    check_database_existence, 
    dbdata_init, 
    get_detailed_database_status
};