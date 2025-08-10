'''
AI 逐文件夹处理

  📁 individual_files/
    📁 source_only/
    📁 header_only/
    📁 misc_files/
  📁 paired_files/
  📁 indices/
    📄 file_mappings.json
    📄 processing_stats.json
    📄 element_indices.json
    📄 analysis_results.json
  📄 processing_log.txt
  📄 processing_report.json
'''
import os
from ..Preprocessing.CallRelationAnalyzer import CallRelationAnalyzer
from ..LLMRequester.LLMRequester import LLMRequester
from ..DatebaseServer.DatabaseManager import DatabaseManager

from ...base.Base import Base

class ConvertService(Base):
    """
    转换服务模块
    转换单文件夹内的所有文件，处理源文件、头文件和其他类型的文件为 Rust 代码。

    Attributes:
        db_manager: 数据库管理器实例
        input_folder: 输入文件夹路径
    """
    def __init__(self, db_client: DatabaseManager, input_folder: str):
        self.db_manager = db_client
        self.input_folder = input_folder
        super().__init__()

    def convert(self):
        """
        执行转换过程
        """
        try:
            self.info(f"开始转换文件夹: {self.input_folder}")

            # 创建 Rust 项目结构
            self._create_rust_project()

            # 遍历输入文件夹中的所有文件
            for root, _, files in os.walk(self.input_folder):
                for file in files:
                    if file.endswith(('.c', '.h')):
                        file_path = os.path.join(root, file)
                        self.info(f"转换文件: {file_path}")
                        with open(file_path, 'r') as f:
                            c_code = f.read()
                        # 使用 LLM 进行代码转换
                        llm_client = LLMRequester()
                        system_prompt = "Convert the following C code to Rust code:"
                        platform_config = {}  # Add appropriate platform configuration
                        messages = [{"role": "user", "content": c_code}]
                        response = llm_client.sent_request(messages, system_prompt, platform_config)
                        # 提取响应内容
                        success, rust_code, error_msg, status_code, tokens = response
                        if not success or rust_code is None:
                            self.error(f"LLM 转换失败: {error_msg}")
                            continue
                        # 保存转换后的 Rust 代码
                        rust_file_name = os.path.splitext(file)[0] + '.rs'
                        self._save_rust_file(rust_file_name, rust_code)

            self.info(f"成功完成文件夹转换: {self.input_folder}")
        except Exception as e:
            self.error(f"转换过程中出现错误: {e}")
            raise

    def _create_rust_project(self):
        """
        将目录转换为 cargo project
        """
        os.makedirs(os.path.join(self.input_folder, "src"), exist_ok=True)
        with open(os.path.join(self.input_folder, "Cargo.toml"), 'w') as f:
            f.write("[package]\nname = \"my_project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n")

    def _save_rust_file(self, file_name: str, rust_code: str):
        """
        保存 Rust 代码到指定文件
        """
        rust_file_path = os.path.join(self.input_folder, "src", f"{file_name}.rs")
        with open(rust_file_path, 'w') as f:
            f.write(rust_code)

class PromptBuilder(Base):
    """
    提示构建器模块

    Attributes:
        FileName: 文件名

    根据文件名查找数据库中的相关函数和结构体，构建提示信息。
    """
    def __init__(self, db_client: DatabaseManager, file_name: str):
        self.db_client = db_client
        self.file_name = file_name
        super().__init__()

    def build_prompt(self) -> str:
        """
        构建提示信息
        """
        try:
            self.info(f"构建提示信息 for 文件: {self.file_name}")
            functions = self.db_client.get_functions_by_file(self.file_name)
            structs = self.db_client.get_structs_by_file(self.file_name)

            prompt_parts = []
            if functions:
                prompt_parts.append("相关函数:\n")
                for func in functions:
                    prompt_parts.append(f"- {func['name']}: {func['code']}\n")

            if structs:
                prompt_parts.append("相关结构体:\n")
                for struct in structs:
                    prompt_parts.append(f"- {struct['name']}: {struct['definition']}\n")

            prompt = "\n".join(prompt_parts) if prompt_parts else "无相关函数或结构体。"
            self.info("成功构建提示信息")
            return prompt
        except Exception as e:
            self.error(f"构建提示信息失败: {e}")
            return "构建提示信息时出错。"