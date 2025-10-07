// 模块声明

pub mod c2rust_translator;

pub mod file_processor;
pub mod pkg_config;
pub mod rust_verifier;
pub mod single_processes;

// 主要处理函数
pub use single_processes::{StageCallback, two_stage_processor_with_callback};

// 导出各模块的公共函数
pub use rust_verifier::{extract_key_errors, verify_and_fix, verify_compilation};
