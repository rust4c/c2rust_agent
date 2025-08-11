#!/usr/bin/env python3
"""
调用关系分析示例

演示如何使用CallRelationAnalyzer分析C项目的函数调用关系和文件依赖。
"""

import sys
import os
import json
from pathlib import Path

# 添加项目根目录到路径
sys.path.append(str(Path(__file__).parent.parent))

from src.modules.DatebaseServer.DatabaseManager import create_database_manager
from src.modules.Preprocessing.SaveIntoDB import SaveIntoDB


def main():
    """主函数"""
    # 配置
    project_dir = "test_project"  # 测试项目目录
    project_name = "test_c_project"
    
    # 创建数据库管理器
    db_manager = create_database_manager(
        sqlite_path="relation_analysis.db",
        qdrant_collection="test_relations",
        vector_size=384
    )
    
    print("=== C项目调用关系分析器示例 ===")
    
    try:
        # 1. 创建保存模块（包含调用关系分析）
        save_module = SaveIntoDB(db_manager, project_dir)
        
        # 2. 分析并保存项目（包括调用关系）
        print(f"正在分析项目: {project_dir}")
        save_module.save(project_name)
        
        # 3. 获取函数调用图
        print("\n=== 函数调用图 ===")
        call_graph = save_module.get_function_call_graph(project_name)
        print(f"找到 {len(call_graph['nodes'])} 个函数节点")
        print(f"找到 {len(call_graph['edges'])} 个调用关系")
        
        # 显示一些调用关系
        if call_graph['edges']:
            print("\n前5个调用关系:")
            for i, edge in enumerate(call_graph['edges'][:5]):
                print(f"  {edge['from']} -> {edge['to']} (在 {edge['file']}:{edge['line']})")
        
        # 4. 获取文件依赖关系
        print("\n=== 文件依赖关系 ===")
        file_deps = save_module.get_file_dependencies(project_name)
        print(f"找到 {len(file_deps['nodes'])} 个文件")
        print(f"找到 {len(file_deps['edges'])} 个依赖关系")
        
        # 显示一些依赖关系
        if file_deps['edges']:
            print("\n前5个文件依赖:")
            for i, edge in enumerate(file_deps['edges'][:5]):
                from_file = os.path.basename(edge['from'])
                to_file = os.path.basename(edge['to'])
                print(f"  {from_file} -> {to_file} ({edge['type']})")
        
        # 5. 分析特定函数（如果存在main函数）
        if 'main' in call_graph['nodes']:
            print("\n=== main函数分析 ===")
            main_analysis = save_module.get_function_usage_analysis(project_name, 'main')
            print(f"main函数定义数量: {main_analysis.get('definition_count', 0)}")
            print(f"main函数被调用次数: {main_analysis.get('call_count', 0)}")
            print(f"main函数调用其他函数数量: {len(main_analysis.get('calls_to', []))}")
        
        # 6. 获取特定函数的调用图
        if call_graph['nodes']:
            # 选择第一个函数进行详细分析
            first_function = list(call_graph['nodes'].keys())[0]
            if first_function != 'unknown':
                print(f"\n=== {first_function} 函数调用图 ===")
                specific_graph = save_module.get_function_call_graph(project_name, first_function)
                print(f"相关节点数: {len(specific_graph['nodes'])}")
                print(f"相关调用数: {len(specific_graph['edges'])}")
        
        # 7. 生成报告
        print("\n=== 生成分析报告 ===")
        generate_analysis_report(call_graph, file_deps, project_name)
        
        print("\n分析完成！")
        print(f"数据库文件: relation_analysis.db")
        print(f"分析报告: {project_name}_analysis_report.json")
        
    except Exception as e:
        print(f"分析失败: {e}")
        import traceback
        traceback.print_exc()


def generate_analysis_report(call_graph, file_deps, project_name):
    """生成分析报告"""
    try:
        # 统计信息
        stats = {
            'project_name': project_name,
            'total_functions': len(call_graph['nodes']),
            'total_function_calls': len(call_graph['edges']),
            'total_files': len(file_deps['nodes']),
            'total_file_dependencies': len(file_deps['edges']),
        }
        
        # 函数调用频率统计
        call_counts = {}
        for edge in call_graph['edges']:
            called_func = edge['to']
            call_counts[called_func] = call_counts.get(called_func, 0) + 1
        
        # 最常被调用的函数
        most_called = sorted(call_counts.items(), key=lambda x: x[1], reverse=True)[:10]
        
        # 文件依赖复杂度统计
        file_deps_count = {}
        for edge in file_deps['edges']:
            source = edge['from']
            file_deps_count[source] = file_deps_count.get(source, 0) + 1
        
        # 依赖最多的文件
        most_dependent = sorted(file_deps_count.items(), key=lambda x: x[1], reverse=True)[:10]
        
        # 生成完整报告
        report = {
            'statistics': stats,
            'most_called_functions': [{'function': func, 'call_count': count} for func, count in most_called],
            'most_dependent_files': [{'file': os.path.basename(file), 'dependency_count': count} for file, count in most_dependent],
            'function_call_graph': call_graph,
            'file_dependency_graph': file_deps
        }
        
        # 保存报告
        report_file = f"{project_name}_analysis_report.json"
        with open(report_file, 'w', encoding='utf-8') as f:
            json.dump(report, f, indent=2, ensure_ascii=False)
        
        print(f"分析统计:")
        print(f"  - 总函数数: {stats['total_functions']}")
        print(f"  - 总调用数: {stats['total_function_calls']}")
        print(f"  - 总文件数: {stats['total_files']}")
        print(f"  - 总依赖数: {stats['total_file_dependencies']}")
        
        if most_called:
            print(f"  - 最常被调用的函数: {most_called[0][0]} ({most_called[0][1]}次)")
        
        if most_dependent:
            most_dep_file = os.path.basename(most_dependent[0][0])
            print(f"  - 依赖最多的文件: {most_dep_file} ({most_dependent[0][1]}个依赖)")
        
    except Exception as e:
        print(f"生成报告失败: {e}")


if __name__ == "__main__":
    main()
