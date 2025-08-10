from typing import Dict, List, Tuple, Any, Optional
from ...base.Base import Base
from .LLMClientFactory import LLMClientFactory


class OpenaiRequester(Base):
    """OpenAI API请求器"""

    def __init__(self) -> None:
        super().__init__()

    def request_openai(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起OpenAI请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            # 获取具体配置
            model_name = platform_config.get("model_name", "gpt-4o-mini")
            request_timeout = platform_config.get("request_timeout", 60)
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            presence_penalty = platform_config.get("presence_penalty", 0)
            frequency_penalty = platform_config.get("frequency_penalty", 0)
            extra_body = platform_config.get("extra_body", {})
            think_switch = platform_config.get("think_switch", False)
            think_depth = platform_config.get("think_depth", "medium")

            # 插入系统消息
            if system_prompt:
                messages.insert(0, {
                    "role": "system",
                    "content": system_prompt
                })

            # 从工厂获取客户端
            client = LLMClientFactory().get_openai_client(platform_config)

            # 针对推理模型的特殊处理
            reasoning_models = {"deepseek-reasoner", "deepseek-r1", "DeepSeek-R1", "o1-mini", "o1-preview"}
            if model_name in reasoning_models:
                # 检查最后的消息是否为用户消息
                if isinstance(messages[-1], dict) and messages[-1].get('role') != 'user':
                    messages = messages[:-1]

            # 参数基础配置
            base_params = {
                "model": model_name,
                "messages": messages,
                "timeout": request_timeout,
                "stream": False
            }

            # 添加extra_body参数
            if extra_body:
                base_params["extra_body"] = extra_body

            # 按需添加参数
            if temperature != 1.0:
                base_params["temperature"] = temperature

            if top_p != 1.0:
                base_params["top_p"] = top_p

            if presence_penalty != 0:
                base_params["presence_penalty"] = presence_penalty

            if frequency_penalty != 0:
                base_params["frequency_penalty"] = frequency_penalty

            # 开启思考开关时添加参数（针对支持推理的模型）
            if think_switch and model_name in reasoning_models:
                base_params["reasoning_effort"] = think_depth

            self.debug(f"发送OpenAI请求: {model_name}")

            # 发起请求
            response = client.chat.completions.create(**base_params)

            # 提取回复内容
            message = response.choices[0].message

            # 自适应提取推理过程
            response_think = ""
            response_content = message.content

            # 检查是否包含<think>标签格式的推理内容
            if "</think>" in message.content:
                parts = message.content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()
            else:
                # 检查是否有reasoning_content属性（OpenAI o1系列模型）
                try:
                    if hasattr(message, 'reasoning_content') and message.reasoning_content:
                        response_think = message.reasoning_content
                except Exception:
                    response_think = ""

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

            self.debug(f"OpenAI请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"OpenAI请求失败: {e}")
            return True, None, None, None, None
