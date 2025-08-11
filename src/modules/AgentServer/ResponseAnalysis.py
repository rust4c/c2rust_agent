import os
import re
from typing import List, Tuple

from ...base.Base import Base


class ResponseAnalysis(Base):
    """
    响应分析模块
    负责分析LLM的响应结果，提取关键信息并进行格式化。

    **输入示例**：
    ```json
    {
    "original": "int add(int a, int b) { return a + b; }",
    "rust_code": "fn add(a: i32, b: i32) -> i32 { a + b }",
    "key_changes": ["使用i32替代int", "移除多余分号"],
    "warnings": []
    }
    """

    def __init__(self, response: str):
        super().__init__()
        self.response = response

    def analyze_response(self) -> dict:
        """
        分析LLM的响应结果，提取关键信息并进行格式化。

        Args:
            response (dict): LLM的原始响应结果

        Returns:
            dict: 格式化后的响应结果
        """
        self.info("分析LLM响应结果")
        original_code, rust_code, key_changes, warnings = self._analysis_response(
            self.response)
        return {
            "original": original_code,
            "rust_code": rust_code,
            "key_changes": key_changes,
            "warnings": warnings
        }

    def _analysis_response(self, response: str):
        """
        内部方法，具体实现响应分析逻辑。

        Args:
            response (str): LLM的原始响应结果

        Returns:
            tuple: 包含原始代码、Rust代码、关键变化和警告信息的元组
        """
        import json

        try:
            # 首先尝试提取 markdown 代码块中的 JSON
            json_match = re.search(
                r'```json\s*\n(.*?)\n```', response, re.DOTALL)
            if json_match:
                json_content = json_match.group(1)
            else:
                # 如果没有找到 markdown 代码块，尝试直接解析整个响应
                json_content = response.strip()

            # 使用更简单的方式解析，如果 JSON 解析失败，使用正则表达式提取字段
            try:
                response_json = json.loads(json_content)
                original_code = response_json.get("original", "")
                rust_code = response_json.get("rust_code", "")
                key_changes = response_json.get("key_changes", [])
                warnings = response_json.get("warnings", [])
            except json.JSONDecodeError:
                # JSON 解析失败，使用正则表达式提取关键字段
                self.warning("JSON 解析失败，尝试使用正则表达式提取字段")
                original_code = self._extract_field(json_content, "original")
                rust_code = self._extract_field(json_content, "rust_code")
                key_changes = self._extract_array_field(
                    json_content, "key_changes")
                warnings = self._extract_array_field(json_content, "warnings")

            return original_code, rust_code, key_changes, warnings

        except Exception as e:
            self.error(f"响应解析失败: {e}")
            self.error(f"原始响应内容: {response}")
            return "", "", [], ["响应格式错误，无法解析"]

    def _extract_field(self, content: str, field_name: str) -> str:
        """
        使用正则表达式提取字符串字段
        """
        pattern = rf'"{field_name}":\s*"(.*?)"(?:,|\s*}})'
        match = re.search(pattern, content, re.DOTALL)
        if match:
            # 处理转义字符
            value = match.group(1)
            value = value.replace('\\"', '"').replace(
                '\\n', '\n').replace('\\\\', '\\')
            return value
        return ""

    def _extract_array_field(self, content: str, field_name: str) -> List[str]:
        """
        使用正则表达式提取数组字段
        """
        pattern = rf'"{field_name}":\s*\[(.*?)\]'
        match = re.search(pattern, content, re.DOTALL)
        if match:
            array_content = match.group(1)
            # 简单解析数组元素（假设是字符串数组）
            items = re.findall(r'"([^"]*)"', array_content)
            return items
        return []
