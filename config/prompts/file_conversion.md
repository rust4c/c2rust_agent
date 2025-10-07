# C 到 Rust 转换指导规则

## 基本转换原则

### 类型映射

- C 的 `int` → Rust 的 `i32`
- C 的 `unsigned int` → Rust 的 `u32`
- C 的 `char` → Rust 的 `i8` (用于数值) 或 `u8` (用于字节)
- C 的 `void*` → Rust 的 `*mut c_void` 或 `*const c_void`
- C 的 `NULL` → Rust 的 `std::ptr::null()` 或 `std::ptr::null_mut()`

### 内存安全转换

1. **指针处理**：
   - C 的裸指针需要包裹在 `unsafe` 块中
   - 优先使用 `&` 和 `&mut` 引用替代裸指针
   - 使用 `Option<&T>` 处理可能为 NULL 的指针

2. **内存管理**：
   - C 的 `malloc/free` → Rust 的 `Box::new()` 和自动 drop
   - C 的手动内存管理 → Rust 的所有权系统

3. **错误处理**：
   - C 的返回错误码 → Rust 的 `Result<T, E>`
   - C 的 errno → Rust 的错误类型

## 可用工具集

### 代码分析工具
1. **文件检索工具** - 通过文件名查找文件
2. **代码行定位工具** - 定位代码行所在的函数
3. **行修改工具** - 修改指定行范围的代码

### 代码修改工具
4. **rs源文件修改工具** - 修改当前项目的.rs文件
5. **cargo.toml文件修改工具** - 修改项目依赖配置

### 输出格式要求

请以 JSON 格式返回转换结果：

```json
{
  "original": "原始C代码",
  "rust_code": "转换后的Rust代码",
  "cargo": "需要添加的crates,使用逗号分隔开,之间不需要添加空格",
  "key_changes": ["关键变更点列表"],
  "warnings": ["潜在问题警告"],
  "tool_usage": {
    "file_search": ["需要查找的文件列表"],
    "line_locate": ["需要定位的代码行"],
    "line_modify": {"start_line": 起始行, "end_line": 结束行, "new_code": "替换代码"},
    "rs_modify": {"file": "文件名.rs", "changes": "修改内容"},
    "cargo_modify": {"dependencies": "要添加的依赖"}
  }
}
```

### 转换工作流程

1. **分析阶段**：
   - 使用文件检索工具查找相关C头文件和源文件
   - 使用代码行定位工具理解函数上下文

2. **转换阶段**：
   - 应用类型映射规则
   - 处理内存安全和错误处理
   - 生成Rust等效代码

3. **集成阶段**：
   - 使用rs文件修改工具写入转换后的代码
   - 使用cargo.toml工具添加必要依赖
   - 使用行修改工具进行精确调整

### 注意事项

1. 遇到未定义行为时，使用 `// FIXME:` 注释标记
2. 所有不安全操作必须显式标注 `unsafe` 块
3. 保持代码简洁，避免过度工程化
4. 使用 `libc` crate 处理系统调用
5. 优先使用工具进行精确修改，避免手动错误

### 示例

**C 代码**：
```c
int add(int a, int b) {
    return a + b;
}
```

**Rust 代码**：
```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

**工具使用示例**：
```json
{
  "tool_usage": {
    "file_search": ["math_utils.h"],
    "line_modify": {
      "start_line": 10,
      "end_line": 15,
      "new_code": "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
    },
    "cargo_modify": {
      "dependencies": "libc"
    }
  }
}
```
