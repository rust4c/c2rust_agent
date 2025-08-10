import argparse
import os
from main import main

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='c2rust-agent')
    subparsers = parser.add_subparsers(dest='command', help='可用命令', required=True)

    # analyze 命令
    analyze_parser = subparsers.add_parser('analyze', help='分析C项目')
    analyze_parser.add_argument('--input-dir', dest='input_dir', type=str, required=True,
                                help='C项目目录（必需）')

    # translate 命令
    translate_parser = subparsers.add_parser('translate', help='转换C项目为Rust')
    translate_parser.add_argument('--input-dir', dest='input_dir', type=str, required=True,
                                help='C项目目录（必需）')
    translate_parser.add_argument('--output-dir', dest='output_dir', type=str,
                                help='输出Rust项目目录（可选，默认为输入目录的上级目录）')
    
    # analyze-relations 命令
    analyze_relations_parser = subparsers.add_parser('analyze-relations', help='分析C项目调用关系并保存到数据库')
    analyze_relations_parser.add_argument('--input-dir', dest='input_dir', type=str, required=True,
                                        help='C项目目录（必需）')
    analyze_relations_parser.add_argument('--project-name', dest='project_name', type=str,
                                        help='项目名称（可选，默认为目录名）')
    analyze_relations_parser.add_argument('--db', type=str, default='relation_analysis.db',
                                        help='数据库文件路径（默认为relation_analysis.db）')
    
    # relation query 命令
    relation_parser = subparsers.add_parser('relation-query', help='查询调用关系数据库')
    relation_parser.add_argument('--db', type=str, default='relation_analysis.db',
                                 help='数据库文件路径（默认为relation_analysis.db）')
    relation_parser.add_argument('--project', type=str, help='项目名称（用于具体查询）')
    relation_parser.add_argument('--query-type', choices=[
        'list-projects', 'stats', 'report', 'find-func', 'call-chain', 
        'file-analysis', 'top-called', 'top-complex', 'deps-analysis', 
        'search', 'func-usage'
    ], default='list-projects', help='查询类型')
    relation_parser.add_argument('--target', type=str, help='目标函数名或文件路径（可选）')
    relation_parser.add_argument('--keyword', type=str, help='搜索关键词（可选）')
    relation_parser.add_argument('--limit', type=int, default=10,
                                 help='限制结果数量（默认为10）')

    # 解析参数
    ARGS = parser.parse_args()

    # 设置输出目录默认值（输入目录的上级目录）
    if ARGS.command == 'translate' and ARGS.output_dir is None:
        ARGS.output_dir = os.path.dirname(os.path.abspath(ARGS.input_dir))

    print(ARGS)

    if ARGS.command == "analyze":
        import src.modules.FileParsing.LSPServices as lsp_services
        project_path = ARGS.input_dir
        print("🚀 开始使用clangd分析C/C++代码...")
        print(f"项目路径: {project_path}")

        # 默认使用简洁模式，如果需要详细输出，可以设置detailed=True
        lsp_services.check_function_and_class_name(project_path, detailed=False)

        print("\n✅ 分析完成!")
    elif ARGS.command == "analyze-relations":
        from src.modules.DatebaseServer.DatabaseManager import create_database_manager
        from src.modules.Preprocessing.SaveIntoDB import SaveIntoDB
        import os
        
        project_path = ARGS.input_dir
        project_name = ARGS.project_name or os.path.basename(os.path.abspath(project_path))
        db_path = ARGS.db
        
        print("🔍 开始分析C/C++项目调用关系...")
        print(f"项目路径: {project_path}")
        print(f"项目名称: {project_name}")
        print(f"数据库路径: {db_path}")
        
        try:
            # 创建数据库管理器
            db_manager = create_database_manager(
                sqlite_path=db_path,
                qdrant_collection=f"{project_name}_vectors",
                vector_size=384
            )
            
            # 创建保存模块（包含调用关系分析）
            save_module = SaveIntoDB(db_manager, project_path)
            
            # 分析并保存项目（包括调用关系）
            save_module.save(project_name)
            
            print(f"\n✅ 调用关系分析完成！")
            print(f"使用以下命令查看结果:")
            print(f"  python run.py relation-query --db {db_path} --command report --project {project_name}")
            
        except Exception as e:
            print(f"❌ 分析失败: {e}")
            import traceback
            traceback.print_exc()
    elif ARGS.command == "translate":
        main(ARGS)
    elif ARGS.command == "relation-query":
        from src.utils.relation_query_tool import RelationQueryTool, print_project_report, get_function_usage_summary
        import json
        from pathlib import Path

        query_tool = RelationQueryTool(ARGS.db)
        try:
            if ARGS.query_type == 'list-projects':
                projects = query_tool.get_all_projects()
                print("可用项目:")
                for project in projects:
                    print(f"  - {project}")

            elif ARGS.query_type == 'stats' and ARGS.project:
                stats = query_tool.get_project_statistics(ARGS.project)
                print(f"项目 '{ARGS.project}' 统计信息:")
                for key, value in stats.items():
                    print(f"  {key}: {value}")

            elif ARGS.query_type == 'report' and ARGS.project:
                print_project_report(query_tool, ARGS.project)

            elif ARGS.query_type == 'find-func' and ARGS.project and ARGS.target:
                definitions = query_tool.find_function_definition(ARGS.project, ARGS.target)
                calls = query_tool.find_function_calls(ARGS.project, ARGS.target)
                print(f"函数 '{ARGS.target}' 搜索结果:")
                print(f"  定义数量: {len(definitions)}")
                print(f"  调用数量: {len(calls)}")
                
                if definitions:
                    print(f"  定义位置:")
                    for defn in definitions[:5]:  # 只显示前5个
                        file_name = Path(defn['file_path']).name
                        print(f"    {defn['function_name']} in {file_name}:{defn['line_number']}")

            elif ARGS.query_type == 'call-chain' and ARGS.project and ARGS.target:
                chain = query_tool.get_function_call_chain(ARGS.project, ARGS.target)
                print(f"函数 '{ARGS.target}' 调用链:")
                print(json.dumps(chain, indent=2, ensure_ascii=False))

            elif ARGS.query_type == 'file-analysis' and ARGS.project and ARGS.target:
                analysis = query_tool.get_file_call_relationships(ARGS.project, ARGS.target)
                print(f"文件 '{ARGS.target}' 调用关系:")
                print(json.dumps(analysis, indent=2, ensure_ascii=False))

            elif ARGS.query_type == 'top-called' and ARGS.project:
                top_called = query_tool.get_most_called_functions(ARGS.project, ARGS.limit)
                print(f"最常被调用的 {ARGS.limit} 个函数:")
                for i, func in enumerate(top_called, 1):
                    print(f"  {i}. {func['function']} - {func['call_count']} 次")

            elif ARGS.query_type == 'top-complex' and ARGS.project:
                top_complex = query_tool.get_most_complex_functions(ARGS.project, ARGS.limit)
                print(f"最复杂的 {ARGS.limit} 个函数:")
                for i, func in enumerate(top_complex, 1):
                    print(f"  {i}. {func['function']} - 调用 {func['calls_made']} 个函数")

            elif ARGS.query_type == 'deps-analysis' and ARGS.project:
                deps = query_tool.get_file_dependency_analysis(ARGS.project)
                print(f"文件依赖分析:")
                print(json.dumps(deps, indent=2, ensure_ascii=False))

            elif ARGS.query_type == 'search' and ARGS.project and ARGS.keyword:
                results = query_tool.search_function_usage(ARGS.project, ARGS.keyword)
                print(f"搜索 '{ARGS.keyword}' 结果:")
                print(f"  找到定义: {results['definitions_found']}")
                print(f"  找到调用: {results['calls_found']}")

            elif ARGS.query_type == 'func-usage' and ARGS.project and ARGS.target:
                usage = get_function_usage_summary(query_tool, ARGS.project, ARGS.target)
                print(f"函数 '{ARGS.target}' 使用分析:")
                print(f"  定义数量: {usage['definition_count']}")
                print(f"  被调用次数: {usage['called_count']}")
                print(f"  调用其他函数数: {usage['calls_made']}")
                
                if usage.get('called_by'):
                    print(f"  被调用的地方:")
                    for call in usage['called_by'][:5]:
                        file_name = Path(call['caller_file']).name if call.get('caller_file') else 'unknown'
                        caller_func = call.get('caller_function') or 'unknown'
                        print(f"    {caller_func} in {file_name}:{call.get('caller_line', '?')}")

            else:
                print("请提供必要的参数，使用 --help 查看帮助")
                print(f"当前查询类型: {ARGS.query_type}")
                print(f"项目: {ARGS.project}")
                print(f"目标: {ARGS.target}")

        finally:
            query_tool.close()
