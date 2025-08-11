#!/usr/bin/env python3
"""
数据库组件简单测试脚本

直接测试数据库功能，避免复杂的类型检查
"""

import os
import sys
import tempfile
import shutil
import numpy as np
from pathlib import Path

# 添加项目根目录到Python路径
project_root = Path(__file__).parent.parent
sys.path.insert(0, str(project_root))

from src.modules.DatebaseServer.SQLiteServer import SQLiteServer
from src.modules.DatebaseServer.DatabaseManager import create_database_manager


def test_sqlite_basic():
    """测试SQLite基本功能"""
    print("🧪 测试 SQLite 基本功能...")
    
    # 创建临时数据库
    test_dir = tempfile.mkdtemp()
    db_path = os.path.join(test_dir, "test.db")
    
    try:
        sqlite_server = SQLiteServer(db_path)
        
        # 测试配置操作
        sqlite_server.set_config("test_key", "test_value")
        value = sqlite_server.get_config("test_key")
        assert value == "test_value", f"配置值不匹配: {value}"
        print("✅ 配置操作测试通过")
        
        # 测试项目操作
        project_id = sqlite_server.create_project("test_project", "/path/to/project")
        projects = sqlite_server.get_projects()
        assert len(projects) > 0, "项目创建失败"
        print("✅ 项目操作测试通过")
        
        # 测试接口操作
        interface_id = sqlite_server.insert_interface(
            name="test_function",
            inputs=[{"name": "param", "type": "int"}],
            outputs=[{"type": "void"}],
            file_path="/test.c",
            qdrant_id="test-uuid-123"
        )
        
        interface = sqlite_server.get_interface(interface_id)
        assert interface is not None, "接口获取失败"
        assert interface["name"] == "test_function", "接口名称不匹配"
        print("✅ 接口操作测试通过")
        
        # 测试转译历史
        history_id = sqlite_server.add_translation_history(
            interface_id=interface_id,
            original_code="void test_function(int param);",
            translated_code="fn test_function(param: i32)",
            translation_method="deepseek",
            success=True
        )
        
        history = sqlite_server.get_translation_history(interface_id)
        assert len(history) > 0, "转译历史获取失败"
        print("✅ 转译历史测试通过")
        
        sqlite_server.close()
        print("✅ SQLite 所有测试通过!")
        
    except Exception as e:
        print(f"❌ SQLite 测试失败: {e}")
        return False
    finally:
        shutil.rmtree(test_dir)
    
    return True


def test_qdrant_basic():
    """测试Qdrant基本功能"""
    print("\n🧪 测试 Qdrant 基本功能...")
    
    try:
        from src.modules.DatebaseServer.QdrantServer import QdrantServer
        from qdrant_client import QdrantClient
        
        # 检查Qdrant服务是否可用
        try:
            test_client = QdrantClient(url="http://localhost:6333")
            test_client.get_collections()
        except Exception:
            print("⚠️  Qdrant 服务不可用，跳过测试")
            return True
        
        qdrant_server = QdrantServer(collection_name="test_collection")
        
        # 测试向量插入
        test_vector = np.random.random(768).tolist()
        point_id = qdrant_server.insert_code_vector(
            code="void test_function() { }",
            vector=test_vector,
            language="c",
            function_name="test_function"
        )
        
        assert isinstance(point_id, str), "向量插入失败"
        print("✅ 向量插入测试通过")
        
        # 测试向量获取
        result = qdrant_server.get_code_by_id(point_id)
        assert result is not None, "向量获取失败"
        print("✅ 向量获取测试通过")
        
        # 测试相似性搜索
        similar = qdrant_server.search_similar_code(test_vector, limit=5)
        assert len(similar) > 0, "相似性搜索失败"
        print("✅ 相似性搜索测试通过")
        
        # 测试健康检查
        health = qdrant_server.health_check()
        assert health, "健康检查失败"
        print("✅ 健康检查测试通过")
        
        # 清理
        qdrant_server.clear_collection()
        qdrant_server.close()
        print("✅ Qdrant 所有测试通过!")
        
    except ImportError:
        print("⚠️  Qdrant 客户端未安装，跳过测试")
        return True
    except Exception as e:
        print(f"❌ Qdrant 测试失败: {e}")
        return False
    
    return True


def test_database_manager():
    """测试数据库管理器"""
    print("\n🧪 测试数据库管理器...")
    
    # 创建临时数据库
    test_dir = tempfile.mkdtemp()
    db_path = os.path.join(test_dir, "test_manager.db")
    
    try:
        # 检查Qdrant是否可用
        qdrant_available = False
        try:
            from qdrant_client import QdrantClient
            test_client = QdrantClient(url="http://localhost:6333")
            test_client.get_collections()
            qdrant_available = True
        except Exception:
            print("⚠️  Qdrant 服务不可用，仅测试SQLite部分")
        
        if qdrant_available:
            manager = create_database_manager(
                sqlite_path=db_path,
                qdrant_collection="test_manager_collection"
            )
        else:
            # 只测试SQLite部分
            from src.modules.DatebaseServer.SQLiteServer import SQLiteServer
            manager = type('MockManager', (), {
                'sqlite_server': SQLiteServer(db_path),
                'create_project': lambda name, path, desc=None: 
                    SQLiteServer(db_path).create_project(name, path, desc),
                'get_config': lambda key, default=None:
                    SQLiteServer(db_path).get_config(key, default),
                'set_config': lambda key, value, desc=None:
                    SQLiteServer(db_path).set_config(key, value, desc)
            })()
        
        # 测试项目创建
        if hasattr(manager, 'create_project'):
            project_id = manager.create_project(
                name="test_manager_project",
                path="/path/to/manager/project"
            )
            print("✅ 项目创建测试通过")
        
        # 测试配置操作
        if hasattr(manager, 'set_config') and hasattr(manager, 'get_config'):
            manager.set_config("test_manager_config", {"value": 123})
            config = manager.get_config("test_manager_config")
            assert config == {"value": 123}, "配置操作失败"
            print("✅ 配置操作测试通过")
        
        # 如果Qdrant可用，测试完整功能
        if qdrant_available and hasattr(manager, 'store_interface_with_vector'):
            test_vector = np.random.random(768).tolist()
            interface_id, qdrant_id = manager.store_interface_with_vector(
                name="manager_test_function",
                inputs=[{"name": "param", "type": "int"}],
                outputs=[{"type": "void"}],
                file_path="/manager_test.c",
                code="void manager_test_function(int param) { }",
                vector=test_vector,
                language="c"
            )
            print("✅ 接口向量存储测试通过")
            
            # 测试系统状态
            status = manager.get_system_status()
            assert "overall_status" in status, "系统状态获取失败"
            print("✅ 系统状态测试通过")
        
        # 清理
        if hasattr(manager, 'close'):
            manager.close()
        
        print("✅ 数据库管理器所有测试通过!")
        
    except Exception as e:
        print(f"❌ 数据库管理器测试失败: {e}")
        return False
    finally:
        shutil.rmtree(test_dir)
    
    return True


def test_real_world_scenario():
    """测试真实世界场景"""
    print("\n🧪 测试真实世界场景...")
    
    test_dir = tempfile.mkdtemp()
    db_path = os.path.join(test_dir, "real_world.db")
    
    try:
        # 检查Qdrant是否可用
        qdrant_available = False
        try:
            from qdrant_client import QdrantClient
            test_client = QdrantClient(url="http://localhost:6333")
            test_client.get_collections()
            qdrant_available = True
        except Exception:
            pass
        
        if not qdrant_available:
            print("⚠️  Qdrant 不可用，跳过真实场景测试")
            return True
        
        manager = create_database_manager(
            sqlite_path=db_path,
            qdrant_collection="real_world_test"
        )
        
        # 模拟C项目分析
        print("📝 模拟 C 项目分析...")
        project_id = manager.create_project(
            name="memory_lib",
            path="/projects/memory_lib",
            description="内存管理库"
        )
        
        # 模拟多个C函数
        c_functions = [
            {
                "name": "malloc_safe",
                "code": "void* malloc_safe(size_t size) { void* ptr = malloc(size); if (!ptr) exit(1); return ptr; }",
                "inputs": [{"name": "size", "type": "size_t"}],
                "outputs": [{"type": "void*"}]
            },
            {
                "name": "free_safe",
                "code": "void free_safe(void** ptr) { if (ptr && *ptr) { free(*ptr); *ptr = NULL; } }",
                "inputs": [{"name": "ptr", "type": "void**"}],
                "outputs": [{"type": "void"}]
            }
        ]
        
        interface_ids = []
        for func in c_functions:
            # 模拟代码向量化（实际中会使用embedding模型）
            vector = np.random.random(768).tolist()
            
            interface_id, qdrant_id = manager.store_interface_with_vector(
                name=func["name"],
                inputs=func["inputs"],
                outputs=func["outputs"],
                file_path=f"/memory_lib/{func['name']}.c",
                code=func["code"],
                vector=vector,
                language="c",
                project_name="memory_lib"
            )
            interface_ids.append(interface_id)
        
        print("✅ C 函数存储完成")
        
        # 模拟AI转译过程
        print("🤖 模拟 AI 转译过程...")
        for i, interface_id in enumerate(interface_ids):
            func = c_functions[i]
            
            if func["name"] == "malloc_safe":
                # 成功转译
                rust_code = "fn malloc_safe(size: usize) -> *mut u8 { let layout = Layout::from_size_align(size, 1).unwrap(); unsafe { alloc(layout) } }"
                rust_vector = np.random.random(768).tolist()
                
                manager.add_translation_record(
                    interface_id=interface_id,
                    original_code=func["code"],
                    translated_code=rust_code,
                    translation_method="deepseek",
                    success=True,
                    translated_vector=rust_vector
                )
            else:
                # 先失败，后成功
                manager.add_translation_record(
                    interface_id=interface_id,
                    original_code=func["code"],
                    translated_code="",
                    translation_method="openai", 
                    success=False,
                    error_message="unsafe code not allowed"
                )
                
                # 重试成功
                rust_code = "fn free_safe(ptr: &mut Option<Box<u8>>) { *ptr = None; }"
                rust_vector = np.random.random(768).tolist()
                
                manager.add_translation_record(
                    interface_id=interface_id,
                    original_code=func["code"],
                    translated_code=rust_code,
                    translation_method="deepseek",
                    success=True,
                    translated_vector=rust_vector
                )
        
        print("✅ AI 转译模拟完成")
        
        # 验证结果
        print("🔍 验证转译结果...")
        
        # 检查所有接口
        all_interfaces = manager.search_interfaces_by_name("", "memory_lib")
        assert len(all_interfaces) == 2, f"接口数量不对: {len(all_interfaces)}"
        
        # 检查转译历史
        for interface_id in interface_ids:
            history = manager.sqlite_server.get_translation_history(interface_id)
            assert len(history) > 0, "转译历史为空"
            
            # 检查是否有成功的转译
            success_count = sum(1 for h in history if h["success"])
            assert success_count > 0, "没有成功的转译记录"
        
        # 测试相似性搜索
        query_vector = np.random.random(768).tolist()
        similar = manager.search_similar_interfaces(
            query_vector=query_vector,
            limit=5,
            language="c",
            project="memory_lib"
        )
        print(f"🔍 找到 {len(similar)} 个相似接口")
        
        # 测试文本搜索
        malloc_results = manager.search_code_by_text("malloc", "c", "memory_lib")
        print(f"🔍 找到 {len(malloc_results)} 个包含'malloc'的代码")
        
        # 获取系统状态
        status = manager.get_system_status()
        print(f"💻 系统状态: {status['overall_status']}")
        
        manager.close()
        print("✅ 真实世界场景测试完成!")
        
    except Exception as e:
        print(f"❌ 真实世界场景测试失败: {e}")
        import traceback
        traceback.print_exc()
        return False
    finally:
        shutil.rmtree(test_dir)
    
    return True


def main():
    """主测试函数"""
    print("🚀 开始数据库组件测试\n")
    
    tests = [
        ("SQLite 基本功能", test_sqlite_basic),
        ("Qdrant 基本功能", test_qdrant_basic), 
        ("数据库管理器", test_database_manager),
        ("真实世界场景", test_real_world_scenario)
    ]
    
    passed = 0
    total = len(tests)
    
    for test_name, test_func in tests:
        print(f"\n{'='*50}")
        print(f"测试: {test_name}")
        print('='*50)
        
        try:
            if test_func():
                passed += 1
                print(f"✅ {test_name} - 通过")
            else:
                print(f"❌ {test_name} - 失败")
        except Exception as e:
            print(f"❌ {test_name} - 异常: {e}")
    
    print(f"\n{'='*50}")
    print(f"测试总结:")
    print(f"通过: {passed}/{total}")
    print(f"失败: {total - passed}/{total}")
    
    if passed == total:
        print("🎉 所有测试通过!")
        return True
    else:
        print("😞 部分测试失败!")
        return False


if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)
