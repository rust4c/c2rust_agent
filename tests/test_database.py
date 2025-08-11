#!/usr/bin/env python3
"""
数据库组件测试用例

测试SQLite、Qdrant和DatabaseManager的完整功能
"""

import os
import sys
import unittest
import tempfile
import shutil
import numpy as np
from pathlib import Path

# 添加项目根目录到Python路径
project_root = Path(__file__).parent.parent
sys.path.insert(0, str(project_root))

from src.modules.DatebaseServer import SQLiteServer, QdrantServer, DatabaseManager
from src.modules.DatebaseServer.DatabaseManager import create_database_manager


class TestSQLiteServer(unittest.TestCase):
    """SQLite服务器测试"""
    
    def setUp(self):
        """测试前准备"""
        self.test_dir = tempfile.mkdtemp()
        self.db_path = os.path.join(self.test_dir, "test.db")
        self.sqlite_server = SQLiteServer(self.db_path)
    
    def tearDown(self):
        """测试后清理"""
        self.sqlite_server.close()
        shutil.rmtree(self.test_dir)
    
    def test_database_initialization(self):
        """测试数据库初始化"""
        self.assertIsNotNone(self.sqlite_server.connection)
        self.assertTrue(os.path.exists(self.db_path))
    
    def test_config_operations(self):
        """测试配置操作"""
        # 设置配置
        self.sqlite_server.set_config("test_key", "test_value", "测试配置")
        
        # 获取配置
        value = self.sqlite_server.get_config("test_key")
        self.assertEqual(value, "test_value")
        
        # 获取不存在的配置
        default_value = self.sqlite_server.get_config("non_existent", "default")
        self.assertEqual(default_value, "default")
        
        # 设置JSON配置
        json_config = {"key1": "value1", "key2": [1, 2, 3]}
        self.sqlite_server.set_config("json_config", json_config)
        retrieved_config = self.sqlite_server.get_config("json_config")
        self.assertEqual(retrieved_config, json_config)
    
    def test_project_operations(self):
        """测试项目操作"""
        # 创建项目
        project_id = self.sqlite_server.create_project(
            name="test_project",
            path="/path/to/project",
            description="测试项目"
        )
        self.assertIsInstance(project_id, int)
        self.assertGreater(project_id, 0)
        
        # 获取项目列表
        projects = self.sqlite_server.get_projects()
        self.assertEqual(len(projects), 1)
        self.assertEqual(projects[0]["name"], "test_project")
        
        # 测试重复项目名称
        with self.assertRaises(Exception):
            self.sqlite_server.create_project("test_project", "/another/path")
    
    def test_interface_operations(self):
        """测试接口操作"""
        # 插入接口
        inputs = [{"name": "size", "type": "int"}]
        outputs = [{"type": "int*"}]
        
        interface_id = self.sqlite_server.insert_interface(
            name="create_buffer",
            inputs=inputs,
            outputs=outputs,
            file_path="/path/to/file.c",
            qdrant_id="test-uuid-123",
            language="c",
            project_name="test_project"
        )
        self.assertIsInstance(interface_id, int)
        
        # 获取接口
        interface = self.sqlite_server.get_interface(interface_id)
        self.assertIsNotNone(interface)
        if interface:  # 类型检查
            self.assertEqual(interface["name"], "create_buffer")
            self.assertEqual(interface["inputs"], inputs)
            self.assertEqual(interface["outputs"], outputs)
        
        # 搜索接口
        results = self.sqlite_server.search_interfaces(name="create")
        self.assertEqual(len(results), 1)
        
        results = self.sqlite_server.search_interfaces(project_name="test_project")
        self.assertEqual(len(results), 1)
        
        results = self.sqlite_server.search_interfaces(language="c")
        self.assertEqual(len(results), 1)
        
        # 不存在的接口
        non_existent = self.sqlite_server.get_interface(9999)
        self.assertIsNone(non_existent)
    
    def test_translation_history(self):
        """测试转译历史"""
        # 先创建接口
        interface_id = self.sqlite_server.insert_interface(
            name="test_func",
            inputs=[],
            outputs=[],
            file_path="/test.c",
            qdrant_id="test-uuid"
        )
        
        # 添加转译历史
        history_id = self.sqlite_server.add_translation_history(
            interface_id=interface_id,
            original_code="int* create_buffer(int size);",
            translated_code="fn create_buffer(size: i32) -> Box<[i32]>",
            translation_method="deepseek",
            success=True
        )
        self.assertIsInstance(history_id, int)
        
        # 获取转译历史
        history = self.sqlite_server.get_translation_history(interface_id)
        self.assertEqual(len(history), 1)
        self.assertEqual(history[0]["translation_method"], "deepseek")
        self.assertTrue(history[0]["success"])
        
        # 添加失败的转译记录
        self.sqlite_server.add_translation_history(
            interface_id=interface_id,
            original_code="int* create_buffer(int size);",
            translated_code="",
            translation_method="openai",
            success=False,
            error_message="编译错误"
        )
        
        history = self.sqlite_server.get_translation_history(interface_id)
        self.assertEqual(len(history), 2)


class TestQdrantServer(unittest.TestCase):
    """Qdrant服务器测试"""
    
    @classmethod
    def setUpClass(cls):
        """类级别设置 - 检查Qdrant是否可用"""
        try:
            from qdrant_client import QdrantClient
            test_client = QdrantClient(url="http://localhost:6333")
            test_client.get_collections()
            cls.qdrant_available = True
        except Exception:
            cls.qdrant_available = False
            print("警告: Qdrant服务不可用，跳过Qdrant测试")
    
    def setUp(self):
        """测试前准备"""
        if not self.qdrant_available:
            self.skipTest("Qdrant服务不可用")
        
        self.test_collection = "test_c2rust_vectors"
        self.qdrant_server = QdrantServer(
            url="http://localhost:6333",
            collection_name=self.test_collection
        )
    
    def tearDown(self):
        """测试后清理"""
        if hasattr(self, 'qdrant_server'):
            try:
                self.qdrant_server.clear_collection()
                self.qdrant_server.close()
            except:
                pass
    
    def test_collection_initialization(self):
        """测试集合初始化"""
        info = self.qdrant_server.get_collection_info()
        self.assertIsNotNone(info)
        self.assertIn("status", info)
    
    def test_vector_operations(self):
        """测试向量操作"""
        # 创建测试向量
        test_vector = np.random.random(768).tolist()
        
        # 插入向量
        point_id = self.qdrant_server.insert_code_vector(
            code="int* create_buffer(int size) { return malloc(size * sizeof(int)); }",
            vector=test_vector,
            language="c",
            function_name="create_buffer",
            project="test_project",
            file_path="/test/buffer.c",
            metadata={"complexity": "simple"}
        )
        self.assertIsInstance(point_id, str)
        
        # 获取向量
        retrieved = self.qdrant_server.get_code_by_id(point_id)
        self.assertIsNotNone(retrieved)
        self.assertEqual(retrieved["payload"]["function_name"], "create_buffer")
        self.assertEqual(retrieved["payload"]["language"], "c")
        
        # 更新向量
        success = self.qdrant_server.update_code_vector(
            point_id=point_id,
            payload={"complexity": "updated"}
        )
        self.assertTrue(success)
        
        # 验证更新
        updated = self.qdrant_server.get_code_by_id(point_id)
        self.assertEqual(updated["payload"]["complexity"], "updated")
        
        # 删除向量
        delete_success = self.qdrant_server.delete_code_vector(point_id)
        self.assertTrue(delete_success)
        
        # 验证删除
        deleted = self.qdrant_server.get_code_by_id(point_id)
        self.assertIsNone(deleted)
    
    def test_similarity_search(self):
        """测试相似性搜索"""
        # 插入多个向量
        vectors_data = []
        for i in range(5):
            vector = np.random.random(768).tolist()
            point_id = self.qdrant_server.insert_code_vector(
                code=f"void function_{i}() {{ /* code {i} */ }}",
                vector=vector,
                language="c",
                function_name=f"function_{i}",
                project="test_project"
            )
            vectors_data.append((point_id, vector))
        
        # 使用第一个向量进行搜索
        search_vector = vectors_data[0][1]
        results = self.qdrant_server.search_similar_code(
            query_vector=search_vector,
            limit=3,
            language="c",
            project="test_project"
        )
        
        self.assertGreater(len(results), 0)
        self.assertLessEqual(len(results), 3)
        
        # 第一个结果应该是最相似的（自己）
        self.assertEqual(results[0]["id"], vectors_data[0][0])
        self.assertGreater(results[0]["score"], 0.9)  # 应该几乎完全匹配
    
    def test_text_search(self):
        """测试文本搜索"""
        # 插入包含特定文本的向量
        test_vector = np.random.random(768).tolist()
        self.qdrant_server.insert_code_vector(
            code="int create_buffer_with_malloc(int size) { return malloc(size); }",
            vector=test_vector,
            language="c",
            function_name="create_buffer_with_malloc"
        )
        
        # 搜索包含"malloc"的代码
        results = self.qdrant_server.search_by_text("malloc")
        self.assertGreater(len(results), 0)
        
        found_malloc = False
        for result in results:
            if "malloc" in result["payload"]["code"]:
                found_malloc = True
                break
        self.assertTrue(found_malloc)
    
    def test_batch_operations(self):
        """测试批量操作"""
        # 准备批量数据
        batch_data = []
        for i in range(3):
            batch_data.append({
                "code": f"void batch_function_{i}() {{ }}",
                "vector": np.random.random(768).tolist(),
                "language": "c",
                "function_name": f"batch_function_{i}",
                "project": "batch_test"
            })
        
        # 批量插入
        point_ids = self.qdrant_server.batch_insert_vectors(batch_data)
        self.assertEqual(len(point_ids), 3)
        
        # 验证插入
        for point_id in point_ids:
            result = self.qdrant_server.get_code_by_id(point_id)
            self.assertIsNotNone(result)
    
    def test_health_check(self):
        """测试健康检查"""
        health = self.qdrant_server.health_check()
        self.assertTrue(health)


class TestDatabaseManager(unittest.TestCase):
    """数据库管理器测试"""
    
    @classmethod
    def setUpClass(cls):
        """类级别设置"""
        try:
            from qdrant_client import QdrantClient
            test_client = QdrantClient(url="http://localhost:6333")
            test_client.get_collections()
            cls.qdrant_available = True
        except Exception:
            cls.qdrant_available = False
    
    def setUp(self):
        """测试前准备"""
        self.test_dir = tempfile.mkdtemp()
        self.db_path = os.path.join(self.test_dir, "test_manager.db")
        
        if self.qdrant_available:
            self.manager = create_database_manager(
                sqlite_path=self.db_path,
                qdrant_url="http://localhost:6333",
                qdrant_collection="test_manager_vectors"
            )
        else:
            # 如果Qdrant不可用，只测试SQLite部分
            self.manager = None
    
    def tearDown(self):
        """测试后清理"""
        if self.manager:
            try:
                self.manager.close()
            except:
                pass
        shutil.rmtree(self.test_dir)
    
    def test_manager_initialization(self):
        """测试管理器初始化"""
        if not self.qdrant_available:
            self.skipTest("Qdrant服务不可用")
        
        self.assertIsNotNone(self.manager)
        
        # 检查系统状态
        status = self.manager.get_system_status()
        self.assertIn("overall_status", status)
        self.assertIn("sqlite", status)
        self.assertIn("qdrant", status)
    
    def test_complete_workflow(self):
        """测试完整工作流程"""
        if not self.qdrant_available:
            self.skipTest("Qdrant服务不可用")
        
        # 1. 创建项目
        project_id = self.manager.create_project(
            name="test_c_project",
            path="/path/to/c/project",
            description="测试C项目"
        )
        self.assertIsInstance(project_id, int)
        
        # 2. 存储接口和向量
        test_vector = np.random.random(768).tolist()
        interface_id, qdrant_id = self.manager.store_interface_with_vector(
            name="create_buffer",
            inputs=[{"name": "size", "type": "int"}],
            outputs=[{"type": "int*"}],
            file_path="/project/buffer.c",
            code="int* create_buffer(int size) { return malloc(size * sizeof(int)); }",
            vector=test_vector,
            language="c",
            project_name="test_c_project",
            metadata={"complexity": "simple"}
        )
        
        self.assertIsInstance(interface_id, int)
        self.assertIsInstance(qdrant_id, str)
        
        # 3. 获取完整接口信息
        interface_with_code = self.manager.get_interface_with_code(interface_id)
        self.assertIsNotNone(interface_with_code)
        self.assertEqual(interface_with_code["name"], "create_buffer")
        self.assertIn("code", interface_with_code)
        
        # 4. 搜索相似接口
        similar_interfaces = self.manager.search_similar_interfaces(
            query_vector=test_vector,
            limit=5,
            language="c",
            project="test_c_project"
        )
        self.assertGreater(len(similar_interfaces), 0)
        
        # 5. 添加转译记录
        rust_vector = np.random.random(768).tolist()
        history_id = self.manager.add_translation_record(
            interface_id=interface_id,
            original_code="int* create_buffer(int size);",
            translated_code="fn create_buffer(size: i32) -> Box<[i32]>",
            translation_method="deepseek",
            success=True,
            translated_vector=rust_vector
        )
        self.assertIsInstance(history_id, int)
        
        # 6. 按名称搜索接口
        name_results = self.manager.search_interfaces_by_name("create", "test_c_project")
        self.assertGreater(len(name_results), 0)
        
        # 7. 按文本搜索代码
        text_results = self.manager.search_code_by_text("malloc", "c", "test_c_project")
        self.assertGreater(len(text_results), 0)
        
        # 8. 获取项目列表
        projects = self.manager.get_projects()
        self.assertGreater(len(projects), 0)
        
        # 9. 配置操作
        self.manager.set_config("test_config", {"setting": "value"})
        config_value = self.manager.get_config("test_config")
        self.assertEqual(config_value, {"setting": "value"})
    
    def test_batch_operations(self):
        """测试批量操作"""
        if not self.qdrant_available:
            self.skipTest("Qdrant服务不可用")
        
        # 准备批量数据
        interfaces_data = []
        for i in range(3):
            interfaces_data.append({
                "name": f"batch_function_{i}",
                "inputs": [{"name": "param", "type": "int"}],
                "outputs": [{"type": "void"}],
                "file_path": f"/batch/file_{i}.c",
                "code": f"void batch_function_{i}(int param) {{ /* function {i} */ }}",
                "vector": np.random.random(768).tolist(),
                "language": "c",
                "project_name": "batch_project"
            })
        
        # 批量存储
        results = self.manager.batch_store_interfaces(interfaces_data)
        self.assertEqual(len(results), 3)
        
        # 验证存储
        for interface_id, qdrant_id in results:
            interface = self.manager.get_interface_with_code(interface_id)
            self.assertIsNotNone(interface)
            self.assertIn("code", interface)
    
    def test_error_handling(self):
        """测试错误处理"""
        if not self.qdrant_available:
            self.skipTest("Qdrant服务不可用")
        
        # 测试获取不存在的接口
        non_existent = self.manager.get_interface_with_code(9999)
        self.assertIsNone(non_existent)
        
        # 测试无效的配置操作
        invalid_config = self.manager.get_config("non_existent_key", "default")
        self.assertEqual(invalid_config, "default")


class TestRealWorldScenarios(unittest.TestCase):
    """真实世界场景测试"""
    
    def setUp(self):
        """测试前准备"""
        self.test_dir = tempfile.mkdtemp()
        self.db_path = os.path.join(self.test_dir, "real_world.db")
        
        try:
            from qdrant_client import QdrantClient
            test_client = QdrantClient(url="http://localhost:6333")
            test_client.get_collections()
            self.qdrant_available = True
            self.manager = create_database_manager(
                sqlite_path=self.db_path,
                qdrant_collection="real_world_test"
            )
        except Exception:
            self.qdrant_available = False
            self.manager = None
    
    def tearDown(self):
        """测试后清理"""
        if self.manager:
            try:
                self.manager.close()
            except:
                pass
        shutil.rmtree(self.test_dir)
    
    def test_c_to_rust_translation_workflow(self):
        """测试C到Rust转译的完整工作流"""
        if not self.qdrant_available:
            self.skipTest("Qdrant服务不可用")
        
        # 模拟真实的C项目
        project_id = self.manager.create_project(
            name="memory_manager",
            path="/home/user/projects/memory_manager",
            description="内存管理库"
        )
        
        # 模拟C函数及其向量表示
        c_functions = [
            {
                "name": "malloc_wrapper",
                "code": "void* malloc_wrapper(size_t size) { return malloc(size); }",
                "inputs": [{"name": "size", "type": "size_t"}],
                "outputs": [{"type": "void*"}],
                "file_path": "/memory_manager/src/malloc.c"
            },
            {
                "name": "free_wrapper", 
                "code": "void free_wrapper(void* ptr) { free(ptr); }",
                "inputs": [{"name": "ptr", "type": "void*"}],
                "outputs": [{"type": "void"}],
                "file_path": "/memory_manager/src/malloc.c"
            },
            {
                "name": "create_buffer",
                "code": "int* create_buffer(int size) { return malloc(size * sizeof(int)); }",
                "inputs": [{"name": "size", "type": "int"}],
                "outputs": [{"type": "int*"}],
                "file_path": "/memory_manager/src/buffer.c"
            }
        ]
        
        # 存储所有C函数
        interface_ids = []
        for func in c_functions:
            # 模拟向量编码（实际中会使用embedding模型）
            vector = np.random.random(768).tolist()
            
            interface_id, qdrant_id = self.manager.store_interface_with_vector(
                name=func["name"],
                inputs=func["inputs"],
                outputs=func["outputs"],
                file_path=func["file_path"],
                code=func["code"],
                vector=vector,
                language="c",
                project_name="memory_manager"
            )
            interface_ids.append(interface_id)
        
        # 模拟AI转译过程
        for i, interface_id in enumerate(interface_ids):
            original_func = c_functions[i]
            
            # 模拟不同的转译结果
            if original_func["name"] == "malloc_wrapper":
                # 成功转译
                rust_code = "fn malloc_wrapper(size: usize) -> *mut u8 { unsafe { std::alloc::alloc(Layout::from_size_align_unchecked(size, 1)) } }"
                rust_vector = np.random.random(768).tolist()
                
                self.manager.add_translation_record(
                    interface_id=interface_id,
                    original_code=original_func["code"],
                    translated_code=rust_code,
                    translation_method="deepseek",
                    success=True,
                    translated_vector=rust_vector
                )
                
            elif original_func["name"] == "free_wrapper":
                # 转译失败，需要重试
                self.manager.add_translation_record(
                    interface_id=interface_id,
                    original_code=original_func["code"],
                    translated_code="",
                    translation_method="openai",
                    success=False,
                    error_message="unsafe code not allowed"
                )
                
                # 重试成功
                rust_code = "fn free_wrapper(ptr: *mut u8) { unsafe { std::alloc::dealloc(ptr, Layout::from_size_align_unchecked(1, 1)) } }"
                rust_vector = np.random.random(768).tolist()
                
                self.manager.add_translation_record(
                    interface_id=interface_id,
                    original_code=original_func["code"],
                    translated_code=rust_code,
                    translation_method="deepseek",
                    success=True,
                    translated_vector=rust_vector
                )
            
            else:  # create_buffer
                # 成功转译为安全的Rust代码
                rust_code = "fn create_buffer(size: i32) -> Box<[i32]> { vec![0; size as usize].into_boxed_slice() }"
                rust_vector = np.random.random(768).tolist()
                
                self.manager.add_translation_record(
                    interface_id=interface_id,
                    original_code=original_func["code"],
                    translated_code=rust_code,
                    translation_method="deepseek",
                    success=True,
                    translated_vector=rust_vector
                )
        
        # 验证转译结果
        all_interfaces = self.manager.search_interfaces_by_name("", "memory_manager")
        self.assertEqual(len(all_interfaces), 3)
        
        # 检查转译历史
        for interface_id in interface_ids:
            history = self.manager.sqlite_server.get_translation_history(interface_id)
            self.assertGreater(len(history), 0)
            
            # 检查是否有成功的转译
            success_count = sum(1 for h in history if h["success"])
            self.assertGreater(success_count, 0)
        
        # 模拟查找相似函数
        query_vector = np.random.random(768).tolist()
        similar = self.manager.search_similar_interfaces(
            query_vector=query_vector,
            limit=5,
            language="c",
            project="memory_manager"
        )
        self.assertGreater(len(similar), 0)
        
        # 获取系统状态
        status = self.manager.get_system_status()
        self.assertEqual(status["overall_status"], "healthy")


def run_tests():
    """运行所有测试"""
    # 创建测试套件
    suite = unittest.TestSuite()
    
    # 添加所有测试类
    test_classes = [
        TestSQLiteServer,
        TestQdrantServer, 
        TestDatabaseManager,
        TestRealWorldScenarios
    ]
    
    for test_class in test_classes:
        tests = unittest.TestLoader().loadTestsFromTestCase(test_class)
        suite.addTests(tests)
    
    # 运行测试
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    
    # 打印总结
    print(f"\n{'='*50}")
    print(f"测试总结:")
    print(f"运行测试: {result.testsRun}")
    print(f"失败: {len(result.failures)}")
    print(f"错误: {len(result.errors)}")
    print(f"跳过: {len(result.skipped)}")
    
    if result.failures:
        print(f"\n失败的测试:")
        for test, traceback in result.failures:
            print(f"- {test}: {traceback}")
    
    if result.errors:
        print(f"\n错误的测试:")
        for test, traceback in result.errors:
            print(f"- {test}: {traceback}")
    
    return result.wasSuccessful()


if __name__ == "__main__":
    success = run_tests()
    sys.exit(0 if success else 1)
