#!/usr/bin/env python3
"""
PreProcessor模块使用示例

展示如何使用完整的预处理模块来处理C项目并保存到数据库
"""

import sys
import tempfile
from pathlib import Path

# 添加项目根目录到Python路径
sys.path.append(str(Path(__file__).parent.parent))

from src.modules.Preprocessing.PreProcessor import PreProcessor
from src.modules.Preprocessing.CProjectPreprocessor import PreprocessConfig
from src.modules.DatebaseServer.DatabaseManager import create_database_manager


def create_sample_project(project_dir: Path):
    """创建示例C项目"""
    project_dir.mkdir(parents=True, exist_ok=True)
    
    # 创建主要的C文件和头文件
    (project_dir / "main.c").write_text("""
#include <stdio.h>
#include "utils.h"
#include "math_lib.h"

int main() {
    printf("Hello, World!\\n");
    
    // 使用utils模块
    print_message("Testing utils");
    
    // 使用数学库
    int result = add_numbers(5, 3);
    printf("5 + 3 = %d\\n", result);
    
    return 0;
}
""")

    (project_dir / "utils.c").write_text("""
#include <stdio.h>
#include "utils.h"

void print_message(const char* msg) {
    printf("Message: %s\\n", msg);
}

int string_length(const char* str) {
    int len = 0;
    while (str[len] != '\\0') {
        len++;
    }
    return len;
}
""")

    (project_dir / "utils.h").write_text("""
#ifndef UTILS_H
#define UTILS_H

/**
 * 打印消息到控制台
 * @param msg 要打印的消息
 */
void print_message(const char* msg);

/**
 * 计算字符串长度
 * @param str 输入字符串
 * @return 字符串长度
 */
int string_length(const char* str);

#endif // UTILS_H
""")

    # 创建数学库
    math_dir = project_dir / "lib"
    math_dir.mkdir(exist_ok=True)
    
    (math_dir / "math_lib.c").write_text("""
#include "math_lib.h"

int add_numbers(int a, int b) {
    return a + b;
}

int multiply_numbers(int a, int b) {
    return a * b;
}

double divide_numbers(double a, double b) {
    if (b != 0.0) {
        return a / b;
    }
    return 0.0;
}
""")

    (math_dir / "math_lib.h").write_text("""
#ifndef MATH_LIB_H
#define MATH_LIB_H

/**
 * 两数相加
 */
int add_numbers(int a, int b);

/**
 * 两数相乘
 */
int multiply_numbers(int a, int b);

/**
 * 两数相除
 */
double divide_numbers(double a, double b);

#endif // MATH_LIB_H
""")

    # 创建配置文件
    (project_dir / "config.h").write_text("""
#ifndef CONFIG_H
#define CONFIG_H

#define VERSION_MAJOR 1
#define VERSION_MINOR 0
#define VERSION_PATCH 0

#define MAX_BUFFER_SIZE 1024
#define DEFAULT_TIMEOUT 30

#endif // CONFIG_H
""")

    # 创建独立的源文件
    (project_dir / "standalone.c").write_text("""
#include <stdio.h>

// 独立的实用函数，没有对应的头文件
void debug_print(const char* file, int line, const char* msg) {
    printf("[DEBUG] %s:%d - %s\\n", file, line, msg);
}
""")

    # 创建README文件
    (project_dir / "README.txt").write_text("""
Sample C Project
================

This is a sample C project for testing the preprocessor.

Files:
- main.c: Main program entry point
- utils.c/utils.h: Utility functions
- lib/math_lib.c/math_lib.h: Mathematical operations
- config.h: Configuration constants
- standalone.c: Standalone functions
""")

    print(f"示例项目已创建在: {project_dir}")


def create_custom_config():
    """创建自定义预处理配置"""
    config = PreprocessConfig(
        WORKER_COUNT=2,  # 使用2个工作线程
        PAIRING_RULES=[
            (r"(.*)\.c", r"\1.h"),      # C文件配对.h文件
            (r"(.*)\.cpp", r"\1.hpp"),  # C++文件配对.hpp文件  
        ],
        EXCLUDE_PATTERNS=[
            "*.bak",
            "*.tmp", 
            "*.o",
            "*.obj",
            "__pycache__/*",
            "build/*",
            "Debug/*",
            "Release/*"
        ],
        HEADER_EXTENSIONS=[".h", ".hpp", ".hxx", ".hh"],
        SOURCE_EXTENSIONS=[".c", ".cpp", ".cxx", ".cc"]
    )
    return config


def demonstration_basic_usage():
    """演示基本用法"""
    print("=== PreProcessor基本用法演示 ===\\n")
    
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)
        project_dir = temp_path / "sample_project"
        cache_dir = temp_path / "cache"
        
        # 创建示例项目
        create_sample_project(project_dir)
        
        # 创建数据库管理器
        db_manager = create_database_manager(
            sqlite_path=str(temp_path / "test.db"),
            qdrant_url="http://localhost:6333",
            qdrant_collection="test_preprocessor"
        )
        
        try:
            # 创建预处理器
            preprocessor = PreProcessor(db_manager, str(cache_dir))
            
            # 执行预处理和保存
            print("1. 执行完整预处理和保存...")
            success, stats = preprocessor.preprocess_and_save(str(project_dir))
            
            if success:
                print(f"✓ 预处理成功！")
                print(f"  - 总文件数: {stats.total_files}")
                print(f"  - 配对文件: {stats.processed_pairs}")
                print(f"  - 仅头文件: {stats.header_only}")
                print(f"  - 仅源文件: {stats.source_only}")
                print(f"  - 其他文件: {stats.misc_files}")
                print(f"  - 处理时间: {stats.processing_time:.2f}秒")
            else:
                print("✗ 预处理失败")
                
        except Exception as e:
            print(f"处理过程中出现错误: {e}")
        finally:
            db_manager.close()


def demonstration_advanced_usage():
    """演示高级用法"""
    print("\\n=== PreProcessor高级用法演示 ===\\n")
    
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)
        project_dir = temp_path / "advanced_project"
        cache_dir = temp_path / "advanced_cache"
        
        # 创建示例项目
        create_sample_project(project_dir)
        
        # 创建数据库管理器
        db_manager = create_database_manager(
            sqlite_path=str(temp_path / "advanced.db"),
            qdrant_url="http://localhost:6333", 
            qdrant_collection="test_advanced"
        )
        
        try:
            # 创建带自定义配置的预处理器
            preprocessor = PreProcessor(db_manager, str(cache_dir))
            
            # 设置自定义配置
            custom_config = create_custom_config()
            preprocessor.set_config(custom_config)
            
            print("1. 仅执行预处理（不保存到数据库）...")
            success, stats = preprocessor.preprocess_only(str(project_dir))
            
            if success:
                print("✓ 预处理完成")
                print(f"  处理时间: {stats.processing_time:.2f}秒")
                
                print("\\n2. 单独保存到数据库...")
                preprocessor.save_only(str(project_dir))
                print("✓ 数据库保存完成")
                
                # 获取统计信息
                final_stats = preprocessor.get_preprocessing_stats()
                print(f"\\n3. 最终统计:")
                print(f"  - 总文件数: {final_stats.total_files}")
                print(f"  - 总大小: {final_stats.total_size} 字节")
                if final_stats.errors:
                    print(f"  - 错误数: {len(final_stats.errors)}")
            else:
                print("✗ 预处理失败")
                
        except Exception as e:
            print(f"高级处理过程中出现错误: {e}")
        finally:
            db_manager.close()


def demonstration_error_handling():
    """演示错误处理"""
    print("\\n=== 错误处理演示 ===\\n")
    
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)
        cache_dir = temp_path / "error_cache"
        
        # 创建数据库管理器
        db_manager = create_database_manager(
            sqlite_path=str(temp_path / "error.db"),
            qdrant_url="http://localhost:6333",
            qdrant_collection="test_error"
        )
        
        try:
            preprocessor = PreProcessor(db_manager, str(cache_dir))
            
            # 尝试处理不存在的项目目录
            print("1. 测试处理不存在的目录...")
            try:
                success, stats = preprocessor.preprocess_and_save("/path/that/does/not/exist")
                if not success:
                    print("✓ 正确处理了不存在目录的情况")
                    if stats.errors:
                        print(f"  错误信息: {stats.errors[0]}")
            except Exception as e:
                print(f"✓ 正确抛出异常: {e}")
                
        except Exception as e:
            print(f"错误处理演示中的错误: {e}")
        finally:
            db_manager.close()


def main():
    """主函数"""
    print("PreProcessor模块使用示例")
    print("=" * 50)
    
    try:
        # 基本用法演示
        demonstration_basic_usage()
        
        # 高级用法演示  
        demonstration_advanced_usage()
        
        # 错误处理演示
        demonstration_error_handling()
        
        print("\\n" + "=" * 50)
        print("所有演示完成！")
        
    except KeyboardInterrupt:
        print("\\n用户中断操作")
    except Exception as e:
        print(f"演示过程中出现未处理的错误: {e}")


if __name__ == "__main__":
    main()
