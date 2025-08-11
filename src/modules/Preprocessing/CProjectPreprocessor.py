"""
C工程预处理模块

功能包括：
1. 目录拷贝：将源目录完整复制到指定Cache目录
2. 头文件处理：识别有对应.c文件的头文件并移动到专用目录
3. 剩余文件处理：将无配对的文件分类到不同目录
4. 并行处理：使用多线程进行文件拷贝优化
5. 进度反馈：实时进度条和详细日志
6. 异常处理：文件权限、磁盘空间等错误处理
"""

import os
import json
import re
import shutil
import errno
import hashlib
import threading
from typing import Dict, List, Optional, Tuple, Any, Set
from pathlib import Path
from dataclasses import dataclass, asdict
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime
import logging
from tqdm import tqdm

try:
    import psutil
    HAS_PSUTIL = True
except ImportError:
    HAS_PSUTIL = False

from ...base.Base import Base


@dataclass
class PreprocessConfig(Base):
    """预处理配置类"""
    # 并行工作者数量 (0=自动检测)
    WORKER_COUNT: int = 0

    # 文件配对规则: [(源模式, 目标模式)]
    PAIRING_RULES: Optional[List[Tuple[str, str]]] = None

    # 排除文件模式
    EXCLUDE_PATTERNS: Optional[List[str]] = None

    # 特殊处理文件扩展名
    HEADER_EXTENSIONS: Optional[List[str]] = None
    SOURCE_EXTENSIONS: Optional[List[str]] = None

    # 大文件阈值 (字节)
    LARGE_FILE_THRESHOLD: int = 1024 * 1024 * 100  # 100MB

    # 块大小 (字节)
    CHUNK_SIZE: int = 16 * 1024 * 1024  # 16MB

    # 最小磁盘空间要求 (字节)
    MIN_DISK_SPACE: int = 1024 * 1024 * 1024  # 1GB

    def __post_init__(self):
        if self.WORKER_COUNT == 0:
            self.WORKER_COUNT = os.cpu_count() or 4

        if self.PAIRING_RULES is None:
            self.PAIRING_RULES = [
                (r"(.*)\.c", r"\1.h"),  # 默认规则: module.c ↔ module.h
                (r"src/(.*)_impl\.c", r"include/\1\.h")  # 自定义规则
            ]

        if self.EXCLUDE_PATTERNS is None:
            self.EXCLUDE_PATTERNS = [
                "*.bak",
                "*.tmp",
                "__pycache__/*",
                "*.pyc",
                ".git/*",
                ".svn/*",
                "*.o",
                "*.obj"
            ]

        if self.HEADER_EXTENSIONS is None:
            self.HEADER_EXTENSIONS = [".h", ".hpp", ".hh", ".hxx"]

        if self.SOURCE_EXTENSIONS is None:
            self.SOURCE_EXTENSIONS = [".c", ".cc", ".cpp", ".cxx"]


@dataclass
class FileInfo:
    """文件信息"""
    path: Path
    size: int
    hash: Optional[str] = None
    is_large: bool = False

    def __post_init__(self):
        self.is_large = self.size > 100 * 1024 * 1024  # 100MB


@dataclass
class ProcessingStats:
    """处理统计信息"""
    total_files: int = 0
    processed_pairs: int = 0
    header_only: int = 0
    source_only: int = 0
    misc_files: int = 0
    skipped_files: int = 0
    processing_time: float = 0.0
    total_size: int = 0
    errors: Optional[List[str]] = None

    def __post_init__(self):
        if self.errors is None:
            self.errors = []


class DiskFullError(Exception):
    """磁盘空间不足异常"""
    pass


class CProjectPreprocessor(Base):
    """C工程预处理器"""

    def __init__(self, config: Optional[PreprocessConfig] = None):
        super().__init__()
        self.config = config or PreprocessConfig()
        self.stats = ProcessingStats()
        self._file_cache: Dict[str, FileInfo] = {}
        self._lock = threading.Lock()

        # 设置日志
        self._setup_logging()

        self.info("C工程预处理器初始化完成")
        self.info(f"工作线程数: {self.config.WORKER_COUNT}")

    def _setup_logging(self):
        """设置日志配置"""
        logging.basicConfig(
            level=logging.INFO,
            format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
            handlers=[
                logging.FileHandler('preprocessing.log'),
                logging.StreamHandler()
            ]
        )

    def preprocess_project(self, source_dir: str, cache_dir: str) -> Tuple[bool, ProcessingStats]:
        """
        预处理C工程项目

        Args:
            source_dir: 源目录路径
            cache_dir: 缓存目录路径

        Returns:
            (是否成功, 处理统计信息)
        """
        start_time = datetime.now()
        source_path = Path(source_dir)
        cache_path = Path(cache_dir)

        try:
            self.info(f"开始预处理项目: {source_dir} -> {cache_dir}")

            # 检查源目录
            if not source_path.exists() or not source_path.is_dir():
                raise ValueError(f"源目录不存在或不是目录: {source_dir}")

            # 检查磁盘空间
            self._check_disk_space(cache_path.parent)

            # 创建缓存目录结构
            self._create_cache_structure(cache_path)

            # 扫描源文件
            all_files = self._scan_source_files(source_path)
            self.stats.total_files = len(all_files)
            self.stats.total_size = sum(f.size for f in all_files)

            self.info(
                f"发现 {self.stats.total_files} 个文件，总大小: {self._format_size(self.stats.total_size)}")

            # 查找文件配对
            file_pairs = self._find_file_pairs(all_files)
            self.stats.processed_pairs = len(file_pairs)

            # 处理配对文件
            paired_files = set()
            if file_pairs:
                paired_files = self._process_paired_files(
                    file_pairs, cache_path)

            # 处理剩余文件
            remaining_files = [
                f for f in all_files if f.path not in paired_files]
            self._process_remaining_files(remaining_files, cache_path)

            # 生成处理报告
            self._generate_processing_report(cache_path)

            # 计算处理时间
            end_time = datetime.now()
            self.stats.processing_time = (
                end_time - start_time).total_seconds()

            self.info(f"预处理完成，耗时: {self.stats.processing_time:.2f}秒")
            return True, self.stats

        except Exception as e:
            self.error(f"预处理失败: {e}")
            if self.stats.errors is not None:
                self.stats.errors.append(str(e))
            return False, self.stats

    def _check_disk_space(self, target_dir: Path):
        """检查磁盘空间"""
        try:
            if not HAS_PSUTIL:
                self.warning("psutil未安装，跳过磁盘空间检查")
                return

            # 获取目标目录的磁盘使用情况
            usage = psutil.disk_usage(str(target_dir))
            available_space = usage.free

            if available_space < self.config.MIN_DISK_SPACE:
                raise DiskFullError(
                    f"磁盘空间不足: 可用 {self._format_size(available_space)}, "
                    f"需要至少 {self._format_size(self.config.MIN_DISK_SPACE)}"
                )

            self.info(f"磁盘可用空间: {self._format_size(available_space)}")

        except Exception as e:
            self.warning(f"无法检查磁盘空间: {e}")

    def _create_cache_structure(self, cache_path: Path):
        """创建缓存目录结构"""
        directories = [
            cache_path,
            cache_path / "paired_files",
            cache_path / "individual_files" / "header_only",
            cache_path / "individual_files" / "source_only",
            cache_path / "individual_files" / "misc_files"
        ]

        for directory in directories:
            directory.mkdir(parents=True, exist_ok=True)
            self.debug(f"创建目录: {directory}")

    def _scan_source_files(self, source_path: Path) -> List[FileInfo]:
        """扫描源文件"""
        self.info("扫描源文件...")
        all_files = []

        for file_path in source_path.rglob("*"):
            if file_path.is_file() and not self._should_exclude_file(file_path):
                try:
                    file_info = FileInfo(
                        path=file_path,
                        size=file_path.stat().st_size
                    )
                    all_files.append(file_info)
                except (OSError, PermissionError) as e:
                    self.warning(f"无法访问文件 {file_path}: {e}")
                    self.stats.skipped_files += 1

        self.info(f"扫描完成，找到 {len(all_files)} 个有效文件")
        return all_files

    def _should_exclude_file(self, file_path: Path) -> bool:
        """检查文件是否应该被排除"""
        exclude_patterns = self.config.EXCLUDE_PATTERNS or []
        for pattern in exclude_patterns:
            if file_path.match(pattern):
                return True
        return False

    def _find_file_pairs(self, files: List[FileInfo]) -> Dict[str, Tuple[FileInfo, FileInfo]]:
        """查找匹配的.h/.c文件对"""
        self.info("查找文件配对...")

        # 分类文件
        source_extensions = self.config.SOURCE_EXTENSIONS or []
        header_extensions = self.config.HEADER_EXTENSIONS or []

        c_files = [f for f in files if f.path.suffix.lower()
                   in source_extensions]
        h_files = [f for f in files if f.path.suffix.lower()
                   in header_extensions]

        self.info(f"发现 {len(c_files)} 个源文件, {len(h_files)} 个头文件")

        pairs = {}
        pairing_rules = self.config.PAIRING_RULES or []

        for c_file in c_files:
            for pattern, replacement in pairing_rules:
                c_relative = str(c_file.path.relative_to(c_file.path.parts[0]))

                if re.match(pattern, c_relative):
                    expected_h_name = re.sub(pattern, replacement, c_relative)
                    h_file = self._find_matching_header(
                        h_files, expected_h_name)

                    if h_file:
                        base_name = c_file.path.stem
                        # 处理重名情况
                        unique_base = self._make_unique_basename(
                            base_name, pairs)
                        pairs[unique_base] = (c_file, h_file)
                        self.debug(
                            f"配对成功: {c_file.path.name} <-> {h_file.path.name}")
                        break

        self.info(f"找到 {len(pairs)} 个文件配对")
        return pairs

    def _find_matching_header(self, h_files: List[FileInfo], expected_name: str) -> Optional[FileInfo]:
        """查找匹配的头文件"""
        for h_file in h_files:
            h_relative = str(h_file.path.relative_to(h_file.path.parts[0]))
            if h_relative == expected_name or h_file.path.name == Path(expected_name).name:
                return h_file
        return None

    def _make_unique_basename(self, base_name: str, existing_pairs: Dict[str, Any]) -> str:
        """生成唯一的基础名称"""
        if base_name not in existing_pairs:
            return base_name

        counter = 1
        while f"{base_name}_{counter}" in existing_pairs:
            counter += 1

        return f"{base_name}_{counter}"

    def _process_paired_files(self, file_pairs: Dict[str, Tuple[FileInfo, FileInfo]],
                              cache_path: Path) -> Set[Path]:
        """处理配对文件"""
        self.info(f"处理 {len(file_pairs)} 个文件配对...")

        paired_files = set()
        tasks = []

        with ThreadPoolExecutor(max_workers=self.config.WORKER_COUNT) as executor:
            # 创建拷贝任务
            for base_name, (c_file, h_file) in file_pairs.items():
                target_dir = cache_path / "paired_files" / base_name

                tasks.append(executor.submit(
                    self._copy_file_pair, c_file, h_file, target_dir))
                paired_files.add(c_file.path)
                paired_files.add(h_file.path)

            # 显示进度
            with tqdm(total=len(tasks), desc="拷贝配对文件") as pbar:
                for future in as_completed(tasks):
                    try:
                        future.result()
                        pbar.update(1)
                    except Exception as e:
                        self.error(f"拷贝配对文件失败: {e}")
                        if self.stats.errors is not None:
                            self.stats.errors.append(str(e))

        return paired_files

    def _copy_file_pair(self, c_file: FileInfo, h_file: FileInfo, target_dir: Path):
        """拷贝文件配对"""
        target_dir.mkdir(parents=True, exist_ok=True)

        # 拷贝C文件
        c_target = target_dir / c_file.path.name
        self._safe_copy_file(c_file.path, c_target)

        # 拷贝头文件
        h_target = target_dir / h_file.path.name
        self._safe_copy_file(h_file.path, h_target)

        self.debug(f"配对文件拷贝完成: {target_dir}")

    def _process_remaining_files(self, remaining_files: List[FileInfo], cache_path: Path):
        """处理剩余文件"""
        if not remaining_files:
            return

        self.info(f"处理 {len(remaining_files)} 个剩余文件...")

        # 分类文件
        categorized = self._categorize_remaining_files(remaining_files)

        tasks = []
        with ThreadPoolExecutor(max_workers=self.config.WORKER_COUNT) as executor:
            for category, files in categorized.items():
                for file_info in files:
                    target_dir = cache_path / "individual_files" / category / file_info.path.stem
                    tasks.append(executor.submit(
                        self._copy_individual_file, file_info, target_dir))

            # 显示进度
            with tqdm(total=len(tasks), desc="拷贝剩余文件") as pbar:
                for future in as_completed(tasks):
                    try:
                        future.result()
                        pbar.update(1)
                    except Exception as e:
                        self.error(f"拷贝剩余文件失败: {e}")
                        if self.stats.errors is not None:
                            self.stats.errors.append(str(e))

    def _categorize_remaining_files(self, files: List[FileInfo]) -> Dict[str, List[FileInfo]]:
        """分类剩余文件"""
        categorized = {
            "header_only": [],
            "source_only": [],
            "misc_files": []
        }

        header_extensions = self.config.HEADER_EXTENSIONS or []
        source_extensions = self.config.SOURCE_EXTENSIONS or []

        for file_info in files:
            suffix = file_info.path.suffix.lower()

            if suffix in header_extensions:
                categorized["header_only"].append(file_info)
                self.stats.header_only += 1
            elif suffix in source_extensions:
                categorized["source_only"].append(file_info)
                self.stats.source_only += 1
            else:
                categorized["misc_files"].append(file_info)
                self.stats.misc_files += 1

        return categorized

    def _copy_individual_file(self, file_info: FileInfo, target_dir: Path):
        """拷贝单个文件"""
        target_dir.mkdir(parents=True, exist_ok=True)
        target_path = target_dir / file_info.path.name

        # 处理文件名冲突
        target_path = self._resolve_filename_conflict(target_path)

        self._safe_copy_file(file_info.path, target_path)
        self.debug(f"文件拷贝完成: {file_info.path} -> {target_path}")

    def _resolve_filename_conflict(self, target_path: Path) -> Path:
        """解决文件名冲突"""
        if not target_path.exists():
            return target_path

        counter = 1
        base_name = target_path.stem
        suffix = target_path.suffix
        parent = target_path.parent

        while True:
            new_name = f"{base_name}_{counter}{suffix}"
            new_path = parent / new_name
            if not new_path.exists():
                self.warning(f"文件名冲突，重命名为: {new_name}")
                return new_path
            counter += 1

    def _safe_copy_file(self, src_path: Path, dst_path: Path):
        """安全拷贝文件"""
        try:
            # 检查是否需要分块拷贝
            file_size = src_path.stat().st_size

            if file_size > self.config.LARGE_FILE_THRESHOLD:
                self._copy_large_file(src_path, dst_path)
            else:
                self._copy_small_file(src_path, dst_path)

            # 保留元数据
            shutil.copystat(src_path, dst_path)

        except PermissionError as e:
            self.warning(f"权限不足: {src_path} -> {dst_path}: {e}")
            raise
        except OSError as e:
            if e.errno == errno.ENOSPC:
                self.error("磁盘空间不足!")
                raise DiskFullError from e
            self.error(f"文件拷贝失败: {src_path} -> {dst_path}: {e}")
            raise

    def _copy_small_file(self, src_path: Path, dst_path: Path):
        """拷贝小文件"""
        shutil.copy2(src_path, dst_path)

    def _copy_large_file(self, src_path: Path, dst_path: Path):
        """拷贝大文件（分块）"""
        self.debug(f"使用分块模式拷贝大文件: {src_path}")

        with open(src_path, 'rb') as f_src, open(dst_path, 'wb') as f_dst:
            while chunk := f_src.read(self.config.CHUNK_SIZE):
                f_dst.write(chunk)

    def _generate_processing_report(self, cache_path: Path):
        """生成处理报告"""
        report_path = cache_path / "processing_log.txt"

        report_data = {
            "timestamp": datetime.now().isoformat(),
            "statistics": asdict(self.stats),
            "config": asdict(self.config)
        }

        try:
            # 写入JSON格式的详细报告
            json_report_path = cache_path / "processing_report.json"
            with open(json_report_path, 'w', encoding='utf-8') as f:
                json.dump(report_data, f, indent=2, ensure_ascii=False)

            # 写入可读的文本报告
            with open(report_path, 'w', encoding='utf-8') as f:
                f.write("C工程预处理报告\n")
                f.write("=" * 50 + "\n")
                f.write(
                    f"处理时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
                f.write(f"总文件数: {self.stats.total_files}\n")
                f.write(f"配对文件: {self.stats.processed_pairs}\n")
                f.write(f"仅头文件: {self.stats.header_only}\n")
                f.write(f"仅源文件: {self.stats.source_only}\n")
                f.write(f"其他文件: {self.stats.misc_files}\n")
                f.write(f"跳过文件: {self.stats.skipped_files}\n")
                f.write(f"处理耗时: {self.stats.processing_time:.2f}秒\n")
                f.write(f"总数据量: {self._format_size(self.stats.total_size)}\n")

                if self.stats.errors:
                    f.write("\n错误信息:\n")
                    for error in self.stats.errors:
                        f.write(f"  - {error}\n")

            self.info(f"处理报告已生成: {report_path}")

        except Exception as e:
            self.error(f"生成处理报告失败: {e}")

    def _format_size(self, size_bytes: int) -> str:
        """格式化文件大小"""
        size = float(size_bytes)
        for unit in ['B', 'KB', 'MB', 'GB', 'TB']:
            if size < 1024.0:
                return f"{size:.2f} {unit}"
            size /= 1024.0
        return f"{size:.2f} PB"

    def get_processing_stats(self) -> ProcessingStats:
        """获取处理统计信息"""
        return self.stats


def main():
    """主函数示例"""
    # 配置
    config = PreprocessConfig(
        WORKER_COUNT=4,
        PAIRING_RULES=[
            (r"(.*)\.c", r"\1.h"),  # 标准配对
            (r"src/(.*)_impl\.c", r"include/\1\.h"),  # 自定义配对
        ],
        EXCLUDE_PATTERNS=[
            "*.bak", "*.tmp", "__pycache__/*", ".git/*"
        ]
    )

    # 创建预处理器
    preprocessor = CProjectPreprocessor(config)

    # 执行预处理
    source_dir = "/path/to/c/project"
    cache_dir = "/path/to/cache"

    success, stats = preprocessor.preprocess_project(source_dir, cache_dir)

    if success:
        print("预处理完成!")
        print(f"总文件数: {stats.total_files}")
        print(f"配对文件: {stats.processed_pairs}")
        print(f"处理时间: {stats.processing_time:.2f}秒")
    else:
        print("预处理失败!")
        if stats.errors:
            for error in stats.errors:
                print(f"错误: {error}")


if __name__ == "__main__":
    main()
