#!/usr/bin/env python3
"""
PreProcessor模块简单测试

测试PreProcessor的基本功能
"""

import unittest
import tempfile
import shutil
from pathlib import Path
import sys

# 添加项目根目录到Python路径
sys.path.append(str(Path(__file__).parent.parent.parent))

from src.modules.Preprocessing.PreProcessor import PreProcessor
from src.modules.Preprocessing.CProjectPreprocessor import PreprocessConfig
from src.modules.DatebaseServer.DatabaseManager import create_database_manager


class TestPreProcessor(unittest.TestCase):
    """测试PreProcessor类"""
    
    def setUp(self):
        """设置测试环境"""
        self.temp_dir = tempfile.mkdtemp()
        self.temp_path = Path(self.temp_dir)
        self.project_dir = self.temp_path / "test_project"
        self.cache_dir = self.temp_path / "cache"
        
        # 创建测试项目
        self._create_test_project()
        
        # 创建数据库管理器
        self.db_manager = create_database_manager(
            sqlite_path=str(self.temp_path / "test.db"),
            qdrant_url="http://localhost:6333",
            qdrant_collection="test_preprocessor"
        )
        
        # 创建预处理器
        self.preprocessor = PreProcessor(self.db_manager, str(self.cache_dir))
    
    def tearDown(self):
        """清理测试环境"""
        try:
            self.db_manager.close()
        except:
            pass
        shutil.rmtree(self.temp_dir, ignore_errors=True)
    
    def _create_test_project(self):
        """创建测试项目"""
        self.project_dir.mkdir(parents=True, exist_ok=True)
        
        # 创建配对文件
        (self.project_dir / "main.c").write_text('#include "main.h"\\nint main() { return 0; }')
        (self.project_dir / "main.h").write_text('#ifndef MAIN_H\\n#define MAIN_H\\n#endif')
        
        # 创建独立文件
        (self.project_dir / "config.h").write_text('#define VERSION 1')
        (self.project_dir / "standalone.c").write_text('void func() {}')
    
    def test_initialization(self):
        """测试初始化"""
        self.assertIsNotNone(self.preprocessor.c_preprocessor)
        self.assertIsNotNone(self.preprocessor.db_saver)
        self.assertEqual(self.preprocessor.cache_dir, str(self.cache_dir))
    
    def test_preprocess_only(self):
        """测试仅预处理功能"""
        success, stats = self.preprocessor.preprocess_only(str(self.project_dir))
        
        self.assertTrue(success)
        self.assertGreater(stats.total_files, 0)
        self.assertGreaterEqual(stats.processed_pairs, 1)  # 至少有main.c/main.h配对
    
    def test_set_config(self):
        """测试设置配置"""
        config = PreprocessConfig(WORKER_COUNT=1)
        self.preprocessor.set_config(config)
        
        self.assertEqual(self.preprocessor.c_preprocessor.config.WORKER_COUNT, 1)
    
    def test_get_stats(self):
        """测试获取统计信息"""
        # 先执行预处理
        self.preprocessor.preprocess_only(str(self.project_dir))
        
        # 获取统计信息
        stats = self.preprocessor.get_preprocessing_stats()
        self.assertIsNotNone(stats)
        self.assertGreater(stats.total_files, 0)
    
    def test_invalid_project_dir(self):
        """测试无效项目目录"""
        success, stats = self.preprocessor.preprocess_only("/invalid/path")
        
        self.assertFalse(success)
        self.assertGreater(len(stats.errors or []), 0)


def run_simple_test():
    """运行简单测试"""
    unittest.main(verbosity=2)


if __name__ == "__main__":
    run_simple_test()
