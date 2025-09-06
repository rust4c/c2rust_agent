pub mod env_checker;
pub mod ai_checker;

// env_checker导出枚举类型和函数
pub use env_checker::{
    DatabaseConnectionStatus, 
    check_database_existence, 
    dbdata_init, 
    get_detailed_database_status
};


// lib.rs (添加以下内容)


// ai_checker 导出枚举类型和函数
pub use ai_checker::{
    AIConnectionStatus, 
    ai_service_init, 
    check_all_ai_services, 
    get_detailed_ai_status
};