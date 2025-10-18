// ai_checker.rs
use anyhow::Result;
use llm_requester::pkg_config::get_config;
use llm_requester::{deepseek_provider, ollama_provider, openai_provider, xai_provider};
use log::{info, warn};

/// AI service connection status enum
#[derive(Debug, Clone, PartialEq)]
pub enum AIConnectionStatus {
    /// All configured AI services are connected normally
    AllConnected,
    /// Only some AI services are connected normally
    PartiallyConnected,
    /// All AI services failed to connect
    AllDisconnected,
    /// Connection status unknown
    Unknown,
    /// Status of specific service
    ServiceStatus {
        deepseek: bool,
        ollama: bool,
        openai: bool,
        xai: bool,
    },
}

/// Initialize AI services and check connection status
pub async fn ai_service_init() -> Result<AIConnectionStatus> {
    info!("Starting AI service connection initialization...");
    // Interactive info: Prompt user in terminal about current operation
    println!("Checking AI service connections...");

    // Get configuration
    let config = match get_config() {
        Ok(config) => config,
        Err(e) => {
            warn!("Failed to get AI configuration: {}", e);
            // Interactive info
            println!("Failed to get AI configuration: {}", e);
            return Ok(AIConnectionStatus::Unknown);
        }
    };

    // Check connection status of currently configured provider
    let provider_status = match check_provider_connection(&config.provider).await {
        Ok(status) => status,
        Err(e) => {
            warn!("Failed to check {} connection: {}", config.provider, e);
            // Interactive info
            println!("Failed to check {} connection: {}", config.provider, e);
            false
        }
    };

    // Determine overall status based on configured provider status
    let connection_status = if provider_status {
        AIConnectionStatus::AllConnected
    } else {
        AIConnectionStatus::AllDisconnected
    };

    // Log connection status
    match connection_status {
        AIConnectionStatus::AllConnected => {
            info!(
                "AI service connection normal: {} connected",
                config.provider
            );
            // Interactive info
            println!(
                "AI service connection normal: {} connected",
                config.provider
            );
        }
        AIConnectionStatus::AllDisconnected => {
            warn!(
                "AI service connection failed: {} not connected",
                config.provider
            );
            // Interactive info
            println!(
                "AI service connection failed: {} not connected",
                config.provider
            );
        }
        _ => {
            info!("AI service connection status: {:?}", connection_status);
        }
    }

    Ok(connection_status)
}

/// Check connection status of all AI services
pub async fn check_all_ai_services() -> Result<AIConnectionStatus> {
    info!("Checking connection status of all AI services...");

    // Check all services in parallel
    let (deepseek_result, ollama_result, openai_result, xai_result) = tokio::join!(
        check_deepseek_connection(),
        check_ollama_connection(),
        check_openai_connection(),
        check_xai_connection()
    );

    let deepseek_connected = deepseek_result.is_ok();
    let ollama_connected = ollama_result.is_ok();
    let openai_connected = openai_result.is_ok();
    let xai_connected = xai_result.is_ok();

    // Log status
    info!(
        "AI service connection status: DeepSeek({}), Ollama({}), OpenAI({}), XAI({})",
        deepseek_connected, ollama_connected, openai_connected, xai_connected
    );

    Ok(AIConnectionStatus::ServiceStatus {
        deepseek: deepseek_connected,
        ollama: ollama_connected,
        openai: openai_connected,
        xai: xai_connected,
    })
}

/// Check connection status of specific provider
async fn check_provider_connection(provider: &str) -> Result<bool> {
    info!("Checking connection status of {} service...", provider);

    let test_messages = vec!["Hello".to_string()];

    match provider {
        "deepseek" => {
            let result = deepseek_provider::DeepSeekProvider::get_llm_request(test_messages).await;
            result.map(|_| true).map_err(|e| e.into())
        }
        "ollama" => {
            let result = ollama_provider::OllamaProvider::get_llm_request(test_messages).await;
            result.map(|_| true).map_err(|e| e.into())
        }
        "openai" => {
            let result = openai_provider::OpenAIProvider::get_llm_request(test_messages).await;
            result.map(|_| true).map_err(|e| e.into())
        }
        "xai" => {
            let result = xai_provider::XAIProvider::get_llm_request(test_messages).await;
            result.map(|_| true).map_err(|e| e.into())
        }
        _ => {
            warn!("Unknown AI service provider: {}", provider);
            Ok(false)
        }
    }
}

/// Check DeepSeek service connection status
async fn check_deepseek_connection() -> Result<()> {
    let test_messages = vec!["Hello".to_string()];
    deepseek_provider::DeepSeekProvider::get_llm_request(test_messages)
        .await
        .map(|_| ())
}

/// Check Ollama service connection status
async fn check_ollama_connection() -> Result<()> {
    let test_messages = vec!["Hello".to_string()];
    ollama_provider::OllamaProvider::get_llm_request(test_messages)
        .await
        .map(|_| ())
}

/// Check OpenAI service connection status
async fn check_openai_connection() -> Result<()> {
    let test_messages = vec!["Hello".to_string()];
    openai_provider::OpenAIProvider::get_llm_request(test_messages)
        .await
        .map(|_| ())
}

/// Check XAI service connection status
async fn check_xai_connection() -> Result<()> {
    let test_messages = vec!["Hello".to_string()];
    xai_provider::XAIProvider::get_llm_request(test_messages)
        .await
        .map(|_| ())
}

/// 获取详细的 AI 服务状态信息
pub async fn get_detailed_ai_status() -> Result<String> {
    info!("获取详细的 AI 服务状态信息...");

    let config = match get_config() {
        Ok(config) => config,
        Err(e) => return Ok(format!("获取配置失败: {}", e)),
    };

    let (deepseek_result, ollama_result, openai_result, xai_result) = tokio::join!(
        check_deepseek_connection(),
        check_ollama_connection(),
        check_openai_connection(),
        check_xai_connection()
    );

    let status_info = format!(
        "Currently configured AI service: {}\n\
         DeepSeek status: {}\n\
         Ollama status: {}\n\
         OpenAI status: {}\n\
         XAI status: {}",
        config.provider,
        if deepseek_result.is_ok() {
            "Connected"
        } else {
            "Connection failed"
        },
        if ollama_result.is_ok() {
            "Connected"
        } else {
            "Connection failed"
        },
        if openai_result.is_ok() {
            "Connected"
        } else {
            "Connection failed"
        },
        if xai_result.is_ok() {
            "Connected"
        } else {
            "Connection failed"
        }
    );

    Ok(status_info)
}
