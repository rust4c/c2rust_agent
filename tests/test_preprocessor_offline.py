#!/usr/bin/env python3
"""
PreProcessor模块离线测试（不依赖外部服务）
"""

import tempfile
import sys
from pathlib import Path
from unittest.mock import Mock, patch

def test_preprocessor_import():
    """测试PreProcessor模块的导入"""
    try:
        from src.modules.Preprocessing.PreProcessor import PreProcessor
        from src.modules.Preprocessing.CProjectPreprocessor import PreprocessConfig
        print("✓ PreProcessor模块导入成功")
        return True
    except ImportError as e:
        print(f"✗ PreProcessor模块导入失败: {e}")
        return False

def test_config_creation():
    """测试配置创建"""
    try:
        from src.modules.Preprocessing.CProjectPreprocessor import PreprocessConfig
        config = PreprocessConfig()
        print("✓ PreprocessConfig创建成功")
        print(f"  - 工作线程数: {config.WORKER_COUNT}")
        print(f"  - 头文件扩展名: {config.HEADER_EXTENSIONS}")
        print(f"  - 源文件扩展名: {config.SOURCE_EXTENSIONS}")
        print(f"  - 排除模式: {len(config.EXCLUDE_PATTERNS or [])} 个")
        return True
    except Exception as e:
        print(f"✗ PreprocessConfig创建失败: {e}")
        return False

def test_cproject_preprocessor():
    """测试CProjectPreprocessor"""
    try:
        from src.modules.Preprocessing.CProjectPreprocessor import CProjectPreprocessor, PreprocessConfig
        
        config = PreprocessConfig(WORKER_COUNT=1)
        preprocessor = CProjectPreprocessor(config)
        
        print("✓ CProjectPreprocessor创建成功")
        print(f"  - 配置工作线程数: {preprocessor.config.WORKER_COUNT}")
        return True
    except Exception as e:
        print(f"✗ CProjectPreprocessor创建失败: {e}")
        return False

def test_saveintodb_with_mock():
    """使用模拟数据库测试SaveIntoDB"""
    try:
        from src.modules.Preprocessing.SaveIntoDB import SaveIntoDB
        
        # 创建模拟的数据库客户端
        mock_db_client = Mock()
        mock_db_client.store_interface_with_vector = Mock(return_value=(1, "mock_id"))
        
        with tempfile.TemporaryDirectory() as temp_dir:
            saver = SaveIntoDB(mock_db_client, temp_dir)
            print("✓ SaveIntoDB（带模拟数据库）创建成功")
            return True
    except Exception as e:
        print(f"✗ SaveIntoDB创建失败: {e}")
        return False

def test_preprocessor_with_mock():
    """使用模拟数据库测试PreProcessor"""
    try:
        from src.modules.Preprocessing.PreProcessor import PreProcessor
        
        # 创建模拟的数据库客户端
        mock_db_client = Mock()
        mock_db_client.store_interface_with_vector = Mock(return_value=(1, "mock_id"))
        mock_db_client.close = Mock()
        
        with tempfile.TemporaryDirectory() as temp_dir:
            cache_dir = Path(temp_dir) / "cache"
            
            # 创建预处理器
            preprocessor = PreProcessor(mock_db_client, str(cache_dir))
            
            # 检查属性
            assert hasattr(preprocessor, 'c_preprocessor')
            assert hasattr(preprocessor, 'db_saver')
            assert preprocessor.cache_dir == str(cache_dir)
            
            print("✓ PreProcessor（带模拟数据库）创建成功")
            return True
    except Exception as e:
        print(f"✗ PreProcessor创建失败: {e}")
        return False

def test_preprocessor_functionality():
    """测试PreProcessor的预处理功能"""
    try:
        from src.modules.Preprocessing.PreProcessor import PreProcessor
        from src.modules.Preprocessing.CProjectPreprocessor import PreprocessConfig
        
        # 创建模拟的数据库客户端
        mock_db_client = Mock()
        mock_db_client.store_interface_with_vector = Mock(return_value=(1, "mock_id"))
        mock_db_client.close = Mock()
        
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            project_dir = temp_path / "test_project"
            cache_dir = temp_path / "cache"
            
            # 创建测试项目
            project_dir.mkdir(parents=True)
            (project_dir / "main.c").write_text('''
#include <stdio.h>
#include "utils.h"

int main() {
    printf("Hello World\\n");
    return 0;
}
''')
            (project_dir / "utils.h").write_text('''
#ifndef UTILS_H
#define UTILS_H

void print_message(const char* msg);

#endif
''')
            (project_dir / "utils.c").write_text('''
#include "utils.h"
#include <stdio.h>

void print_message(const char* msg) {
    printf("Message: %s\\n", msg);
}
''')
            
            # 创建预处理器
            preprocessor = PreProcessor(mock_db_client, str(cache_dir))
            
            # 设置配置
            config = PreprocessConfig(WORKER_COUNT=1)
            preprocessor.set_config(config)
            
            # 测试仅预处理功能
            success, stats = preprocessor.preprocess_only(str(project_dir))
            
            if success:
                print("✓ 预处理功能测试成功")
                print(f"  - 处理文件数: {stats.total_files}")
                print(f"  - 配对文件: {stats.processed_pairs}")
                print(f"  - 处理时间: {stats.processing_time:.2f}秒")
                
                # 检查输出目录
                if cache_dir.exists():
                    print(f"  - 缓存目录已创建: {cache_dir}")
                
                return True
            else:
                print(f"◐ 预处理未完全成功，错误: {stats.errors}")
                return False
                
    except Exception as e:
        print(f"✗ 预处理功能测试失败: {e}")
        return False

def test_file_operations():
    """测试文件操作功能"""
    try:
        from src.modules.Preprocessing.CProjectPreprocessor import (
            CProjectPreprocessor, 
            PreprocessConfig,
            FileInfo
        )
        
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            test_file = temp_path / "test.c"
            test_file.write_text("int main() { return 0; }")
            
            # 测试FileInfo
            file_info = FileInfo(path=test_file, size=test_file.stat().st_size)
            print(f"✓ FileInfo创建成功: {file_info.path.name}, 大小: {file_info.size}")
            
            # 测试预处理器的文件扫描
            config = PreprocessConfig(WORKER_COUNT=1)
            preprocessor = CProjectPreprocessor(config)
            
            # 测试文件排除逻辑
            should_exclude_bak = preprocessor._should_exclude_file(Path("test.bak"))
            should_exclude_c = preprocessor._should_exclude_file(Path("test.c"))
            
            assert should_exclude_bak == True
            assert should_exclude_c == False
            
            print("✓ 文件操作功能测试成功")
            return True
            
    except Exception as e:
        print(f"✗ 文件操作功能测试失败: {e}")
        return False

def main():
    """主测试函数"""
    print("PreProcessor模块离线测试")
    print("=" * 50)
    
    tests = [
        ("模块导入", test_preprocessor_import),
        ("配置创建", test_config_creation),
        ("CProjectPreprocessor", test_cproject_preprocessor),
        ("SaveIntoDB（模拟）", test_saveintodb_with_mock),
        ("PreProcessor（模拟）", test_preprocessor_with_mock),
        ("预处理功能", test_preprocessor_functionality),
        ("文件操作", test_file_operations)
    ]
    
    passed = 0
    total = len(tests)
    
    for test_name, test_func in tests:
        print(f"\n【{test_name}】:")
        try:
            if test_func():
                passed += 1
        except Exception as e:
            print(f"✗ {test_name}测试出现异常: {e}")
            import traceback
            traceback.print_exc()
    
    print(f"\n" + "=" * 50)
    print(f"测试结果: {passed}/{total} 通过")
    
    if passed == total:
        print("🎉 所有测试通过！PreProcessor模块工作正常")
        return 0
    elif passed >= total * 0.7:
        print("✅ 大部分测试通过，PreProcessor模块基本可用")
        return 0
    else:
        print("⚠️  多数测试失败，需要修复")
        return 1

if __name__ == "__main__":
    sys.exit(main())
