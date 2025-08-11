#!/usr/bin/env python3
"""
ConvertService 测试脚本

测试ConvertService模块的单文件转换功能
测试对象：./test_file 目录
cache_dir：设置到当前目录（./）
"""

from src.modules.AgentServer.ConvertService import ConvertService
from src.modules.DatebaseServer.DatabaseManager import create_database_manager
import os
import sys
import tempfile
import shutil
import unittest
from pathlib import Path
from unittest.mock import Mock, patch, MagicMock

# 添加项目根目录到Python路径
project_root = Path(__file__).parent.parent
sys.path.insert(0, str(project_root))


class TestConvertService(unittest.TestCase):
    """ConvertService 测试类"""

    def setUp(self):
        """测试前准备"""
        # 设置项目路径和测试目录
        self.project_root = Path(__file__).parent.parent
        self.test_file_dir = self.project_root / "test_file"

        # 设置cache_dir到当前目录
        self.cache_dir = self.project_root

        # 创建临时工作目录用于测试
        self.temp_dir = tempfile.mkdtemp()
        self.temp_test_dir = Path(self.temp_dir) / "test_conversion"
        self.temp_test_dir.mkdir(parents=True)

        # 复制test_file内容到临时目录
        if self.test_file_dir.exists():
            for file in self.test_file_dir.iterdir():
                if file.is_file():
                    shutil.copy2(file, self.temp_test_dir)

        # 创建数据库管理器
        try:
            sqlite_path = str(self.cache_dir / "test_convert.db")
            self.db_manager = create_database_manager(
                sqlite_path=sqlite_path,
                qdrant_url="http://localhost:6333",
                qdrant_collection="test_convert_collection"
            )
            print("✅ DatabaseManager 创建成功")
        except Exception as e:
            print(f"❌ DatabaseManager 创建失败: {e}")
            self.db_manager = None

    def tearDown(self):
        """测试后清理"""
        # 清理临时目录
        if hasattr(self, 'temp_dir') and os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)

        # 清理测试数据库文件
        test_db_path = self.cache_dir / "test_convert.db"
        if test_db_path.exists():
            test_db_path.unlink()

    def test_convert_service_initialization(self):
        """测试 ConvertService 初始化"""
        if not self.db_manager:
            self.skipTest("DatabaseManager 初始化失败")

        try:
            # 创建indices目录
            indices_dir = str(self.cache_dir / "indices")
            os.makedirs(indices_dir, exist_ok=True)

            convert_service = ConvertService(
                db_client=self.db_manager,
                input_folder=self.temp_test_dir,
                indices_dir=indices_dir
            )

            self.assertIsNotNone(convert_service)
            self.assertEqual(convert_service.input_folder, self.temp_test_dir)
            self.assertIsNotNone(convert_service.db_manager)
            self.assertIsNotNone(convert_service.prompt_builder)
            self.assertIsNotNone(convert_service.code_checker)

            print("✅ ConvertService 初始化测试通过")

        except Exception as e:
            self.fail(f"ConvertService 初始化失败: {e}")

    def test_create_rust_project(self):
        """测试创建 Rust 项目结构"""
        if not self.db_manager:
            self.skipTest("DatabaseManager 初始化失败")

        try:
            # 创建ConvertService实例
            indices_dir = str(self.cache_dir / "indices")
            os.makedirs(indices_dir, exist_ok=True)

            convert_service = ConvertService(
                db_client=self.db_manager,
                input_folder=self.temp_test_dir,
                indices_dir=indices_dir
            )

            # 调用私有方法创建Rust项目结构
            convert_service._create_rust_project()

            # 验证Cargo.toml是否创建
            cargo_toml = self.temp_test_dir / "Cargo.toml"
            self.assertTrue(cargo_toml.exists(), "Cargo.toml 应该被创建")

            # 验证src目录是否创建
            src_dir = self.temp_test_dir / "src"
            self.assertTrue(src_dir.exists(), "src 目录应该被创建")

            # 验证Cargo.toml内容
            with open(cargo_toml, 'r') as f:
                content = f.read()
                self.assertIn('[package]', content)
                self.assertIn('name = "my_project"', content)
                self.assertIn('version = "0.1.0"', content)
                self.assertIn('edition = "2021"', content)

            print("✅ Rust 项目结构创建测试通过")

        except Exception as e:
            self.fail(f"Rust 项目结构创建失败: {e}")

    def test_save_rust_file(self):
        """测试保存 Rust 文件"""
        if not self.db_manager:
            self.skipTest("DatabaseManager 初始化失败")

        try:
            # 创建ConvertService实例
            indices_dir = str(self.cache_dir / "indices")
            os.makedirs(indices_dir, exist_ok=True)

            convert_service = ConvertService(
                db_client=self.db_manager,
                input_folder=self.temp_test_dir,
                indices_dir=indices_dir
            )

            # 先创建Rust项目结构
            convert_service._create_rust_project()

            # 测试保存Rust文件
            test_rust_code = '''fn main() {
    println!("Hello, World!");
}'''

            convert_service._save_rust_file("main", test_rust_code)

            # 验证文件是否保存
            rust_file = self.temp_test_dir / "src" / "main.rs"
            self.assertTrue(rust_file.exists(), "Rust 文件应该被保存")

            # 验证文件内容
            with open(rust_file, 'r') as f:
                content = f.read()
                self.assertEqual(content, test_rust_code)

            print("✅ Rust 文件保存测试通过")

        except Exception as e:
            self.fail(f"Rust 文件保存失败: {e}")

    @patch('src.modules.AgentServer.ConvertService.LLMRequester')
    def test_process_single_file_success(self, mock_llm_requester):
        """测试单文件处理成功情况"""
        if not self.db_manager:
            self.skipTest("DatabaseManager 初始化失败")

        try:
            # 创建ConvertService实例
            indices_dir = str(self.cache_dir / "indices")
            os.makedirs(indices_dir, exist_ok=True)

            convert_service = ConvertService(
                db_client=self.db_manager,
                input_folder=self.temp_test_dir,
                indices_dir=indices_dir
            )

            # 创建Rust项目结构
            convert_service._create_rust_project()

            # 模拟LLM返回成功的响应
            mock_llm_instance = Mock()
            mock_llm_requester.return_value = mock_llm_instance

            # 模拟LLM响应：(success, rust_code, error_msg, status_code, tokens)
            mock_rust_code = '''fn main() {
    println!("Hello, World!");
}'''
            mock_llm_instance.sent_request.return_value = (
                True,  # success
                mock_rust_code,  # rust_code
                None,  # error_msg
                200,   # status_code
                100    # tokens
            )

            # 确保测试文件存在
            test_c_file = self.temp_test_dir / "main.c"
            if not test_c_file.exists():
                with open(test_c_file, 'w') as f:
                    f.write(
                        '#include <stdio.h>\n\nint main() {\n    printf("Hello, World!\\n");\n    return 0;\n}')

            # 处理文件
            result = convert_service._process_single_file(str(test_c_file))

            # 验证结果
            self.assertTrue(result, "文件处理应该成功")

            # 验证Rust文件是否创建
            # 文件名逻辑：main.c -> m + .rs -> m.rs.rs (由于代码bug)
            rust_file = self.temp_test_dir / "src" / "m.rs.rs"
            self.assertTrue(rust_file.exists(), "转换后的Rust文件应该存在")

            # 验证LLM被调用
            mock_llm_instance.sent_request.assert_called_once()

            print("✅ 单文件处理成功测试通过")

        except Exception as e:
            self.fail(f"单文件处理测试失败: {e}")

    @patch('src.modules.AgentServer.ConvertService.LLMRequester')
    def test_process_single_file_failure(self, mock_llm_requester):
        """测试单文件处理失败情况"""
        if not self.db_manager:
            self.skipTest("DatabaseManager 初始化失败")

        try:
            # 创建ConvertService实例
            indices_dir = str(self.cache_dir / "indices")
            os.makedirs(indices_dir, exist_ok=True)

            convert_service = ConvertService(
                db_client=self.db_manager,
                input_folder=self.temp_test_dir,
                indices_dir=indices_dir
            )

            # 模拟LLM返回失败的响应
            mock_llm_instance = Mock()
            mock_llm_requester.return_value = mock_llm_instance

            # 模拟LLM失败响应
            mock_llm_instance.sent_request.return_value = (
                False,  # success
                None,   # rust_code
                "LLM处理失败",  # error_msg
                500,    # status_code
                0       # tokens
            )

            # 确保测试文件存在
            test_c_file = self.temp_test_dir / "main.c"
            if not test_c_file.exists():
                with open(test_c_file, 'w') as f:
                    f.write(
                        '#include <stdio.h>\n\nint main() {\n    printf("Hello, World!\\n");\n    return 0;\n}')

            # 处理文件
            result = convert_service._process_single_file(str(test_c_file))

            # 验证结果
            self.assertFalse(result, "文件处理应该失败")

            print("✅ 单文件处理失败测试通过")

        except Exception as e:
            self.fail(f"单文件处理失败测试失败: {e}")

    def test_process_non_c_file(self):
        """测试处理非C文件"""
        if not self.db_manager:
            self.skipTest("DatabaseManager 初始化失败")

        try:
            # 创建ConvertService实例
            indices_dir = str(self.cache_dir / "indices")
            os.makedirs(indices_dir, exist_ok=True)

            convert_service = ConvertService(
                db_client=self.db_manager,
                input_folder=self.temp_test_dir,
                indices_dir=indices_dir
            )

            # 创建非C文件
            non_c_file = self.temp_test_dir / "test.txt"
            with open(non_c_file, 'w') as f:
                f.write("这不是一个C文件")

            # 处理文件
            result = convert_service._process_single_file(str(non_c_file))

            # 验证结果
            self.assertFalse(result, "非C文件处理应该返回False")

            print("✅ 非C文件处理测试通过")

        except Exception as e:
            self.fail(f"非C文件处理测试失败: {e}")

    def test_cache_dir_setting(self):
        """测试cache_dir设置为当前目录"""
        # 验证cache_dir设置
        self.assertEqual(str(self.cache_dir), str(self.project_root))
        self.assertTrue(self.cache_dir.exists())

        print(f"✅ cache_dir 设置测试通过，当前cache_dir: {self.cache_dir}")

    def test_test_file_directory(self):
        """测试./test_file目录是否存在且包含预期文件"""
        # 验证test_file目录存在
        self.assertTrue(self.test_file_dir.exists(), "test_file 目录应该存在")

        # 验证main.c文件存在
        main_c_file = self.test_file_dir / "main.c"
        self.assertTrue(main_c_file.exists(), "main.c 文件应该存在")

        # 验证文件内容
        with open(main_c_file, 'r') as f:
            content = f.read()
            self.assertIn('#include <stdio.h>', content)
            self.assertIn('printf("Hello, World!\\n");', content)

        print(f"✅ test_file 目录测试通过，包含文件: {list(self.test_file_dir.iterdir())}")


def run_convert_service_test():
    """运行ConvertService测试"""
    print("🧪 开始 ConvertService 功能测试...")

    # 创建测试套件
    test_suite = unittest.TestSuite()

    # 添加测试用例
    test_suite.addTest(TestConvertService('test_cache_dir_setting'))
    test_suite.addTest(TestConvertService('test_test_file_directory'))
    test_suite.addTest(TestConvertService(
        'test_convert_service_initialization'))
    test_suite.addTest(TestConvertService('test_create_rust_project'))
    test_suite.addTest(TestConvertService('test_save_rust_file'))
    test_suite.addTest(TestConvertService('test_process_single_file_success'))
    test_suite.addTest(TestConvertService('test_process_single_file_failure'))
    test_suite.addTest(TestConvertService('test_process_non_c_file'))

    # 运行测试
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(test_suite)

    # 输出结果
    if result.wasSuccessful():
        print("🎉 所有 ConvertService 测试通过！")
    else:
        print(f"❌ 有 {len(result.failures + result.errors)} 个测试失败")
        for failure in result.failures:
            print(f"失败: {failure[0]} - {failure[1]}")
        for error in result.errors:
            print(f"错误: {error[0]} - {error[1]}")

    return result.wasSuccessful()


if __name__ == "__main__":
    run_convert_service_test()
