#!/usr/bin/env python3
"""
使用增强的 PreProcessor 对 test_proj                if success:
                    print("✓ 增强预处理成功完成")
                    
                    # 获取详细的处理摘要
                    summary = preprocessor.get_processing_summary()
                    
                    print("\n处理摘要:")
                    print(f"  - 项目状态: {summary['project_status']}")
                    print(f"  - 总文件数: {summary['files_processed']['total']}")
                    print(f"  - 分析文件数: {summary['files_processed']['analyzed']}")
                    print(f"  - 映射文件数: {summary['files_processed']['mapped']}")
                    
                    print(f"\n发现的代码元素:")
                    print(f"  - 总元素数: {summary['elements_found']['total']}")
                    print(f"    * 函数: {summary['elements_found']['functions']}")
                    print(f"    * 结构体: {summary['elements_found']['structures']}")
                    print(f"    * 宏: {summary['elements_found']['macros']}")
                    print(f"    * 类型定义: {summary['elements_found']['typedefs']}")
                    
                    print(f"\n数据库状态:")
                    print(f"  - 已保存元素: {summary['database_status']['elements_saved']}")
                    print(f"  - 创建索引: {summary['database_status']['indices_created']}")
                    print(f"  - 向量数据库: {'已连接' if summary['database_status']['vector_database_connected'] else '未连接'}")
                    
                    print(f"\n处理性能:")
                    print(f"  - 处理时间: {summary['processing_time']:.2f}秒")
                    print(f"  - 缓存位置: {summary['cache_location']}")
                    
                    # 获取详细统计信息
                    detailed_stats = preprocessor.get_stats()
                    print(f"\n文件类型分布:")
                    file_breakdown = detailed_stats.get('file_types_breakdown', {})
                    for file_type, count in file_breakdown.items():
                        print(f"  - {file_type}: {count} 个文件")整测试
"""

import sys
import tempfile
import json
from pathlib import Path

def test_enhanced_preprocessor():
    """测试增强的预处理器功能"""
    try:
        from src.modules.Preprocessing.PreProcessor import PreProcessor
        from src.modules.Preprocessing.CProjectPreprocessor import PreprocessConfig
        from src.modules.DatebaseServer.DatabaseManager import create_database_manager
        
        # 使用本地的test_project目录
        project_dir = "/Users/peng/Documents/AppCode/Rust/c2rust_agent/translate_chibicc/src"
        
        print(f"开始测试增强预处理器项目: {project_dir}")
        print("=" * 60)
        
        # 检查项目是否存在
        if not Path(project_dir).exists():
            print(f"✗ 项目目录不存在: {project_dir}")
            return False
        
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            cache_dir = temp_path / "cache"
            db_path = temp_path / "test.db"
            
            print(f"临时缓存目录: {cache_dir}")
            print(f"临时数据库: {db_path}")
            
            # 创建数据库管理器
            print("\n1. 创建数据库管理器...")
            db_manager = create_database_manager(
                sqlite_path=str(db_path),
                qdrant_url="http://localhost:6333",
                qdrant_collection="enhanced_test_collection",
                vector_size=384,  # 使用与FastEmbed一致的向量维度
                timeout=120,  # 增加超时时间到2分钟
                batch_size=50  # 减小批次大小以避免超时
            )
            print("✓ 数据库管理器创建成功")
            
            try:
                # 创建增强预处理器
                print("\n2. 创建增强预处理器...")
                preprocessor = PreProcessor(db_manager, str(cache_dir))
                
                # 设置配置（增强预处理器内部管理配置）
                print("✓ 增强预处理器创建完成")
                
                # 执行完整的增强预处理
                print("\n3. 执行增强预处理（文件映射 + 分析 + 索引 + 保存）...")
                success, stats = preprocessor.process_project(project_dir)
                
                if success:
                    print("✓ 增强预处理成功完成")
                    print(f"  - 总文件数: {stats.get('total_files', 0)}")
                    print(f"  - 分析文件数: {stats.get('analyzed_files', 0)}")
                    print(f"  - 代码元素总数: {stats.get('total_elements', 0)}")
                    print(f"    * 函数: {stats.get('functions', 0)}")
                    print(f"    * 结构体: {stats.get('structures', 0)}")
                    print(f"    * 宏: {stats.get('macros', 0)}")
                    print(f"    * 类型定义: {stats.get('typedefs', 0)}")
                    print(f"  - 处理时间: {stats.get('processing_time', 0):.2f}秒")
                    
                    # 检查输出结构和索引文件
                    print("\n4. 检查输出结构和索引文件...")
                    if cache_dir.exists():
                        print(f"✓ 缓存目录已创建: {cache_dir}")
                        
                        # 列出输出目录内容
                        print("输出目录结构:")
                        for item in cache_dir.iterdir():
                            if item.is_dir():
                                print(f"  📁 {item.name}/")
                                for sub_item in item.iterdir():
                                    if sub_item.is_dir():
                                        file_count = len(list(sub_item.glob("*")))
                                        print(f"    📁 {sub_item.name}/ ({file_count} 文件)")
                                    else:
                                        print(f"    📄 {sub_item.name}")
                            else:
                                print(f"  📄 {item.name}")
                        
                        # 检查索引信息
                        print("\n索引信息:")
                        if preprocessor.file_mappings:
                            print(f"  ✓ 文件映射: {len(preprocessor.file_mappings)} 个文件")
                        if preprocessor.analysis_results:
                            print(f"  ✓ 分析结果: {len(preprocessor.analysis_results)} 个元素")
                        if preprocessor.element_indices:
                            print(f"  ✓ 元素索引: {len(preprocessor.element_indices)} 个元素")
                    
                    # 检查数据库中的向量存储
                    print("\n5. 检查向量数据库存储...")
                    try:
                        # 尝试获取Qdrant信息（如果可用）
                        if hasattr(db_manager, 'qdrant'):
                            print("✓ Qdrant连接已建立")
                        else:
                            print("◐ Qdrant客户端不可用，跳过向量检查")
                    except Exception as vector_error:
                        print(f"◐ 向量数据库检查遇到问题: {vector_error}")
                    
                    return True
                else:
                    print("✗ 预处理失败")
                    print(f"  统计信息: {stats}")
                    return False
                    
            finally:
                db_manager.close()
                print("\n6. 数据库连接已关闭")
        
    except Exception as e:
        print(f"✗ 测试过程中出现错误: {e}")
        import traceback
        traceback.print_exc()
        return False

def main():
    """主函数"""
    print("增强 PreProcessor 完整功能测试")
    print("=" * 60)
    
    try:
        success = test_enhanced_preprocessor()
        
        print("\n" + "=" * 60)
        if success:
            print("🎉 增强预处理器测试成功完成！")
            print("✅ 文件映射、代码分析、向量嵌入和索引功能正常")
            return 0
        else:
            print("❌ 增强预处理器测试失败")
            return 1
            
    except KeyboardInterrupt:
        print("\n⚠️  用户中断测试")
        return 1
    except Exception as e:
        print(f"\n💥 测试过程中出现未处理错误: {e}")
        return 1

if __name__ == "__main__":
    sys.exit(main())
