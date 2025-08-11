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
import json
from typing import Dict, List, Optional
from pathlib import Path

from ..LLMRequester.LLMRequester import LLMRequester
from ..DatebaseServer.DatabaseManager import DatabaseManager

from ...base.Base import Base
from .PromptBuilder import PromptBuilder
from .RustCodeCheck import RustCodeCheck
from .ResponseAnalysis import ResponseAnalysis


class ConvertService(Base):
    """
    转换服务模块
    转换单文件夹内的所有文件，处理源文件、头文件和其他类型的文件为 Rust 代码。

    Attributes:
        db_manager: 数据库管理器实例
        input_folder: 输入文件夹路径
        prompt_builder: 提示构建器实例
        code_checker: 项目检查器实例
    """

    def __init__(self, db_client: DatabaseManager, input_folder: Path, indices_dir: Optional[str] = None):
        self.db_manager = db_client
        self.input_folder = input_folder

        # 如果没有提供indices_dir，尝试从input_folder推断
        if indices_dir is None:
            indices_dir = os.path.join(input_folder, "indices")

        self.prompt_builder = PromptBuilder(
            db_client, os.path.basename(input_folder), indices_dir)
        self.code_checker = RustCodeCheck(input_folder)
        super().__init__()

    def convert_paired_files(self):
        pass

    def convert_singles_file(self):
        """
        执行转换过程
        """
        try:
            self.info(f"开始转换文件夹: {self.input_folder}")

            # 创建 Rust 项目结构
            self._create_rust_project()

            # 获取文件夹中的唯一文件
            files = [f for f in os.listdir(
                self.input_folder) if f.endswith(('.c', '.h'))]
            if len(files) != 1:
                raise ValueError(
                    f"文件夹应包含且仅包含一个 .c 或 .h 文件，实际找到 {len(files)} 个")

            file_path = os.path.join(self.input_folder, files[0])

            # 处理文件直到通过检查
            config = self.load_config().get("convert_services", {})
            max_retries = config.get("max_retries", "3")
            retry_count = 0

            while retry_count < max_retries:
                success = self._process_single_file(file_path)
                if not success:
                    retry_count += 1
                    continue

                check_result = self.code_checker.check_rust_project()
                if check_result is True:
                    self.info("Rust 项目检查通过")
                    break
                else:
                    self.warning(
                        f"Rust 项目检查失败 (第{retry_count + 1}次): {check_result}")
                    # 将检查结果加入到下次处理的上下文中
                    self.prompt_builder.add_error_context(str(check_result))
                    retry_count += 1

            if retry_count >= max_retries:
                raise Exception(f"经过 {max_retries} 次重试，Rust 项目仍未通过检查")

            self.info(f"成功完成文件夹转换: {self.input_folder}")
        except Exception as e:
            self.error(f"转换过程中出现错误: {e}")
            raise

    def _process_single_file(self, file_path) -> bool:
        if os.path.basename(file_path).endswith(('.c', '.h')):
            self.info(f"转换文件: {file_path}")
            system_prompt = self.load_config().get("llm", {}).get("prompt", {}).get("system", "")

            # 获取LLM配置
            llm_config = self.load_config().get("llm", {})
            target_platform = llm_config.get("target_platform", "openai")

            # 获取特定provider的配置并合并通用配置
            providers_config = llm_config.get("providers", {})
            if target_platform in providers_config:
                platform_config = providers_config[target_platform].copy()
                platform_config["target_platform"] = target_platform
            else:
                # 如果没有找到对应的provider配置，使用通用配置
                platform_config = llm_config.copy()

            with open(file_path, 'r') as f:
                c_code = f.read()
            messages = [{"role": "user", "content": c_code}]

            # 使用 LLM 进行代码转换
            llm_client = LLMRequester()
            try:
                system_prompt = self.prompt_builder.build_file_context_prompt(
                    file_path)
            except Exception as e:
                self.error(f"构建请求消息时出错: {e}。尝试继续")
            response = llm_client.sent_request(
                messages, system_prompt, platform_config)

            # 提取响应内容
            is_false, thinking, responding, input_token, output_token = response
            self.info(f"{is_false=}, {thinking=}, {responding=}, {input_token=}, {output_token=}")
            if is_false or responding is None:
                self.error(f"LLM 转换失败: {responding}")
                return False

            # 保存转换后的 Rust 代码
            analysis_result = ResponseAnalysis(responding).analyze_response()
            # 从文件路径中提取文件名，去掉扩展名，然后添加.rs扩展名
            base_name = os.path.splitext(os.path.basename(file_path))[0]  # main.c -> main
            rust_file_name = base_name  # 不在这里添加.rs，因为_save_rust_file会添加
            self._save_rust_file(rust_file_name, analysis_result.get("rust_code", ""))
            return True
        else:
            return False

    def _create_rust_project(self):
        """
        将目录转换为 cargo project
        """
        os.makedirs(os.path.join(self.input_folder, "src"), exist_ok=True)
        with open(os.path.join(self.input_folder, "Cargo.toml"), 'w') as f:
            f.write(
                "[package]\nname = \"my_project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n")

    def _save_rust_file(self, file_name: str, rust_code: str):
        """
        保存 Rust 代码到指定文件
        """
        rust_file_path = os.path.join(
            self.input_folder, "src", f"{file_name}.rs")
        with open(rust_file_path, 'w') as f:
            f.write(rust_code)

if __name__ == "__main__":
    db_client = DatabaseManager("/Users/peng/Documents/AppCode/Python/c2rust_agent/relation_analysis.db")
    convert = ConvertService(db_client,
                            Path("/Users/peng/Documents/AppCode/Python/c2rust_agent/test_file"))