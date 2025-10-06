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

### 输出格式要求

请以 JSON 格式返回转换结果：

```json
{
  "original": "原始C代码",
  "rust_code": "转换后的Rust代码",
  "cargo": "需要添加的crates,使用逗号分隔开,之间不需要添加空格",
  "key_changes": ["关键变更点列表"],
  "warnings": ["潜在问题警告"]
}
```

### 注意事项

1. 遇到未定义行为时，使用 `// FIXME:` 注释标记
2. 所有不安全操作必须显式标注 `unsafe` 块
3. 保持代码简洁，避免过度工程化
4. 使用 `libc` crate 处理系统调用

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
