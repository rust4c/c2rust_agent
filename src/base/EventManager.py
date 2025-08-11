import threading
from typing import Dict, List, Callable, Any


class EventManager:
    """事件管理器 - 简化版本，不依赖PyQt"""

    _singleton = None
    _lock = threading.RLock()

    def __init__(self):
        # 事件回调列表
        self.event_callbacks: Dict[int, List[Callable]] = {}
        # 线程锁
        self._event_lock = threading.RLock()

    @classmethod
    def get_singleton(cls):
        """获取单例"""
        with cls._lock:
            if cls._singleton is None:
                cls._singleton = EventManager()
            return cls._singleton

    def process_event(self, event: int, data: Any):
        """处理事件"""
        with self._event_lock:
            if event in self.event_callbacks:
                for handler in self.event_callbacks[event]:
                    try:
                        handler(event, data)
                    except Exception as e:
                        print(f"Error in event handler for event {event}: {e}")

    def emit(self, event: int, data: Any = None):
        """触发事件"""
        self.process_event(event, data)

    def subscribe(self, event: int, handler: Callable):
        """订阅事件"""
        with self._event_lock:
            if event not in self.event_callbacks:
                self.event_callbacks[event] = []
            if handler not in self.event_callbacks[event]:
                self.event_callbacks[event].append(handler)

    def unsubscribe(self, event: int, handler: Callable):
        """取消订阅事件"""
        with self._event_lock:
            if event in self.event_callbacks and handler in self.event_callbacks[event]:
                self.event_callbacks[event].remove(handler)
