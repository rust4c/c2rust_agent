import json
import time
import hashlib

from typing import Dict, List, Optional, Tuple, Any
from pathlib import Path
from dataclasses import dataclass, asdict
from datetime import datetime

from .CProjectPreprocessor import CProjectPreprocessor, PreprocessConfig
from .CallRelationAnalyzer import CallRelationAnalyzer
from ..FileParsing.LSPServices import ClangdAnalyzer
from ..DatebaseServer.DatabaseManager import DatabaseManager
from fastembed import TextEmbedding

from ...base.Base import Base


@dataclass
class FileMapping:
    """文件映射信息"""
    original_path: str              # 原始文件路径
    cached_path: str               # 缓存文件路径
    file_type: str                 # 文件类型: 'source', 'header', 'misc'
    pair_name: Optional[str] = None  # 配对名称(如果是配对文件)
    file_hash: Optional[str] = None  # 文件内容哈希
    file_size: int = 0             # 文件大小

    def to_dict(self) -> Dict:
        return asdict(self)


@dataclass
class AnalysisResult:
    """代码分析结果"""
    element_type: str              # 元素类型: 'function', 'struct', 'macro', 'typedef'
    name: str                      # 元素名称
    definition: str                # 完整定义
    file_path: str                 # 所在文件路径
    line_start: int                # 开始行号
    line_end: int                  # 结束行号
    signature: Optional[str] = None  # 函数签名或结构体声明
    parameters: Optional[List[Dict]] = None  # 参数信息
    return_type: Optional[str] = None  # 返回类型
    dependencies: Optional[List[str]] = None  # 依赖关系
    vector_id: Optional[str] = None  # Qdrant中的向量ID

    def to_dict(self) -> Dict:
        return asdict(self)


@dataclass
class CodeElementIndex:
    """代码元素索引"""
    element_id: str                # 唯一标识符
    analysis_result: AnalysisResult  # 分析结果
    file_mapping: FileMapping      # 文件映射
    embedding_vector: Optional[List[float]] = None  # 嵌入向量
    created_at: Optional[str] = None  # 创建时间

    def to_dict(self) -> Dict:
        return asdict(self)


class PreProcessor(Base):
    """
    增强的预处理模块
    1. 建立复制前后位置索引
    2. 分析原始代码并保存到Qdrant
    3. 建立分析结果与保存位置的索引关系
    """

    def __init__(self, db_client: DatabaseManager, cache_dir: Path):
        super().__init__()
        self.cache_dir : Path = Path(cache_dir)
        self.db_client : DatabaseManager = db_client

        # 创建预处理配置
        self.config = PreprocessConfig()
        self.c_preprocessor = CProjectPreprocessor(self.config)

        # 初始化分析器和嵌入器
        self.analyzer = None  # 将在处理时初始化
        try:
            # 使用较小的模型以匹配Qdrant配置
            self.embedder = TextEmbedding(model_name="BAAI/bge-small-en-v1.5")
            self.vector_size = 384  # BGE-small模型的向量维度
        except Exception as e:
            self.warning(f"TextEmbedding初始化失败，将使用默认配置: {e}")
            self.embedder = TextEmbedding()
            self.vector_size = 384  # 默认向量维度

        # 索引存储
        self.file_mappings: Dict[str, FileMapping] = {}        # 文件映射索引
        self.analysis_results: Dict[str, AnalysisResult] = {}  # 分析结果索引
        self.element_indices: Dict[str, CodeElementIndex] = {}  # 代码元素索引

        # 统计信息
        self.stats = {
            "total_files": 0,
            "analyzed_files": 0,
            "total_elements": 0,
            "functions": 0,
            "structs": 0,
            "macros": 0,
            "typedefs": 0,
            "processing_time": 0.0
        }

        self.info("增强预处理模块初始化完成")

    def process_project(self, project_dir: str) -> Tuple[bool, Dict]:
        """
        完整处理C项目：复制、分析、索引、保存

        Args:
            project_dir: 项目目录路径

        Returns:
            (是否成功, 处理统计信息)
        """
        start_time = datetime.now()
        project_path = Path(project_dir)

        try:
            self.info(f"开始完整处理项目: {project_dir}")

            # 第一步：建立文件映射和复制
            self.info("步骤1: 建立文件映射并复制文件")
            success = self._create_file_mappings_and_copy(project_path)
            if not success:
                return False, self.stats

            # 第二步：分析原始文件
            self.info("步骤2: 分析原始文件")
            success, result = self._analyze_project(str(project_path))

            if not success:
                return False, self.stats
            
            # 分析依赖关系
            self.info("分析依赖关系")
            success = self._analyze_relations(project_path)
            if not success:
                return False, self.stats

            # 转换分析结果
            self.info("转换分析结果为标准格式")
            success = self._convert_analysis_results(result)
            if not success:
                return False, self.stats

            # 第三步：保存分析结果到Qdrant
            self.info("步骤3: 保存分析结果到向量数据库")
            success = self._save_analysis_to_qdrant()
            if not success:
                return False, self.stats

            # 第四步：建立完整索引
            self.info("步骤4: 建立代码元素索引")
            success = self._build_element_indices()
            if not success:
                return False, self.stats

            # 第五步：保存索引信息
            self.info("步骤5: 保存索引信息")
            success = self._save_indices()
            if not success:
                return False, self.stats

            # 更新统计信息
            self.stats["processing_time"] = (
                datetime.now() - start_time).total_seconds()

            self.info(f"项目处理完成，共处理 {self.stats['total_elements']} 个代码元素")
            return True, self.stats

        except Exception as e:
            self.error(f"项目处理失败: {e}")
            return False, self.stats

    def _analyze_relations(self, input_dir: Path) -> bool:
        """分析项目中的调用关系"""
        try:
            self.info(f"分析项目调用关系: {input_dir}")
            db_manager = self.db_client
            analyzer = CallRelationAnalyzer(db_manager, input_dir)
            analyzer.analyze_project_relations(str(time.time()))
            self.info("调用关系分析完成")
            return True
        except Exception as e:
            self.error(f"调用关系分析失败: {e}")
            return False

    def _create_file_mappings_and_copy(self, project_path: Path) -> bool:
        """建立文件映射并复制文件"""
        try:
            # 使用现有的预处理器进行文件复制
            success, copy_stats = self.c_preprocessor.preprocess_project(
                str(project_path), str(self.cache_dir)
            )

            if not success:
                self.error("文件复制失败")
                return False

            # 扫描复制后的文件，建立映射关系
            self._scan_and_map_files(project_path)

            self.stats["total_files"] = copy_stats.total_files
            self.info(f"成功建立 {len(self.file_mappings)} 个文件映射")
            return True

        except Exception as e:
            self.error(f"建立文件映射失败: {e}")
            return False

    def _scan_and_map_files(self, project_path: Path):
        """扫描并建立文件映射"""
        try:
            # 扫描缓存目录中的文件
            for cached_file in self.cache_dir.rglob("*"):
                if cached_file.is_file() and cached_file.suffix in ['.c', '.h', '.cpp', '.hpp']:
                    # 计算相对路径来找到原始文件
                    rel_path = cached_file.relative_to(self.cache_dir)

                    # 根据缓存目录结构推断原始位置
                    original_path = self._infer_original_path(
                        rel_path, project_path)

                    if original_path and original_path.exists():
                        # 计算文件哈希
                        file_hash = self._calculate_file_hash(original_path)

                        # 确定文件类型和配对信息
                        file_type, pair_name = self._determine_file_type_and_pair(
                            cached_file)

                        # 创建文件映射
                        mapping = FileMapping(
                            original_path=str(original_path),
                            cached_path=str(cached_file),
                            file_type=file_type,
                            pair_name=pair_name,
                            file_hash=file_hash,
                            file_size=original_path.stat().st_size
                        )

                        self.file_mappings[str(original_path)] = mapping

        except Exception as e:
            self.error(f"扫描文件映射失败: {e}")

    def _infer_original_path(self, rel_path: Path, project_path: Path) -> Optional[Path]:
        """根据缓存路径推断原始路径"""
        try:
            # 处理配对文件路径
            if "paired_files" in rel_path.parts:
                # paired_files/pair_name/file.c -> 需要递归搜索原始位置
                filename = rel_path.name
                for original_file in project_path.rglob(filename):
                    if original_file.is_file():
                        return original_file

            # 处理独立文件路径
            elif "individual_files" in rel_path.parts:
                filename = rel_path.name
                for original_file in project_path.rglob(filename):
                    if original_file.is_file():
                        return original_file

            return None

        except Exception as e:
            self.error(f"推断原始路径失败: {e}")
            return None

    def _determine_file_type_and_pair(self, cached_file: Path) -> Tuple[str, Optional[str]]:
        """确定文件类型和配对信息"""
        try:
            parts = cached_file.parts

            if "paired_files" in parts:
                # 找到paired_files后的目录名作为配对名
                idx = parts.index("paired_files")
                if idx + 1 < len(parts):
                    pair_name = parts[idx + 1]
                    if cached_file.suffix in ['.c', '.cpp']:
                        return "source", pair_name
                    else:
                        return "header", pair_name

            elif "individual_files" in parts:
                if "header_only" in parts:
                    return "header", None
                elif "source_only" in parts:
                    return "source", None
                else:
                    return "misc", None

            return "misc", None

        except Exception as e:
            self.error(f"确定文件类型失败: {e}")
            return "misc", None

    def _calculate_file_hash(self, file_path: Path) -> str:
        """计算文件内容哈希"""
        try:
            with open(file_path, 'rb') as f:
                return hashlib.md5(f.read()).hexdigest()
        except Exception as e:
            self.error(f"计算文件哈希失败: {e}")
            return ""

    def _analyze_project(self, project_dir: str) -> Tuple[bool, Dict]:
        """分析项目"""
        try:
            if self.analyzer is None:
                self.analyzer = ClangdAnalyzer(project_dir)
            self.analyzer.analyze_project()
            structures = self.analyzer.get_structure()
            self.info(f"成功分析项目 {project_dir}，找到 {len(structures)} 个结构体")
            classes = self.analyzer.get_classes()
            self.info(f"找到 {len(classes)} 个类")
            functions = self.analyzer.get_functions()
            self.info(f"找到 {len(functions)} 个函数")
            macros = self.analyzer.get_macros()
            self.info(f"找到 {len(macros)} 个宏")
            result = {
                "structures": structures,
                "classes": classes,
                "functions": functions,
                "macros": macros
            }
            return True, result
        except Exception as e:
            self.error(f"项目分析失败: {e}")
            return False, {}

    def _convert_analysis_results(self, analysis_data: Dict) -> bool:
        """将分析器结果转换为AnalysisResult格式"""
        try:
            total_converted = 0

            # 处理结构体 - 可能是字典格式 {key: [list_of_structs]}
            structures = analysis_data.get("structures", {})
            if isinstance(structures, dict):
                for struct_key, struct_list in structures.items():
                    if isinstance(struct_list, list):
                        for struct_info in struct_list:
                            if isinstance(struct_info, dict):
                                struct_name = struct_info.get(
                                    'name', f'struct_{total_converted}')
                                self._process_single_element(
                                    struct_name, struct_info, "struct")
                                total_converted += 1
                                self.stats["structs"] += 1

            # 处理类 - 列表格式
            classes = analysis_data.get("classes", [])
            if isinstance(classes, list):
                for class_info in classes:
                    if isinstance(class_info, dict):
                        class_name = class_info.get(
                            'name', f'class_{total_converted}')
                        self._process_single_element(
                            class_name, class_info, "class")
                        total_converted += 1
                        self.stats["structs"] += 1  # 类归类为结构体统计

            # 处理函数 - 列表格式
            functions = analysis_data.get("functions", [])
            if isinstance(functions, list):
                for func_info in functions:
                    if isinstance(func_info, dict):
                        func_name = func_info.get(
                            'name', f'function_{total_converted}')
                        self._process_single_element(
                            func_name, func_info, "function")
                        total_converted += 1
                        self.stats["functions"] += 1

            # 处理宏 - 列表格式
            macros = analysis_data.get("macros", [])
            if isinstance(macros, list):
                for macro_info in macros:
                    if isinstance(macro_info, dict):
                        macro_name = macro_info.get(
                            'name', f'macro_{total_converted}')
                        self._process_single_element(
                            macro_name, macro_info, "macro")
                        total_converted += 1
                        self.stats["macros"] += 1

            self.stats["total_elements"] = total_converted
            self.info(f"成功转换 {total_converted} 个代码元素")
            return True

        except Exception as e:
            self.error(f"转换分析结果失败: {e}")
            return False

    def _convert_element_to_analysis_result(self, name: str, element_info: Dict, element_type: str):
        """将单个元素转换为AnalysisResult"""
        try:
            # 处理结构体数据的特殊情况（key-value形式，value是列表）
            if element_type == "struct" and isinstance(element_info, list):
                # 如果element_info是列表，处理列表中的每个元素
                for item in element_info:
                    if isinstance(item, dict):
                        self._process_single_element(name, item, element_type)
                return

            # 处理单个元素
            self._process_single_element(name, element_info, element_type)

        except Exception as e:
            self.error(f"转换元素 {name} 失败: {e}")

    def _process_single_element(self, name: str, element_info: Dict, element_type: str):
        """处理单个代码元素"""
        try:
            # 获取文件路径信息
            file_path = element_info.get(
                'file_path', element_info.get('file', ''))

            # 查找对应的文件映射
            file_mapping = self.file_mappings.get(file_path)
            if not file_mapping:
                # 尝试通过文件名查找映射
                file_name = Path(file_path).name if file_path else 'unknown'
                for mapped_path, mapping in self.file_mappings.items():
                    if Path(mapped_path).name == file_name:
                        file_mapping = mapping
                        file_path = mapped_path
                        break

            if not file_mapping:
                # 如果找不到映射，创建一个默认的文件哈希
                file_hash = hashlib.md5(file_path.encode()).hexdigest()[:8]
            else:
                file_hash = file_mapping.file_hash

            # 处理参数信息
            parameters = []
            if element_type == "function" and 'parameters' in element_info:
                param_data = element_info['parameters']
                if isinstance(param_data, list):
                    for param in param_data:
                        if isinstance(param, dict):
                            parameters.append({
                                "name": param.get('name', 'unknown'),
                                "type": param.get('type', 'unknown')
                            })
                        else:
                            parameters.append(
                                {"name": str(param), "type": "unknown"})

            # 创建AnalysisResult
            analysis_result = AnalysisResult(
                element_type=element_type,
                name=name,
                definition=element_info.get('definition', element_info.get(
                    'code', f"{element_type} {name}")),
                file_path=file_path,
                line_start=element_info.get(
                    'line_start', element_info.get('line', 0)),
                line_end=element_info.get(
                    'line_end', element_info.get('line', 0)),
                signature=element_info.get(
                    'signature', element_info.get('definition', '')),
                parameters=parameters if parameters else None,
                return_type=element_info.get('return_type'),
                dependencies=element_info.get('dependencies')
            )

            # 生成唯一ID
            element_id = f"{element_type}_{file_hash}_{name}_{analysis_result.line_start}"
            self.analysis_results[element_id] = analysis_result

        except Exception as e:
            self.error(f"处理元素 {name} 失败: {e}")

    def _analyze_original_files(self, project_path: Path) -> bool:
        """分析原始文件"""
        try:
            # 初始化分析器
            self.analyzer = ClangdAnalyzer(str(project_path))

            # 生成编译数据库
            if not self.analyzer.generate_compile_commands():
                self.warning("生成编译数据库失败，将尝试手动分析")

            analyzed_count = 0

            # 分析每个映射的原始文件
            for original_path, mapping in self.file_mappings.items():
                if mapping.file_type in ['source', 'header']:
                    try:
                        self._analyze_single_file(Path(original_path), mapping)
                        analyzed_count += 1
                    except Exception as e:
                        self.warning(f"分析文件 {original_path} 失败: {e}")

            self.stats["analyzed_files"] = analyzed_count
            self.info(f"成功分析 {analyzed_count} 个文件")
            return True

        except Exception as e:
            self.error(f"文件分析失败: {e}")
            return False

    def _analyze_single_file(self, file_path: Path, mapping: FileMapping):
        """分析单个文件"""
        try:
            # 确保分析器已初始化
            if self.analyzer is None:
                self.warning(f"分析器未初始化，跳过文件分析: {file_path}")
                return

            # 尝试使用clang AST分析文件
            analysis_result = None
            if hasattr(self.analyzer, 'analyze_with_clang_ast'):
                try:
                    analysis_result = self.analyzer.analyze_with_clang_ast(
                        str(file_path))
                except Exception as clang_error:
                    self.warning(f"Clang分析失败: {clang_error}")

            if analysis_result:
                # 处理函数
                functions = analysis_result.get('functions', [])
                for func in functions:
                    self._process_function_dict(func, mapping)

                # 处理结构体
                structs = analysis_result.get('structs', [])
                for struct in structs:
                    self._process_struct_dict(struct, mapping)

                # 处理其他类型（typedef, enum等）
                typedefs = analysis_result.get('typedefs', [])
                for typedef in typedefs:
                    self._process_typedef_dict(typedef, mapping)
            else:
                # 如果clang分析失败，使用备用方法
                self._fallback_analyze_file(file_path, mapping)

        except Exception as e:
            self.warning(f"分析文件 {file_path} 失败，尝试备用方法: {e}")
            self._fallback_analyze_file(file_path, mapping)

    def _fallback_analyze_file(self, file_path: Path, mapping: FileMapping):
        """备用的简单文本分析方法"""
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            # 简单的函数匹配
            import re
            func_pattern = r'(\w+\s+)+(\w+)\s*\([^)]*\)\s*\{'
            matches = re.finditer(func_pattern, content, re.MULTILINE)

            for match in matches:
                func_name = match.group(2)
                line_num = content[:match.start()].count('\n') + 1

                # 创建简单的分析结果
                result = AnalysisResult(
                    element_type="function",
                    name=func_name,
                    definition=match.group(0),
                    file_path=mapping.original_path,
                    line_start=line_num,
                    line_end=line_num,
                    signature=match.group(0)
                )

                element_id = f"func_{mapping.file_hash}_{func_name}_{line_num}"
                self.analysis_results[element_id] = result
                self.stats["functions"] += 1
                self.stats["total_elements"] += 1

        except Exception as e:
            self.error(f"备用分析失败 {file_path}: {e}")

    def _process_function_dict(self, func_dict: Dict, mapping: FileMapping):
        """处理函数信息字典"""
        try:
            func_name = func_dict.get('name', 'unknown')
            definition = func_dict.get('definition', '')

            # 提取参数信息
            parameters = []
            for param in func_dict.get('parameters', []):
                parameters.append({
                    "name": param.get('name', 'unknown'),
                    "type": param.get('type', 'unknown')
                })

            # 创建分析结果
            result = AnalysisResult(
                element_type="function",
                name=func_name,
                definition=definition,
                file_path=mapping.original_path,
                line_start=func_dict.get('line_start', 0),
                line_end=func_dict.get('line_end', 0),
                signature=func_dict.get('signature', definition),
                parameters=parameters,
                return_type=func_dict.get('return_type', 'void')
            )

            element_id = f"func_{mapping.file_hash}_{func_name}"
            self.analysis_results[element_id] = result
            self.stats["functions"] += 1
            self.stats["total_elements"] += 1

        except Exception as e:
            self.error(f"处理函数信息失败: {e}")

    def _process_struct_dict(self, struct_dict: Dict, mapping: FileMapping):
        """处理结构体信息字典"""
        try:
            struct_name = struct_dict.get('name', 'unknown')
            definition = struct_dict.get('definition', '')

            # 创建分析结果
            result = AnalysisResult(
                element_type="struct",
                name=struct_name,
                definition=definition,
                file_path=mapping.original_path,
                line_start=struct_dict.get('line_start', 0),
                line_end=struct_dict.get('line_end', 0)
            )

            element_id = f"struct_{mapping.file_hash}_{struct_name}"
            self.analysis_results[element_id] = result
            self.stats["structs"] += 1
            self.stats["total_elements"] += 1

        except Exception as e:
            self.error(f"处理结构体信息失败: {e}")

    def _process_typedef_dict(self, typedef_dict: Dict, mapping: FileMapping):
        """处理typedef信息字典"""
        try:
            typedef_name = typedef_dict.get('name', 'unknown')
            definition = typedef_dict.get('definition', '')

            # 创建分析结果
            result = AnalysisResult(
                element_type="typedef",
                name=typedef_name,
                definition=definition,
                file_path=mapping.original_path,
                line_start=typedef_dict.get('line_start', 0),
                line_end=typedef_dict.get('line_end', 0)
            )

            element_id = f"typedef_{mapping.file_hash}_{typedef_name}"
            self.analysis_results[element_id] = result
            self.stats["typedefs"] += 1
            self.stats["total_elements"] += 1

        except Exception as e:
            self.error(f"处理typedef信息失败: {e}")

    def _save_analysis_to_qdrant(self) -> bool:
        """保存分析结果到Qdrant"""
        try:
            if not self.analysis_results:
                self.warning("没有分析结果需要保存")
                return True

            # 准备批量保存数据
            interfaces_data = []
            saved_count = 0

            for element_id, analysis in self.analysis_results.items():
                try:
                    # 生成嵌入向量
                    text_content = f"{analysis.name} {analysis.definition}"
                    embedding_iter = self.embedder.embed([text_content])

                    # 将迭代器转换为列表并获取第一个向量
                    embedding_list = list(embedding_iter)
                    if not embedding_list:
                        self.warning(f"无法为元素 {element_id} 生成嵌入向量")
                        continue

                    vector = embedding_list[0].tolist()

                    # 准备接口数据
                    interface_data = {
                        "name": analysis.name,
                        "inputs": analysis.parameters or [],
                        "outputs": [{"type": analysis.return_type}] if analysis.return_type else [],
                        "file_path": analysis.file_path,
                        "code": analysis.definition,
                        "vector": vector,
                        "language": "c",
                        "project_name": Path(analysis.file_path).parent.name,
                        "metadata": {
                            "element_type": analysis.element_type,
                            "element_id": element_id,
                            "line_start": analysis.line_start,
                            "line_end": analysis.line_end,
                            "signature": analysis.signature
                        }
                    }

                    interfaces_data.append(interface_data)

                except Exception as e:
                    self.warning(f"准备元素 {element_id} 数据失败: {e}")

            # 批量保存到数据库
            if interfaces_data:
                self.info(f"开始批量保存 {len(interfaces_data)} 个代码元素")
                results = self.db_client.batch_store_interfaces(
                    interfaces_data)

                # 更新分析结果中的向量ID
                for i, (interface_id, qdrant_id) in enumerate(results):
                    if i < len(list(self.analysis_results.values())):
                        analysis = list(self.analysis_results.values())[i]
                        analysis.vector_id = qdrant_id
                        saved_count += 1

            self.info(f"成功保存 {saved_count} 个代码元素到向量数据库")
            return True

        except Exception as e:
            self.error(f"保存到Qdrant失败: {e}")
            return False

    def _build_element_indices(self) -> bool:
        """建立代码元素索引"""
        try:
            for element_id, analysis in self.analysis_results.items():
                # 找到对应的文件映射
                file_mapping = self.file_mappings.get(analysis.file_path)
                if not file_mapping:
                    self.warning(f"未找到文件映射: {analysis.file_path}")
                    continue

                # 创建代码元素索引
                index = CodeElementIndex(
                    element_id=element_id,
                    analysis_result=analysis,
                    file_mapping=file_mapping,
                    created_at=datetime.now().isoformat()
                )

                self.element_indices[element_id] = index

            self.info(f"成功建立 {len(self.element_indices)} 个代码元素索引")
            return True

        except Exception as e:
            self.error(f"建立元素索引失败: {e}")
            return False

    def _save_indices(self) -> bool:
        """保存索引信息到文件"""
        try:
            index_dir = self.cache_dir / "indices"
            index_dir.mkdir(exist_ok=True)

            # 保存文件映射索引
            file_mappings_file = index_dir / "file_mappings.json"
            with open(file_mappings_file, 'w', encoding='utf-8') as f:
                mappings_dict = {k: v.to_dict()
                                 for k, v in self.file_mappings.items()}
                json.dump(mappings_dict, f, indent=2, ensure_ascii=False)

            # 保存分析结果索引
            analysis_results_file = index_dir / "analysis_results.json"
            with open(analysis_results_file, 'w', encoding='utf-8') as f:
                results_dict = {k: v.to_dict()
                                for k, v in self.analysis_results.items()}
                json.dump(results_dict, f, indent=2, ensure_ascii=False)

            # 保存代码元素索引
            element_indices_file = index_dir / "element_indices.json"
            with open(element_indices_file, 'w', encoding='utf-8') as f:
                indices_dict = {k: v.to_dict()
                                for k, v in self.element_indices.items()}
                json.dump(indices_dict, f, indent=2, ensure_ascii=False)

            # 保存统计信息
            stats_file = index_dir / "processing_stats.json"
            with open(stats_file, 'w', encoding='utf-8') as f:
                json.dump(self.stats, f, indent=2, ensure_ascii=False)

            self.info(f"成功保存索引信息到 {index_dir}")
            return True

        except Exception as e:
            self.error(f"保存索引信息失败: {e}")
            return False

    def get_element_by_name(self, name: str) -> Optional[CodeElementIndex]:
        """根据名称获取代码元素"""
        for element in self.element_indices.values():
            if element.analysis_result.name == name:
                return element
        return None

    def get_elements_by_file(self, file_path: str) -> List[CodeElementIndex]:
        """根据文件路径获取代码元素"""
        return [
            element for element in self.element_indices.values()
            if element.analysis_result.file_path == file_path
        ]

    def get_cached_file_path(self, original_path: str) -> Optional[str]:
        """获取原始文件对应的缓存路径"""
        mapping = self.file_mappings.get(original_path)
        return mapping.cached_path if mapping else None

    def load_indices(self) -> bool:
        """加载已保存的索引信息"""
        try:
            index_dir = self.cache_dir / "indices"
            if not index_dir.exists():
                return False

            # 加载文件映射
            file_mappings_file = index_dir / "file_mappings.json"
            if file_mappings_file.exists():
                with open(file_mappings_file, 'r', encoding='utf-8') as f:
                    mappings_data = json.load(f)
                    self.file_mappings = {
                        k: FileMapping(**v) for k, v in mappings_data.items()
                    }

            # 加载分析结果
            analysis_results_file = index_dir / "analysis_results.json"
            if analysis_results_file.exists():
                with open(analysis_results_file, 'r', encoding='utf-8') as f:
                    results_data = json.load(f)
                    self.analysis_results = {
                        k: AnalysisResult(**v) for k, v in results_data.items()
                    }

            # 加载代码元素索引
            element_indices_file = index_dir / "element_indices.json"
            if element_indices_file.exists():
                with open(element_indices_file, 'r', encoding='utf-8') as f:
                    indices_data = json.load(f)
                    self.element_indices = {
                        k: CodeElementIndex(
                            element_id=v['element_id'],
                            analysis_result=AnalysisResult(
                                **v['analysis_result']),
                            file_mapping=FileMapping(**v['file_mapping']),
                            embedding_vector=v.get('embedding_vector'),
                            created_at=v.get('created_at')
                        ) for k, v in indices_data.items()
                    }

            self.info("成功加载索引信息")
            return True

        except Exception as e:
            self.error(f"加载索引信息失败: {e}")
            return False

    def get_stats(self) -> Dict:
        """获取处理统计信息"""
        enhanced_stats = self.stats.copy()

        # 添加更详细的状态信息
        enhanced_stats.update({
            "file_mappings_count": len(self.file_mappings),
            "analysis_results_count": len(self.analysis_results),
            "element_indices_count": len(self.element_indices),
            "processing_status": "completed" if enhanced_stats["total_elements"] > 0 else "pending",
            "cache_directory": str(self.cache_dir),
            "database_saved": any(result.vector_id for result in self.analysis_results.values()),
            "file_types_breakdown": self._get_file_types_breakdown(),
            "elements_by_type": {
                "functions": enhanced_stats.get("functions", 0),
                "structs": enhanced_stats.get("structs", 0),
                "macros": enhanced_stats.get("macros", 0),
                "typedefs": enhanced_stats.get("typedefs", 0)
            }
        })

        return enhanced_stats

    def _get_file_types_breakdown(self) -> Dict[str, int]:
        """获取文件类型分布"""
        breakdown = {"source": 0, "header": 0, "misc": 0}
        for mapping in self.file_mappings.values():
            file_type = mapping.file_type
            if file_type in breakdown:
                breakdown[file_type] += 1
            else:
                breakdown["misc"] += 1
        return breakdown

    def get_processing_summary(self) -> Dict:
        """获取处理摘要信息"""
        summary = {
            "project_status": "completed" if self.stats["total_elements"] > 0 else "pending",
            "files_processed": {
                "total": self.stats.get("total_files", 0),
                "analyzed": self.stats.get("analyzed_files", 0),
                "mapped": len(self.file_mappings)
            },
            "elements_found": {
                "total": self.stats.get("total_elements", 0),
                "functions": self.stats.get("functions", 0),
                "structures": self.stats.get("structs", 0),
                "macros": self.stats.get("macros", 0),
                "typedefs": self.stats.get("typedefs", 0)
            },
            "database_status": {
                "elements_saved": sum(1 for result in self.analysis_results.values() if result.vector_id),
                "indices_created": len(self.element_indices),
                "vector_database_connected": hasattr(self.db_client, 'qdrant_server')
            },
            "processing_time": self.stats.get("processing_time", 0.0),
            "cache_location": str(self.cache_dir)
        }
        return summary

    def set_config(self, config: PreprocessConfig):
        """设置预处理配置"""
        self.config = config
        self.c_preprocessor.config = config
        self.info("预处理配置已更新")
