from typing import Dict, List, Tuple, Any, Optional
from ...base.Base import Base
from .LLMClientFactory import LLMClientFactory


class CohereRequester(Base):
    """Cohere API请求器"""

    def __init__(self) -> None:
        super().__init__()

    def request_cohere(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起Cohere请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            model_name = platform_config.get("model_name", "command-r-plus")
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            presence_penalty = platform_config.get("presence_penalty", 0)
            frequency_penalty = platform_config.get("frequency_penalty", 0)
            request_timeout = platform_config.get("request_timeout", 60)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 准备消息 - Cohere使用不同的消息格式
            cohere_messages = []

            # 添加系统消息（如果有）
            if system_prompt:
                cohere_messages.append({
                    "role": "system",
                    "content": system_prompt
                })

            # 添加对话消息
            for msg in messages:
                if msg.get("role") in ["user", "assistant"]:
                    cohere_messages.append({
                        "role": msg["role"],
                        "content": msg["content"]
                    })

            # 参数基础配置
            base_params = {
                "model": model_name,
                "messages": cohere_messages,
                "max_tokens": max_tokens,
            }

            # 按需添加参数
            if temperature != 1.0:
                base_params["temperature"] = temperature

            if top_p != 1.0:
                base_params["top_p"] = top_p

            if presence_penalty != 0:
                base_params["presence_penalty"] = presence_penalty

            if frequency_penalty != 0:
                base_params["frequency_penalty"] = frequency_penalty

            self.debug(f"发送Cohere请求: {model_name}")

            # 从工厂获取客户端
            client = LLMClientFactory().get_cohere_client(platform_config)

            # 发送请求
            response = client.chat(**base_params)

            # 提取回复内容
            response_think = ""
            response_content = ""

            try:
                if hasattr(response, 'message') and response.message:
                    if hasattr(response.message, 'content') and response.message.content:
                        if isinstance(response.message.content, list) and len(response.message.content) > 0:
                            # 提取第一个文本内容
                            first_content = response.message.content[0]
                            # 使用安全的属性访问
                            if hasattr(first_content, 'text'):
                                response_content = str(getattr(first_content, 'text', ''))
                            elif hasattr(first_content, 'content'):
                                response_content = str(getattr(first_content, 'content', ''))
                            else:
                                response_content = str(first_content)
                        else:
                            response_content = str(response.message.content)
                    else:
                        response_content = str(response.message)
                else:
                    response_content = str(response)
            except Exception:
                response_content = str(response)

            # 尝试提取思考过程
            if "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # 获取token消耗
            prompt_tokens = 0
            completion_tokens = 0

            try:
                if hasattr(response, 'usage') and response.usage:
                    # 使用getattr来安全地访问属性
                    input_tokens = getattr(response.usage, 'input_tokens', None) or \
                                 getattr(response.usage, 'prompt_tokens', None) or \
                                 getattr(response.usage, 'billed_input_tokens', None)
                    if input_tokens is not None:
                        prompt_tokens = int(input_tokens)

                    output_tokens = getattr(response.usage, 'output_tokens', None) or \
                                  getattr(response.usage, 'completion_tokens', None) or \
                                  getattr(response.usage, 'billed_output_tokens', None)
                    if output_tokens is not None:
                        completion_tokens = int(output_tokens)

                # 尝试从meta中获取（某些版本的API可能使用这种结构）
                elif hasattr(response, 'meta'):
                    meta_usage = getattr(response, 'meta', None)
                    if meta_usage and hasattr(meta_usage, 'usage'):
                        usage = getattr(meta_usage, 'usage', None)
                        if usage:
                            input_tokens = getattr(usage, 'input_tokens', None) or \
                                         getattr(usage, 'billed_input_tokens', None)
                            if input_tokens is not None:
                                prompt_tokens = int(input_tokens)
                            
                            output_tokens = getattr(usage, 'output_tokens', None) or \
                                          getattr(usage, 'billed_output_tokens', None)
                            if output_tokens is not None:
                                completion_tokens = int(output_tokens)
            except (AttributeError, TypeError, ValueError):
                # 如果无法获取token信息，使用默认值
                pass

            self.debug(f"Cohere请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Cohere请求失败: {e}")
            return True, "", "", 0, 0
