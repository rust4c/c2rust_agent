from pathlib import Path

from typing import Dict, List, Optional, Any
from fastembed import TextEmbedding

from ..DatebaseServer.DatabaseManager import DatabaseManager
from ..FileParsing.LSPServices import ClangdAnalyzer
from .CallRelationAnalyzer import CallRelationAnalyzer

from ...base.Base import Base

class SaveIntoDB(Base):
    """
    数据库分割保存模块
    """

    def __init__(self, DBClient: DatabaseManager, input_dir: str):
        super().__init__()
        self.db_client = DBClient
        self.analyzer = ClangdAnalyzer(input_dir)
        self.embedder = TextEmbedding()
        self.relation_analyzer = CallRelationAnalyzer(DBClient, Path(input_dir))

    def save(self, project_dir: str):
        """
        保存项目到数据库

        Args:
            project_dir: 项目目录路径
        """
        try:
            # 分析项目目录
            self.analyzer.analyze_project()

            # 获取结构体定义和函数信息
            structs = self.analyzer.get_structure()
            functions = self.analyzer.get_functions()

            # 嵌入向量化
            struct_embeddings = self.embedder.embed([str(s) for s in structs])
            function_embeddings = self.embedder.embed(
                [str(f) for f in functions])

            # 保存结构体到数据库
            for struct, embedding in zip(structs, struct_embeddings):
                self._save_struct_to_db(
                    struct, embedding.tolist(), project_dir)

            # 保存函数到数据库
            for function, embedding in zip(functions, function_embeddings):
                self._save_function_to_db(
                    function, embedding.tolist(), project_dir)

            # 分析并保存调用关系
            self.info("开始分析项目调用关系...")
            self.relation_analyzer.analyze_project_relations(project_dir)

            self.info(f"成功保存项目 {project_dir} 到数据库，包括调用关系")
        except Exception as e:
            self.error(f"保存项目到数据库失败: {e}")
            raise

    def _save_struct_to_db(self, struct, embedding, project_dir):
        """保存结构体到数据库"""
        try:
            # 提取结构体信息
            struct_name = getattr(struct, 'name', str(struct))
            struct_code = str(struct)

            # 构造输入输出信息（结构体没有输入输出，设为空）
            inputs = []
            outputs = []

            # 构造元数据
            metadata = {
                "type": "struct",
                "definition": struct_code
            }

            # 使用 store_interface_with_vector 保存
            self.db_client.store_interface_with_vector(
                name=struct_name,
                inputs=inputs,
                outputs=outputs,
                file_path=getattr(struct, 'file_path', project_dir),
                code=struct_code,
                vector=embedding,
                language='c',
                project_name=project_dir,
                metadata=metadata
            )
        except Exception as e:
            self.error(f"保存结构体失败: {e}")

    def _save_function_to_db(self, function, embedding, project_dir):
        """保存函数到数据库"""
        try:
            # 提取函数信息
            func_name = getattr(function, 'name', str(function))
            func_code = str(function)

            # 构造输入输出信息
            inputs = []
            outputs = []

            # 如果函数对象有参数信息，提取它们
            if hasattr(function, 'parameters'):
                for param in function.parameters:
                    inputs.append({
                        "name": getattr(param, 'name', 'unknown'),
                        "type": getattr(param, 'type', 'unknown')
                    })

            # 如果函数对象有返回类型信息
            if hasattr(function, 'return_type'):
                outputs.append({
                    "type": getattr(function, 'return_type', 'void')
                })

            # 构造元数据
            metadata = {
                "type": "function",
                "definition": func_code
            }

            # 使用 store_interface_with_vector 保存
            self.db_client.store_interface_with_vector(
                name=func_name,
                inputs=inputs,
                outputs=outputs,
                file_path=getattr(function, 'file_path', project_dir),
                code=func_code,
                vector=embedding,
                language='c',
                project_name=project_dir,
                metadata=metadata
            )
        except Exception as e:
            self.error(f"保存函数失败: {e}")

    def get_function_call_graph(self, project_name: str, function_name: Optional[str] = None):
        """
        获取函数调用图

        Args:
            project_name: 项目名称
            function_name: 特定函数名（可选）

        Returns:
            调用图数据
        """
        return self.relation_analyzer.get_function_call_graph(project_name, function_name)

    def get_file_dependencies(self, project_name: str, file_path: Optional[str] = None):
        """
        获取文件依赖关系

        Args:
            project_name: 项目名称
            file_path: 特定文件路径（可选）

        Returns:
            文件依赖数据
        """
        return self.relation_analyzer.get_file_dependencies(project_name, file_path)

    def get_function_usage_analysis(self, project_name: str, function_name: str):
        """
        获取函数使用情况分析

        Args:
            project_name: 项目名称
            function_name: 函数名

        Returns:
            函数使用分析结果
        """
        try:
            sqlite_server = self.db_client.sqlite_server
            if not sqlite_server.connection:
                return {}

            conn = sqlite_server.connection

            # 获取函数定义信息
            def_cursor = conn.execute("""
                SELECT * FROM function_definitions 
                WHERE project_name = ? AND function_name = ?
            """, (project_name, function_name))
            definitions = def_cursor.fetchall()

            # 获取函数调用信息
            call_cursor = conn.execute("""
                SELECT * FROM function_calls 
                WHERE project_name = ? AND called_function = ?
            """, (project_name, function_name))
            calls = call_cursor.fetchall()

            # 获取被该函数调用的其他函数
            caller_cursor = conn.execute("""
                SELECT * FROM function_calls 
                WHERE project_name = ? AND caller_function = ?
            """, (project_name, function_name))
            called_by_function = caller_cursor.fetchall()

            return {
                'function_name': function_name,
                'definitions': definitions,
                'called_by': calls,  # 被哪些地方调用
                'calls_to': called_by_function,  # 调用了哪些函数
                'call_count': len(calls),
                'definition_count': len(definitions)
            }

        except Exception as e:
            self.error(f"获取函数使用分析失败: {e}")
            return {}
