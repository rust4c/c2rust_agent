# 两阶段 C2Rust 翻译功能

## 功能介绍

本项目现在支持两阶段翻译流程，将 C 代码转换为高质量的 Rust 代码：

1. **第一阶段：C2Rust 自动翻译**
   - 使用 C2Rust 工具将 C 代码自动转换为功能等价的 Rust 代码
   - 生成基础的、可编译的 Rust 代码（通常包含 unsafe 代码块）

2. **第二阶段：AI 优化翻译**
   - 使用 LLM 对 C2Rust 生成的代码进行优化
   - 移除不必要的 unsafe 代码
   - 改进内存管理和错误处理
   - 使代码更符合 Rust 最佳实践

## 安装要求

### 1. 安装 C2Rust 工具

```bash
# 使用 cargo 安装 C2Rust
cargo install c2rust

# 验证安装
c2rust --version
```

### 2. 确保系统依赖

C2Rust 需要以下系统依赖：

**macOS:**
```bash
# 安装 LLVM (如果使用 Homebrew)
brew install llvm

# 或者使用 Xcode Command Line Tools
xcode-select --install
```

**Ubuntu/Debian:**
```bash
sudo apt-get update
sudo apt-get install clang libclang-dev llvm-dev
```

**其他 Linux 发行版:**
请参考 [C2Rust 官方文档](https://c2rust.com/)

### 3. 配置 LLM API

确保已配置好 LLM API（如 OpenAI、Claude 等），项目将在第二阶段使用。

## 使用方法

### 运行测试示例

```bash
# 编译项目
cargo build

# 运行两阶段翻译测试
cargo run --example test_two_stage_translation
```

### 在代码中使用

```rust
use main_processor::process_single_path;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 处理单个目录（包含 .c/.h 文件）
    let path = Path::new("path/to/your/c/project");
    process_single_path(path).await?;
    Ok(())
}
```

### 批量处理

```rust
use main_processor::{discover_src_cache_projects, process_batch_paths, MainProcessorConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 发现所有可处理的项目
    let projects = discover_src_cache_projects(Path::new("src_cache")).await?;
    
    // 配置批量处理
    let config = MainProcessorConfig {
        concurrent_limit: 4,  // 并发处理数量
        max_retry_attempts: 3, // 最大重试次数
    };
    
    // 批量处理
    process_batch_paths(config, projects).await?;
    Ok(())
}
```

## 输出结构

两阶段翻译完成后，会在输入目录下创建以下结构：

```
input-directory/
├── two-stage-translation/
│   ├── c2rust-output/          # C2Rust 原始输出
│   │   ├── src/
│   │   │   └── main.rs         # C2Rust 生成的代码
│   │   ├── Cargo.toml
│   │   └── compile_commands.json
│   └── final-output/           # 最终优化结果
│       ├── src/
│       │   └── main.rs         # AI 优化后的代码
│       ├── Cargo.toml
│       └── c2rust_original.rs  # C2Rust 原始输出备份
```

## 故障排除

### C2Rust 安装问题

如果 C2Rust 安装失败，请：

1. 确保 Rust 工具链是最新版本：`rustup update`
2. 检查系统是否有必要的编译工具
3. 参考 [C2Rust 官方安装指南](https://c2rust.com/manual/installation.html)

### C2Rust 转换失败

如果第一阶段 C2Rust 转换失败，系统会自动回退到纯 AI 翻译模式，确保任务仍能完成。

### LLM API 问题

如果第二阶段 AI 优化失败，请检查：

1. LLM API 配置是否正确
2. 网络连接是否正常
3. API 额度是否充足
4. 请求超时设置（默认 100 分钟）

## 优势

1. **更高的翻译质量**：结合自动化工具和 AI 智能，生成更好的 Rust 代码
2. **渐进式优化**：即使 C2Rust 失败，仍能使用纯 AI 翻译
3. **可比较性**：保留 C2Rust 原始输出，便于对比和调试
4. **批量处理**：支持大规模项目的并发处理

## 注意事项

- C2Rust 要求输入的 C 代码能够通过编译
- 复杂的 C 代码可能需要手动调整才能被 C2Rust 正确处理
- AI 优化阶段的质量取决于 LLM 的能力和上下文理解
- 建议对生成的代码进行测试验证