from typing import Dict, List, Tuple, Any, Optional
from ...base.Base import Base
from .LLMClientFactory import LLMClientFactory


class DashscopeRequester(Base):
    """DashScope API请求器 - 阿里云灵积模型服务"""

    def __init__(self) -> None:
        super().__init__()

    def request_dashscope(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起DashScope请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            model_name = platform_config.get("model_name", "qwen-turbo")
            request_timeout = platform_config.get("request_timeout", 60)
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            max_tokens = platform_config.get("max_tokens", 4096)
            api_key = platform_config.get("api_key", "")

            if not api_key:
                self.error("DashScope API密钥未配置")
                return True, "", "", 0, 0

            # 准备消息
            dashscope_messages = []

            # 添加系统消息
            if system_prompt:
                dashscope_messages.append({
                    "role": "system",
                    "content": system_prompt
                })

            # 添加对话消息
            for msg in messages:
                role = msg.get("role", "")
                content = msg.get("content", "")

                if role in ["user", "assistant", "system"]:
                    dashscope_messages.append({
                        "role": role,
                        "content": content
                    })

            # 构建请求参数
            request_params = {
                "model": model_name,
                "messages": dashscope_messages,
                "result_format": "message"
            }

            # 添加可选参数
            if temperature != 1.0:
                request_params["temperature"] = temperature

            if top_p != 1.0:
                request_params["top_p"] = top_p

            if max_tokens:
                request_params["max_tokens"] = max_tokens

            self.debug(f"发送DashScope请求: {model_name}")

            # 使用dashscope库发送请求
            try:
                import dashscope
                from dashscope import Generation
                from typing import cast, Any

                # 设置API密钥
                dashscope.api_key = api_key

                # 发送请求
                raw_response = Generation.call(**request_params)
                
                # 使用类型转换来处理响应
                response = cast(Any, raw_response)

            except ImportError:
                self.error("dashscope库未安装，请运行: pip install dashscope")
                return True, "", "", 0, 0

            # 检查响应是否存在
            if response is None:
                self.error("DashScope请求失败：无响应")
                return True, "", "", 0, 0

            # 检查响应状态
            try:
                if hasattr(response, 'status_code') and response.status_code != 200:
                    self.error(f"DashScope请求失败，状态码: {response.status_code}")
                    return True, "", "", 0, 0
                elif hasattr(response, 'code') and response.code != 'Success':
                    self.error(f"DashScope请求失败，错误码: {response.code}")
                    return True, "", "", 0, 0
            except Exception:
                # 如果无法检查状态，继续处理
                pass

            # 提取回复内容
            response_think = ""
            response_content = ""

            try:
                if hasattr(response, 'output') and response.output:
                    if hasattr(response.output, 'text'):
                        response_content = response.output.text
                    elif hasattr(response.output, 'choices') and response.output.choices:
                        choice = response.output.choices[0]
                        if hasattr(choice, 'message') and hasattr(choice.message, 'content'):
                            response_content = choice.message.content
                        else:
                            response_content = str(choice)
                    else:
                        response_content = str(response.output)
            except Exception:
                response_content = str(response)

            # 确保response_content是字符串
            if not isinstance(response_content, str):
                response_content = str(response_content)

            # 尝试提取思考过程
            if response_content and "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # 获取token消耗
            prompt_tokens = 0
            completion_tokens = 0

            try:
                if hasattr(response, 'usage') and response.usage:
                    try:
                        if hasattr(response.usage, 'input_tokens'):
                            prompt_tokens = int(response.usage.input_tokens)
                        elif hasattr(response.usage, 'prompt_tokens'):
                            prompt_tokens = int(response.usage.prompt_tokens)
                    except (AttributeError, TypeError, ValueError):
                        prompt_tokens = 0

                    try:
                        if hasattr(response.usage, 'output_tokens'):
                            completion_tokens = int(response.usage.output_tokens)
                        elif hasattr(response.usage, 'completion_tokens'):
                            completion_tokens = int(response.usage.completion_tokens)
                    except (AttributeError, TypeError, ValueError):
                        completion_tokens = 0
            except Exception:
                # 如果无法获取token信息，使用默认值
                pass

            self.debug(f"DashScope请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"DashScope请求失败: {e}")
            return True, "", "", 0, 0

    def request_openai(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        兼容方法：使用OpenAI格式发起DashScope请求
        这是为了保持向后兼容性
        """
        return self.request_dashscope(messages, system_prompt, platform_config)

    def validate_config(self, platform_config: Dict[str, Any]) -> bool:
        """
        验证DashScope配置

        Args:
            platform_config: 平台配置字典

        Returns:
            bool: 配置是否有效
        """
        required_fields = ["api_key"]

        for field in required_fields:
            if field not in platform_config or not platform_config[field]:
                self.error(f"DashScope配置缺少必需字段: {field}")
                return False

        return True

    def get_supported_models(self) -> List[str]:
        """获取支持的模型列表"""
        return [
            "qwen-turbo",
            "qwen-plus",
            "qwen-max",
            "qwen-max-longcontext",
            "qwen1.5-72b-chat",
            "qwen1.5-14b-chat",
            "qwen1.5-7b-chat",
            "qwen2-72b-instruct",
            "qwen2-7b-instruct",
            "qwen2-1.5b-instruct",
            "qwen2-0.5b-instruct"
        ]
