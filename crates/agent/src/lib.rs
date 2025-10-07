pub mod agent;
pub mod pkg_config;

// Re-export main types for convenience
pub use agent::{
    Agent, AgentMessage, AgentStatus, CompilationStatus, ErrorLocation, MessageType, ProjectConfig,
    SourceInfo, TranslationResult,
};
