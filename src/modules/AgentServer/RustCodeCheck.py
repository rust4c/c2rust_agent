"""
Rust-analyzer模块

对 Rust 项目，检查是否有 cargo 存在，并尝试编译，返回编译结果
"""
import os
from pathlib import Path

from ..LLMRequester.LLMRequester import LLMRequester
from ..DatebaseServer.DatabaseManager import DatabaseManager

from ...base.Base import Base

class RustCodeCheck(Base):
    """
    Rust-analyzer模块

    对 Rust 项目，检查是否有 cargo 存在，并尝试编译，返回编译结果
    """
    def __init__(self, project_dir: Path):
        self.project_dir = project_dir
        super().__init__()

    def check_rust_project(self) -> bool | str:
        if not self.project_dir:
            return False

        cargo_toml = self.project_dir / "Cargo.toml"
        if not cargo_toml.exists():
            return False

        # 尝试编译项目
        is_success, result = self._run_cargo_build()

        if is_success:
            return True
        return result

    def _run_cargo_build(self) -> tuple[bool, str]:
        """
        尝试运行 cargo build 命令
        返回编译是否成功以及编译结果
        """
        import subprocess
        try:
            # 使用 subprocess 来同时捕获 stdout 和 stderr
            result = subprocess.run(
                ["cargo", "build"],
                cwd=self.project_dir,
                capture_output=True,
                text=True,
                timeout=30
            )
            
            # 合并 stdout 和 stderr
            full_output = result.stdout + result.stderr
            
            # 检查返回码，0 表示成功
            if result.returncode == 0:
                return True, full_output
            else:
                self.error(f"Cargo build 失败，返回码: {result.returncode}")
                self.error(f"错误输出: {full_output}")
                return False, full_output
                
        except subprocess.TimeoutExpired:
            return False, "构建超时"
        except Exception as e:
            return False, str(e)

if __name__ == "__main__":
    rust_checker = RustCodeCheck(Path("/Users/peng/Documents/AppCode/Rust/c2rust_agent/src-tauri"))
    result = rust_checker.check_rust_project()
    print(result)