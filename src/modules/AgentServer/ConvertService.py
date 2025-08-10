'''
AI 逐文件夹处理

  📁 individual_files/
    📁 source_only/
    📁 header_only/
    📁 misc_files/
  📁 paired_files/
  📁 indices/
    📄 file_mappings.json
    📄 processing_stats.json
    📄 element_indices.json
    📄 analysis_results.json
  📄 processing_log.txt
  📄 processing_report.json
'''
import os
import json
from typing import Dict, List, Optional, Set
from pathlib import Path
from ..Preprocessing.CallRelationAnalyzer import CallRelationAnalyzer
from ..LLMRequester.LLMRequester import LLMRequester
from ..DatebaseServer.DatabaseManager import DatabaseManager

from ...base.Base import Base

class ConvertService(Base):
    """
    转换服务模块
    转换单文件夹内的所有文件，处理源文件、头文件和其他类型的文件为 Rust 代码。

    Attributes:
        db_manager: 数据库管理器实例
        input_folder: 输入文件夹路径
    """
    def __init__(self, db_client: DatabaseManager, input_folder: str):
        self.db_manager = db_client
        self.input_folder = input_folder
        super().__init__()

    def convert(self):
        """
        执行转换过程
        """
        try:
            self.info(f"开始转换文件夹: {self.input_folder}")

            # 创建 Rust 项目结构
            self._create_rust_project()

            # 遍历输入文件夹中的所有文件
            for root, _, files in os.walk(self.input_folder):
                for file in files:
                    if file.endswith(('.c', '.h')):
                        file_path = os.path.join(root, file)
                        self.info(f"转换文件: {file_path}")
                        with open(file_path, 'r') as f:
                            c_code = f.read()
                        # 使用 LLM 进行代码转换
                        llm_client = LLMRequester()
                        system_prompt = "Convert the following C code to Rust code:"
                        platform_config = {}  # Add appropriate platform configuration
                        messages = [{"role": "user", "content": c_code}]
                        response = llm_client.sent_request(messages, system_prompt, platform_config)
                        # 提取响应内容
                        success, rust_code, error_msg, status_code, tokens = response
                        if not success or rust_code is None:
                            self.error(f"LLM 转换失败: {error_msg}")
                            continue
                        # 保存转换后的 Rust 代码
                        rust_file_name = os.path.splitext(file)[0] + '.rs'
                        self._save_rust_file(rust_file_name, rust_code)

            self.info(f"成功完成文件夹转换: {self.input_folder}")
        except Exception as e:
            self.error(f"转换过程中出现错误: {e}")
            raise

    def _create_rust_project(self):
        """
        将目录转换为 cargo project
        """
        os.makedirs(os.path.join(self.input_folder, "src"), exist_ok=True)
        with open(os.path.join(self.input_folder, "Cargo.toml"), 'w') as f:
            f.write("[package]\nname = \"my_project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n")

    def _save_rust_file(self, file_name: str, rust_code: str):
        """
        保存 Rust 代码到指定文件
        """
        rust_file_path = os.path.join(self.input_folder, "src", f"{file_name}.rs")
        with open(rust_file_path, 'w') as f:
            f.write(rust_code)

class PromptBuilder(Base):
    """
    基于关系数据的提示构建器模块
    
    根据已保存的调用关系、函数定义、文件依赖等信息，
    为特定文件构建包含上下文的智能提示词。
    """
    
    def __init__(self, db_manager: DatabaseManager, project_name: str):
        super().__init__()
        self.db_manager = db_manager
        self.project_name = project_name
        # CallRelationAnalyzer只用于查询，不需要输入目录
        self.relation_analyzer = None
        
    def build_file_context_prompt(self, file_path: str, target_functions: Optional[List[str]] = None) -> str:
        """
        为特定文件构建包含关系上下文的提示词
        
        Args:
            file_path: 目标文件路径
            target_functions: 特定关注的函数列表（可选）
            
        Returns:
            构建的提示词字符串
        """
        try:
            self.info(f"为文件 {file_path} 构建上下文提示词")
            
            prompt_sections = []
            
            # 1. 文件基本信息
            file_info = self._get_file_basic_info(file_path)
            if file_info:
                prompt_sections.append(self._format_file_info(file_info))
            
            # 2. 文件中定义的函数
            defined_functions = self._get_defined_functions(file_path)
            if defined_functions:
                prompt_sections.append(self._format_defined_functions(defined_functions))
            
            # 3. 函数调用关系
            call_relationships = self._get_call_relationships(file_path, target_functions)
            if call_relationships:
                prompt_sections.append(self._format_call_relationships(call_relationships))
            
            # 4. 文件依赖关系
            file_dependencies = self._get_file_dependencies(file_path)
            if file_dependencies:
                prompt_sections.append(self._format_file_dependencies(file_dependencies))
            
            # 5. 相关接口信息（从向量数据库）
            interface_context = self._get_interface_context(file_path)
            if interface_context:
                prompt_sections.append(self._format_interface_context(interface_context))
            
            # 6. 构建完整提示词
            full_prompt = self._build_complete_prompt(file_path, prompt_sections)
            
            self.info(f"成功构建提示词，包含 {len(prompt_sections)} 个上下文部分")
            return full_prompt
            
        except Exception as e:
            self.error(f"构建文件上下文提示词失败: {e}")
            return self._get_fallback_prompt(file_path)
    
    def build_function_context_prompt(self, function_name: str, include_callers: bool = True, 
                                    include_callees: bool = True) -> str:
        """
        为特定函数构建包含调用上下文的提示词
        
        Args:
            function_name: 函数名
            include_callers: 是否包含调用者信息
            include_callees: 是否包含被调用者信息
            
        Returns:
            构建的提示词字符串
        """
        try:
            self.info(f"为函数 {function_name} 构建调用上下文提示词")
            
            prompt_sections = []
            
            # 1. 函数定义信息
            func_definition = self._get_function_definition(function_name)
            if func_definition:
                prompt_sections.append(self._format_function_definition(func_definition))
            
            # 2. 调用者信息
            if include_callers:
                callers = self._get_function_callers(function_name)
                if callers:
                    prompt_sections.append(self._format_function_callers(callers))
            
            # 3. 被调用函数信息
            if include_callees:
                callees = self._get_function_callees(function_name)
                if callees:
                    prompt_sections.append(self._format_function_callees(callees))
            
            # 4. 构建完整提示词
            full_prompt = self._build_function_prompt(function_name, prompt_sections)
            
            self.info(f"成功构建函数提示词，包含 {len(prompt_sections)} 个上下文部分")
            return full_prompt
            
        except Exception as e:
            self.error(f"构建函数上下文提示词失败: {e}")
            return self._get_fallback_function_prompt(function_name)
    
    def _get_file_basic_info(self, file_path: str) -> Optional[Dict]:
        """获取文件基本信息"""
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                return None
            
            file_name = Path(file_path).name
            
            # 从interfaces表获取文件信息
            cursor = sqlite_server.connection.execute("""
                SELECT file_path, language, project_name, COUNT(*) as interface_count
                FROM interfaces 
                WHERE file_path LIKE ? AND project_name = ?
                GROUP BY file_path, language, project_name
            """, (f"%{file_name}%", self.project_name))
            
            result = cursor.fetchone()
            if result:
                return {
                    'file_path': result[0],
                    'language': result[1],
                    'project_name': result[2],
                    'interface_count': result[3]
                }
            return None
            
        except Exception as e:
            self.error(f"获取文件基本信息失败: {e}")
            return None
    
    def _get_defined_functions(self, file_path: str) -> List[Dict]:
        """获取文件中定义的函数"""
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                return []
            
            file_name = Path(file_path).name
            
            cursor = sqlite_server.connection.execute("""
                SELECT function_name, line_number, return_type, parameters, signature
                FROM function_definitions 
                WHERE file_path LIKE ? AND project_name = ?
                ORDER BY line_number
            """, (f"%{file_name}%", self.project_name))
            
            functions = []
            for row in cursor.fetchall():
                functions.append({
                    'name': row[0],
                    'line_number': row[1],
                    'return_type': row[2],
                    'parameters': row[3],
                    'signature': row[4]
                })
            
            return functions
            
        except Exception as e:
            self.error(f"获取定义函数失败: {e}")
            return []
    
    def _get_call_relationships(self, file_path: str, target_functions: Optional[List[str]] = None) -> Dict:
        """获取调用关系"""
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                return {}
            
            file_name = Path(file_path).name
            
            # 获取文件内的函数调用
            if target_functions:
                placeholders = ','.join(['?' for _ in target_functions])
                cursor = sqlite_server.connection.execute(f"""
                    SELECT caller_function, called_function, caller_line
                    FROM function_calls 
                    WHERE caller_file LIKE ? AND project_name = ? 
                    AND (caller_function IN ({placeholders}) OR called_function IN ({placeholders}))
                """, [f"%{file_name}%", self.project_name] + target_functions + target_functions)
            else:
                cursor = sqlite_server.connection.execute("""
                    SELECT caller_function, called_function, caller_line
                    FROM function_calls 
                    WHERE caller_file LIKE ? AND project_name = ?
                """, (f"%{file_name}%", self.project_name))
            
            internal_calls = []
            for row in cursor.fetchall():
                internal_calls.append({
                    'caller': row[0],
                    'called': row[1],
                    'line': row[2]
                })
            
            # 获取外部对该文件函数的调用
            cursor = sqlite_server.connection.execute("""
                SELECT fc.caller_file, fc.caller_function, fc.called_function, fc.caller_line
                FROM function_calls fc
                JOIN function_definitions fd ON fc.called_function = fd.function_name
                WHERE fd.file_path LIKE ? AND fd.project_name = ? 
                AND fc.caller_file NOT LIKE ?
            """, (f"%{file_name}%", self.project_name, f"%{file_name}%"))
            
            external_calls = []
            for row in cursor.fetchall():
                external_calls.append({
                    'caller_file': row[0],
                    'caller_function': row[1],
                    'called_function': row[2],
                    'caller_line': row[3]
                })
            
            return {
                'internal_calls': internal_calls,
                'external_calls': external_calls
            }
            
        except Exception as e:
            self.error(f"获取调用关系失败: {e}")
            return {}
    
    def _get_file_dependencies(self, file_path: str) -> Dict:
        """获取文件依赖关系"""
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                return {}
            
            file_name = Path(file_path).name
            
            cursor = sqlite_server.connection.execute("""
                SELECT source_file, target_file, dependency_type
                FROM file_dependencies 
                WHERE project_name = ? AND (source_file LIKE ? OR target_file LIKE ?)
            """, (self.project_name, f"%{file_name}%", f"%{file_name}%"))
            
            dependencies = cursor.fetchall()
            
            # 构建依赖图格式
            dep_graph = {
                'nodes': set(),
                'edges': []
            }
            
            for dep in dependencies:
                source = dep[0]  # source_file
                target = dep[1]  # target_file
                dep_type = dep[2]  # dependency_type
                
                dep_graph['nodes'].add(source)
                dep_graph['nodes'].add(target)
                dep_graph['edges'].append({
                    'from': source,
                    'to': target,
                    'type': dep_type
                })
            
            dep_graph['nodes'] = list(dep_graph['nodes'])
            return dep_graph
            
        except Exception as e:
            self.error(f"获取文件依赖失败: {e}")
            return {}
    
    def _get_function_definition(self, function_name: str) -> Optional[Dict]:
        """获取函数定义信息"""
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                return None
            
            cursor = sqlite_server.connection.execute("""
                SELECT function_name, file_path, line_number, return_type, parameters, signature
                FROM function_definitions 
                WHERE function_name = ? AND project_name = ?
                LIMIT 1
            """, (function_name, self.project_name))
            
            result = cursor.fetchone()
            if result:
                return {
                    'name': result[0],
                    'file_path': result[1],
                    'line_number': result[2],
                    'return_type': result[3],
                    'parameters': result[4],
                    'signature': result[5]
                }
            return None
            
        except Exception as e:
            self.error(f"获取函数定义失败: {e}")
            return None
    
    def _get_function_callers(self, function_name: str) -> List[Dict]:
        """获取函数的调用者"""
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                return []
            
            cursor = sqlite_server.connection.execute("""
                SELECT caller_file, caller_function, caller_line
                FROM function_calls 
                WHERE called_function = ? AND project_name = ?
            """, (function_name, self.project_name))
            
            callers = []
            for row in cursor.fetchall():
                callers.append({
                    'caller_file': row[0],
                    'caller_function': row[1],
                    'caller_line': row[2]
                })
            
            return callers
            
        except Exception as e:
            self.error(f"获取函数调用者失败: {e}")
            return []
    
    def _get_function_callees(self, function_name: str) -> List[Dict]:
        """获取函数调用的其他函数"""
        try:
            sqlite_server = self.db_manager.sqlite_server
            if not sqlite_server.connection:
                return []
            
            cursor = sqlite_server.connection.execute("""
                SELECT called_function, caller_line, called_file
                FROM function_calls 
                WHERE caller_function = ? AND project_name = ?
            """, (function_name, self.project_name))
            
            callees = []
            for row in cursor.fetchall():
                callees.append({
                    'called_function': row[0],
                    'caller_line': row[1],
                    'called_file': row[2]
                })
            
            return callees
            
        except Exception as e:
            self.error(f"获取函数被调用者失败: {e}")
            return []
    
    def _get_interface_context(self, file_path: str) -> List[Dict]:
        """从向量数据库获取相关接口上下文"""
        try:
            file_name = Path(file_path).name
            
            # 搜索相关接口
            interfaces = self.db_manager.sqlite_server.search_interfaces(
                project_name=self.project_name
            )
            
            # 过滤出当前文件相关的接口
            relevant_interfaces = []
            for interface in interfaces:
                if file_name in interface.get('file_path', ''):
                    relevant_interfaces.append(interface)
            
            return relevant_interfaces[:10]  # 限制数量
            
        except Exception as e:
            self.error(f"获取接口上下文失败: {e}")
            return []
    
    # 格式化方法
    def _format_file_info(self, file_info: Dict) -> str:
        """格式化文件信息"""
        return f"""## 文件信息
- 文件路径: {file_info['file_path']}
- 编程语言: {file_info['language']}
- 项目名称: {file_info['project_name']}
- 接口数量: {file_info['interface_count']}
"""
    
    def _format_defined_functions(self, functions: List[Dict]) -> str:
        """格式化定义的函数"""
        if not functions:
            return ""
        
        section = "## 文件中定义的函数\n"
        for func in functions:
            section += f"""
### {func['name']} (行 {func['line_number']})
- 返回类型: {func['return_type'] or 'unknown'}
- 函数签名: `{func['signature'] or func['name']}`
- 参数: {func['parameters'] or 'void'}
"""
        return section
    
    def _format_call_relationships(self, relationships: Dict) -> str:
        """格式化调用关系"""
        if not relationships:
            return ""
        
        section = "## 函数调用关系\n"
        
        internal_calls = relationships.get('internal_calls', [])
        if internal_calls:
            section += "### 文件内部调用\n"
            for call in internal_calls:
                section += f"- `{call['caller']}` 调用 `{call['called']}` (行 {call['line']})\n"
        
        external_calls = relationships.get('external_calls', [])
        if external_calls:
            section += "\n### 外部文件调用\n"
            for call in external_calls:
                caller_file = Path(call['caller_file']).name
                section += f"- `{caller_file}:{call['caller_function']}` 调用 `{call['called_function']}` (行 {call['caller_line']})\n"
        
        return section
    
    def _format_file_dependencies(self, dependencies: Dict) -> str:
        """格式化文件依赖"""
        if not dependencies or not dependencies.get('edges'):
            return ""
        
        section = "## 文件依赖关系\n"
        edges = dependencies.get('edges', [])
        
        for edge in edges[:10]:  # 限制数量
            source_file = Path(edge['from']).name
            target_file = Path(edge['to']).name
            dep_type = edge.get('type', 'unknown')
            section += f"- `{source_file}` → `{target_file}` ({dep_type})\n"
        
        return section
    
    def _format_interface_context(self, interfaces: List[Dict]) -> str:
        """格式化接口上下文"""
        if not interfaces:
            return ""
        
        section = "## 相关接口信息\n"
        for interface in interfaces[:5]:  # 限制数量
            section += f"""
### {interface.get('name', 'unknown')}
- 文件: {Path(interface.get('file_path', '')).name}
- 语言: {interface.get('language', 'c')}
"""
        return section
    
    def _format_function_definition(self, func_def: Dict) -> str:
        """格式化函数定义"""
        return f"""## 函数定义
- 函数名: {func_def['name']}
- 文件: {Path(func_def['file_path']).name}
- 行号: {func_def['line_number']}
- 返回类型: {func_def['return_type'] or 'unknown'}
- 函数签名: `{func_def['signature'] or func_def['name']}`
- 参数: {func_def['parameters'] or 'void'}
"""
    
    def _format_function_callers(self, callers: List[Dict]) -> str:
        """格式化函数调用者"""
        if not callers:
            return ""
        
        section = "## 调用该函数的位置\n"
        for caller in callers:
            caller_file = Path(caller['caller_file']).name
            caller_func = caller['caller_function'] or 'global'
            section += f"- `{caller_file}:{caller_func}` (行 {caller['caller_line']})\n"
        
        return section
    
    def _format_function_callees(self, callees: List[Dict]) -> str:
        """格式化被调用函数"""
        if not callees:
            return ""
        
        section = "## 该函数调用的其他函数\n"
        for callee in callees:
            called_file = Path(callee['called_file']).name if callee['called_file'] else 'unknown'
            section += f"- `{callee['called_function']}` 在 `{called_file}` (行 {callee['caller_line']})\n"
        
        return section
    
    def _build_complete_prompt(self, file_path: str, sections: List[str]) -> str:
        """构建完整的提示词"""
        file_name = Path(file_path).name
        
        header = f"""# C到Rust转换上下文信息

正在转换文件: **{file_name}**

以下是基于项目调用关系分析得到的上下文信息，请在转换过程中参考这些信息以保持函数调用关系和接口一致性。

"""
        
        content = "\n".join(sections)
        
        footer = """
## 转换指导原则

1. **保持函数签名一致性**: 确保转换后的Rust函数能够被其他模块正确调用
2. **处理依赖关系**: 注意文件间的依赖关系，确保模块导入正确
3. **类型映射**: 将C类型正确映射为Rust类型
4. **内存安全**: 利用Rust的所有权系统替代C的手动内存管理
5. **错误处理**: 使用Rust的Result类型处理可能的错误情况

请基于以上上下文信息进行准确的C到Rust代码转换。
"""
        
        return header + content + footer
    
    def _build_function_prompt(self, function_name: str, sections: List[str]) -> str:
        """构建函数特定的提示词"""
        header = f"""# 函数转换上下文信息

正在转换函数: **{function_name}**

以下是该函数的调用关系和上下文信息：

"""
        
        content = "\n".join(sections)
        
        footer = """
## 函数转换指导

请根据上述调用关系信息，确保转换后的Rust函数：
1. 保持与调用者的接口兼容性
2. 正确处理被调用函数的依赖关系
3. 使用适当的Rust类型和错误处理机制
"""
        
        return header + content + footer
    
    def _get_fallback_prompt(self, file_path: str) -> str:
        """获取备用提示词"""
        file_name = Path(file_path).name
        return f"""# C到Rust转换

正在转换文件: **{file_name}**

由于无法获取详细的上下文信息，请按照以下基本原则进行转换：

1. 保持函数接口的基本结构
2. 使用Rust标准的类型映射
3. 添加适当的错误处理
4. 确保内存安全

请进行标准的C到Rust代码转换。
"""
    
    def _get_fallback_function_prompt(self, function_name: str) -> str:
        """获取函数备用提示词"""
        return f"""# 函数转换

正在转换函数: **{function_name}**

请按照标准的C到Rust转换原则进行转换：
1. 保持函数签名的基本语义
2. 使用Rust类型系统
3. 添加错误处理
4. 确保内存安全
"""