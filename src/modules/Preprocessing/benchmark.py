"""
C工程预处理模块性能基准测试

用于测试预处理模块在不同规模项目上的性能表现
"""

from src.modules.Preprocessing.CProjectPreprocessor import CProjectPreprocessor, PreprocessConfig
import os
import time
import tempfile
import shutil
from pathlib import Path
from typing import List, Tuple
import sys

# 添加项目根目录到Python路径
sys.path.append(str(Path(__file__).parent.parent.parent.parent))


class BenchmarkSuite:
    """性能基准测试套件"""

    def __init__(self):
        self.results: List[dict] = []

    def create_test_project(self, temp_dir: Path, num_files: int, file_size: int = 1024) -> str:
        """
        创建测试项目

        Args:
            temp_dir: 临时目录
            num_files: 文件数量
            file_size: 单个文件大小（字节）

        Returns:
            项目目录路径
        """
        project_dir = temp_dir / f"test_project_{num_files}"
        project_dir.mkdir(parents=True, exist_ok=True)

        # 生成文件内容
        c_content = "/* C source file */\n" + \
            "int dummy_var = 0;\n" * (file_size // 20)
        h_content = "/* Header file */\n" + \
            "#define DUMMY_MACRO 1\n" * (file_size // 25)
        other_content = "# Config file\n" + \
            "setting=value\n" * (file_size // 15)

        # 创建配对文件 (60%的文件)
        pair_count = int(num_files * 0.6 / 2)
        for i in range(pair_count):
            base_name = f"module_{i:04d}"

            # 创建子目录结构
            sub_dir = project_dir / f"src/level_{i % 5}"
            sub_dir.mkdir(parents=True, exist_ok=True)

            (sub_dir / f"{base_name}.c").write_text(c_content)
            (sub_dir / f"{base_name}.h").write_text(h_content)

        # 创建独立头文件 (20%的文件)
        header_only_count = int(num_files * 0.2)
        for i in range(header_only_count):
            header_dir = project_dir / f"include/level_{i % 3}"
            header_dir.mkdir(parents=True, exist_ok=True)
            (header_dir / f"header_{i:04d}.h").write_text(h_content)

        # 创建独立源文件 (10%的文件)
        source_only_count = int(num_files * 0.1)
        for i in range(source_only_count):
            source_dir = project_dir / f"src/standalone"
            source_dir.mkdir(parents=True, exist_ok=True)
            (source_dir / f"standalone_{i:04d}.c").write_text(c_content)

        # 创建其他文件 (10%的文件)
        misc_count = num_files - (pair_count * 2) - \
            header_only_count - source_only_count
        for i in range(misc_count):
            misc_dir = project_dir / f"config"
            misc_dir.mkdir(parents=True, exist_ok=True)
            (misc_dir / f"config_{i:04d}.txt").write_text(other_content)

        return str(project_dir)

    def run_benchmark(self, project_dir: str, cache_dir: str, description: str) -> dict:
        """
        运行单个基准测试

        Args:
            project_dir: 项目目录
            cache_dir: 缓存目录
            description: 测试描述

        Returns:
            基准测试结果
        """
        print(f"\n运行基准测试: {description}")
        print("-" * 50)

        # 创建预处理器
        config = PreprocessConfig(
            WORKER_COUNT=4,
            PAIRING_RULES=[(r"(.*)\.c", r"\1.h")],
            EXCLUDE_PATTERNS=["*.bak", "*.tmp"],
        )

        preprocessor = CProjectPreprocessor(config)

        # 预热（避免首次运行的开销）
        temp_cache = Path(cache_dir).parent / "warmup_cache"
        preprocessor.preprocess_project(project_dir, str(temp_cache))
        shutil.rmtree(temp_cache, ignore_errors=True)

        # 正式测试
        start_time = time.time()
        start_memory = self._get_memory_usage()

        success, stats = preprocessor.preprocess_project(
            project_dir, cache_dir)

        end_time = time.time()
        end_memory = self._get_memory_usage()

        # 计算性能指标
        total_time = end_time - start_time
        memory_delta = end_memory - start_memory
        throughput = stats.total_size / total_time if total_time > 0 else 0
        files_per_second = stats.total_files / total_time if total_time > 0 else 0

        result = {
            "description": description,
            "success": success,
            "total_files": stats.total_files,
            "total_size_mb": stats.total_size / (1024 * 1024),
            "processing_time_sec": total_time,
            "memory_delta_mb": memory_delta,
            "throughput_mb_per_sec": throughput / (1024 * 1024),
            "files_per_second": files_per_second,
            "processed_pairs": stats.processed_pairs,
            "header_only": stats.header_only,
            "source_only": stats.source_only,
            "misc_files": stats.misc_files,
            "errors": len(stats.errors) if stats.errors else 0
        }

        # 显示结果
        self._print_result(result)

        return result

    def _get_memory_usage(self) -> float:
        """获取当前内存使用量（MB）"""
        try:
            import psutil
            process = psutil.Process(os.getpid())
            return process.memory_info().rss / (1024 * 1024)
        except ImportError:
            return 0.0

    def _print_result(self, result: dict):
        """打印基准测试结果"""
        print(f"✅ 成功: {result['success']}")
        print(f"📁 总文件数: {result['total_files']}")
        print(f"📊 总大小: {result['total_size_mb']:.2f} MB")
        print(f"⏱️  处理时间: {result['processing_time_sec']:.3f} 秒")
        print(f"🚀 处理速度: {result['throughput_mb_per_sec']:.2f} MB/秒")
        print(f"📈 文件速度: {result['files_per_second']:.1f} 文件/秒")
        print(f"💾 内存变化: {result['memory_delta_mb']:.2f} MB")
        print(f"🔗 配对文件: {result['processed_pairs']} 对")
        print(f"❌ 错误数: {result['errors']}")

    def run_all_benchmarks(self):
        """运行所有基准测试"""
        print("=" * 60)
        print("C工程预处理模块性能基准测试")
        print("=" * 60)

        # 测试场景配置
        test_scenarios = [
            (50, 1024, "小型项目 (50个文件, 1KB/文件)"),
            (200, 2048, "中小型项目 (200个文件, 2KB/文件)"),
            (500, 4096, "中型项目 (500个文件, 4KB/文件)"),
            (1000, 8192, "大型项目 (1000个文件, 8KB/文件)"),
            (100, 1024*1024, "大文件项目 (100个文件, 1MB/文件)"),
        ]

        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)

            for file_count, file_size, description in test_scenarios:
                try:
                    # 创建测试项目
                    project_dir = self.create_test_project(
                        temp_path, file_count, file_size)
                    cache_dir = str(temp_path / f"cache_{file_count}")

                    # 运行基准测试
                    result = self.run_benchmark(
                        project_dir, cache_dir, description)
                    self.results.append(result)

                    # 清理缓存目录
                    shutil.rmtree(cache_dir, ignore_errors=True)

                except Exception as e:
                    print(f"❌ 基准测试失败: {description} - {e}")
                    self.results.append({
                        "description": description,
                        "success": False,
                        "error": str(e)
                    })

        # 显示汇总结果
        self._print_summary()

    def _print_summary(self):
        """打印汇总结果"""
        print("\n" + "=" * 60)
        print("基准测试汇总")
        print("=" * 60)

        successful_results = [
            r for r in self.results if r.get('success', False)]

        if not successful_results:
            print("❌ 没有成功的测试结果")
            return

        # 计算汇总统计
        total_files = sum(r['total_files'] for r in successful_results)
        total_size = sum(r['total_size_mb'] for r in successful_results)
        total_time = sum(r['processing_time_sec'] for r in successful_results)
        avg_throughput = sum(r['throughput_mb_per_sec']
                             for r in successful_results) / len(successful_results)
        avg_files_per_sec = sum(r['files_per_second']
                                for r in successful_results) / len(successful_results)

        print(f"📊 测试总数: {len(self.results)}")
        print(f"✅ 成功测试: {len(successful_results)}")
        print(f"📁 总处理文件: {total_files}")
        print(f"📦 总处理大小: {total_size:.2f} MB")
        print(f"⏱️  总处理时间: {total_time:.3f} 秒")
        print(f"🚀 平均吞吐量: {avg_throughput:.2f} MB/秒")
        print(f"📈 平均文件速度: {avg_files_per_sec:.1f} 文件/秒")

        # 显示详细表格
        print(f"\n详细结果:")
        print(
            f"{'描述':<25} {'文件数':<8} {'大小(MB)':<10} {'时间(s)':<10} {'速度(MB/s)':<12} {'文件/s':<10}")
        print("-" * 75)

        for result in successful_results:
            print(f"{result['description']:<25} "
                  f"{result['total_files']:<8} "
                  f"{result['total_size_mb']:<10.2f} "
                  f"{result['processing_time_sec']:<10.3f} "
                  f"{result['throughput_mb_per_sec']:<12.2f} "
                  f"{result['files_per_second']:<10.1f}")


def main():
    """主函数"""
    print("开始性能基准测试...")

    benchmark = BenchmarkSuite()
    benchmark.run_all_benchmarks()

    print("\n🎉 基准测试完成!")


if __name__ == "__main__":
    main()
