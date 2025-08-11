import sqlite3
import json
import uuid
from pathlib import Path
from typing import Dict, List, Optional, Any, Tuple
from datetime import datetime
from ...base.Base import Base


class SQLiteServer(Base):
    """SQLite数据库服务器 - 用于存储代码接口元数据"""
    
    def __init__(self, db_path: str = "c2rust_metadata.db"):
        super().__init__()
        self.db_path = Path(db_path)
        self.connection: Optional[sqlite3.Connection] = None
        self._init_database()
    
    def _init_database(self):
        """初始化数据库连接和表结构"""
        try:
            self.connection = sqlite3.connect(str(self.db_path), check_same_thread=False)
            self.connection.execute("PRAGMA foreign_keys = ON")
            self._create_tables()
            self.info(f"SQLite数据库初始化成功: {self.db_path}")
        except Exception as e:
            self.error(f"SQLite数据库初始化失败: {e}")
            raise
    
    def _create_tables(self):
        """创建数据库表结构"""
        if not self.connection:
            raise RuntimeError("数据库连接未初始化")
            
        tables_sql = [
            # 接口元数据表
            """
            CREATE TABLE IF NOT EXISTS interfaces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                inputs TEXT,  -- JSON 格式参数类型
                outputs TEXT,
                file_path TEXT NOT NULL,
                qdrant_id TEXT NOT NULL,
                language TEXT DEFAULT 'c',
                project_name TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            """,
            
            # 配置表
            """
            CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                description TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            """,
            
            # 项目表
            """
            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                path TEXT NOT NULL,
                description TEXT,
                status TEXT DEFAULT 'active',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
            """,
            
            # 转译历史表
            """
            CREATE TABLE IF NOT EXISTS translation_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                interface_id INTEGER,
                original_code TEXT NOT NULL,
                translated_code TEXT NOT NULL,
                translation_method TEXT,
                success BOOLEAN DEFAULT FALSE,
                error_message TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (interface_id) REFERENCES interfaces (id)
            )
            """,
            
            # 索引
            "CREATE INDEX IF NOT EXISTS idx_interfaces_name ON interfaces(name)",
            "CREATE INDEX IF NOT EXISTS idx_interfaces_project ON interfaces(project_name)",
            "CREATE INDEX IF NOT EXISTS idx_interfaces_qdrant ON interfaces(qdrant_id)",
            "CREATE INDEX IF NOT EXISTS idx_translation_interface ON translation_history(interface_id)"
        ]
        
        for sql in tables_sql:
            self.connection.execute(sql)
        
        self.connection.commit()
    
    def insert_interface(self, name: str, inputs: List[Dict], outputs: List[Dict], 
                        file_path: str, qdrant_id: str, language: str = 'c', 
                        project_name: Optional[str] = None) -> int:
        """插入接口元数据"""
        if not self.connection:
            raise RuntimeError("数据库连接未初始化")
            
        try:
            cursor = self.connection.execute(
                """
                INSERT INTO interfaces (name, inputs, outputs, file_path, qdrant_id, language, project_name)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (name, json.dumps(inputs), json.dumps(outputs), file_path, qdrant_id, language, project_name)
            )
            self.connection.commit()
            interface_id = cursor.lastrowid
            if interface_id is None:
                raise RuntimeError("插入失败，未获取到ID")
            self.debug(f"插入接口: {name}, ID: {interface_id}")
            return interface_id
        except Exception as e:
            self.error(f"插入接口失败: {e}")
            raise
    
    def get_interface(self, interface_id: int) -> Optional[Dict]:
        """根据ID获取接口信息"""
        if not self.connection:
            return None
            
        try:
            cursor = self.connection.execute(
                "SELECT * FROM interfaces WHERE id = ?", (interface_id,)
            )
            row = cursor.fetchone()
            if row:
                return self._row_to_dict(cursor, row)
            return None
        except Exception as e:
            self.error(f"获取接口失败: {e}")
            return None
    
    def search_interfaces(self, name: Optional[str] = None, project_name: Optional[str] = None, 
                         language: Optional[str] = None) -> List[Dict]:
        """搜索接口"""
        if not self.connection:
            return []
            
        try:
            conditions = []
            params = []
            
            if name:
                conditions.append("name LIKE ?")
                params.append(f"%{name}%")
            if project_name:
                conditions.append("project_name = ?")
                params.append(project_name)
            if language:
                conditions.append("language = ?")
                params.append(language)
            
            where_clause = "WHERE " + " AND ".join(conditions) if conditions else ""
            
            cursor = self.connection.execute(
                f"SELECT * FROM interfaces {where_clause} ORDER BY created_at DESC",
                params
            )
            
            return [self._row_to_dict(cursor, row) for row in cursor.fetchall()]
        except Exception as e:
            self.error(f"搜索接口失败: {e}")
            return []
    
    def set_config(self, key: str, value: Any, description: Optional[str] = None):
        """设置配置项"""
        if not self.connection:
            raise RuntimeError("数据库连接未初始化")
            
        try:
            value_str = json.dumps(value) if not isinstance(value, str) else value
            self.connection.execute(
                """
                INSERT OR REPLACE INTO config (key, value, description, updated_at)
                VALUES (?, ?, ?, CURRENT_TIMESTAMP)
                """,
                (key, value_str, description)
            )
            self.connection.commit()
            self.debug(f"设置配置: {key} = {value}")
        except Exception as e:
            self.error(f"设置配置失败: {e}")
            raise
    
    def get_config(self, key: str, default: Any = None) -> Any:
        """获取配置项"""
        if not self.connection:
            return default
            
        try:
            cursor = self.connection.execute(
                "SELECT value FROM config WHERE key = ?", (key,)
            )
            row = cursor.fetchone()
            if row:
                value = row[0]
                try:
                    return json.loads(value)
                except json.JSONDecodeError:
                    return value
            return default
        except Exception as e:
            self.error(f"获取配置失败: {e}")
            return default
    
    def create_project(self, name: str, path: str, description: Optional[str] = None) -> int:
        """创建项目"""
        if not self.connection:
            raise RuntimeError("数据库连接未初始化")
            
        try:
            cursor = self.connection.execute(
                """
                INSERT INTO projects (name, path, description)
                VALUES (?, ?, ?)
                """,
                (name, path, description)
            )
            self.connection.commit()
            project_id = cursor.lastrowid
            if project_id is None:
                raise RuntimeError("创建项目失败，未获取到ID")
            self.info(f"创建项目: {name}, ID: {project_id}")
            return project_id
        except Exception as e:
            self.error(f"创建项目失败: {e}")
            raise
    
    def get_projects(self) -> List[Dict]:
        """获取所有项目"""
        if not self.connection:
            return []
            
        try:
            cursor = self.connection.execute(
                "SELECT * FROM projects ORDER BY created_at DESC"
            )
            return [self._row_to_dict(cursor, row) for row in cursor.fetchall()]
        except Exception as e:
            self.error(f"获取项目列表失败: {e}")
            return []
    
    def add_translation_history(self, interface_id: int, original_code: str,
                               translated_code: str, translation_method: str,
                               success: bool = True, error_message: Optional[str] = None) -> int:
        """添加转译历史记录"""
        if not self.connection:
            raise RuntimeError("数据库连接未初始化")
            
        try:
            cursor = self.connection.execute(
                """
                INSERT INTO translation_history 
                (interface_id, original_code, translated_code, translation_method, success, error_message)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                (interface_id, original_code, translated_code, translation_method, success, error_message)
            )
            self.connection.commit()
            history_id = cursor.lastrowid
            if history_id is None:
                raise RuntimeError("添加转译历史失败，未获取到ID")
            return history_id
        except Exception as e:
            self.error(f"添加转译历史失败: {e}")
            raise
    
    def get_translation_history(self, interface_id: int) -> List[Dict]:
        """获取接口的转译历史"""
        if not self.connection:
            return []
            
        try:
            cursor = self.connection.execute(
                """
                SELECT * FROM translation_history 
                WHERE interface_id = ? 
                ORDER BY created_at DESC
                """,
                (interface_id,)
            )
            return [self._row_to_dict(cursor, row) for row in cursor.fetchall()]
        except Exception as e:
            self.error(f"获取转译历史失败: {e}")
            return []
    
    def _row_to_dict(self, cursor, row) -> Dict:
        """将数据库行转换为字典"""
        columns = [description[0] for description in cursor.description]
        result = dict(zip(columns, row))
        
        # 解析JSON字段
        for field in ['inputs', 'outputs']:
            if field in result and result[field]:
                try:
                    result[field] = json.loads(result[field])
                except json.JSONDecodeError:
                    pass
        
        return result
    
    def close(self):
        """关闭数据库连接"""
        if self.connection:
            self.connection.close()
            self.connection = None
            self.info("SQLite数据库连接已关闭")
    
    def __del__(self):
        """析构函数"""
        self.close()