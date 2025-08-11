import os
import threading
import traceback
import logging
from typing import Dict, Any, Optional

try:
    import rapidjson as json
except ImportError:
    import json

from .EventManager import EventManager


# 事件列表
class Event:
    # C项目分析相关事件
    PROJECT_ANALYSIS_START = 100              # 项目分析开始
    PROJECT_ANALYSIS_DONE = 101               # 项目分析完成
    FILE_ANALYSIS_START = 102                 # 文件分析开始
    FILE_ANALYSIS_DONE = 103                  # 文件分析完成

    # C到Rust转换相关事件
    CONVERSION_START = 200                    # 转换开始
    CONVERSION_UPDATE = 201                   # 转换状态更新
    CONVERSION_DONE = 202                     # 转换完成
    CONVERSION_ERROR = 203                    # 转换错误

    # LLM相关事件
    LLM_REQUEST_START = 300                   # LLM请求开始
    LLM_REQUEST_DONE = 301                    # LLM请求完成
    LLM_REQUEST_ERROR = 302                   # LLM请求错误

    # 文件操作事件
    FILE_GENERATED = 400                      # 文件生成
    FILE_SAVED = 401                          # 文件保存

    # 应用事件
    APP_SHUT_DOWN = 999                       # 应用关闭


# 运行状态列表
class Status:
    IDLE = 1000                               # 无任务
    ANALYZING = 1001                          # 分析中
    CONVERTING = 1002                         # 转换中
    ERROR = 1003                              # 错误状态
    COMPLETED = 1004                          # 完成状态


class Base:
    """基础类 - 提供配置管理、事件处理、日志输出等基础功能"""

    # 事件列表
    EVENT = Event()

    # 状态列表
    STATUS = Status()

    # 配置文件路径
    CONFIG_PATH = os.path.join(".", "config", "config.json")

    # 类线程锁
    CONFIG_FILE_LOCK = threading.Lock()

    # 日志格式
    LOG_FORMAT = "[%(asctime)s | %(name)s | %(levelname)s] %(message)s"

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

        # 默认配置
        self.default = {}

        # 获取事件管理器单例
        self.event_manager_singleton = EventManager.get_singleton()

        # 类变量
        Base.work_status = Base.STATUS.IDLE if not hasattr(
            Base, "work_status") else Base.work_status

        # 初始化日志系统
        self._init_logging()

    def _init_logging(self):
        """初始化日志系统"""
        # 获取类名作为日志名称
        logger_name = self.__class__.__name__
        self.logger = logging.getLogger(logger_name)

        # 如果日志处理器已配置，则跳过
        if self.logger.handlers:
            return

        # 设置日志级别
        self.logger.setLevel(logging.DEBUG)

        # 创建控制台处理器
        console_handler = logging.StreamHandler()
        console_handler.setLevel(
            logging.DEBUG if self.is_debug() else logging.INFO)

        # 设置日志格式
        formatter = logging.Formatter(Base.LOG_FORMAT)
        console_handler.setFormatter(formatter)

        # 添加到日志器
        self.logger.addHandler(console_handler)

    # 检查是否处于调试模式
    def is_debug(self) -> bool:
        """检查是否为调试模式"""
        if getattr(Base, "_is_debug", None) is None:
            debug_path = os.path.join(".", "debug.txt")
            Base._is_debug = os.path.isfile(debug_path)
        return Base._is_debug

    # 重置调试模式检查状态
    def reset_debug(self) -> None:
        """重置调试模式状态"""
        Base._is_debug = None

    # 日志方法
    def print(self, msg: str) -> None:
        """打印消息（信息级别）"""
        self.logger.info(msg)

    def debug(self, msg: str, e: Exception = None) -> None:
        """调试日志"""
        if not self.is_debug():
            return

        if e is None:
            self.logger.debug(msg)
        else:
            self.logger.debug(
                f"{msg}\n{''.join(traceback.format_exception(None, e, e.__traceback__))}")

    def info(self, msg: str) -> None:
        """信息日志"""
        self.logger.info(msg)

    def error(self, msg: str, e: Exception = None) -> None:
        """错误日志"""
        if e is None:
            self.logger.error(msg)
        else:
            self.logger.error(
                f"{msg}\n{''.join(traceback.format_exception(None, e, e.__traceback__))}")

    def warning(self, msg: str) -> None:
        """警告日志"""
        self.logger.warning(msg)

    # 配置文件操作
    def load_config(self) -> dict:
        """载入配置文件"""
        config = {}

        with Base.CONFIG_FILE_LOCK:
            if os.path.exists(Base.CONFIG_PATH):
                try:
                    with open(Base.CONFIG_PATH, "r", encoding="utf-8") as reader:
                        config = json.load(reader)
                except Exception as e:
                    self.error(f"读取配置文件失败: {e}")
            else:
                self.debug("配置文件不存在，将使用默认配置")

        return config

    def save_config(self, new: dict) -> dict:
        """保存配置文件"""
        old = {}

        # 创建配置目录
        config_dir = os.path.dirname(Base.CONFIG_PATH)
        os.makedirs(config_dir, exist_ok=True)

        # 读取配置文件
        with Base.CONFIG_FILE_LOCK:
            if os.path.exists(Base.CONFIG_PATH):
                try:
                    with open(Base.CONFIG_PATH, "r", encoding="utf-8") as reader:
                        old = json.load(reader)
                except Exception as e:
                    self.error(f"读取配置文件失败: {e}")

        # 对比新旧数据是否一致，一致则跳过后续步骤
        if old == new:
            return old

        # 更新配置数据
        for k, v in new.items():
            old[k] = v

        # 写入配置文件
        with Base.CONFIG_FILE_LOCK:
            try:
                with open(Base.CONFIG_PATH, "w", encoding="utf-8") as writer:
                    writer.write(json.dumps(old, indent=4, ensure_ascii=False))
            except Exception as e:
                self.error(f"保存配置文件失败: {e}")

        return old

    def fill_config(self, old: dict, new: dict) -> dict:
        """深度合并字典配置"""
        for k, v in new.items():
            if isinstance(v, dict) and k in old and isinstance(old[k], dict):
                # 递归合并子字典
                old[k] = self.fill_config(old[k], v)
            elif k not in old:
                old[k] = v
        return old

    def load_config_from_default(self) -> dict:
        """用默认值更新并加载配置文件"""
        # 1. 加载已有配置
        config = self.load_config()

        # 2. 合并默认配置
        config = self.fill_config(
            old=config,
            new=getattr(self, "default", {})
        )

        # 3. 返回合并结果
        return config

    # 事件处理
    def emit(self, event: int, data: Any = None) -> None:
        """触发事件"""
        self.event_manager_singleton.emit(event, data)

    def subscribe(self, event: int, handler: callable) -> None:
        """订阅事件"""
        self.event_manager_singleton.subscribe(event, handler)

    def unsubscribe(self, event: int, handler: callable) -> None:
        """取消订阅事件"""
        self.event_manager_singleton.unsubscribe(event, handler)
