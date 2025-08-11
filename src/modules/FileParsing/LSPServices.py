import os
import json
import subprocess
import tempfile
from typing import List, Dict, Optional, Any
import re
from pathlib import Path

class ClangdAnalyzer:
    """使用clangd LSP服务分析C/C++代码的类和函数信息"""

    def __init__(self, project_root: str):
        self.project_root = Path(project_root).resolve()
        self.compile_commands_path = self.project_root / "compile_commands.json"
        self.functions = []
        self.classes = []
        self.variables = []
        self.macros = []

    def generate_compile_commands(self) -> bool:
        """使用compiledb生成compile_commands.json文件"""
        print(f"使用compiledb生成编译数据库...")
        try:
            # 在项目根目录运行 compiledb -n make 生成编译数据库
            result = subprocess.run(
                ["compiledb", "-n", "make"],
                cwd=self.project_root,
                capture_output=True,
                text=True,
                timeout=120  # 延长超时时间
            )

            if result.returncode != 0:
                print(f"compiledb 失败，退出码: {result.returncode}")
                print(f"错误输出:\n{result.stderr}")
                return False

            print(f"成功生成编译数据库")
            return True

        except (subprocess.TimeoutExpired, FileNotFoundError) as e:
            print(f"生成编译数据库时出错: {e}")
            return False

    def get_source_files_from_compile_commands(self) -> List[str]:
        """从compile_commands.json中获取源文件列表"""
        if not self.compile_commands_path.exists():
            print(f"编译数据库不存在: {self.compile_commands_path}")
            return []

        try:
            with open(self.compile_commands_path, 'r') as f:
                compile_commands = json.load(f)

            # 提取所有源文件路径
            source_files = [entry['file'] for entry in compile_commands if 'file' in entry]

            # 转换为绝对路径
            source_files = [
                str(self.project_root / Path(file).relative_to('.'))
                if not os.path.isabs(file) else file
                for file in source_files
            ]

            print(f"从编译数据库中找到 {len(source_files)} 个源文件")
            return source_files

        except Exception as e:
            print(f"读取编译数据库失败: {e}")
            return []

    def analyze_with_clang_ast(self, file_path: str) -> Dict[str, Any]:
        """使用clang AST dump分析文件"""
        try:
            # 查找文件的编译命令
            compile_command = self.find_compile_command_for_file(file_path)

            # 构建AST dump命令
            cmd = [
                'clang',
                '-Xclang',
                '-ast-dump=json',
                '-fsyntax-only',
                '-w',  # 禁用警告
                '-Wno-error',  # 不将警告视为错误
                '-ferror-limit=0',  # 不限制错误数量
            ]

            # 添加原始编译命令的选项（排除源文件和-o选项）
            if compile_command:
                # 分割命令字符串为参数列表
                args = compile_command.split()
                filtered_args = []
                skip_next = False

                for i, arg in enumerate(args):
                    if skip_next:
                        skip_next = False
                        continue

                    # 跳过源文件名和输出选项
                    if arg == '-o' or arg == '-c':
                        skip_next = True
                        continue

                    # 跳过源文件本身
                    if arg.endswith(('.c', '.cpp', '.cc', '.cxx')):
                        continue

                    filtered_args.append(arg)

                cmd.extend(filtered_args)

            # 添加要分析的文件
            cmd.append(file_path)

            # 运行clang命令
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30
            )

            if result.returncode != 0:
                print(f"clang AST 分析失败: {file_path}")
                print(f"命令: {' '.join(cmd)}")
                print(f"错误输出:\n{result.stderr[:500]}...")
                return self.fallback_parse(file_path)

            try:
                return json.loads(result.stdout)
            except json.JSONDecodeError:
                print(f"解析AST JSON失败: {file_path}")
                return self.fallback_parse(file_path)

        except (subprocess.TimeoutExpired, FileNotFoundError) as e:
            print(f"分析 {file_path} 时出错: {e}")
            return self.fallback_parse(file_path)

    def find_compile_command_for_file(self, file_path: str) -> Optional[str]:
        """为指定文件查找编译命令"""
        if not self.compile_commands_path.exists():
            return None

        try:
            with open(self.compile_commands_path, 'r') as f:
                compile_commands = json.load(f)

            # 查找匹配的编译命令
            for entry in compile_commands:
                entry_file = entry.get('file', '')
                # 处理相对路径
                if not os.path.isabs(entry_file):
                    entry_file = str(self.project_root / entry_file)

                if os.path.abspath(entry_file) == os.path.abspath(file_path):
                    return entry.get('command', '')

            print(f"未找到 {file_path} 的编译命令")
            return None

        except Exception as e:
            print(f"查找编译命令失败: {e}")
            return None

    def fallback_parse(self, file_path: str) -> Dict[str, Any]:
        """当clang失败时的回退解析方法"""
        try:
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()

            # 使用正则表达式提取函数定义
            self.extract_functions_with_regex(content, file_path)
            self.extract_structs_with_regex(content, file_path)

            return {}  # 返回空字典，因为我们直接处理了
        except Exception as e:
            print(f"回退解析失败: {file_path}: {e}")
            return {}

    def extract_functions_with_regex(self, content: str, file_path: str) -> None:
        """使用正则表达式提取函数定义"""
        # 匹配函数定义的正则表达式
        func_pattern = r'(?:static\s+)?(?:inline\s+)?(\w+(?:\s*\*)*)\s+(\w+)\s*\(([^{]*?)\)\s*(?:\{|;)'

        for match in re.finditer(func_pattern, content, re.MULTILINE | re.DOTALL):
            return_type = match.group(1).strip()
            func_name = match.group(2).strip()
            params_str = match.group(3).strip()

            # 解析参数
            params = []
            if params_str and params_str != 'void':
                param_parts = [p.strip() for p in re.split(r',\s*(?![^()]*\))', params_str) if p.strip()]
                for param in param_parts:
                    # 简单的参数解析
                    parts = re.split(r'\s+', param.strip(), 1)
                    if len(parts) == 2:
                        param_type = parts[0].strip()
                        param_name = parts[1].strip()
                        # 清理参数名
                        param_name = re.sub(r'[\[\]*&]', '', param_name)
                        params.append({'name': param_name, 'type': param_type})
                    elif len(parts) == 1:
                        params.append({'name': 'param', 'type': parts[0]})

            # 计算行号
            line_num = content[:match.start()].count('\n') + 1

            self.functions.append({
                'name': func_name,
                'file': file_path,
                'return_type': return_type,
                'parameters': params,
                'line': line_num
            })

    def extract_structs_with_regex(self, content: str, file_path: str) -> None:
        """使用正则表达式提取结构体定义"""
        # 匹配结构体定义的正则表达式
        struct_pattern = r'(?:typedef\s+)?struct\s+(\w+)?\s*\{([^}]*)\}(?:\s*(\w+))?;?'

        for match in re.finditer(struct_pattern, content, re.MULTILINE | re.DOTALL):
            struct_name = match.group(1) or match.group(3) or 'anonymous'
            members_str = match.group(2)

            if struct_name == 'anonymous':
                continue

            # 解析成员
            members = []
            if members_str:
                member_lines = [line.strip() for line in members_str.split('\n') if line.strip()]
                for line in member_lines:
                    # 移除注释和空行
                    if line.startswith('//') or line.startswith('/*') or not line:
                        continue
                    # 移除行尾分号
                    line = line.rstrip(';').strip()

                    # 简单的成员解析
                    parts = re.split(r'\s+', line, 1)
                    if len(parts) == 2:
                        member_type = parts[0].strip()
                        member_name = parts[1].strip()
                        # 清理成员名
                        member_name = re.sub(r'[\[\]*]', '', member_name)
                        members.append({'name': member_name, 'type': member_type})
                    elif len(parts) == 1:
                        members.append({'name': 'unnamed', 'type': parts[0]})

            # 计算行号
            line_num = content[:match.start()].count('\n') + 1

            self.classes.append({
                'name': struct_name,
                'file': file_path,
                'members': members,
                'line': line_num
            })

    def extract_function_info(self, node: Dict, file_path: str) -> None:
        """从AST节点提取函数信息"""
        if node.get('kind') == 'FunctionDecl':
            func_name = node.get('name', 'unnamed')

            # 提取返回类型
            return_type = "void"
            if 'type' in node:
                type_info = node['type']
                if 'qualType' in type_info:
                    qual_type = type_info['qualType']
                    # 解析返回类型 (从函数签名中提取)
                    if '(' in qual_type:
                        return_type = qual_type.split('(')[0].strip()

            # 提取参数信息
            params = []
            if 'inner' in node:
                for inner_node in node['inner']:
                    if inner_node.get('kind') == 'ParmVarDecl':
                        param_name = inner_node.get('name', 'unnamed')
                        param_type = "unknown"
                        if 'type' in inner_node and 'qualType' in inner_node['type']:
                            param_type = inner_node['type']['qualType']
                        params.append({
                            'name': param_name,
                            'type': param_type
                        })

            self.functions.append({
                'name': func_name,
                'file': file_path,
                'return_type': return_type,
                'parameters': params,
                'line': node.get('loc', {}).get('line', 0)
            })

    def extract_struct_info(self, node: Dict, file_path: str) -> None:
        """从AST节点提取结构体/类信息"""
        if node.get('kind') in ['RecordDecl', 'CXXRecordDecl']:
            struct_name = node.get('name', 'unnamed')
            if not struct_name or struct_name == 'unnamed':
                return

            # 提取成员变量
            members = []
            if 'inner' in node:
                for inner_node in node['inner']:
                    if inner_node.get('kind') == 'FieldDecl':
                        member_name = inner_node.get('name', 'unnamed')
                        member_type = "unknown"
                        if 'type' in inner_node and 'qualType' in inner_node['type']:
                            member_type = inner_node['type']['qualType']
                        members.append({
                            'name': member_name,
                            'type': member_type
                        })

            self.classes.append({
                'name': struct_name,
                'file': file_path,
                'members': members,
                'line': node.get('loc', {}).get('line', 0)
            })

    def extract_variable_info(self, node: Dict, file_path: str) -> None:
        """从AST节点提取变量信息"""
        if node.get('kind') == 'VarDecl':
            var_name = node.get('name', 'unnamed')
            var_type = "unknown"
            if 'type' in node and 'qualType' in node['type']:
                var_type = node['type']['qualType']

            # 只记录全局变量（非局部变量）
            if not node.get('loc', {}).get('includedFrom'):
                self.variables.append({
                    'name': var_name,
                    'file': file_path,
                    'type': var_type,
                    'line': node.get('loc', {}).get('line', 0)
                })

    def extract_macro_info(self, node: Dict, file_path: str) -> None:
        """从AST节点提取宏定义信息"""
        if node.get('kind') == 'MacroDefinition':
            macro_name = node.get('name', 'unnamed')
            macro_value = node.get('value', '')

            self.macros.append({
                'name': macro_name,
                'file': file_path,
                'value': macro_value,
                'line': node.get('loc', {}).get('line', 0)
            })

    def traverse_ast(self, node: Dict, file_path: str) -> None:
        """递归遍历AST节点"""
        if not isinstance(node, dict):
            return

        # 提取不同类型的信息
        self.extract_function_info(node, file_path)
        self.extract_struct_info(node, file_path)
        self.extract_variable_info(node, file_path)

        # 递归处理子节点
        if 'inner' in node:
            for child in node['inner']:
                self.traverse_ast(child, file_path)

    def analyze_project(self) -> None:
        """分析整个项目"""
        print(f"正在分析项目: {self.project_root}")

        # 生成编译数据库
        if not self.generate_compile_commands():
            print("⚠️ 编译数据库生成失败，尝试继续分析...")

        # 从编译数据库获取源文件
        source_files = self.get_source_files_from_compile_commands()

        if not source_files:
            print("⚠️ 未找到源文件，尝试手动查找...")
            # 回退方法：手动查找源文件
            source_files = []
            for ext in ['*.c', '*.cpp', '*.cxx', '*.cc']:
                source_files.extend([str(f) for f in self.project_root.rglob(ext)])
            print(f"找到 {len(source_files)} 个源文件")

        # 分析每个C/C++文件
        for i, file_path in enumerate(source_files, 1):
            print(f"正在分析 ({i}/{len(source_files)}): {os.path.relpath(file_path, self.project_root)}")
            ast_data = self.analyze_with_clang_ast(file_path)
            if ast_data:
                self.traverse_ast(ast_data, file_path)

    def print_analysis_results(self, detailed: bool = True) -> None:
        """打印分析结果"""
        print("\n" + "="*80)
        print("代码分析结果")
        print("="*80)

        if detailed:
            # 打印函数信息
            print(f"\n📋 函数列表 ({len(self.functions)} 个):")
            print("-" * 60)
            for func in sorted(self.functions, key=lambda x: x['name']):
                file_rel = os.path.relpath(func['file'], self.project_root)
                print(f"🔧 {func['name']}")
                print(f"   文件: {file_rel}:{func['line']}")
                print(f"   返回类型: {func['return_type']}")
                if func['parameters']:
                    print(f"   参数:")
                    for param in func['parameters']:
                        print(f"     - {param['name']}: {param['type']}")
                else:
                    print(f"   参数: 无")
                print()

            # 打印结构体/类信息
            print(f"\n📊 结构体/类列表 ({len(self.classes)} 个):")
            print("-" * 60)
            for cls in sorted(self.classes, key=lambda x: x['name']):
                file_rel = os.path.relpath(cls['file'], self.project_root)
                print(f"🏗️  {cls['name']}")
                print(f"   文件: {file_rel}:{cls['line']}")
                if cls['members']:
                    print(f"   成员:")
                    for member in cls['members']:
                        print(f"     - {member['name']}: {member['type']}")
                else:
                    print(f"   成员: 无")
                print()

            # 打印全局变量信息
            print(f"\n🌐 全局变量列表 ({len(self.variables)} 个):")
            print("-" * 60)
            for var in sorted(self.variables, key=lambda x: x['name']):
                file_rel = os.path.relpath(var['file'], self.project_root)
                print(f"📦 {var['name']}")
                print(f"   文件: {file_rel}:{var['line']}")
                print(f"   类型: {var['type']}")
                print()
        else:
            # 简洁模式：只显示重要的函数和结构体
            important_functions = [f for f in self.functions if not f['name'].startswith('__') and len(f['parameters']) <= 5]
            print(f"\n📋 主要函数列表 (显示 {min(20, len(important_functions))} 个):")
            print("-" * 60)
            for func in sorted(important_functions, key=lambda x: x['name'])[:20]:
                file_rel = os.path.relpath(func['file'], self.project_root)
                params_str = ", ".join([f"{p['name']}: {p['type']}" for p in func['parameters']])
                print(f"🔧 {func['return_type']} {func['name']}({params_str})")
                print(f"   文件: {file_rel}:{func['line']}")
                print()

            # 显示所有结构体
            if self.classes:
                print(f"\n📊 结构体/类列表 ({len(self.classes)} 个):")
                print("-" * 60)
                for cls in sorted(self.classes, key=lambda x: x['name']):
                    file_rel = os.path.relpath(cls['file'], self.project_root)
                    print(f"🏗️  {cls['name']} ({len(cls['members'])} 成员)")
                    print(f"   文件: {file_rel}:{cls['line']}")
                    if cls['members']:
                        for member in cls['members'][:3]:  # 只显示前3个成员
                            print(f"     - {member['name']}: {member['type']}")
                        if len(cls['members']) > 3:
                            print(f"     ... 还有 {len(cls['members']) - 3} 个成员")
                    print()

        # 统计信息
        print("\n📈 统计信息:")
        print("-" * 30)
        print(f"函数总数: {len(self.functions)}")
        print(f"结构体/类总数: {len(self.classes)}")
        print(f"全局变量总数: {len(self.variables)}")

        # 按文件统计
        file_stats = {}
        for func in self.functions:
            file_rel = os.path.relpath(func['file'], self.project_root)
            file_stats[file_rel] = file_stats.get(file_rel, {'functions': 0, 'classes': 0, 'variables': 0})
            file_stats[file_rel]['functions'] += 1

        for cls in self.classes:
            file_rel = os.path.relpath(cls['file'], self.project_root)
            file_stats[file_rel] = file_stats.get(file_rel, {'functions': 0, 'classes': 0, 'variables': 0})
            file_stats[file_rel]['classes'] += 1

        for var in self.variables:
            file_rel = os.path.relpath(var['file'], self.project_root)
            file_stats[file_rel] = file_stats.get(file_rel, {'functions': 0, 'classes': 0, 'variables': 0})
            file_stats[file_rel]['variables'] += 1

        print(f"\n📁 按文件统计:")
        for file, stats in sorted(file_stats.items()):
            print(f"  {file}: {stats['functions']}函数, {stats['classes']}结构体, {stats['variables']}变量")

    def get_structure(self) -> Dict[str, Any]:
        """获取项目结构"""
        return {
            'functions': self.functions,
            'classes': self.classes,
            'variables': self.variables
        }

    def get_classes(self) -> List[Dict[str, Any]]:
        """获取所有类信息"""
        return self.classes

    def get_functions(self) -> List[Dict[str, Any]]:
        """获取所有函数信息"""
        return self.functions
    
    def get_macros(self) -> List[Dict[str, Any]]:
        """获取所有宏定义"""
        return self.macros

def check_function_and_class_name(project_path: str, detailed: bool = False):
    """
    检查指定项目中的所有类、对象与函数的输入/输出类型

    Args:
        project_path: 项目路径
        detailed: 是否显示详细信息
    """
    analyzer = ClangdAnalyzer(project_path)
    analyzer.analyze_project()
    analyzer.print_analysis_results(detailed)


if __name__ == "__main__":
    # 分析指定目录
    project_path = "/Users/peng/Documents/AppCode/Rust/c2rust_agent/translate_chibicc/src"

    print("🚀 开始使用clangd分析C/C++代码...")
    print(f"项目路径: {project_path}")

    # 默认使用简洁模式，如果需要详细输出，可以设置detailed=True
    check_function_and_class_name(project_path, detailed=False)

    print("\n✅ 分析完成!")
