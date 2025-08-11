"""
C2Rust Agent - C语言到Rust语言转换工具

这是一个基于LLM的C到Rust代码转换工具，提供以下功能：
- C项目结构分析
- 智能代码转换
- 依赖关系处理
- 生成标准Rust项目结构

模块结构:
- base: 基础功能模块（配置、事件、日志等）
- modules: 核心功能模块
  - LLMRequester: LLM请求处理
  - FileParsing: 文件解析和分析
  - AgentServer: 代理服务器
  - QrantDBServer: 向量数据库服务
  - SQLiteServer: SQLite数据库服务
"""

__version__ = "0.1.0"
__author__ = "C2Rust Agent Team"
__description__ = "C to Rust conversion tool powered by LLM"

# 导出主要类和函数
from .base.Base import Base, Event, Status
from .base.EventManager import EventManager

__all__ = [
    "Base",
    "Event",
    "Status",
    "EventManager",
]
