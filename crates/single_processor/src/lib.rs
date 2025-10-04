// 模块声明
pub mod ai_optimizer;
pub mod c2rust_translator;
pub mod code_splitter;
pub mod file_processor;
pub mod rust_verifier;
pub mod single_processes;

// 导出主要公共接口（保持向后兼容）
pub use single_processes::{singlefile_processor, two_stage_processor};

// 导出各模块的公共函数
pub use file_processor::{create_rust_project_structure, process_c_h_files};
pub use rust_verifier::{extract_key_errors, verify_and_fix, verify_compilation};
