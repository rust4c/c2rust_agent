import uuid
import logging
import numpy as np
from tqdm import tqdm
from typing import Dict, List, Optional, Any, Tuple
from qdrant_client import QdrantClient
from qdrant_client.models import Distance, VectorParams, PointStruct, Filter, FieldCondition, MatchValue
from qdrant_client.http.models import CollectionStatus
from ...base.Base import Base


class QdrantServer(Base):
    """Qdrant向量数据库服务器 - 用于存储代码向量表示"""
    
    def __init__(self, url: str = "http://localhost:6333", collection_name: str = "c2rust_vectors", 
                 vector_size: int = 384, timeout: int = 60, batch_size: int = 100):
        super().__init__()
        self.url = url
        self.collection_name = collection_name
        self.client: Optional[QdrantClient] = None
        self.vector_size = vector_size  # 使用传入的向量维度
        self.timeout = timeout  # 请求超时时间（秒）
        self.batch_size = batch_size  # 批量处理大小
        self._init_client()
    
    def _init_client(self):
        """初始化Qdrant客户端"""
        try:
            # 创建客户端时设置超时
            self.client = QdrantClient(url=self.url, timeout=self.timeout)
            self._ensure_collection()
            self.info(f"Qdrant客户端初始化成功: {self.url} (超时: {self.timeout}s, 批次大小: {self.batch_size})")
        except Exception as e:
            self.error(f"Qdrant客户端初始化失败: {e}")
            raise
    
    def _ensure_collection(self):
        """确保集合存在"""
        if not self.client:
            raise RuntimeError("Qdrant客户端未初始化")
        
        try:
            # 检查集合是否存在
            collections = self.client.get_collections()
            collection_names = [col.name for col in collections.collections]
            
            if self.collection_name not in collection_names:
                # 创建集合
                self.client.create_collection(
                    collection_name=self.collection_name,
                    vectors_config=VectorParams(
                        size=self.vector_size,
                        distance=Distance.COSINE
                    )
                )
                self.info(f"创建Qdrant集合: {self.collection_name}")
            else:
                # 集合已存在，尝试删除并重新创建以确保维度正确
                try:
                    self.client.delete_collection(self.collection_name)
                    self.info(f"删除现有集合: {self.collection_name}")
                    
                    # 重新创建集合
                    self.client.create_collection(
                        collection_name=self.collection_name,
                        vectors_config=VectorParams(
                            size=self.vector_size,
                            distance=Distance.COSINE
                        )
                    )
                    self.info(f"重新创建Qdrant集合: {self.collection_name} (维度: {self.vector_size})")
                except Exception as recreate_error:
                    self.warning(f"重新创建集合失败: {recreate_error}")
                    self.info(f"使用现有集合: {self.collection_name}")
        except Exception as e:
            self.error(f"确保集合存在失败: {e}")
            raise
    
    def insert_code_vector(self, code: str, vector: List[float], language: str = 'c',
                          function_name: str = "", project: str = "", 
                          file_path: str = "", metadata: Optional[Dict] = None) -> str:
        """插入代码向量"""
        if not self.client:
            raise RuntimeError("Qdrant客户端未初始化")
        
        try:
            # 生成唯一ID
            point_id = str(uuid.uuid4())
            
            # 构建payload
            payload = {
                "code": code,
                "language": language,
                "function_name": function_name,
                "project": project,
                "file_path": file_path,
                "timestamp": self._get_timestamp()
            }
            
            # 添加额外元数据
            if metadata:
                payload.update(metadata)
            
            # 插入向量
            self.client.upsert(
                collection_name=self.collection_name,
                points=[PointStruct(
                    id=point_id,
                    vector=vector,
                    payload=payload
                )]
            )
            
            self.debug(f"插入代码向量: {function_name}, ID: {point_id}")
            return point_id
        except Exception as e:
            self.error(f"插入代码向量失败: {e}")
            raise
    
    def search_similar_code(self, query_vector: List[float], limit: int = 10,
                           language: Optional[str] = None, project: Optional[str] = None,
                           score_threshold: float = 0.7) -> List[Dict]:
        """搜索相似代码"""
        if not self.client:
            return []
        
        try:
            # 构建过滤条件
            filter_conditions = []
            
            if language:
                filter_conditions.append(
                    FieldCondition(key="language", match=MatchValue(value=language))
                )
            
            if project:
                filter_conditions.append(
                    FieldCondition(key="project", match=MatchValue(value=project))
                )
            
            # 执行搜索 - 使用传统search方法以保持兼容性
            # TODO: 未来版本需要迁移到 query_points 方法
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", DeprecationWarning)
                search_result = self.client.search(
                    collection_name=self.collection_name,
                    query_vector=query_vector,
                    limit=limit,
                    score_threshold=score_threshold,
                    query_filter=Filter(must=filter_conditions) if filter_conditions else None
                )
            
            # 转换结果
            results = []
            for point in search_result:
                result = {
                    "id": str(point.id),
                    "score": float(point.score),
                    "payload": point.payload
                }
                results.append(result)
            
            self.debug(f"搜索到 {len(results)} 个相似代码")
            return results
        except Exception as e:
            self.error(f"搜索相似代码失败: {e}")
            return []
    
    def get_code_by_id(self, point_id: str) -> Optional[Dict]:
        """根据ID获取代码"""
        if not self.client:
            return None
        
        try:
            result = self.client.retrieve(
                collection_name=self.collection_name,
                ids=[point_id],
                with_payload=True,
                with_vectors=True
            )
            
            if result:
                point = result[0]
                return {
                    "id": str(point.id),
                    "vector": point.vector,
                    "payload": point.payload
                }
            return None
        except Exception as e:
            self.error(f"获取代码失败: {e}")
            return None
    
    def update_code_vector(self, point_id: str, vector: Optional[List[float]] = None,
                          payload: Optional[Dict] = None) -> bool:
        """更新代码向量"""
        if not self.client:
            return False
        
        try:
            # 如果只更新payload
            if payload and not vector:
                self.client.set_payload(
                    collection_name=self.collection_name,
                    payload=payload,
                    points=[point_id]
                )
            elif vector:
                # 更新向量和payload
                point_struct = PointStruct(
                    id=point_id,
                    vector=vector,
                    payload=payload or {}
                )
                self.client.upsert(
                    collection_name=self.collection_name,
                    points=[point_struct]
                )
            
            self.debug(f"更新代码向量: {point_id}")
            return True
        except Exception as e:
            self.error(f"更新代码向量失败: {e}")
            return False
    
    def delete_code_vector(self, point_id: str) -> bool:
        """删除代码向量"""
        if not self.client:
            return False
        
        try:
            self.client.delete(
                collection_name=self.collection_name,
                points_selector=[point_id]
            )
            self.debug(f"删除代码向量: {point_id}")
            return True
        except Exception as e:
            self.error(f"删除代码向量失败: {e}")
            return False
    
    def get_collection_info(self) -> Optional[Dict]:
        """获取集合信息"""
        if not self.client:
            return None
        
        try:
            info = self.client.get_collection(self.collection_name)
            
            # 安全地获取属性
            result = {
                "status": getattr(info, 'status', 'unknown'),
                "vectors_count": getattr(info, 'points_count', 0),
                "segments_count": getattr(info, 'segments_count', 0)
            }
            
            # 尝试获取配置信息
            try:
                if hasattr(info, 'config') and info.config:
                    config_info = {}
                    if hasattr(info.config, 'params') and info.config.params:
                        params = info.config.params
                        # 处理vectors配置
                        if hasattr(params, 'vectors') and params.vectors:
                            vectors_config = params.vectors
                            if isinstance(vectors_config, dict):
                                # 如果是字典，取第一个向量配置
                                for key, vector_params in vectors_config.items():
                                    config_info["vector_size"] = getattr(vector_params, 'size', self.vector_size)
                                    config_info["distance"] = str(getattr(vector_params, 'distance', 'COSINE'))
                                    break
                            else:
                                # 如果是单个配置对象
                                config_info["vector_size"] = getattr(vectors_config, 'size', self.vector_size)
                                config_info["distance"] = str(getattr(vectors_config, 'distance', 'COSINE'))
                    
                    result["config"] = config_info
            except Exception as config_error:
                self.debug(f"获取配置信息时出错: {config_error}")
                result["config"] = {"vector_size": self.vector_size, "distance": "COSINE"}
            
            return result
        except Exception as e:
            self.error(f"获取集合信息失败: {e}")
            return None
    
    def search_by_text(self, query_text: str, language: Optional[str] = None,
                      project: Optional[str] = None, limit: int = 10) -> List[Dict]:
        """基于文本内容搜索（使用payload过滤）"""
        if not self.client:
            return []
        
        try:
            # 构建过滤条件
            filter_conditions = []
            
            if language:
                filter_conditions.append(
                    FieldCondition(key="language", match=MatchValue(value=language))
                )
            
            if project:
                filter_conditions.append(
                    FieldCondition(key="project", match=MatchValue(value=project))
                )
            
            # 使用scroll API进行文本搜索
            result = self.client.scroll(
                collection_name=self.collection_name,
                scroll_filter=Filter(must=filter_conditions) if filter_conditions else None,
                limit=limit,
                with_payload=True,
                with_vectors=False
            )
            
            # 过滤包含查询文本的结果
            filtered_results = []
            for point in result[0]:  # result是tuple (points, next_page_offset)
                payload = point.payload or {}
                code_content = payload.get("code", "") if payload else ""
                if query_text.lower() in code_content.lower():
                    filtered_results.append({
                        "id": str(point.id),
                        "payload": payload
                    })
            
            self.debug(f"文本搜索到 {len(filtered_results)} 个匹配项")
            return filtered_results
        except Exception as e:
            self.error(f"文本搜索失败: {e}")
            return []
    
    def batch_insert_vectors(self, vectors_data: List[Dict]) -> List[str]:
        """批量插入向量 - 支持大规模数据的分批处理"""
        if not self.client:
            return []
        
        try:
            total_vectors = len(vectors_data)
            if total_vectors == 0:
                return []
            
            # 首先进行健康检查
            if not self.health_check():
                self.error("Qdrant服务不可用，尝试重新连接...")
                self._init_client()
                if not self.health_check():
                    raise RuntimeError("无法连接到Qdrant服务")
            
            self.info(f"开始批量插入 {total_vectors} 个向量，批次大小: {self.batch_size}")
            
            all_point_ids = []
            
            # 导入tqdm进度条
            
            # 分批处理以避免超时
            total_batches = (total_vectors + self.batch_size - 1) // self.batch_size
            
            # 使用tqdm的miniters参数减少更新频率，并禁用httpx日志
            logging.getLogger("httpx").setLevel(logging.WARNING)

            with tqdm(total=total_vectors, desc="批量插入向量", unit="vectors", miniters=self.batch_size) as pbar:
                for i in range(0, total_vectors, self.batch_size):
                    batch_data = vectors_data[i:i + self.batch_size]
                    batch_num = i // self.batch_size + 1
                    
                    pbar.set_description(f"处理批次 {batch_num}/{total_batches}")
                    
                    # 处理当前批次
                    batch_point_ids = self._insert_batch_with_retry(batch_data, batch_num)
                    all_point_ids.extend(batch_point_ids)
                    
                    # 更新进度条
                    pbar.update(len(batch_point_ids))
            
            self.info(f"批量插入完成，成功插入 {len(all_point_ids)}/{total_vectors} 个向量")
            return all_point_ids
            
        except Exception as e:
            self.error(f"批量插入向量失败: {e}")
            return []
    
    def _insert_batch_with_retry(self, batch_data: List[Dict], batch_num: int, max_retries: int = 3) -> List[str]:
        """插入单个批次，带重试机制"""
        import time
        
        for attempt in range(max_retries):
            try:
                points = []
                point_ids = []
                
                for data in batch_data:
                    point_id = str(uuid.uuid4())
                    point_ids.append(point_id)
                    
                    payload = {
                        "code": data.get("code", ""),
                        "language": data.get("language", "c"),
                        "function_name": data.get("function_name", ""),
                        "project": data.get("project", ""),
                        "file_path": data.get("file_path", ""),
                        "timestamp": self._get_timestamp(),
                        "batch_num": batch_num  # 添加批次标识
                    }
                    
                    # 添加额外元数据
                    if "metadata" in data:
                        payload.update(data["metadata"])
                    
                    points.append(PointStruct(
                        id=point_id,
                        vector=data["vector"],
                        payload=payload
                    ))
                
                # 检查客户端状态
                if not self.client:
                    raise RuntimeError("Qdrant客户端未初始化")
                
                # 执行批量插入
                self.client.upsert(
                    collection_name=self.collection_name,
                    points=points,
                    wait=True  # 等待操作完成
                )
                
                self.debug(f"批次 {batch_num} 插入成功: {len(points)} 个向量")
                return point_ids
                
            except Exception as e:
                if attempt < max_retries - 1:
                    wait_time = (attempt + 1) * 2  # 指数退避
                    self.warning(f"批次 {batch_num} 插入失败 (尝试 {attempt + 1}/{max_retries}): {e}")
                    self.warning(f"等待 {wait_time} 秒后重试...")
                    time.sleep(wait_time)
                else:
                    self.error(f"批次 {batch_num} 插入失败，已达最大重试次数: {e}")
                    raise
        
        return []
    
    def clear_collection(self) -> bool:
        """清空集合"""
        if not self.client:
            return False
        
        try:
            self.client.delete_collection(self.collection_name)
            self._ensure_collection()  # 重新创建集合
            self.info(f"清空集合: {self.collection_name}")
            return True
        except Exception as e:
            self.error(f"清空集合失败: {e}")
            return False
    
    def _get_timestamp(self) -> str:
        """获取当前时间戳"""
        from datetime import datetime

        return datetime.now().isoformat()
    
    def health_check(self) -> bool:
        """健康检查"""
        try:
            if not self.client:
                return False
            
            # 尝试获取集合信息
            info = self.get_collection_info()
            return info is not None
        except Exception as e:
            self.error(f"健康检查失败: {e}")
            return False
    
    def close(self):
        """关闭客户端连接"""
        if self.client:
            # Qdrant客户端通常不需要显式关闭
            self.client = None
            self.info("Qdrant客户端连接已关闭")
    
    def __del__(self):
        """析构函数"""
        self.close()