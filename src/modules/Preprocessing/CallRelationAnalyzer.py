"""
调用关系分析器

分析C/C++项目中的函数调用关系，并建立关系数据库。
"""
import re
import os
import json
from typing import Dict, List, Set, Optional, Tuple, Any
from pathlib import Path

from ..FileParsing.LSPServices import ClangdAnalyzer
from ..DatebaseServer.DatabaseManager import DatabaseManager
from ...base.Base import Base


class CallRelationAnalyzer(Base):
    """
    调用关系分析器

    功能：
    1. 分析C/C++项目中的函数调用关系
    2. 建立函数定义与调用的映射关系
    3. 跟踪跨文件的调用依赖
    4. 存储到关系数据库中
    """

    def __init__(self, db_manager: DatabaseManager, input_dir: Path):
        super().__init__()
        self.db_manager = db_manager
        self.analyzer = ClangdAnalyzer(str(input_dir))
        self.project_root = input_dir.resolve()

        # 存储分析结果
        self.function_definitions: Dict[str, Dict] = {}  # 函数名 -> 定义信息
        self.function_calls: Dict[str, List[Dict]] = {}  # 文件路径 -> 调用列表
        self.file_dependencies: Dict[str, Set[str]] = {}  # 文件依赖关系

    def analyze_project_relations(self, project_name: str):
        """
        分析整个项目的调用关系

        Args:
            project_name: 项目名称
        """
        try:
            self.info(f"开始分析项目 {project_name} 的调用关系")

            # 1. 创建关系表
            self._create_relation_tables()

            # 2. 获取所有源文件
            source_files = self._get_project_source_files()

            # 3. 分析每个文件的函数定义
            self._analyze_function_definitions(source_files)

            # 4. 分析每个文件的函数调用
            self._analyze_function_calls(source_files)

            # 5. 分析文件依赖关系
            self._analyze_file_dependencies()

            # 6. 保存关系到数据库
            self._save_relations_to_db(project_name)

            self.info(f"项目 {project_name} 调用关系分析完成")

        except Exception as e:
            self.error(f"分析项目调用关系失败: {e}")
            raise

    def _create_relation_tables(self):
        """创建关系数据库表"""
        try:
            # 直接使用SQLiteServer的方法来执行SQL
            sqlite_server = self.db_manager.sqlite_server

            # 确保数据库连接存在
            if not sqlite_server.connection:
                # 尝试重新初始化数据库连接
                try:
                    sqlite_server._init_database()
                except Exception as init_error:
                    raise RuntimeError(f"SQLite数据库连接初始化失败: {init_error}")

            # 再次检查连接
            if not sqlite_server.connection:
                raise RuntimeError("SQLite数据库连接仍然未初始化")

            # 创建函数定义表
            sqlite_server.connection.execute("""
                CREATE TABLE IF NOT EXISTS function_definitions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    function_name TEXT NOT NULL,
                    file_path TEXT NOT NULL,
                    line_number INTEGER,
                    return_type TEXT,
                    parameters TEXT,  -- JSON格式
                    signature TEXT,
                    project_name TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    UNIQUE(function_name, file_path, project_name)
                )
            """)

            # 创建函数调用表
            sqlite_server.connection.execute("""
                CREATE TABLE IF NOT EXISTS function_calls (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    caller_file TEXT NOT NULL,
                    caller_function TEXT,
                    caller_line INTEGER,
                    called_function TEXT NOT NULL,
                    called_file TEXT,
                    project_name TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            """)

            # 创建文件依赖表
            sqlite_server.connection.execute("""
                CREATE TABLE IF NOT EXISTS file_dependencies (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_file TEXT NOT NULL,
                    target_file TEXT NOT NULL,
                    dependency_type TEXT,  -- include, call, etc.
                    project_name TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    UNIQUE(source_file, target_file, project_name)
                )
            """)

            # 创建调用关系图表
            sqlite_server.connection.execute("""
                CREATE TABLE IF NOT EXISTS call_relationships (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    definition_id INTEGER,
                    call_id INTEGER,
                    relationship_type TEXT,  -- direct_call, indirect_call
                    project_name TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY (definition_id) REFERENCES function_definitions (id),
                    FOREIGN KEY (call_id) REFERENCES function_calls (id)
                )
            """)

            # 创建索引
            indexes = [
                "CREATE INDEX IF NOT EXISTS idx_func_def_name ON function_definitions(function_name)",
                "CREATE INDEX IF NOT EXISTS idx_func_def_file ON function_definitions(file_path)",
                "CREATE INDEX IF NOT EXISTS idx_func_calls_caller ON function_calls(caller_file)",
                "CREATE INDEX IF NOT EXISTS idx_func_calls_called ON function_calls(called_function)",
                "CREATE INDEX IF NOT EXISTS idx_file_deps_source ON file_dependencies(source_file)",
                "CREATE INDEX IF NOT EXISTS idx_file_deps_target ON file_dependencies(target_file)"
            ]

            for index_sql in indexes:
                sqlite_server.connection.execute(index_sql)

            sqlite_server.connection.commit()
            self.info("关系数据库表创建成功")

        except Exception as e:
            self.error(f"创建关系数据库表失败: {e}")
            raise

    def _get_project_source_files(self) -> List[str]:
        """获取项目中的所有C/C++源文件"""
        source_extensions = {'.c', '.cpp', '.cc', '.cxx', '.h', '.hpp', '.hxx'}
        source_files = []

        for root, dirs, files in os.walk(self.project_root):
            # 跳过常见的构建目录
            dirs[:] = [d for d in dirs if d not in {
                'build', '.git', '__pycache__', '.vscode'}]

            for file in files:
                if Path(file).suffix.lower() in source_extensions:
                    source_files.append(os.path.join(root, file))

        self.info(f"找到 {len(source_files)} 个源文件")
        return source_files

    def _analyze_function_definitions(self, source_files: List[str]):
        """分析函数定义"""
        for file_path in source_files:
            try:
                self._extract_function_definitions(file_path)
            except Exception as e:
                self.error(f"分析文件 {file_path} 的函数定义失败: {e}")

    def _extract_function_definitions(self, file_path: str):
        """从文件中提取函数定义"""
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            # 使用正则表达式匹配函数定义
            # 匹配模式：返回类型 函数名(参数) {
            function_pattern = r'^\s*([a-zA-Z_][a-zA-Z0-9_*\s]*)\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(([^)]*)\)\s*\{'

            lines = content.split('\n')
            for line_num, line in enumerate(lines, 1):
                match = re.match(function_pattern, line)
                if match:
                    return_type = match.group(1).strip()
                    function_name = match.group(2).strip()
                    parameters = match.group(3).strip()

                    # 跳过预处理器宏
                    if return_type.startswith('#'):
                        continue

                    # 构造函数信息
                    func_info = {
                        'name': function_name,
                        'file_path': file_path,
                        'line_number': line_num,
                        'return_type': return_type,
                        'parameters': parameters,
                        'signature': f"{return_type} {function_name}({parameters})"
                    }

                    # 存储函数定义
                    full_name = f"{function_name}@{file_path}"
                    self.function_definitions[full_name] = func_info

        except Exception as e:
            self.error(f"提取函数定义失败 {file_path}: {e}")

    def _analyze_function_calls(self, source_files: List[str]):
        """分析函数调用"""
        for file_path in source_files:
            try:
                self._extract_function_calls(file_path)
            except Exception as e:
                self.error(f"分析文件 {file_path} 的函数调用失败: {e}")

    def _extract_function_calls(self, file_path: str):
        """从文件中提取函数调用"""
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            # 初始化该文件的调用列表
            if file_path not in self.function_calls:
                self.function_calls[file_path] = []

            # 匹配函数调用模式：函数名(
            call_pattern = r'([a-zA-Z_][a-zA-Z0-9_]*)\s*\('

            lines = content.split('\n')
            current_function = None

            for line_num, line in enumerate(lines, 1):
                # 检查是否在函数定义内
                func_def_match = re.match(
                    r'^\s*[a-zA-Z_][a-zA-Z0-9_*\s]*\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*\{', line)
                if func_def_match:
                    current_function = func_def_match.group(1)

                # 查找函数调用
                calls = re.finditer(call_pattern, line)
                for call_match in calls:
                    called_function = call_match.group(1)

                    # 过滤掉一些常见的非函数调用
                    if called_function in {'if', 'while', 'for', 'switch', 'sizeof', 'typeof'}:
                        continue

                    call_info = {
                        'caller_file': file_path,
                        'caller_function': current_function,
                        'caller_line': line_num,
                        'called_function': called_function
                    }

                    self.function_calls[file_path].append(call_info)

        except Exception as e:
            self.error(f"提取函数调用失败 {file_path}: {e}")

    def _analyze_file_dependencies(self):
        """分析文件依赖关系"""
        for file_path, calls in self.function_calls.items():
            if file_path not in self.file_dependencies:
                self.file_dependencies[file_path] = set()

            # 分析#include依赖
            self._analyze_include_dependencies(file_path)

            # 分析函数调用依赖
            for call in calls:
                called_function = call['called_function']
                # 查找被调用函数的定义文件
                for func_key, func_info in self.function_definitions.items():
                    if func_info['name'] == called_function:
                        target_file = func_info['file_path']
                        if target_file != file_path:
                            self.file_dependencies[file_path].add(target_file)
                            call['called_file'] = target_file

    def _analyze_include_dependencies(self, file_path: str):
        """分析#include依赖"""
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            include_pattern = r'#include\s*[<"]([^>"]+)[>"]'
            matches = re.finditer(include_pattern, content)

            for match in matches:
                include_file = match.group(1)
                # 尝试解析为绝对路径
                possible_paths = [
                    self.project_root / include_file,
                    self.project_root / 'include' / include_file,
                    self.project_root / 'src' / include_file
                ]

                for path in possible_paths:
                    if path.exists():
                        self.file_dependencies[file_path].add(str(path))
                        break

        except Exception as e:
            self.error(f"分析include依赖失败 {file_path}: {e}")

    def _save_relations_to_db(self, project_name: str):
        """保存关系到数据库"""
        try:
            sqlite_server = self.db_manager.sqlite_server

            # 确保数据库连接存在
            if not sqlite_server.connection:
                # 尝试重新初始化数据库连接
                try:
                    sqlite_server._init_database()
                except Exception as init_error:
                    raise RuntimeError(f"SQLite数据库连接初始化失败: {init_error}")

            # 再次检查连接
            if not sqlite_server.connection:
                raise RuntimeError("SQLite数据库连接仍然未初始化")

            conn = sqlite_server.connection

            # 保存函数定义
            for func_info in self.function_definitions.values():
                conn.execute("""
                    INSERT OR REPLACE INTO function_definitions 
                    (function_name, file_path, line_number, return_type, parameters, signature, project_name)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                """, (
                    func_info['name'],
                    func_info['file_path'],
                    func_info['line_number'],
                    func_info['return_type'],
                    func_info['parameters'],
                    func_info['signature'],
                    project_name
                ))

            # 保存函数调用
            for calls in self.function_calls.values():
                for call in calls:
                    conn.execute("""
                        INSERT INTO function_calls 
                        (caller_file, caller_function, caller_line, called_function, called_file, project_name)
                        VALUES (?, ?, ?, ?, ?, ?)
                    """, (
                        call['caller_file'],
                        call.get('caller_function'),
                        call['caller_line'],
                        call['called_function'],
                        call.get('called_file'),
                        project_name
                    ))

            # 保存文件依赖
            for source_file, target_files in self.file_dependencies.items():
                for target_file in target_files:
                    conn.execute("""
                        INSERT OR REPLACE INTO file_dependencies 
                        (source_file, target_file, dependency_type, project_name)
                        VALUES (?, ?, ?, ?)
                    """, (source_file, target_file, 'call', project_name))

            conn.commit()
            self.info(
                f"成功保存 {len(self.function_definitions)} 个函数定义和 {sum(len(calls) for calls in self.function_calls.values())} 个函数调用")

        except Exception as e:
            self.error(f"保存关系到数据库失败: {e}")
            raise

    def get_function_call_graph(self, project_name: str, function_name: Optional[str] = None) -> Dict[str, Any]:
        """
        获取函数调用图

        Args:
            project_name: 项目名称
            function_name: 特定函数名（可选）

        Returns:
            调用图数据
        """
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                # 尝试重新初始化数据库连接
                try:
                    sqlite_server._init_database()
                except Exception as init_error:
                    self.error(f"数据库连接初始化失败: {init_error}")
                    return {'nodes': {}, 'edges': []}

            # 再次检查连接
            if not sqlite_server.connection:
                self.error("数据库连接仍然未初始化")
                return {'nodes': {}, 'edges': []}

            conn = sqlite_server.connection

            if function_name:
                # 获取特定函数的调用关系
                cursor = conn.execute("""
                    SELECT fc.*, fd.file_path as def_file, fd.line_number as def_line
                    FROM function_calls fc
                    LEFT JOIN function_definitions fd ON fc.called_function = fd.function_name
                    WHERE fc.project_name = ? AND (fc.called_function = ? OR fc.caller_function = ?)
                """, (project_name, function_name, function_name))
            else:
                # 获取所有调用关系
                cursor = conn.execute("""
                    SELECT fc.*, fd.file_path as def_file, fd.line_number as def_line
                    FROM function_calls fc
                    LEFT JOIN function_definitions fd ON fc.called_function = fd.function_name
                    WHERE fc.project_name = ?
                """, (project_name,))

            calls = cursor.fetchall()

            # 构建调用图
            call_graph = {
                'nodes': {},
                'edges': []
            }

            for call in calls:
                caller = call[1] or 'unknown'  # caller_function
                called = call[3]  # called_function

                # 添加节点
                if caller not in call_graph['nodes']:
                    call_graph['nodes'][caller] = {
                        'name': caller,
                        'file': call[0],  # caller_file
                        'type': 'caller'
                    }

                if called not in call_graph['nodes']:
                    call_graph['nodes'][called] = {
                        'name': called,
                        'file': call[4] or 'unknown',  # called_file
                        # def_line
                        'def_line': call[7] if len(call) > 7 else None,
                        'type': 'called'
                    }

                # 添加边
                call_graph['edges'].append({
                    'from': caller,
                    'to': called,
                    'file': call[0],
                    'line': call[2]
                })

            return call_graph

        except Exception as e:
            self.error(f"获取函数调用图失败: {e}")
            return {'nodes': {}, 'edges': []}

    def get_file_dependencies(self, project_name: str, file_path: Optional[str] = None) -> Dict[str, Any]:
        """
        获取文件依赖关系

        Args:
            project_name: 项目名称
            file_path: 特定文件路径（可选）

        Returns:
            文件依赖数据
        """
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                # 尝试重新初始化数据库连接
                try:
                    sqlite_server._init_database()
                except Exception as init_error:
                    self.error(f"数据库连接初始化失败: {init_error}")
                    return {'nodes': [], 'edges': []}

            # 再次检查连接
            if not sqlite_server.connection:
                self.error("数据库连接仍然未初始化")
                return {'nodes': [], 'edges': []}

            conn = sqlite_server.connection

            if file_path:
                cursor = conn.execute("""
                    SELECT * FROM file_dependencies 
                    WHERE project_name = ? AND (source_file = ? OR target_file = ?)
                """, (project_name, file_path, file_path))
            else:
                cursor = conn.execute("""
                    SELECT * FROM file_dependencies WHERE project_name = ?
                """, (project_name,))

            dependencies = cursor.fetchall()

            # 构建依赖图
            dep_graph = {
                'nodes': set(),
                'edges': []
            }

            for dep in dependencies:
                source = dep[1]  # source_file
                target = dep[2]  # target_file

                dep_graph['nodes'].add(source)
                dep_graph['nodes'].add(target)
                dep_graph['edges'].append({
                    'from': source,
                    'to': target,
                    'type': dep[3]  # dependency_type
                })

            dep_graph['nodes'] = list(dep_graph['nodes'])
            return dep_graph

        except Exception as e:
            self.error(f"获取文件依赖关系失败: {e}")
            return {'nodes': [], 'edges': []}
