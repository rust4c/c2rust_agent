"""
Base模块 - 提供基础功能支持

这个模块包含了C2Rust Agent的基础组件：
- Base: 基础类，提供配置管理、事件处理、日志输出等功能
- EventManager: 事件管理器，处理应用内事件通信
- PluginManager: 插件管理器，支持扩展功能

所有其他模块都应该继承自Base类来获得基础功能支持。
"""

from .Base import Base, Event, Status
from .EventManager import EventManager

__all__ = [
    "Base",
    "Event",
    "Status",
    "EventManager",
]
