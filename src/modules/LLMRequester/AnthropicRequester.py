from typing import Dict, List, Tuple, Any, Optional
from ...base.Base import Base
from .LLMClientFactory import LLMClientFactory


def is_claude3_model(model_name: str) -> bool:
    """判断是否为Claude 3系列模型"""
    if not model_name:
        return False
    return any(variant in model_name.lower() for variant in ["3-haiku", "3-opus", "3-sonnet"])


def is_claude35_model(model_name: str) -> bool:
    """判断是否为Claude 3.5系列模型"""
    if not model_name:
        return False
    return any(variant in model_name.lower() for variant in ["3-5-sonnet", "3-5-haiku"])


class AnthropicRequester(Base):
    """Anthropic Claude API请求器"""

    def __init__(self) -> None:
        super().__init__()

    def request_anthropic(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起Anthropic请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            model_name = platform_config.get("model_name", "claude-3-5-sonnet-20241022")
            request_timeout = platform_config.get("request_timeout", 60)
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            think_switch = platform_config.get("think_switch", False)
            think_depth = platform_config.get("think_depth", "medium")

            # 确定最大token数
            max_tokens = 4096  # 默认值
            if is_claude3_model(model_name):
                max_tokens = 4096
            elif is_claude35_model(model_name):
                max_tokens = 8192
            else:
                max_tokens = 4096  # 保守设置

            # 从配置中获取自定义max_tokens
            if "max_tokens" in platform_config:
                max_tokens = platform_config["max_tokens"]

            # 准备消息（Anthropic不需要在messages中插入system消息）
            anthropic_messages = []
            for msg in messages:
                if msg.get("role") != "system":  # 过滤掉system消息
                    anthropic_messages.append(msg)

            # 参数基础配置
            base_params = {
                "model": model_name,
                "messages": anthropic_messages,
                "max_tokens": max_tokens,
                "timeout": request_timeout
            }

            # 添加系统提示词
            if system_prompt:
                base_params["system"] = system_prompt

            # 按需添加参数
            if temperature != 1.0:
                base_params["temperature"] = temperature

            if top_p != 1.0:
                base_params["top_p"] = top_p

            # 处理思考开关（如果支持）
            if think_switch:
                # Claude目前不支持显式的思考模式，可以在system prompt中添加指令
                if system_prompt:
                    base_params["system"] = f"{system_prompt}\n\nPlease think through your response carefully and show your reasoning process."
                else:
                    base_params["system"] = "Please think through your response carefully and show your reasoning process."

            self.debug(f"发送Anthropic请求: {model_name}")

            # 从工厂获取客户端
            client = LLMClientFactory().get_anthropic_client(platform_config)

            # 发送请求
            response = client.messages.create(**base_params)

            # 提取回复内容
            response_think = ""
            response_content = ""

            # 处理响应内容
            if hasattr(response, 'content') and response.content:
                if isinstance(response.content, list) and len(response.content) > 0:
                    # Claude通常返回一个包含text块的列表
                    text_blocks = [block for block in response.content if hasattr(block, 'text')]
                    if text_blocks:
                        response_content = text_blocks[0].text
                    else:
                        response_content = str(response.content[0])
                else:
                    response_content = str(response.content)

            # 尝试提取思考过程（如果存在）
            if "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # 获取token消耗
            prompt_tokens = 0
            completion_tokens = 0

            if hasattr(response, 'usage') and response.usage:
                try:
                    prompt_tokens = int(response.usage.input_tokens)
                except (AttributeError, TypeError, ValueError):
                    prompt_tokens = 0

                try:
                    completion_tokens = int(response.usage.output_tokens)
                except (AttributeError, TypeError, ValueError):
                    completion_tokens = 0

            self.debug(f"Anthropic请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Anthropic请求失败: {e}")
            return True, None, None, None, None

    def request_anthropic_bedrock(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起Anthropic Bedrock请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            model_name = platform_config.get("model_name", "anthropic.claude-3-5-sonnet-20241022-v2:0")
            request_timeout = platform_config.get("request_timeout", 60)
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 准备消息
            anthropic_messages = []
            for msg in messages:
                if msg.get("role") != "system":
                    anthropic_messages.append(msg)

            # 参数基础配置
            base_params = {
                "model": model_name,
                "messages": anthropic_messages,
                "max_tokens": max_tokens,
                "timeout": request_timeout
            }

            # 添加系统提示词
            if system_prompt:
                base_params["system"] = system_prompt

            # 按需添加参数
            if temperature != 1.0:
                base_params["temperature"] = temperature

            if top_p != 1.0:
                base_params["top_p"] = top_p

            self.debug(f"发送Anthropic Bedrock请求: {model_name}")

            # 从工厂获取Bedrock客户端
            client = LLMClientFactory().get_anthropic_bedrock(platform_config)

            # 发送请求
            response = client.messages.create(**base_params)

            # 提取回复内容
            response_think = ""
            response_content = ""

            if hasattr(response, 'content') and response.content:
                if isinstance(response.content, list) and len(response.content) > 0:
                    text_blocks = [block for block in response.content if hasattr(block, 'text')]
                    if text_blocks:
                        response_content = text_blocks[0].text
                    else:
                        response_content = str(response.content[0])
                else:
                    response_content = str(response.content)

            # 尝试提取思考过程
            if "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # 获取token消耗
            prompt_tokens = 0
            completion_tokens = 0

            if hasattr(response, 'usage') and response.usage:
                try:
                    prompt_tokens = int(response.usage.input_tokens)
                except (AttributeError, TypeError, ValueError):
                    prompt_tokens = 0

                try:
                    completion_tokens = int(response.usage.output_tokens)
                except (AttributeError, TypeError, ValueError):
                    completion_tokens = 0

            self.debug(f"Anthropic Bedrock请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Anthropic Bedrock请求失败: {e}")
            return True, None, None, None, None
