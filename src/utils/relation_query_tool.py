"""
调用关系数据库查询工具

提供便捷的接口来查询函数调用关系和文件依赖关系。
"""

import sqlite3
import json
from typing import List, Dict, Optional, Any
from pathlib import Path


class RelationQueryTool:
    """调用关系查询工具"""
    
    def __init__(self, db_path: str = "relation_analysis.db"):
        self.db_path = db_path
        self.connection: Optional[sqlite3.Connection] = None
        self._connect()
    
    def _connect(self):
        """连接数据库"""
        try:
            self.connection = sqlite3.connect(self.db_path)
            self.connection.row_factory = sqlite3.Row  # 返回字典格式
        except Exception as e:
            print(f"连接数据库失败: {e}")
    
    def close(self):
        """关闭数据库连接"""
        if self.connection:
            self.connection.close()
    
    def get_all_projects(self) -> List[str]:
        """获取所有项目名称"""
        if not self.connection:
            return []
        try:
            cursor = self.connection.execute("""
                SELECT DISTINCT project_name FROM function_definitions
                WHERE project_name IS NOT NULL
            """)
            return [row[0] for row in cursor.fetchall()]
        except Exception as e:
            print(f"获取项目列表失败: {e}")
            return []
    
    def get_project_statistics(self, project_name: str) -> Dict[str, Any]:
        """获取项目统计信息"""
        if not self.connection:
            return {}
        try:
            stats = {}
            
            # 函数定义统计
            cursor = self.connection.execute("""
                SELECT COUNT(*) as count FROM function_definitions WHERE project_name = ?
            """, (project_name,))
            stats['function_definitions'] = cursor.fetchone()[0]
            
            # 函数调用统计
            cursor = self.connection.execute("""
                SELECT COUNT(*) as count FROM function_calls WHERE project_name = ?
            """, (project_name,))
            stats['function_calls'] = cursor.fetchone()[0]
            
            # 文件依赖统计
            cursor = self.connection.execute("""
                SELECT COUNT(*) as count FROM file_dependencies WHERE project_name = ?
            """, (project_name,))
            stats['file_dependencies'] = cursor.fetchone()[0]
            
            # 唯一文件数统计
            cursor = self.connection.execute("""
                SELECT COUNT(DISTINCT file_path) as count FROM function_definitions WHERE project_name = ?
            """, (project_name,))
            stats['unique_files'] = cursor.fetchone()[0]
            
            return stats
        except Exception as e:
            print(f"获取项目统计失败: {e}")
            return {}
    
    def find_function_definition(self, project_name: str, function_name: str) -> List[Dict]:
        """查找函数定义"""
        try:
            cursor = self.connection.execute("""
                SELECT * FROM function_definitions 
                WHERE project_name = ? AND function_name LIKE ?
            """, (project_name, f"%{function_name}%"))
            return [dict(row) for row in cursor.fetchall()]
        except Exception as e:
            print(f"查找函数定义失败: {e}")
            return []
    
    def find_function_calls(self, project_name: str, function_name: str) -> List[Dict]:
        """查找函数调用"""
        try:
            cursor = self.connection.execute("""
                SELECT * FROM function_calls 
                WHERE project_name = ? AND called_function LIKE ?
            """, (project_name, f"%{function_name}%"))
            return [dict(row) for row in cursor.fetchall()]
        except Exception as e:
            print(f"查找函数调用失败: {e}")
            return []
    
    def get_function_call_chain(self, project_name: str, function_name: str, max_depth: int = 3) -> Dict[str, Any]:
        """获取函数调用链"""
        try:
            # 递归查找调用链
            def find_calls_recursive(func_name, depth, visited):
                if depth <= 0 or func_name in visited:
                    return []
                
                visited.add(func_name)
                
                cursor = self.connection.execute("""
                    SELECT called_function, caller_file, caller_line FROM function_calls 
                    WHERE project_name = ? AND caller_function = ?
                """, (project_name, func_name))
                
                calls = []
                for row in cursor.fetchall():
                    called_func = row[0]
                    call_info = {
                        'function': called_func,
                        'file': row[1],
                        'line': row[2],
                        'depth': max_depth - depth + 1,
                        'children': find_calls_recursive(called_func, depth - 1, visited.copy())
                    }
                    calls.append(call_info)
                
                return calls
            
            call_chain = {
                'root_function': function_name,
                'max_depth': max_depth,
                'call_tree': find_calls_recursive(function_name, max_depth, set())
            }
            
            return call_chain
        except Exception as e:
            print(f"获取函数调用链失败: {e}")
            return {}
    
    def get_file_call_relationships(self, project_name: str, file_path: str) -> Dict[str, Any]:
        """获取文件的调用关系"""
        try:
            file_name = Path(file_path).name
            
            # 该文件定义的函数
            cursor = self.connection.execute("""
                SELECT function_name, line_number, return_type FROM function_definitions 
                WHERE project_name = ? AND file_path LIKE ?
            """, (project_name, f"%{file_name}%"))
            
            defined_functions = [dict(row) for row in cursor.fetchall()]
            
            # 该文件中的函数调用
            cursor = self.connection.execute("""
                SELECT caller_function, called_function, caller_line FROM function_calls 
                WHERE project_name = ? AND caller_file LIKE ?
            """, (project_name, f"%{file_name}%"))
            
            function_calls = [dict(row) for row in cursor.fetchall()]
            
            # 调用该文件函数的外部调用
            defined_func_names = [func['function_name'] for func in defined_functions]
            external_calls = []
            
            for func_name in defined_func_names:
                cursor = self.connection.execute("""
                    SELECT caller_file, caller_function, caller_line FROM function_calls 
                    WHERE project_name = ? AND called_function = ? AND caller_file NOT LIKE ?
                """, (project_name, func_name, f"%{file_name}%"))
                
                for row in cursor.fetchall():
                    external_calls.append({
                        'called_function': func_name,
                        'caller_file': row[0],
                        'caller_function': row[1],
                        'caller_line': row[2]
                    })
            
            return {
                'file_path': file_path,
                'defined_functions': defined_functions,
                'internal_calls': function_calls,
                'external_calls': external_calls
            }
        except Exception as e:
            print(f"获取文件调用关系失败: {e}")
            return {}
    
    def get_most_called_functions(self, project_name: str, limit: int = 10) -> List[Dict]:
        """获取最常被调用的函数"""
        try:
            cursor = self.connection.execute("""
                SELECT called_function, COUNT(*) as call_count
                FROM function_calls 
                WHERE project_name = ?
                GROUP BY called_function
                ORDER BY call_count DESC
                LIMIT ?
            """, (project_name, limit))
            
            return [{'function': row[0], 'call_count': row[1]} for row in cursor.fetchall()]
        except Exception as e:
            print(f"获取最常调用函数失败: {e}")
            return []
    
    def get_most_complex_functions(self, project_name: str, limit: int = 10) -> List[Dict]:
        """获取调用最多其他函数的函数（复杂度最高）"""
        try:
            cursor = self.connection.execute("""
                SELECT caller_function, COUNT(DISTINCT called_function) as called_count
                FROM function_calls 
                WHERE project_name = ? AND caller_function IS NOT NULL
                GROUP BY caller_function
                ORDER BY called_count DESC
                LIMIT ?
            """, (project_name, limit))
            
            return [{'function': row[0], 'calls_made': row[1]} for row in cursor.fetchall()]
        except Exception as e:
            print(f"获取最复杂函数失败: {e}")
            return []
    
    def get_file_dependency_analysis(self, project_name: str) -> Dict[str, Any]:
        """获取文件依赖分析"""
        try:
            # 获取所有文件依赖
            cursor = self.connection.execute("""
                SELECT source_file, target_file, dependency_type FROM file_dependencies 
                WHERE project_name = ?
            """, (project_name,))
            
            dependencies = cursor.fetchall()
            
            # 统计每个文件的依赖数量
            source_deps = {}
            target_deps = {}
            
            for dep in dependencies:
                source = Path(dep[0]).name
                target = Path(dep[1]).name
                
                source_deps[source] = source_deps.get(source, 0) + 1
                target_deps[target] = target_deps.get(target, 0) + 1
            
            # 找出依赖最多的文件（出度）
            most_dependent_files = sorted(source_deps.items(), key=lambda x: x[1], reverse=True)[:10]
            
            # 找出被依赖最多的文件（入度）
            most_depended_files = sorted(target_deps.items(), key=lambda x: x[1], reverse=True)[:10]
            
            return {
                'total_dependencies': len(dependencies),
                'unique_source_files': len(source_deps),
                'unique_target_files': len(target_deps),
                'most_dependent_files': [{'file': f, 'dependency_count': c} for f, c in most_dependent_files],
                'most_depended_files': [{'file': f, 'depended_count': c} for f, c in most_depended_files]
            }
        except Exception as e:
            print(f"获取文件依赖分析失败: {e}")
            return {}
    
    def search_function_usage(self, project_name: str, keyword: str) -> Dict[str, Any]:
        """搜索函数使用情况"""
        try:
            # 搜索函数定义
            cursor = self.connection.execute("""
                SELECT * FROM function_definitions 
                WHERE project_name = ? AND (
                    function_name LIKE ? OR 
                    signature LIKE ? OR 
                    file_path LIKE ?
                )
            """, (project_name, f"%{keyword}%", f"%{keyword}%", f"%{keyword}%"))
            
            definitions = [dict(row) for row in cursor.fetchall()]
            
            # 搜索函数调用
            cursor = self.connection.execute("""
                SELECT * FROM function_calls 
                WHERE project_name = ? AND (
                    called_function LIKE ? OR 
                    caller_function LIKE ? OR 
                    caller_file LIKE ?
                )
            """, (project_name, f"%{keyword}%", f"%{keyword}%", f"%{keyword}%"))
            
            calls = [dict(row) for row in cursor.fetchall()]
            
            return {
                'keyword': keyword,
                'definitions_found': len(definitions),
                'calls_found': len(calls),
                'definitions': definitions,
                'calls': calls
            }
        except Exception as e:
            print(f"搜索函数使用失败: {e}")
            return {}


def print_project_report(query_tool: RelationQueryTool, project_name: str):
    """打印项目完整报告"""
    print(f"\n=== 项目 '{project_name}' 调用关系分析报告 ===")
    
    # 基本统计
    stats = query_tool.get_project_statistics(project_name)
    if stats:
        print(f"\n📊 基本统计:")
        print(f"  函数定义数: {stats.get('function_definitions', 0)}")
        print(f"  函数调用数: {stats.get('function_calls', 0)}")
        print(f"  文件依赖数: {stats.get('file_dependencies', 0)}")
        print(f"  唯一文件数: {stats.get('unique_files', 0)}")
    
    # 最常被调用的函数
    top_called = query_tool.get_most_called_functions(project_name, 5)
    if top_called:
        print(f"\n🔥 最常被调用的函数:")
        for i, func in enumerate(top_called, 1):
            print(f"  {i}. {func['function']} - {func['call_count']} 次")
    
    # 最复杂的函数
    top_complex = query_tool.get_most_complex_functions(project_name, 5)
    if top_complex:
        print(f"\n🔧 最复杂的函数:")
        for i, func in enumerate(top_complex, 1):
            print(f"  {i}. {func['function']} - 调用 {func['calls_made']} 个函数")
    
    # 文件依赖分析
    deps_analysis = query_tool.get_file_dependency_analysis(project_name)
    if deps_analysis:
        print(f"\n📁 文件依赖分析:")
        print(f"  总依赖数: {deps_analysis.get('total_dependencies', 0)}")
        
        most_dependent = deps_analysis.get('most_dependent_files', [])[:3]
        if most_dependent:
            print(f"  依赖最多的文件:")
            for file_info in most_dependent:
                print(f"    {file_info['file']} - {file_info['dependency_count']} 个依赖")
        
        most_depended = deps_analysis.get('most_depended_files', [])[:3]
        if most_depended:
            print(f"  被依赖最多的文件:")
            for file_info in most_depended:
                print(f"    {file_info['file']} - 被 {file_info['depended_count']} 个文件依赖")


def get_function_usage_summary(query_tool: RelationQueryTool, project_name: str, function_name: str) -> Dict[str, Any]:
    """获取函数使用概要"""
    definitions = query_tool.find_function_definition(project_name, function_name)
    calls = query_tool.find_function_calls(project_name, function_name)
    
    # 获取该函数调用的其他函数
    if not query_tool.connection:
        return {}
    
    try:
        cursor = query_tool.connection.execute("""
            SELECT called_function FROM function_calls 
            WHERE project_name = ? AND caller_function = ?
        """, (project_name, function_name))
        calls_made = cursor.fetchall()
        
        return {
            'definition_count': len(definitions),
            'called_count': len(calls),
            'calls_made': len(calls_made),
            'called_by': calls,
            'definitions': definitions
        }
    except Exception as e:
        print(f"获取函数使用概要失败: {e}")
        return {}


def main():
    """命令行工具演示"""
    import argparse
    
    parser = argparse.ArgumentParser(description='调用关系数据库查询工具')
    parser.add_argument('--db', default='relation_analysis.db', help='数据库文件路径')
    parser.add_argument('--project', required=True, help='项目名称')
    parser.add_argument('--command', required=True, choices=[
        'stats', 'find-func', 'call-chain', 'file-analysis', 
        'top-called', 'top-complex', 'deps-analysis', 'search'
    ], help='查询命令')
    parser.add_argument('--target', help='目标函数名或文件路径')
    parser.add_argument('--keyword', help='搜索关键词')
    parser.add_argument('--limit', type=int, default=10, help='结果限制数量')
    
    args = parser.parse_args()
    
    # 创建查询工具
    query_tool = RelationQueryTool(args.db)
    
    try:
        if args.command == 'stats':
            # 项目统计
            stats = query_tool.get_project_statistics(args.project)
            print(f"项目 {args.project} 统计信息:")
            for key, value in stats.items():
                print(f"  {key}: {value}")
        
        elif args.command == 'find-func' and args.target:
            # 查找函数
            definitions = query_tool.find_function_definition(args.project, args.target)
            calls = query_tool.find_function_calls(args.project, args.target)
            print(f"函数 {args.target} 搜索结果:")
            print(f"  定义数量: {len(definitions)}")
            print(f"  调用数量: {len(calls)}")
            
        elif args.command == 'call-chain' and args.target:
            # 函数调用链
            chain = query_tool.get_function_call_chain(args.project, args.target)
            print(f"函数 {args.target} 调用链:")
            print(json.dumps(chain, indent=2, ensure_ascii=False))
            
        elif args.command == 'file-analysis' and args.target:
            # 文件分析
            analysis = query_tool.get_file_call_relationships(args.project, args.target)
            print(f"文件 {args.target} 调用关系:")
            print(json.dumps(analysis, indent=2, ensure_ascii=False))
            
        elif args.command == 'top-called':
            # 最常被调用的函数
            top_called = query_tool.get_most_called_functions(args.project, args.limit)
            print(f"最常被调用的 {args.limit} 个函数:")
            for i, func in enumerate(top_called, 1):
                print(f"  {i}. {func['function']} - {func['call_count']} 次")
        
        elif args.command == 'top-complex':
            # 最复杂的函数
            top_complex = query_tool.get_most_complex_functions(args.project, args.limit)
            print(f"最复杂的 {args.limit} 个函数:")
            for i, func in enumerate(top_complex, 1):
                print(f"  {i}. {func['function']} - 调用 {func['calls_made']} 个函数")
        
        elif args.command == 'deps-analysis':
            # 文件依赖分析
            deps = query_tool.get_file_dependency_analysis(args.project)
            print(f"文件依赖分析:")
            print(json.dumps(deps, indent=2, ensure_ascii=False))
        
        elif args.command == 'search' and args.keyword:
            # 搜索
            results = query_tool.search_function_usage(args.project, args.keyword)
            print(f"搜索 '{args.keyword}' 结果:")
            print(f"  找到定义: {results['definitions_found']}")
            print(f"  找到调用: {results['calls_found']}")
        
        else:
            print("请提供必要的参数")
    
    finally:
        query_tool.close()


if __name__ == "__main__":
    main()
