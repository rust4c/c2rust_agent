use rig::client::ProviderClient;
use rig::completion::Prompt;
use rig::providers::{deepseek, gemini, mistral, ollama, openai};

pub struct LLMFactory {
    provider: String,
}

impl LLMFactory {
    pub fn new(provider: String) -> Self {
        LLMFactory { provider }
    }

    pub fn create_llm(&self, api_key: &str) -> Box<dyn ProviderClient> {
        match self.provider.as_str() {
            "deepseek" => Box::new(deepseek::Client::new(api_key)),
            "gemini" => Box::new(gemini::Client::new(api_key)),
            "mistral" => Box::new(mistral::Client::new(api_key)),
            "ollama" => Box::new(ollama::Client::new()),
            "openai" => Box::new(openai::Client::new(api_key)),
            _ => panic!("Unsupported provider"),
        }
    }
}
