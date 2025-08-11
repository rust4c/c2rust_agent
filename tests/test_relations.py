#!/usr/bin/env python3
"""
测试调用关系分析功能

使用test_project目录测试完整的调用关系分析流程。
"""

import os
import sys
import subprocess
from pathlib import Path

def run_command(cmd, description):
    """运行命令并显示结果"""
    print(f"\n🔹 {description}")
    print(f"命令: {cmd}")
    print("=" * 50)
    
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=60)
        
        if result.stdout:
            print("输出:")
            print(result.stdout)
        
        if result.stderr:
            print("错误:")
            print(result.stderr)
        
        if result.returncode != 0:
            print(f"❌ 命令执行失败，返回码: {result.returncode}")
            return False
        else:
            print("✅ 命令执行成功")
            return True
            
    except subprocess.TimeoutExpired:
        print("❌ 命令执行超时")
        return False
    except Exception as e:
        print(f"❌ 命令执行异常: {e}")
        return False

def main():
    """主测试流程"""
    print("=== C项目调用关系分析测试 ===")
    
    # 检查test_project是否存在
    test_project = "test_project"
    if not os.path.exists(test_project):
        print(f"❌ 测试项目目录 {test_project} 不存在")
        return
    
    project_name = "test_c_project"
    db_name = "test_relations.db"
    
    # 清理之前的数据库文件
    if os.path.exists(db_name):
        os.remove(db_name)
        print(f"🗑️  清理旧数据库文件: {db_name}")
    
    # 1. 分析项目调用关系
    success = run_command(
        f"python run.py analyze-relations --input-dir {test_project} --project-name {project_name} --db {db_name}",
        "分析项目调用关系"
    )
    
    if not success:
        print("❌ 调用关系分析失败，停止测试")
        return
    
    # 2. 列出所有项目
    run_command(
        f"python run.py relation-query --db {db_name} --query-type list-projects",
        "列出所有项目"
    )
    
    # 3. 生成项目报告
    run_command(
        f"python run.py relation-query --db {db_name} --query-type report --project {project_name}",
        "生成项目报告"
    )
    
    # 4. 显示项目统计
    run_command(
        f"python run.py relation-query --db {db_name} --query-type stats --project {project_name}",
        "显示项目统计"
    )
    
    # 5. 查找main函数
    run_command(
        f"python run.py relation-query --db {db_name} --query-type find-func --project {project_name} --target main",
        "查找main函数"
    )
    
    # 6. 显示最常被调用的函数
    run_command(
        f"python run.py relation-query --db {db_name} --query-type top-called --project {project_name} --limit 5",
        "显示最常被调用的函数"
    )
    
    # 7. 显示最复杂的函数
    run_command(
        f"python run.py relation-query --db {db_name} --query-type top-complex --project {project_name} --limit 5",
        "显示最复杂的函数"
    )
    
    # 8. 文件依赖分析
    run_command(
        f"python run.py relation-query --db {db_name} --query-type deps-analysis --project {project_name}",
        "文件依赖分析"
    )
    
    # 9. 搜索函数
    run_command(
        f"python run.py relation-query --db {db_name} --query-type search --project {project_name} --keyword printf",
        "搜索printf函数"
    )
    
    # 10. 函数使用分析
    run_command(
        f"python run.py relation-query --db {db_name} --query-type func-usage --project {project_name} --target main",
        "分析main函数使用情况"
    )
    
    print(f"\n🎉 测试完成！")
    print(f"数据库文件: {db_name}")
    print(f"可以继续使用以下命令进行查询:")
    print(f"  python run.py relation-query --db {db_name} --query-type <command> --project {project_name}")


if __name__ == "__main__":
    main()
