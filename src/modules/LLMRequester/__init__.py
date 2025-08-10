"""
LLMRequester模块 - LLM请求处理模块

这个模块提供了与各种LLM服务交互的功能：
- 支持多种LLM提供商（OpenAI、Anthropic、Cohere、Google、Amazon Bedrock等）
- 统一的客户端工厂模式
- 本地和云端LLM服务支持
- 自动重试和错误处理
- 连接池和HTTP/2支持

主要组件:
- LLMRequester: 主请求器，统一的LLM请求入口
- LLMClientFactory: LLM客户端工厂，管理和缓存不同的LLM客户端
- OpenaiRequester: OpenAI API请求器
- LocalLLMRequester: 本地LLM请求器，处理本地LLM服务请求
- AnthropicRequester: Anthropic Claude API请求器
- CohereRequester: Cohere API请求器
- GoogleRequester: Google Gemini API请求器
- AmazonbedrockRequester: Amazon Bedrock API请求器
- SakuraRequester: Sakura API请求器
- DashscopeRequester: 阿里云DashScope API请求器

使用示例:
```python
from src.modules.LLMRequester import LLMRequester, LLMClientFactory

# 创建主请求器
requester = LLMRequester()

# 配置OpenAI服务
openai_config = {
    "target_platform": "openai",
    "api_key": "your_api_key",
    "api_url": "https://api.openai.com/v1",
    "model_name": "gpt-4o-mini",
    "temperature": 0.7,
    "request_timeout": 60
}

# 配置本地LLM服务
local_config = {
    "target_platform": "LocalLLM",
    "api_key": "none_api_key",
    "api_url": "http://localhost:8000/v1",
    "model_name": "deepseek-r1:7b",
    "temperature": 0.7,
    "request_timeout": 60,
    "think_switch": True
}

# 发送请求
messages = [{"role": "user", "content": "Hello, convert this C code to Rust"}]
system_prompt = "You are a helpful assistant for C to Rust conversion."

error, think, content, prompt_tokens, completion_tokens = requester.sent_request(
    messages, system_prompt, openai_config
)

if not error:
    print(f"Response: {content}")
    if think:
        print(f"Thinking process: {think}")
    print(f"Tokens: {prompt_tokens} in, {completion_tokens} out")
```

C到Rust转换专用示例:
```python
# C到Rust转换系统提示词
c_to_rust_prompt = '''You are an expert in converting C code to Rust.
Convert the provided C code to safe, idiomatic Rust code.
Focus on:
1. Memory safety without sacrificing performance
2. Proper error handling
3. Idiomatic Rust patterns
4. Clear documentation
'''

c_code = '''
#include <stdio.h>
#include <stdlib.h>

int add(int a, int b) {
    return a + b;
}

int main() {
    int result = add(3, 4);
    printf("Result: %d\\n", result);
    return 0;
}
'''

messages = [
    {
        "role": "user",
        "content": f"Convert this C code to Rust:\\n\\n```c\\n{c_code}\\n```"
    }
]

error, think, rust_code, prompt_tokens, completion_tokens = requester.sent_request(
    messages, c_to_rust_prompt, local_config
)

if not error:
    print("Converted Rust code:")
    print(rust_code)
```
"""

from .LLMRequester import LLMRequester
from .LLMClientFactory import LLMClientFactory, create_httpx_client
from .OpenaiRequester import OpenaiRequester
from .LocalLLMRequester import LocalLLMRequester
from .AnthropicRequester import AnthropicRequester, is_claude3_model, is_claude35_model
from .CohereRequester import CohereRequester
from .GoogleRequester import GoogleRequester
from .AmazonbedrockRequester import AmazonbedrockRequester
from .SakuraRequester import SakuraRequester
from .DashscopeRequester import DashscopeRequester

__all__ = [
    # 主要类
    "LLMRequester",
    "LLMClientFactory",

    # 具体请求器
    "OpenaiRequester",
    "LocalLLMRequester",
    "AnthropicRequester",
    "CohereRequester",
    "GoogleRequester",
    "AmazonbedrockRequester",
    "SakuraRequester",
    "DashscopeRequester",

    # 工具函数
    "create_httpx_client",
    "is_claude3_model",
    "is_claude35_model",
]

# 版本信息
__version__ = "0.1.0"
__author__ = "C2Rust Agent Team"
__description__ = "LLM request handling module for C to Rust conversion"

# 支持的平台列表
SUPPORTED_PLATFORMS = [
    "openai",
    "LocalLLM",
    "anthropic",
    "cohere",
    "google",
    "amazonbedrock",
    "sakura",
    "dashscope"
]

# 默认配置模板
DEFAULT_CONFIGS = {
    "openai": {
        "target_platform": "openai",
        "api_format": "OpenAI",
        "api_key": "",
        "api_url": "https://api.openai.com/v1",
        "model_name": "gpt-4o-mini",
        "temperature": 0.7,
        "top_p": 1.0,
        "frequency_penalty": 0,
        "presence_penalty": 0,
        "request_timeout": 60,
        "max_tokens": 4096,
        "think_switch": False
    },

    "LocalLLM": {
        "target_platform": "LocalLLM",
        "api_format": "OpenAI",
        "api_key": "none_api_key",
        "api_url": "http://localhost:8000/v1",
        "model_name": "deepseek-r1:7b",
        "temperature": 0.7,
        "top_p": 1.0,
        "frequency_penalty": 0,
        "request_timeout": 60,
        "max_tokens": 4096,
        "think_switch": True
    },

    "anthropic": {
        "target_platform": "anthropic",
        "api_format": "Anthropic",
        "api_key": "",
        "api_url": "https://api.anthropic.com",
        "model_name": "claude-3-5-sonnet-20241022",
        "temperature": 0.7,
        "top_p": 1.0,
        "request_timeout": 60,
        "max_tokens": 4096
    },

    "cohere": {
        "target_platform": "cohere",
        "api_format": "Cohere",
        "api_key": "",
        "api_url": "https://api.cohere.ai/v1",
        "model_name": "command-r-plus",
        "temperature": 0.7,
        "top_p": 1.0,
        "request_timeout": 60,
        "max_tokens": 4096
    },

    "google": {
        "target_platform": "google",
        "api_format": "Google",
        "api_key": "",
        "api_url": "",
        "model_name": "gemini-1.5-pro-002",
        "temperature": 0.7,
        "top_p": 1.0,
        "request_timeout": 60,
        "max_tokens": 4096,
        "extra_body": {}
    },

    "amazonbedrock": {
        "target_platform": "amazonbedrock",
        "api_format": "Bedrock",
        "region": "us-east-1",
        "access_key": "",
        "secret_key": "",
        "model_name": "anthropic.claude-3-5-sonnet-20241022-v2:0",
        "temperature": 0.7,
        "top_p": 1.0,
        "request_timeout": 60,
        "max_tokens": 4096
    }
}

def get_default_config(platform: str) -> dict:
    """
    获取指定平台的默认配置

    Args:
        platform: 平台名称

    Returns:
        dict: 默认配置字典
    """
    return DEFAULT_CONFIGS.get(platform, {}).copy()

def get_supported_platforms() -> list:
    """获取支持的平台列表"""
    return SUPPORTED_PLATFORMS.copy()
