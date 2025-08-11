"""
C工程预处理模块单元测试

测试CProjectPreprocessor的各项功能
"""

import unittest
import tempfile
import shutil
from pathlib import Path
import sys

# 添加项目根目录到Python路径
sys.path.append(str(Path(__file__).parent.parent.parent.parent))

from src.modules.Preprocessing.CProjectPreprocessor import (
    CProjectPreprocessor, 
    PreprocessConfig, 
    FileInfo,
    ProcessingStats,
    DiskFullError
)


class TestPreprocessConfig(unittest.TestCase):
    """测试预处理配置类"""
    
    def test_default_config(self):
        """测试默认配置"""
        config = PreprocessConfig()
        
        # 检查默认值设置
        self.assertGreater(config.WORKER_COUNT, 0)
        self.assertIsNotNone(config.PAIRING_RULES)
        self.assertIsNotNone(config.EXCLUDE_PATTERNS)
        self.assertIsNotNone(config.HEADER_EXTENSIONS)
        self.assertIsNotNone(config.SOURCE_EXTENSIONS)
        
        # 检查默认配对规则
        self.assertIn((r"(.*)\.c", r"\1.h"), config.PAIRING_RULES or [])
        
        # 检查默认排除模式
        self.assertIn("*.bak", config.EXCLUDE_PATTERNS or [])
        self.assertIn("__pycache__/*", config.EXCLUDE_PATTERNS or [])
        
        # 检查扩展名
        self.assertIn(".h", config.HEADER_EXTENSIONS or [])
        self.assertIn(".c", config.SOURCE_EXTENSIONS or [])
    
    def test_custom_config(self):
        """测试自定义配置"""
        config = PreprocessConfig(
            WORKER_COUNT=2,
            PAIRING_RULES=[(r"(.*)\.cpp", r"\1.hpp")],
            EXCLUDE_PATTERNS=["*.test"],
            HEADER_EXTENSIONS=[".hpp"],
            SOURCE_EXTENSIONS=[".cpp"]
        )
        
        self.assertEqual(config.WORKER_COUNT, 2)
        self.assertEqual(config.PAIRING_RULES, [(r"(.*)\.cpp", r"\1.hpp")])
        self.assertEqual(config.EXCLUDE_PATTERNS, ["*.test"])
        self.assertEqual(config.HEADER_EXTENSIONS, [".hpp"])
        self.assertEqual(config.SOURCE_EXTENSIONS, [".cpp"])


class TestFileInfo(unittest.TestCase):
    """测试文件信息类"""
    
    def test_file_info_creation(self):
        """测试文件信息创建"""
        path = Path("/test/file.c")
        size = 1024
        
        file_info = FileInfo(path=path, size=size)
        
        self.assertEqual(file_info.path, path)
        self.assertEqual(file_info.size, size)
        self.assertIsNone(file_info.hash)
        self.assertFalse(file_info.is_large)
    
    def test_large_file_detection(self):
        """测试大文件检测"""
        path = Path("/test/large_file.c")
        large_size = 200 * 1024 * 1024  # 200MB
        
        file_info = FileInfo(path=path, size=large_size)
        
        self.assertTrue(file_info.is_large)


class TestProcessingStats(unittest.TestCase):
    """测试处理统计类"""
    
    def test_stats_initialization(self):
        """测试统计信息初始化"""
        stats = ProcessingStats()
        
        self.assertEqual(stats.total_files, 0)
        self.assertEqual(stats.processed_pairs, 0)
        self.assertEqual(stats.header_only, 0)
        self.assertEqual(stats.source_only, 0)
        self.assertEqual(stats.misc_files, 0)
        self.assertEqual(stats.skipped_files, 0)
        self.assertEqual(stats.processing_time, 0.0)
        self.assertEqual(stats.total_size, 0)
        self.assertIsNotNone(stats.errors)
        self.assertEqual(len(stats.errors or []), 0)


class TestCProjectPreprocessor(unittest.TestCase):
    """测试C工程预处理器"""
    
    def setUp(self):
        """设置测试环境"""
        self.temp_dir = tempfile.mkdtemp()
        self.source_dir = Path(self.temp_dir) / "source"
        self.cache_dir = Path(self.temp_dir) / "cache"
        
        # 创建测试文件结构
        self._create_test_files()
        
        # 创建预处理器
        config = PreprocessConfig(
            WORKER_COUNT=1,  # 使用单线程便于测试
            PAIRING_RULES=[
                (r"(.*)\.c", r"\1.h"),
            ],
            EXCLUDE_PATTERNS=[
                "*.bak",
                "*.tmp",
                "__pycache__/*"
            ]
        )
        self.preprocessor = CProjectPreprocessor(config)
    
    def tearDown(self):
        """清理测试环境"""
        shutil.rmtree(self.temp_dir, ignore_errors=True)
    
    def _create_test_files(self):
        """创建测试文件"""
        self.source_dir.mkdir(parents=True, exist_ok=True)
        
        # 创建配对文件
        (self.source_dir / "main.c").write_text("#include \"main.h\"\nint main() { return 0; }")
        (self.source_dir / "main.h").write_text("#ifndef MAIN_H\n#define MAIN_H\n#endif")
        
        (self.source_dir / "utils.c").write_text("#include \"utils.h\"\nvoid helper() {}")
        (self.source_dir / "utils.h").write_text("#ifndef UTILS_H\n#define UTILS_H\nvoid helper();\n#endif")
        
        # 创建独立文件
        (self.source_dir / "config.h").write_text("#define VERSION 1")  # 仅头文件
        (self.source_dir / "standalone.c").write_text("void func() {}")  # 仅源文件
        (self.source_dir / "readme.txt").write_text("This is a readme.")  # 其他文件
        
        # 创建应该被排除的文件
        (self.source_dir / "backup.bak").write_text("backup")
        (self.source_dir / "temp.tmp").write_text("temp")
        
        # 创建子目录
        sub_dir = self.source_dir / "subdir"
        sub_dir.mkdir()
        (sub_dir / "sub.c").write_text("#include \"sub.h\"\nvoid sub_func() {}")
        (sub_dir / "sub.h").write_text("void sub_func();")
    
    def test_scan_source_files(self):
        """测试源文件扫描"""
        files = self.preprocessor._scan_source_files(self.source_dir)
        
        # 检查文件数量（应该排除.bak和.tmp文件）
        expected_files = {
            "main.c", "main.h", "utils.c", "utils.h", 
            "config.h", "standalone.c", "readme.txt",
            "sub.c", "sub.h"
        }
        
        found_files = {f.path.name for f in files}
        self.assertEqual(found_files, expected_files)
    
    def test_should_exclude_file(self):
        """测试文件排除逻辑"""
        # 应该被排除的文件
        self.assertTrue(self.preprocessor._should_exclude_file(Path("test.bak")))
        self.assertTrue(self.preprocessor._should_exclude_file(Path("test.tmp")))
        self.assertTrue(self.preprocessor._should_exclude_file(Path("__pycache__/module.pyc")))
        
        # 不应该被排除的文件
        self.assertFalse(self.preprocessor._should_exclude_file(Path("test.c")))
        self.assertFalse(self.preprocessor._should_exclude_file(Path("test.h")))
        self.assertFalse(self.preprocessor._should_exclude_file(Path("readme.txt")))
    
    def test_find_file_pairs(self):
        """测试文件配对查找"""
        files = self.preprocessor._scan_source_files(self.source_dir)
        pairs = self.preprocessor._find_file_pairs(files)
        
        # 应该找到3个配对：main, utils, sub
        self.assertEqual(len(pairs), 3)
        
        # 检查配对名称
        pair_names = set(pairs.keys())
        expected_pairs = {"main", "utils", "sub"}
        self.assertEqual(pair_names, expected_pairs)
        
        # 检查配对内容
        for base_name, (c_file, h_file) in pairs.items():
            self.assertEqual(c_file.path.suffix, ".c")
            self.assertEqual(h_file.path.suffix, ".h")
            self.assertEqual(c_file.path.stem, base_name)
            self.assertEqual(h_file.path.stem, base_name)
    
    def test_categorize_remaining_files(self):
        """测试剩余文件分类"""
        files = self.preprocessor._scan_source_files(self.source_dir)
        pairs = self.preprocessor._find_file_pairs(files)
        
        # 获取配对文件的路径
        paired_paths = set()
        for c_file, h_file in pairs.values():
            paired_paths.add(c_file.path)
            paired_paths.add(h_file.path)
        
        # 获取剩余文件
        remaining_files = [f for f in files if f.path not in paired_paths]
        
        # 分类剩余文件
        categorized = self.preprocessor._categorize_remaining_files(remaining_files)
        
        # 检查分类结果
        self.assertEqual(len(categorized["header_only"]), 1)  # config.h
        self.assertEqual(len(categorized["source_only"]), 1)  # standalone.c
        self.assertEqual(len(categorized["misc_files"]), 1)   # readme.txt
        
        # 检查具体文件
        header_only_names = {f.path.name for f in categorized["header_only"]}
        source_only_names = {f.path.name for f in categorized["source_only"]}
        misc_files_names = {f.path.name for f in categorized["misc_files"]}
        
        self.assertEqual(header_only_names, {"config.h"})
        self.assertEqual(source_only_names, {"standalone.c"})
        self.assertEqual(misc_files_names, {"readme.txt"})
    
    def test_resolve_filename_conflict(self):
        """测试文件名冲突解决"""
        # 创建目标目录和冲突文件
        target_dir = self.cache_dir / "test"
        target_dir.mkdir(parents=True, exist_ok=True)
        
        original_file = target_dir / "test.txt"
        original_file.write_text("original")
        
        # 测试冲突解决
        resolved_path = self.preprocessor._resolve_filename_conflict(original_file)
        self.assertNotEqual(resolved_path, original_file)
        self.assertEqual(resolved_path.name, "test_1.txt")
        
        # 创建更多冲突文件测试递增编号
        (target_dir / "test_1.txt").write_text("conflict1")
        resolved_path2 = self.preprocessor._resolve_filename_conflict(original_file)
        self.assertEqual(resolved_path2.name, "test_2.txt")
    
    def test_format_size(self):
        """测试文件大小格式化"""
        self.assertEqual(self.preprocessor._format_size(0), "0.00 B")
        self.assertEqual(self.preprocessor._format_size(1024), "1.00 KB")
        self.assertEqual(self.preprocessor._format_size(1024 * 1024), "1.00 MB")
        self.assertEqual(self.preprocessor._format_size(1024 * 1024 * 1024), "1.00 GB")
    
    def test_full_preprocessing_workflow(self):
        """测试完整的预处理工作流"""
        success, stats = self.preprocessor.preprocess_project(
            str(self.source_dir), 
            str(self.cache_dir)
        )
        
        # 检查处理结果
        self.assertTrue(success)
        self.assertGreater(stats.total_files, 0)
        self.assertEqual(stats.processed_pairs, 3)  # main, utils, sub
        self.assertEqual(stats.header_only, 1)      # config.h
        self.assertEqual(stats.source_only, 1)      # standalone.c
        self.assertEqual(stats.misc_files, 1)       # readme.txt
        self.assertGreater(stats.processing_time, 0)
        
        # 检查输出目录结构
        self.assertTrue(self.cache_dir.exists())
        self.assertTrue((self.cache_dir / "paired_files").exists())
        self.assertTrue((self.cache_dir / "individual_files").exists())
        self.assertTrue((self.cache_dir / "processing_report.json").exists())
        self.assertTrue((self.cache_dir / "processing_log.txt").exists())
        
        # 检查配对文件目录
        paired_dirs = list((self.cache_dir / "paired_files").iterdir())
        self.assertEqual(len(paired_dirs), 3)
        
        paired_names = {d.name for d in paired_dirs}
        self.assertEqual(paired_names, {"main", "utils", "sub"})
        
        # 检查每个配对目录包含正确的文件
        for pair_dir in paired_dirs:
            files_in_pair = list(pair_dir.glob("*"))
            self.assertEqual(len(files_in_pair), 2)  # 一个.c文件和一个.h文件
            
            file_extensions = {f.suffix for f in files_in_pair}
            self.assertEqual(file_extensions, {".c", ".h"})
        
        # 检查独立文件目录
        individual_dirs = list((self.cache_dir / "individual_files").iterdir())
        expected_categories = {"header_only", "source_only", "misc_files"}
        actual_categories = {d.name for d in individual_dirs}
        self.assertEqual(actual_categories, expected_categories)
    
    def test_invalid_source_directory(self):
        """测试无效源目录处理"""
        invalid_dir = "/path/that/does/not/exist"
        
        success, stats = self.preprocessor.preprocess_project(
            invalid_dir, 
            str(self.cache_dir)
        )
        
        self.assertFalse(success)
        self.assertGreater(len(stats.errors or []), 0)


class TestIntegration(unittest.TestCase):
    """集成测试"""
    
    def test_example_usage_integration(self):
        """测试示例用法集成"""
        # 这里可以测试example_usage.py中的功能
        # 由于需要创建实际文件，这里只做基本的导入测试
        try:
            from examples.preprocess_example_usage import create_sample_config, _format_size
            
            config = create_sample_config()
            self.assertIsInstance(config, PreprocessConfig)
            
            size_str = _format_size(1024)
            self.assertEqual(size_str, "1.00 KB")
            
        except ImportError as e:
            self.fail(f"Failed to import example_usage: {e}")


def run_tests():
    """运行所有测试"""
    # 创建测试套件
    test_suite = unittest.TestSuite()
    
    # 添加测试类
    test_classes = [
        TestPreprocessConfig,
        TestFileInfo,
        TestProcessingStats,
        TestCProjectPreprocessor,
        TestIntegration
    ]
    
    for test_class in test_classes:
        tests = unittest.TestLoader().loadTestsFromTestCase(test_class)
        test_suite.addTests(tests)
    
    # 运行测试
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(test_suite)
    
    return result.wasSuccessful()


if __name__ == "__main__":
    success = run_tests()
    sys.exit(0 if success else 1)
