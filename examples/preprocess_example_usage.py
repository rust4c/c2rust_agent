"""
C工程预处理模块使用示例

展示如何使用CProjectPreprocessor进行C工程的预处理
"""

import sys
import argparse
from pathlib import Path

# 添加项目根目录到Python路径
sys.path.append(str(Path(__file__).parent.parent.parent.parent))

from src.modules.Preprocessing.CProjectPreprocessor import CProjectPreprocessor, PreprocessConfig


def create_sample_config():
    """创建示例配置"""
    return PreprocessConfig(
        WORKER_COUNT=4,  # 使用4个工作线程
        PAIRING_RULES=[
            (r"(.*)\.c", r"\1.h"),  # 标准配对: module.c ↔ module.h
            (r"src/(.*)_impl\.c", r"include/\1\.h"),  # 自定义配对: src/xxx_impl.c ↔ include/xxx.h
            (r"lib/(.*)\.c", r"include/lib/\1\.h"),  # 库文件配对
        ],
        EXCLUDE_PATTERNS=[
            "*.bak", 
            "*.tmp",
            "*.swp",
            "__pycache__/*",
            "*.pyc",
            ".git/*",
            ".svn/*",
            "*.o",
            "*.obj",
            "*.exe",
            "*.dll",
            "*.so",
            "build/*",
            "debug/*",
            "release/*",
        ],
        HEADER_EXTENSIONS=[".h", ".hpp", ".hh", ".hxx", ".h++"],
        SOURCE_EXTENSIONS=[".c", ".cc", ".cpp", ".cxx", ".c++"],
        LARGE_FILE_THRESHOLD=50 * 1024 * 1024,  # 50MB
        CHUNK_SIZE=8 * 1024 * 1024,  # 8MB 块大小
        MIN_DISK_SPACE=500 * 1024 * 1024,  # 最少需要500MB空间
    )


def preprocess_project_example(source_dir: str, cache_dir: str):
    """
    预处理项目示例
    
    Args:
        source_dir: 源C工程目录
        cache_dir: 缓存输出目录
    """
    print("=" * 60)
    print("C工程预处理示例")
    print("=" * 60)
    
    # 创建配置
    config = create_sample_config()
    print(f"配置信息:")
    print(f"  工作线程数: {config.WORKER_COUNT}")
    print(f"  配对规则数: {len(config.PAIRING_RULES or [])}")
    print(f"  排除模式数: {len(config.EXCLUDE_PATTERNS or [])}")
    print(f"  大文件阈值: {config.LARGE_FILE_THRESHOLD / (1024*1024):.1f}MB")
    print()
    
    # 创建预处理器
    preprocessor = CProjectPreprocessor(config)
    
    # 执行预处理
    print(f"开始预处理:")
    print(f"  源目录: {source_dir}")
    print(f"  缓存目录: {cache_dir}")
    print()
    
    success, stats = preprocessor.preprocess_project(source_dir, cache_dir)
    
    # 显示结果
    print("\n" + "=" * 60)
    print("预处理结果")
    print("=" * 60)
    
    if success:
        print("✅ 预处理成功完成!")
        print()
        print("统计信息:")
        print(f"  总文件数: {stats.total_files}")
        print(f"  配对文件: {stats.processed_pairs} 对")
        print(f"  仅头文件: {stats.header_only}")
        print(f"  仅源文件: {stats.source_only}")
        print(f"  其他文件: {stats.misc_files}")
        print(f"  跳过文件: {stats.skipped_files}")
        print(f"  处理耗时: {stats.processing_time:.2f}秒")
        print(f"  总数据量: {_format_size(stats.total_size)}")
        
        if stats.total_files > 0:
            throughput = stats.total_size / stats.processing_time if stats.processing_time > 0 else 0
            print(f"  处理速度: {_format_size(int(throughput))}/秒")
        
        print()
        print("输出目录结构:")
        _show_output_structure(cache_dir)
        
    else:
        print("❌ 预处理失败!")
        if stats.errors:
            print("\n错误信息:")
            for i, error in enumerate(stats.errors, 1):
                print(f"  {i}. {error}")


def _format_size(size_bytes: int) -> str:
    """格式化文件大小"""
    size = float(size_bytes)
    for unit in ['B', 'KB', 'MB', 'GB', 'TB']:
        if size < 1024.0:
            return f"{size:.2f} {unit}"
        size /= 1024.0
    return f"{size:.2f} PB"


def _show_output_structure(cache_dir: str):
    """显示输出目录结构"""
    cache_path = Path(cache_dir)
    
    if not cache_path.exists():
        print("  缓存目录不存在")
        return
    
    def _print_tree(path: Path, indent: str = "  "):
        """递归打印目录树"""
        try:
            items = list(path.iterdir())
            items.sort(key=lambda x: (x.is_file(), x.name))
            
            for i, item in enumerate(items):
                is_last = i == len(items) - 1
                current_indent = "└── " if is_last else "├── "
                
                if item.is_dir():
                    print(f"{indent}{current_indent}{item.name}/")
                    
                    # 显示目录下的文件数量
                    try:
                        file_count = len([f for f in item.rglob("*") if f.is_file()])
                        if file_count > 0:
                            print(f"{indent}{'    ' if is_last else '│   '}({file_count} 个文件)")
                    except:
                        pass
                    
                    # 只显示前两层
                    if len(path.parts) - len(cache_path.parts) < 2:
                        next_indent = indent + ("    " if is_last else "│   ")
                        _print_tree(item, next_indent)
                else:
                    size_info = ""
                    try:
                        size = item.stat().st_size
                        size_info = f" ({_format_size(size)})"
                    except:
                        pass
                    print(f"{indent}{current_indent}{item.name}{size_info}")
        except PermissionError:
            print(f"{indent}(权限不足)")
    
    print(f"  {cache_path.name}/")
    _print_tree(cache_path)


def create_test_project(test_dir: str):
    """创建测试项目"""
    test_path = Path(test_dir)
    test_path.mkdir(parents=True, exist_ok=True)
    
    # 创建测试文件
    files_to_create = [
        ("src/main.c", "#include <stdio.h>\nint main() { return 0; }"),
        ("src/utils.c", "#include \"utils.h\"\nvoid helper() {}"),
        ("src/utils.h", "#ifndef UTILS_H\n#define UTILS_H\nvoid helper();\n#endif"),
        ("include/common.h", "#ifndef COMMON_H\n#define COMMON_H\n#define VERSION 1\n#endif"),
        ("lib/math_impl.c", "#include \"../include/lib/math.h\"\nint add(int a, int b) { return a + b; }"),
        ("include/lib/math.h", "#ifndef MATH_H\n#define MATH_H\nint add(int a, int b);\n#endif"),
        ("config.txt", "# Configuration file\ndebug=1"),
        ("README.md", "# Test Project\nThis is a test project."),
        ("build/temp.o", "binary data"),  # 应该被排除
        (".git/config", "git config"),  # 应该被排除
    ]
    
    for file_path, content in files_to_create:
        full_path = test_path / file_path
        full_path.parent.mkdir(parents=True, exist_ok=True)
        full_path.write_text(content, encoding='utf-8')
    
    print(f"✅ 测试项目已创建在: {test_dir}")
    return test_dir


def main():
    """主函数"""
    parser = argparse.ArgumentParser(description="C工程预处理示例")
    parser.add_argument("--source", "-s", help="源目录路径")
    parser.add_argument("--cache", "-c", help="缓存目录路径")
    parser.add_argument("--create-test", action="store_true", help="创建测试项目")
    parser.add_argument("--test-dir", default="./test_project", help="测试项目目录")
    
    args = parser.parse_args()
    
    if args.create_test:
        # 创建测试项目
        test_dir = create_test_project(args.test_dir)
        cache_dir = str(Path(args.test_dir).parent / "test_cache")
        preprocess_project_example(test_dir, cache_dir)
    elif args.source and args.cache:
        # 使用指定的源目录和缓存目录
        preprocess_project_example(args.source, args.cache)
    else:
        # 显示帮助
        print("C工程预处理模块使用示例")
        print()
        print("使用方法:")
        print("1. 创建测试项目并运行:")
        print("   python example_usage.py --create-test")
        print()
        print("2. 处理指定项目:")
        print("   python example_usage.py --source /path/to/c/project --cache /path/to/cache")
        print()
        parser.print_help()


if __name__ == "__main__":
    main()
