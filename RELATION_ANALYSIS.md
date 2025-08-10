# C项目调用关系分析功能

本功能提供了对C/C++项目进行全面调用关系分析的能力，可以分析函数定义、函数调用、文件依赖等关系，并将结果存储到关系数据库中。

## 功能特性

- 🔍 **函数定义分析**: 提取项目中所有函数的定义信息
- 📞 **函数调用分析**: 分析函数之间的调用关系
- 📁 **文件依赖分析**: 分析文件间的include和调用依赖
- 🗄️ **关系数据库**: 将分析结果存储到SQLite数据库中
- 🔎 **灵活查询**: 提供多种查询接口来检索分析结果
- 📊 **统计报告**: 生成项目的调用关系统计报告

## 安装要求

确保已安装以下依赖：

```bash
pip install sqlite3 fastembed pathlib
```

## 使用方法

### 1. 分析项目调用关系

```bash
# 分析项目并保存到数据库
python run.py analyze-relations --input-dir /path/to/c/project --project-name my_project

# 指定数据库文件
python run.py analyze-relations --input-dir /path/to/c/project --project-name my_project --db my_relations.db
```

### 2. 查询调用关系

#### 列出所有项目
```bash
python run.py relation-query --db relation_analysis.db --command list-projects
```

#### 生成项目报告
```bash
python run.py relation-query --db relation_analysis.db --command report --project my_project
```

#### 显示项目统计
```bash
python run.py relation-query --db relation_analysis.db --command stats --project my_project
```

#### 查找特定函数
```bash
python run.py relation-query --db relation_analysis.db --command find-func --project my_project --target main
```

#### 显示函数调用链
```bash
python run.py relation-query --db relation_analysis.db --command call-chain --project my_project --target main
```

#### 分析文件调用关系
```bash
python run.py relation-query --db relation_analysis.db --command file-analysis --project my_project --target main.c
```

#### 显示最常被调用的函数
```bash
python run.py relation-query --db relation_analysis.db --command top-called --project my_project --limit 10
```

#### 显示最复杂的函数
```bash
python run.py relation-query --db relation_analysis.db --command top-complex --project my_project --limit 10
```

#### 文件依赖分析
```bash
python run.py relation-query --db relation_analysis.db --command deps-analysis --project my_project
```

#### 搜索函数
```bash
python run.py relation-query --db relation_analysis.db --command search --project my_project --keyword printf
```

#### 函数使用分析
```bash
python run.py relation-query --db relation_analysis.db --command func-usage --project my_project --target main
```

## 数据库结构

系统创建以下数据表来存储调用关系：

### function_definitions 表
存储函数定义信息：
- `function_name`: 函数名
- `file_path`: 定义文件路径
- `line_number`: 定义行号
- `return_type`: 返回类型
- `parameters`: 参数列表（JSON格式）
- `signature`: 函数签名

### function_calls 表
存储函数调用关系：
- `caller_file`: 调用方文件
- `caller_function`: 调用方函数
- `caller_line`: 调用行号
- `called_function`: 被调用函数
- `called_file`: 被调用函数文件

### file_dependencies 表
存储文件依赖关系：
- `source_file`: 源文件
- `target_file`: 目标文件
- `dependency_type`: 依赖类型（include、call等）

## 示例输出

### 项目统计报告
```
=== 项目 'test_c_project' 调用关系分析报告 ===

📊 基本统计:
  函数定义数: 15
  函数调用数: 45
  文件依赖数: 8
  唯一文件数: 6

🔥 最常被调用的函数:
  1. printf - 12 次
  2. malloc - 8 次
  3. strlen - 6 次

🔧 最复杂的函数:
  1. main - 调用 8 个函数
  2. process_data - 调用 5 个函数
  3. init_system - 调用 4 个函数

📁 文件依赖分析:
  总依赖数: 8
  依赖最多的文件:
    main.c - 3 个依赖
    utils.c - 2 个依赖
```

### 函数调用链
```json
{
  "root_function": "main",
  "max_depth": 3,
  "call_tree": [
    {
      "function": "init_system",
      "file": "main.c",
      "line": 15,
      "depth": 1,
      "children": [
        {
          "function": "malloc",
          "file": "init.c",
          "line": 8,
          "depth": 2,
          "children": []
        }
      ]
    }
  ]
}
```

## 快速测试

使用提供的测试脚本快速验证功能：

```bash
python test_relations.py
```

此脚本会使用`test_project`目录进行完整的分析和查询测试。

## 注意事项

1. **项目准备**: 确保C项目包含有效的`compile_commands.json`文件或可以通过make生成
2. **文件编码**: 源文件应使用UTF-8编码
3. **数据库大小**: 大型项目可能生成较大的数据库文件
4. **性能**: 首次分析大项目可能需要较长时间

## 故障排除

### 常见问题

1. **编译数据库缺失**
   ```
   错误: 编译数据库不存在
   解决: 确保项目根目录有compile_commands.json文件
   ```

2. **数据库连接失败**
   ```
   错误: 连接数据库失败
   解决: 检查数据库文件路径和权限
   ```

3. **函数解析失败**
   ```
   错误: clang AST 分析失败
   解决: 检查源文件语法和编译选项
   ```

### 调试选项

启用详细输出来调试问题：

```bash
# 在代码中设置详细模式
detailed=True  # 在LSPServices中
```

## 扩展功能

可以通过修改以下文件来扩展功能：

- `CallRelationAnalyzer.py`: 添加新的分析逻辑
- `relation_query_tool.py`: 添加新的查询方法
- `run.py`: 添加新的命令行选项

## 相关文件

- `src/modules/Preprocessing/CallRelationAnalyzer.py`: 核心分析引擎
- `src/modules/Preprocessing/SaveIntoDB.py`: 数据保存接口
- `src/utils/relation_query_tool.py`: 查询工具
- `examples/call_relation_example.py`: 使用示例
- `test_relations.py`: 测试脚本
