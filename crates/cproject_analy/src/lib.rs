pub mod file_remanager;
use file_remanager::CProjectPreprocessor;

use db_services::DatabaseManager;
use llm_requester::llm_request_with_prompt;
use prompt_builder::PromptBuilder;

use indicatif::{MultiProgress, ProgressBar};
use log::{error, info};

pub struct PreProcessor {
    // Fields and methods for the PreProcessor struct
}
