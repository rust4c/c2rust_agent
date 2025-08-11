from typing import Dict, List, Tuple, Any, Optional
import json
from ...base.Base import Base
from .LLMClientFactory import LLMClientFactory


class AmazonbedrockRequester(Base):
    """Amazon Bedrock API请求器"""

    def __init__(self) -> None:
        super().__init__()

    def request_amazonbedrock(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """
        发起Amazon Bedrock请求

        Args:
            messages: 对话消息列表
            system_prompt: 系统提示词
            platform_config: 平台配置

        Returns:
            Tuple[是否出错, 思考过程, 回复内容, 输入tokens, 输出tokens]
        """
        try:
            model_id = platform_config.get("model_name", "anthropic.claude-3-5-sonnet-20241022-v2:0")
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 检查是否使用Anthropic格式的模型
            if "anthropic" in model_id.lower():
                return self._request_anthropic_bedrock(messages, system_prompt, platform_config)
            elif "amazon" in model_id.lower() or "titan" in model_id.lower():
                return self._request_amazon_titan(messages, system_prompt, platform_config)
            elif "meta" in model_id.lower() or "llama" in model_id.lower():
                return self._request_meta_llama(messages, system_prompt, platform_config)
            elif "cohere" in model_id.lower():
                return self._request_cohere_bedrock(messages, system_prompt, platform_config)
            else:
                # 默认使用Anthropic格式
                return self._request_anthropic_bedrock(messages, system_prompt, platform_config)

        except Exception as e:
            self.error(f"Amazon Bedrock请求失败: {e}")
            return True, None, None, None, None

    def _request_anthropic_bedrock(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """使用Anthropic格式的Bedrock请求"""
        try:
            model_id = platform_config.get("model_name", "anthropic.claude-3-5-sonnet-20241022-v2:0")
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 准备Anthropic格式的消息
            anthropic_messages = []
            for msg in messages:
                if msg.get("role") != "system":
                    anthropic_messages.append({
                        "role": msg["role"],
                        "content": msg["content"]
                    })

            # 构建请求体
            request_body = {
                "anthropic_version": "bedrock-2023-05-31",
                "messages": anthropic_messages,
                "max_tokens": max_tokens
            }

            if system_prompt:
                request_body["system"] = system_prompt

            if temperature != 1.0:
                request_body["temperature"] = temperature

            if top_p != 1.0:
                request_body["top_p"] = top_p

            self.debug(f"发送Amazon Bedrock (Anthropic) 请求: {model_id}")

            # 从工厂获取Bedrock客户端
            bedrock_client = LLMClientFactory().get_boto3_bedrock(platform_config)

            # 发送请求
            response = bedrock_client.invoke_model(
                modelId=model_id,
                body=json.dumps(request_body),
                contentType="application/json"
            )

            # 解析响应
            response_body = json.loads(response["body"].read())

            response_think = ""
            response_content = ""

            if "content" in response_body and response_body["content"]:
                if isinstance(response_body["content"], list) and len(response_body["content"]) > 0:
                    response_content = response_body["content"][0].get("text", "")
                else:
                    response_content = str(response_body["content"])

            # 尝试提取思考过程
            if "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # 获取token使用情况
            prompt_tokens = 0
            completion_tokens = 0

            if "usage" in response_body:
                usage = response_body["usage"]
                prompt_tokens = usage.get("input_tokens", 0)
                completion_tokens = usage.get("output_tokens", 0)

            self.debug(f"Amazon Bedrock (Anthropic) 请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Amazon Bedrock (Anthropic) 请求失败: {e}")
            return True, None, None, None, None

    def _request_amazon_titan(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """使用Amazon Titan格式的Bedrock请求"""
        try:
            model_id = platform_config.get("model_name", "amazon.titan-text-premier-v1:0")
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 构建输入文本
            input_text = ""
            if system_prompt:
                input_text += f"System: {system_prompt}\n\n"

            for msg in messages:
                role = msg.get("role", "")
                content = msg.get("content", "")
                if role == "user":
                    input_text += f"Human: {content}\n\n"
                elif role == "assistant":
                    input_text += f"Assistant: {content}\n\n"

            input_text += "Assistant: "

            # 构建请求体
            request_body = {
                "inputText": input_text,
                "textGenerationConfig": {
                    "maxTokenCount": max_tokens,
                    "temperature": temperature,
                    "topP": top_p
                }
            }

            self.debug(f"发送Amazon Bedrock (Titan) 请求: {model_id}")

            # 从工厂获取Bedrock客户端
            bedrock_client = LLMClientFactory().get_boto3_bedrock(platform_config)

            # 发送请求
            response = bedrock_client.invoke_model(
                modelId=model_id,
                body=json.dumps(request_body),
                contentType="application/json"
            )

            # 解析响应
            response_body = json.loads(response["body"].read())

            response_think = ""
            response_content = ""

            if "results" in response_body and response_body["results"]:
                response_content = response_body["results"][0].get("outputText", "").strip()

            # 尝试提取思考过程
            if "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # Titan模型的token计算可能不同
            prompt_tokens = 0
            completion_tokens = 0

            self.debug(f"Amazon Bedrock (Titan) 请求成功")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Amazon Bedrock (Titan) 请求失败: {e}")
            return True, None, None, None, None

    def _request_meta_llama(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """使用Meta Llama格式的Bedrock请求"""
        try:
            model_id = platform_config.get("model_name", "meta.llama3-70b-instruct-v1:0")
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 构建提示文本
            prompt = ""
            if system_prompt:
                prompt += f"<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n{system_prompt}<|eot_id|>\n"

            for msg in messages:
                role = msg.get("role", "")
                content = msg.get("content", "")
                if role == "user":
                    prompt += f"<|start_header_id|>user<|end_header_id|>\n{content}<|eot_id|>\n"
                elif role == "assistant":
                    prompt += f"<|start_header_id|>assistant<|end_header_id|>\n{content}<|eot_id|>\n"

            prompt += "<|start_header_id|>assistant<|end_header_id|>\n"

            # 构建请求体
            request_body = {
                "prompt": prompt,
                "max_gen_len": max_tokens,
                "temperature": temperature,
                "top_p": top_p
            }

            self.debug(f"发送Amazon Bedrock (Llama) 请求: {model_id}")

            # 从工厂获取Bedrock客户端
            bedrock_client = LLMClientFactory().get_boto3_bedrock(platform_config)

            # 发送请求
            response = bedrock_client.invoke_model(
                modelId=model_id,
                body=json.dumps(request_body),
                contentType="application/json"
            )

            # 解析响应
            response_body = json.loads(response["body"].read())

            response_think = ""
            response_content = ""

            if "generation" in response_body:
                response_content = response_body["generation"].strip()

            # 尝试提取思考过程
            if "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # 获取token使用情况
            prompt_tokens = response_body.get("prompt_token_count", 0)
            completion_tokens = response_body.get("generation_token_count", 0)

            self.debug(f"Amazon Bedrock (Llama) 请求成功，输入tokens: {prompt_tokens}, 输出tokens: {completion_tokens}")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Amazon Bedrock (Llama) 请求失败: {e}")
            return True, None, None, None, None

    def _request_cohere_bedrock(
        self,
        messages: List[Dict[str, Any]],
        system_prompt: str,
        platform_config: Dict[str, Any]
    ) -> Tuple[bool, Optional[str], Optional[str], Optional[int], Optional[int]]:
        """使用Cohere格式的Bedrock请求"""
        try:
            model_id = platform_config.get("model_name", "cohere.command-r-plus-v1:0")
            temperature = platform_config.get("temperature", 1.0)
            top_p = platform_config.get("top_p", 1.0)
            max_tokens = platform_config.get("max_tokens", 4096)

            # 构建聊天历史
            chat_history = []
            for msg in messages[:-1]:  # 除了最后一个消息
                if msg.get("role") in ["user", "assistant"]:
                    chat_history.append({
                        "role": "USER" if msg["role"] == "user" else "CHATBOT",
                        "message": msg["content"]
                    })

            # 最后一个消息作为当前消息
            current_message = messages[-1]["content"] if messages else ""

            # 构建请求体
            request_body = {
                "message": current_message,
                "chat_history": chat_history,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "p": top_p
            }

            if system_prompt:
                request_body["preamble"] = system_prompt

            self.debug(f"发送Amazon Bedrock (Cohere) 请求: {model_id}")

            # 从工厂获取Bedrock客户端
            bedrock_client = LLMClientFactory().get_boto3_bedrock(platform_config)

            # 发送请求
            response = bedrock_client.invoke_model(
                modelId=model_id,
                body=json.dumps(request_body),
                contentType="application/json"
            )

            # 解析响应
            response_body = json.loads(response["body"].read())

            response_think = ""
            response_content = ""

            if "text" in response_body:
                response_content = response_body["text"].strip()

            # 尝试提取思考过程
            if "</think>" in response_content:
                parts = response_content.split("</think>")
                response_think = parts[0].replace("<think>", "").strip()
                response_content = parts[-1].strip()

            # 获取token使用情况
            prompt_tokens = 0
            completion_tokens = 0

            self.debug(f"Amazon Bedrock (Cohere) 请求成功")

            return False, response_think, response_content, prompt_tokens, completion_tokens

        except Exception as e:
            self.error(f"Amazon Bedrock (Cohere) 请求失败: {e}")
            return True, None, None, None, None
