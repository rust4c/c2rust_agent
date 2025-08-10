from typing import Dict, List, Tuple, Any, Optional
from ...base.Base import Base
from .LLMClientFactory import LLMClientFactory
from pydoc import text


class GoogleRequester(Base):
    """Google Gemini API请求器"""

    def __init__(self) -> None:
        super().__init__()

    def request_google(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起Google Gemini请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            model_name = platform_config.get("model_name", "gemini-1.5-pro-002")
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            request_timeout = platform_config.get("request_timeout", 60)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 准备消息 - Google Gemini格式
            gemini_messages = []

            # Google Gemini使用特定的消息格式
            # system消息需要特殊处理
            system_content = ""
            if system_prompt:
                system_content = system_prompt

            # 处理对话消息
            for msg in messages:
                role = msg.get("role", "")
                content = msg.get("content", "")

                if role == "system":
                    # 将system消息合并到system_content中
                    system_content += f"\n{content}"
                elif role == "user":
                    gemini_messages.append({
                        "role": "user",
                        "parts": [{"text": content}]
                    })
                elif role == "assistant":
                    gemini_messages.append({
                        "role": "model",  # Google使用"model"而不是"assistant"
                        "parts": [{"text": content}]
                    })

            # 构建请求参数
            generation_config = {}

            if temperature != 1.0:
                generation_config["temperature"] = temperature

            if top_p != 1.0:
                generation_config["top_p"] = top_p

            if max_tokens:
                generation_config["max_output_tokens"] = max_tokens

            # 构建请求内容
            request_content = {
                "contents": gemini_messages,
                "generation_config": generation_config if generation_config else None
            }

            # 添加系统指令
            if system_content.strip():
                request_content["system_instruction"] = {
                    "parts": [{"text": system_content.strip()}]
                }

            self.debug(f"发送Google Gemini请求: {model_name}")

            # 从工厂获取客户端
            client = LLMClientFactory().get_google_client(platform_config)

            # 发送请求
            response = client.models.generate_content(
                model=model_name,
                **request_content
            )

            # 提取回复内容
            response_think = ""
            response_content = ""

            if hasattr(response, 'candidates') and response.candidates:
                candidate = response.candidates[0]
                if hasattr(candidate, 'content') and candidate.content:
                    if hasattr(candidate.content, 'parts') and candidate.content.parts:
                        # 合并所有parts的文本
                        text_parts = []
                        for part in candidate.content.parts:
                            if hasattr(part, 'text'):
                                text_parts.append(part.text)
                        response_content = "".join(text_parts)
                    else:
                        response_content = str(candidate.content)
                else:
                    # candidate没有content属性时，尝试直接转换为字符串
                    response_content = str(candidate)
            elif hasattr(response, 'text'):
                response_content = response.text
            else:
                response_content = str(response)

            # 尝试提取思考过程
            if response_content and "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()
            
            # 确保response_content不为None
            if response_content is None:
                response_content = ""

            # 获取token消耗
            prompt_tokens = 0
            completion_tokens = 0

            if hasattr(response, 'usage_metadata') and response.usage_metadata:
                try:
                    if hasattr(response.usage_metadata, 'prompt_token_count') and response.usage_metadata.prompt_token_count is not None:
                        prompt_tokens = int(response.usage_metadata.prompt_token_count)
                except (AttributeError, TypeError, ValueError):
                    prompt_tokens = 0

                try:
                    if hasattr(response.usage_metadata, 'candidates_token_count') and response.usage_metadata.candidates_token_count is not None:
                        completion_tokens = int(response.usage_metadata.candidates_token_count)
                except (AttributeError, TypeError, ValueError):
                    completion_tokens = 0

            self.debug(f"Google Gemini请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Google Gemini请求失败: {e}")
            return True, None, None, None, None

    def _convert_messages_to_gemini_format(self, messages: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
        """将标准消息格式转换为Gemini格式"""
        gemini_messages = []

        for msg in messages:
            role = msg.get("role", "")
            content = msg.get("content", "")

            if role == "user":
                gemini_messages.append({
                    "role": "user",
                    "parts": [{"text": content}]
                })
            elif role == "assistant":
                gemini_messages.append({
                    "role": "model",
                    "parts": [{"text": content}]
                })
            # system消息在其他地方处理

        return gemini_messages
