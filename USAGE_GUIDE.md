# C/C++ 项目调用关系分析系统 - 使用指南

## 概述

这个系统能够分析C/C++项目中的函数调用关系和文件依赖关系，并将结果存储在关系数据库中供查询分析。

## 快速开始

### 1. 分析项目

```bash
uv run python run.py analyze-relations --input-dir <项目目录> --project-name <项目名称>
```

示例：
```bash
uv run python run.py analyze-relations --input-dir test_project --project-name my_c_project
```

### 2. 查询分析结果

#### 列出所有已分析的项目
```bash
uv run python run.py relation-query --query-type list-projects
```

#### 生成项目完整报告
```bash
uv run python run.py relation-query --query-type report --project <项目名称>
```

#### 查看项目统计信息
```bash
uv run python run.py relation-query --query-type stats --project <项目名称>
```

#### 查找特定函数
```bash
uv run python run.py relation-query --query-type find-func --project <项目名称> --target <函数名>
```

#### 查看函数使用情况
```bash
uv run python run.py relation-query --query-type func-usage --project <项目名称> --target <函数名>
```

#### 分析函数调用链
```bash
uv run python run.py relation-query --query-type call-chain --project <项目名称> --target <函数名>
```

#### 查看最常被调用的函数
```bash
uv run python run.py relation-query --query-type top-called --project <项目名称> --limit 10
```

#### 查看最复杂的函数（调用最多其他函数）
```bash
uv run python run.py relation-query --query-type top-complex --project <项目名称> --limit 10
```

#### 文件依赖分析
```bash
uv run python run.py relation-query --query-type deps-analysis --project <项目名称>
```

#### 搜索函数
```bash
uv run python run.py relation-query --query-type search --project <项目名称> --keyword <关键词>
```

## 数据库结构

系统创建了4个关系表来存储分析结果：

- **function_definitions**: 函数定义信息
- **function_calls**: 函数调用关系  
- **file_dependencies**: 文件依赖关系
- **call_relationships**: 综合调用关系

## 实际测试示例

测试项目 `test_project` 的分析结果：

```bash
# 分析项目
uv run python run.py analyze-relations --input-dir test_project --project-name test_c_project

# 生成报告
uv run python run.py relation-query --query-type report --project test_c_project
```

输出示例：
```
=== 项目 'test_c_project' 调用关系分析报告 ===

📊 基本统计:
  函数定义数: 3
  函数调用数: 5
  文件依赖数: 4
  唯一文件数: 3

🔥 最常被调用的函数:
  1. add - 2 次
  2. helper - 2 次
  3. main - 1 次

🔧 最复杂的函数:
  1. main - 调用 1 个函数
  2. helper - 调用 1 个函数
  3. add - 调用 1 个函数
```

## 命令行参数说明

### analyze-relations 参数
- `--input-dir`: 要分析的项目目录
- `--project-name`: 项目名称（用于数据库存储）
- `--db`: 数据库文件路径（可选，默认: relation_analysis.db）

### relation-query 参数
- `--query-type`: 查询类型（必需）
- `--project`: 项目名称（大部分查询需要）
- `--target`: 目标函数名（某些查询需要）
- `--keyword`: 搜索关键词（搜索查询需要）
- `--limit`: 结果数量限制（可选，默认: 10）
- `--db`: 数据库文件路径（可选，默认: relation_analysis.db）

## 运行测试

运行完整测试套件：
```bash
uv run python test_relations.py
```

## 功能特点

1. **自动函数识别**: 识别C/C++源文件中的函数定义和调用
2. **文件依赖分析**: 分析头文件包含关系
3. **关系数据库存储**: 使用SQLite存储结构化数据
4. **多样化查询**: 支持多种查询和统计分析
5. **JSON格式输出**: 复杂查询结果以JSON格式输出
6. **中文界面**: 友好的中文用户界面

这个系统特别适用于大型C/C++项目的代码分析、重构规划和依赖关系梳理。
