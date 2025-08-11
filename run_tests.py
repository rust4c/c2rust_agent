#!/usr/bin/env python3
"""
测试运行脚本

统一运行所有数据库相关测试
"""

import subprocess
import sys
from pathlib import Path


def run_command(command: str, description: str) -> bool:
    """运行命令并返回是否成功"""
    print(f"\n{'='*60}")
    print(f"🔄 {description}")
    print(f"{'='*60}")

    try:
        result = subprocess.run(
            command.split(),
            capture_output=True,
            text=True,
            cwd=Path(__file__).parent
        )

        if result.returncode == 0:
            print(result.stdout)
            print(f"✅ {description} - 成功")
            return True
        else:
            print(f"❌ {description} - 失败")
            print(f"错误输出: {result.stderr}")
            return False
    except Exception as e:
        print(f"❌ {description} - 异常: {e}")
        return False


def main():
    """主函数"""
    print("🚀 开始运行 C2Rust Agent 数据库测试套件")

    tests = [
        ("uv run python tests/test_database_simple.py", "简单数据库测试"),
        ("uv run python tests/test_database.py", "完整数据库测试"),
        ("uv run python -m unittest tests.test_database -v", "单元测试详细模式")
    ]

    success_count = 0
    total_count = len(tests)

    for command, description in tests:
        if run_command(command, description):
            success_count += 1

    print(f"\n{'='*60}")
    print(f"📊 测试总结")
    print(f"{'='*60}")
    print(f"通过: {success_count}/{total_count}")
    print(f"失败: {total_count - success_count}/{total_count}")

    if success_count == total_count:
        print("🎉 所有测试通过!")
        return 0
    else:
        print("❌ 部分测试失败")
        return 1


if __name__ == "__main__":
    sys.exit(main())
