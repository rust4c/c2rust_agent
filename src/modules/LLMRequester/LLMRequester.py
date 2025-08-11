from typing import Dict, List, Tuple, Any, Optional
from ...base.Base import Base
from .OpenaiRequester import OpenaiRequester
from .LocalLLMRequester import LocalLLMRequester
from .AnthropicRequester import AnthropicRequester
from .CohereRequester import CohereRequester
from .GoogleRequester import GoogleRequester
from .AmazonbedrockRequester import AmazonbedrockRequester
from .SakuraRequester import SakuraRequester
from .DashscopeRequester import DashscopeRequester


class LLMRequester(Base):
    """
    LLM请求器主类 - 统一的LLM请求入口

    支持多种LLM提供商：
    - OpenAI (包括兼容的API)
    - Anthropic Claude
    - Google Gemini
    - Cohere
    - Amazon Bedrock
    - 本地LLM服务
    - Sakura (特殊格式)
    - DashScope (阿里云)
    """

    def __init__(self) -> None:
        super().__init__()
        self.info("LLM请求器初始化完成")

    def sent_request(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发送LLM请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            # 获取平台参数
            target_platform = platform_config.get("target_platform", "openai")
            api_format = platform_config.get("api_format", "OpenAI")

            self.debug(f"使用LLM平台: {target_platform}, API格式: {api_format}")

            # 根据平台选择对应的请求器
            if target_platform == "openai_local":
                # openai_local 应该使用 OpenaiRequester，但使用本地配置
                requester = OpenaiRequester()
                return requester.request_openai(messages, system_prompt, platform_config)

            elif target_platform == "openai" or target_platform.startswith("openai"):
                requester = OpenaiRequester()
                return requester.request_openai(messages, system_prompt, platform_config)

            elif target_platform == "LocalLLM" or target_platform == "local":
                requester = LocalLLMRequester()
                return requester.request_LocalLLM(messages, system_prompt, platform_config)

            elif target_platform == "anthropic" or (target_platform.startswith("custom_platform_") and api_format == "Anthropic"):
                requester = AnthropicRequester()
                return requester.request_anthropic(messages, system_prompt, platform_config)

            elif target_platform == "cohere":
                requester = CohereRequester()
                return requester.request_cohere(messages, system_prompt, platform_config)

            elif target_platform == "google" or (target_platform.startswith("custom_platform_") and api_format == "Google"):
                requester = GoogleRequester()
                return requester.request_google(messages, system_prompt, platform_config)

            elif target_platform == "amazonbedrock" or target_platform == "bedrock":
                requester = AmazonbedrockRequester()
                return requester.request_amazonbedrock(messages, system_prompt, platform_config)

            elif target_platform == "sakura":
                requester = SakuraRequester()
                return requester.request_sakura(messages, system_prompt, platform_config)

            elif target_platform == "dashscope":
                requester = DashscopeRequester()
                return requester.request_dashscope(messages, system_prompt, platform_config)

            else:
                # 默认使用OpenAI格式
                self.warning(f"未知的平台类型 '{target_platform}'，使用默认OpenAI请求器")
                requester = OpenaiRequester()
                return requester.request_openai(messages, system_prompt, platform_config)

        except Exception as e:
            self.error(f"LLM请求分发失败: {e}")
            return True, None, None, None, None

    def get_supported_platforms(self) -> List[str]:
        """获取支持的平台列表"""
        return [
            "openai",
            "LocalLLM",
            "anthropic",
            "cohere",
            "google",
            "amazonbedrock",
            "sakura",
            "dashscope"
        ]

    def validate_config(self, platform_config: Dict[str, Any]) -> bool:
        """
        验证平台配置

        Args:
            platform_config: 平台配置字典

        Returns:
            bool: 配置是否有效
        """
        required_fields = ["target_platform"]

        for field in required_fields:
            if field not in platform_config:
                self.error(f"配置缺少必需字段: {field}")
                return False

        target_platform = platform_config.get("target_platform")
        if target_platform not in self.get_supported_platforms():
            self.warning(f"未知的平台类型: {target_platform}")
            # 不返回False，允许使用默认处理器

        return True
