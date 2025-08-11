from typing import Dict, List, Tuple, Any, Optional
from ...base.Base import Base
from .LLMClientFactory import LLMClientFactory


class SakuraRequester(Base):
    """Sakura API请求器 - 支持特殊的Sakura格式请求"""

    def __init__(self) -> None:
        super().__init__()

    def request_sakura(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起Sakura请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            model_name = platform_config.get("model_name", "sakura")
            request_timeout = platform_config.get("request_timeout", 60)
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            frequency_penalty = platform_config.get("frequency_penalty", 0)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 插入系统消息
            if system_prompt:
                messages.insert(0, {
                    "role": "system",
                    "content": system_prompt
                })

            # 从工厂获取客户端（使用OpenAI兼容格式）
            client = LLMClientFactory().get_openai_client_sakura(platform_config)

            # 参数基础配置
            base_params = {
                "model": model_name,
                "messages": messages,
                "timeout": request_timeout,
                "stream": False,
                "max_tokens": max_tokens
            }

            # 按需添加参数
            if temperature != 1.0:
                base_params["temperature"] = temperature

            if top_p != 1.0:
                base_params["top_p"] = top_p

            if frequency_penalty != 0:
                base_params["frequency_penalty"] = frequency_penalty

            self.debug(f"发送Sakura请求: {model_name}")

            # 发起请求
            response = client.chat.completions.create(**base_params)

            # 提取回复内容
            message = response.choices[0].message
            response_content = message.content
            response_think = ""

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
                    prompt_tokens = int(response.usage.prompt_tokens)
                except (AttributeError, TypeError, ValueError):
                    prompt_tokens = 0

                try:
                    completion_tokens = int(response.usage.completion_tokens)
                except (AttributeError, TypeError, ValueError):
                    completion_tokens = 0

            self.debug(f"Sakura请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Sakura请求失败: {e}")
            return True, None, None, None, None

    def format_sakura_messages(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str = ""
    ) -> List[Dict[str, Any]]:
        """
        格式化消息为Sakura特定格式

        Args:
            messages: 原始消息列表
            system_prompt: 系统提示词

        Returns:
            格式化后的消息列表
        """
        formatted_messages = []

        # 添加系统消息
        if system_prompt:
            formatted_messages.append({
                "role": "system",
                "content": system_prompt
            })

        # 处理其他消息
        for msg in messages:
            role = msg.get("role", "")
            content = msg.get("content", "")

            if role in ["user", "assistant"]:
                formatted_messages.append({
                    "role": role,
                    "content": content
                })

        return formatted_messages
