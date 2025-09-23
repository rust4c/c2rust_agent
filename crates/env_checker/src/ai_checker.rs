// ai_checker.rs
use anyhow::Result;
use llm_requester::pkg_config::get_config;
use llm_requester::{deepseek_provider, ollama_provider, openai_provider, xai_provider};
use log::{info, warn};

/// AI 服务连接状态枚举
#[derive(Debug, Clone, PartialEq)]
pub enum AIConnectionStatus {
    /// 所有配置的 AI 服务都连接正常
    AllConnected,
    /// 只有部分 AI 服务连接正常
    PartiallyConnected,
    /// 所有 AI 服务都连接失败
    AllDisconnected,
    /// 连接状态未知
    Unknown,
    /// 特定服务的状态
    ServiceStatus {
        deepseek: bool,
        ollama: bool,
        openai: bool,
        xai: bool,
    },
}

/// 初始化 AI 服务并检查连接状态
pub async fn ai_service_init() -> Result<AIConnectionStatus> {
    info!("开始初始化 AI 服务连接...");

    // 获取配置
    let config = match get_config() {
        Ok(config) => config,
        Err(e) => {
            warn!("获取 AI 配置失败: {}", e);
            return Ok(AIConnectionStatus::Unknown);
        }
    };

    // 检查当前配置的 provider 的连接状态
    let provider_status = match check_provider_connection(&config.provider).await {
        Ok(status) => status,
        Err(e) => {
            warn!("检查 {} 连接失败: {}", config.provider, e);
            false
        }
    };

    // 根据配置的 provider 状态确定整体状态
    let connection_status = if provider_status {
        AIConnectionStatus::AllConnected
    } else {
        AIConnectionStatus::AllDisconnected
    };

    // 记录连接状态
    match connection_status {
        AIConnectionStatus::AllConnected => info!("AI 服务连接正常: {} 已连接", config.provider),
        AIConnectionStatus::AllDisconnected => warn!("AI 服务连接失败: {} 未连接", config.provider),
        _ => info!("AI 服务连接状态: {:?}", connection_status),
    }

    Ok(connection_status)
}

/// 检查所有 AI 服务的连接状态
pub async fn check_all_ai_services() -> Result<AIConnectionStatus> {
    info!("检查所有 AI 服务的连接状态...");

    // 并行检查所有服务
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

    // 记录状态
    info!(
        "AI 服务连接状态: DeepSeek({}), Ollama({}), OpenAI({}), XAI({})",
        deepseek_connected, ollama_connected, openai_connected, xai_connected
    );

    Ok(AIConnectionStatus::ServiceStatus {
        deepseek: deepseek_connected,
        ollama: ollama_connected,
        openai: openai_connected,
        xai: xai_connected,
    })
}

/// 检查特定 provider 的连接状态
async fn check_provider_connection(provider: &str) -> Result<bool> {
    info!("检查 {} 服务的连接状态...", provider);

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
            warn!("未知的 AI 服务提供商: {}", provider);
            Ok(false)
        }
    }
}

/// 检查 DeepSeek 服务连接状态
async fn check_deepseek_connection() -> Result<()> {
    let test_messages = vec!["Hello".to_string()];
    deepseek_provider::DeepSeekProvider::get_llm_request(test_messages)
        .await
        .map(|_| ())
}

/// 检查 Ollama 服务连接状态
async fn check_ollama_connection() -> Result<()> {
    let test_messages = vec!["Hello".to_string()];
    ollama_provider::OllamaProvider::get_llm_request(test_messages)
        .await
        .map(|_| ())
}

/// 检查 OpenAI 服务连接状态
async fn check_openai_connection() -> Result<()> {
    let test_messages = vec!["Hello".to_string()];
    openai_provider::OpenAIProvider::get_llm_request(test_messages)
        .await
        .map(|_| ())
}

/// 检查 XAI 服务连接状态
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
        "当前配置的 AI 服务: {}\n\
         DeepSeek 状态: {}\n\
         Ollama 状态: {}\n\
         OpenAI 状态: {}\n\
         XAI 状态: {}",
        config.provider,
        if deepseek_result.is_ok() {
            "连接正常"
        } else {
            "连接失败"
        },
        if ollama_result.is_ok() {
            "连接正常"
        } else {
            "连接失败"
        },
        if openai_result.is_ok() {
            "连接正常"
        } else {
            "连接失败"
        },
        if xai_result.is_ok() {
            "连接正常"
        } else {
            "连接失败"
        }
    );

    Ok(status_info)
}
