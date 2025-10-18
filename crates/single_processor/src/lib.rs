// Module declarations

pub mod c2rust_translator;

pub mod file_processor;
pub mod pkg_config;
pub mod rust_verifier;
pub mod single_processes;

// Main processing functions
pub use single_processes::{StageCallback, two_stage_processor_with_callback};

// Export public functions from each module
pub use rust_verifier::{extract_key_errors, verify_and_fix, verify_compilation};
