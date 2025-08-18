//! Translation module for C to Rust conversion
//!
//! This module contains functionality for translating C code to Rust,
//! including template generation and prompt building.

pub mod prompt;
pub mod templates;

pub use prompt::{
    build_basic_prompt, build_context_aware_prompt, build_context_aware_prompt_with_retry,
    build_enhanced_basic_prompt, parse_llm_json_response, TranslationResult,
};
pub use templates::{
    generate_library_template, generate_multi_module_template, generate_project_template,
    generate_single_file_template,
};
