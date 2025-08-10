from typing import Dict, List, Optional, Any, Tuple
import uuid
from .SQLiteServer import SQLiteServer
from .QdrantServer import QdrantServer
from ...base.Base import Base

def create_database_manager(
    sqlite_path: str = "c2rust_metadata.db",
    qdrant_url: str = "http://localhost:6333",
    qdrant_collection: str = "c2rust_vectors",
    vector_size: int = 384,  # 添加向量维度参数
    timeout: int = 60,  # 添加超时参数
    batch_size: int = 100  # 添加批次大小参数
) -> 'DatabaseManager':
    """
    创建数据库管理器实例

    Args:
        sqlite_path: SQLite数据库文件路径
        qdrant_url: Qdrant服务URL
        qdrant_collection: Qdrant集合名称
        vector_size: 向量维度
        timeout: 请求超时时间（秒）
        batch_size: 批量操作大小

    Returns:
        DatabaseManager实例
    """
    return DatabaseManager(sqlite_path, qdrant_url, qdrant_collection, vector_size, timeout, batch_size)

class DatabaseManager(Base):
    """数据库管理器 - 统一管理SQLite和Qdrant数据库"""
    
    def __init__(self, sqlite_path: str = "c2rust_metadata.db", 
                 qdrant_url: str = "http://localhost:6333",
                 qdrant_collection: str = "c2rust_vectors",
                 vector_size: int = 384,
                 timeout: int = 60,
                 batch_size: int = 100):
        super().__init__()
        self.sqlite_server = SQLiteServer(sqlite_path)
        self.qdrant_server = QdrantServer(qdrant_url, qdrant_collection, vector_size, timeout, batch_size)
        self._init_config()
    
    def _init_config(self):
        """初始化配置"""
        try:
            # 设置默认配置
            default_configs = {
                "ai_source": "deepseek",
                "auto_confirm_threshold": 0.85,
                "strict_mode": True,
                "pointer_strategy": "box",
                "max_translation_attempts": 3,
                "vector_similarity_threshold": 0.7
            }
            
            for key, value in default_configs.items():
                existing = self.sqlite_server.get_config(key)
                if existing is None:
                    self.sqlite_server.set_config(key, value, f"默认配置: {key}")
            
            self.info("数据库管理器初始化完成")
        except Exception as e:
            self.error(f"初始化配置失败: {e}")
    
    def store_interface_with_vector(self, name: str, inputs: List[Dict], outputs: List[Dict],
                                   file_path: str, code: str, vector: List[float],
                                   language: str = 'c', project_name: Optional[str] = None,
                                   metadata: Optional[Dict] = None) -> Tuple[int, str]:
        """存储接口元数据和向量表示"""
        try:
            # 首先存储向量到Qdrant
            qdrant_id = self.qdrant_server.insert_code_vector(
                code=code,
                vector=vector,
                language=language,
                function_name=name,
                project=project_name or "default",
                file_path=file_path,
                metadata=metadata
            )
            
            # 然后存储元数据到SQLite
            interface_id = self.sqlite_server.insert_interface(
                name=name,
                inputs=inputs,
                outputs=outputs,
                file_path=file_path,
                qdrant_id=qdrant_id,
                language=language,
                project_name=project_name
            )
            
            self.info(f"存储接口完成: {name}, SQLite ID: {interface_id}, Qdrant ID: {qdrant_id}")
            return interface_id, qdrant_id
        except Exception as e:
            self.error(f"存储接口失败: {e}")
            raise
    
    def search_similar_interfaces(self, query_vector: List[float], limit: int = 10,
                                 language: Optional[str] = None, 
                                 project: Optional[str] = None) -> List[Dict]:
        """搜索相似接口"""
        try:
            # 从Qdrant搜索相似向量
            similar_vectors = self.qdrant_server.search_similar_code(
                query_vector=query_vector,
                limit=limit,
                language=language,
                project=project,
                score_threshold=self.get_config("vector_similarity_threshold", 0.7)
            )
            
            # 获取对应的SQLite元数据
            results = []
            for vector_result in similar_vectors:
                qdrant_id = vector_result["id"]
                # 在SQLite中查找对应的接口
                interfaces = self.sqlite_server.search_interfaces()
                for interface in interfaces:
                    if interface.get("qdrant_id") == qdrant_id:
                        combined_result = {
                            "interface": interface,
                            "vector_info": vector_result,
                            "similarity_score": vector_result["score"]
                        }
                        results.append(combined_result)
                        break
            
            self.debug(f"搜索到 {len(results)} 个相似接口")
            return results
        except Exception as e:
            self.error(f"搜索相似接口失败: {e}")
            return []
    
    def get_interface_with_code(self, interface_id: int) -> Optional[Dict]:
        """获取接口及其代码"""
        try:
            # 获取SQLite中的接口元数据
            interface = self.sqlite_server.get_interface(interface_id)
            if not interface:
                return None
            
            # 获取Qdrant中的代码向量
            qdrant_id = interface.get("qdrant_id")
            if qdrant_id:
                code_data = self.qdrant_server.get_code_by_id(qdrant_id)
                if code_data:
                    interface["code"] = code_data["payload"].get("code", "")
                    interface["vector"] = code_data["vector"]
            
            return interface
        except Exception as e:
            self.error(f"获取接口代码失败: {e}")
            return None
    
    def add_translation_record(self, interface_id: int, original_code: str,
                              translated_code: str, translation_method: str,
                              success: bool = True, error_message: Optional[str] = None,
                              translated_vector: Optional[List[float]] = None) -> int:
        """添加转译记录"""
        try:
            # 添加转译历史到SQLite
            history_id = self.sqlite_server.add_translation_history(
                interface_id=interface_id,
                original_code=original_code,
                translated_code=translated_code,
                translation_method=translation_method,
                success=success,
                error_message=error_message
            )
            
            # 如果转译成功且有向量，存储Rust代码向量
            if success and translated_vector:
                interface = self.sqlite_server.get_interface(interface_id)
                if interface:
                    self.qdrant_server.insert_code_vector(
                        code=translated_code,
                        vector=translated_vector,
                        language="rust",
                        function_name=interface["name"],
                        project=interface.get("project_name", "default"),
                        file_path=interface["file_path"],
                        metadata={
                            "original_interface_id": interface_id,
                            "translation_method": translation_method,
                            "translation_history_id": history_id
                        }
                    )
            
            return history_id
        except Exception as e:
            self.error(f"添加转译记录失败: {e}")
            raise
    
    def search_interfaces_by_name(self, name: str, project: Optional[str] = None) -> List[Dict]:
        """按名称搜索接口"""
        try:
            return self.sqlite_server.search_interfaces(name=name, project_name=project)
        except Exception as e:
            self.error(f"按名称搜索接口失败: {e}")
            return []
    
    def search_code_by_text(self, query_text: str, language: Optional[str] = None,
                           project: Optional[str] = None) -> List[Dict]:
        """按文本内容搜索代码"""
        try:
            return self.qdrant_server.search_by_text(
                query_text=query_text,
                language=language,
                project=project
            )
        except Exception as e:
            self.error(f"按文本搜索代码失败: {e}")
            return []
    
    def create_project(self, name: str, path: str, description: Optional[str] = None) -> int:
        """创建项目"""
        try:
            return self.sqlite_server.create_project(name, path, description)
        except Exception as e:
            self.error(f"创建项目失败: {e}")
            raise
    
    def get_projects(self) -> List[Dict]:
        """获取项目列表"""
        try:
            return self.sqlite_server.get_projects()
        except Exception as e:
            self.error(f"获取项目列表失败: {e}")
            return []
    
    def get_config(self, key: str, default: Any = None) -> Any:
        """获取配置"""
        try:
            return self.sqlite_server.get_config(key, default)
        except Exception as e:
            self.error(f"获取配置失败: {e}")
            return default
    
    def set_config(self, key: str, value: Any, description: Optional[str] = None):
        """设置配置"""
        try:
            self.sqlite_server.set_config(key, value, description)
        except Exception as e:
            self.error(f"设置配置失败: {e}")
            raise
    
    def get_system_status(self) -> Dict:
        """获取系统状态"""
        try:
            sqlite_info = {
                "status": "connected" if self.sqlite_server.connection else "disconnected",
                "db_path": str(self.sqlite_server.db_path)
            }
            
            qdrant_info = self.qdrant_server.get_collection_info()
            if not qdrant_info:
                qdrant_info = {"status": "disconnected"}
            
            qdrant_health = self.qdrant_server.health_check()
            qdrant_info["health"] = "healthy" if qdrant_health else "unhealthy"
            
            return {
                "sqlite": sqlite_info,
                "qdrant": qdrant_info,
                "overall_status": "healthy" if sqlite_info["status"] == "connected" and qdrant_health else "unhealthy"
            }
        except Exception as e:
            self.error(f"获取系统状态失败: {e}")
            return {"overall_status": "error", "error": str(e)}
    
    def batch_store_interfaces(self, interfaces_data: List[Dict]) -> List[Tuple[int, str]]:
        """批量存储接口"""
        results = []
        vectors_data = []
        
        try:
            # 准备向量数据
            for data in interfaces_data:
                vector_data = {
                    "code": data["code"],
                    "vector": data["vector"],
                    "language": data.get("language", "c"),
                    "function_name": data["name"],
                    "project": data.get("project_name", "default"),
                    "file_path": data["file_path"],
                    "metadata": data.get("metadata", {})
                }
                vectors_data.append(vector_data)
            
            # 批量插入向量
            qdrant_ids = self.qdrant_server.batch_insert_vectors(vectors_data)
            
            # 逐个插入SQLite元数据
            for i, data in enumerate(interfaces_data):
                if i < len(qdrant_ids):
                    interface_id = self.sqlite_server.insert_interface(
                        name=data["name"],
                        inputs=data["inputs"],
                        outputs=data["outputs"],
                        file_path=data["file_path"],
                        qdrant_id=qdrant_ids[i],
                        language=data.get("language", "c"),
                        project_name=data.get("project_name")
                    )
                    results.append((interface_id, qdrant_ids[i]))
            
            self.info(f"批量存储 {len(results)} 个接口完成")
            return results
        except Exception as e:
            self.error(f"批量存储接口失败: {e}")
            return []
    
    def clear_project_data(self, project_name: str) -> bool:
        """清空项目数据"""
        try:
            # 获取项目的所有接口
            interfaces = self.sqlite_server.search_interfaces(project_name=project_name)
            
            # 删除Qdrant中的向量
            for interface in interfaces:
                qdrant_id = interface.get("qdrant_id")
                if qdrant_id:
                    self.qdrant_server.delete_code_vector(qdrant_id)
            
            # 这里简化处理，实际应该添加级联删除
            self.info(f"清空项目数据: {project_name}")
            return True
        except Exception as e:
            self.error(f"清空项目数据失败: {e}")
            return False
    
    def close(self):
        """关闭数据库连接"""
        try:
            self.sqlite_server.close()
            self.qdrant_server.close()
            self.info("数据库管理器已关闭")
        except Exception as e:
            self.error(f"关闭数据库管理器失败: {e}")
    
    def __del__(self):
        """析构函数"""
        self.close()

if __name__ == "__main__":
    manager = create_database_manager()
    
    # 测试 sqlite
    sqlite_test = manager.sqlite_server.get_config("test_key", "default_value")
    print(f"SQLite 测试配置: {sqlite_test}")