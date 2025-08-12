use crate::llm_factory::LLMFactory;

struct AgentFactory {
    llm_factory: LLMFactory,
}

impl AgentFactory {
    pub fn new(llm_factory: LLMFactory) -> Self {
        Self { llm_factory }
    }
}
