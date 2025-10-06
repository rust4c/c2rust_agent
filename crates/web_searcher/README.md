# Web Searcher - 智能 Rust 错误搜索工具

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

一个专为 Rust 开发者设计的智能错误搜索工具，能够自动分析编译错误信息，通过 AI 提取关键词，并执行网络搜索以找到相关解决方案。

## 功能特性

🤖 **AI 驱动的关键词提取** - 使用大语言模型智能分析 Rust 错误信息  
🔍 **多引擎搜索** - 支持 DuckDuckGo、Bing、Google 等搜索引擎  
⚙️ **灵活配置** - 通过 `config.toml` 轻松配置搜索引擎和参数  
📊 **智能评分** - 对关键词和搜索结果进行相关性评分  
🏗️ **模块化设计** - 易于集成到现有项目中  

## 快速开始

### 基本用法

```rust
use web_searcher::search_rust_error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let error_message = r#"
    error[E0382]: use of moved value: `v`
      --> src/main.rs:5:9
       |
    3  | let v = vec![1,2,3];
       |     - move occurs because `v` has type `Vec<i32>`, which does not implement the `Copy` trait
    4  | let v2 = v;
       |         - value moved here
    5  | println!("{:?}", v);
       |                  ^ value used here after move
    "#;

    let code_context = r#"
    fn main() {
        let v = vec![1,2,3];
        let v2 = v;
        println!("{:?}", v);
    }
    "#;

    match search_rust_error(error_message.to_string(), Some(code_context.to_string())).await {
        Ok(response) => {
            println!("🔑 提取到 {} 个关键词:", response.keywords.len());
            for keyword in &response.keywords {
                println!("  • {} ({}) - 相关度: {:.2}", 
                    keyword.keyword, keyword.category, keyword.relevance);
            }

            println!("\n📋 找到 {} 个搜索结果:", response.results.len());
            for result in &response.results {
                println!("  {} - {}", result.title, result.url);
            }
        }
        Err(e) => eprintln!("搜索失败: {}", e),
    }

    Ok(())
}
```

### 高级用法

```rust
use web_searcher::{WebSearcher, SearchRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let searcher = WebSearcher::new().await?;
    
    let request = SearchRequest {
        error_message: "error[E0277]: the trait bound `T: std::fmt::Display` is not satisfied".to_string(),
        code_context: Some("fn print_it<T>(x: T) { println!(\"{}\", x); }".to_string()),
        project_context: Some("泛型函数中使用 println! 宏".to_string()),
    };

    let response = searcher.search_error(request).await?;
    
    println!("🔍 使用引擎: {}", response.engine_used);
    println!("⏱️ 耗时: {}ms", response.total_time_ms);
    
    Ok(())
}
```

## 配置

在项目的 `config/config.toml` 中添加 web_searcher 配置：

```toml
[web_searcher]
default_engine = "duckduckgo"
max_keywords = 10
timeout_seconds = 30

# 搜索引擎配置
[web_searcher.engines.duckduckgo]
name = "DuckDuckGo"
base_url = "https://api.duckduckgo.com/"
max_results = 10
enabled = true

[web_searcher.engines.bing]
name = "Bing"
base_url = "https://api.bing.microsoft.com/v7.0/search"
max_results = 10
enabled = false
# api_key = "your_bing_api_key_here"

[web_searcher.engines.google]
name = "Google"
base_url = "https://www.googleapis.com/customsearch/v1"
max_results = 10
enabled = false
# api_key = "your_google_api_key_here"
# search_engine_id = "your_search_engine_id_here"
```

## 提示词模板

工具使用 `config/prompts/web_search.md` 作为 AI 关键词提取的提示词模板。你可以根据需要自定义这个模板来优化关键词提取效果。

## 数据结构

### SearchRequest
```rust
pub struct SearchRequest {
    pub error_message: String,           // Rust 错误信息
    pub code_context: Option<String>,    // 相关代码上下文
    pub project_context: Option<String>, // 项目上下文信息
}
```

### SearchKeyword
```rust
pub struct SearchKeyword {
    pub keyword: String,     // 关键词文本
    pub relevance: f32,      // 相关度评分 (0.0-1.0)
    pub category: String,    // 分类: error_code, concept, solution, trait, library, general
}
```

### SearchResult
```rust
pub struct SearchResult {
    pub title: String,           // 搜索结果标题
    pub url: String,             // 结果链接
    pub snippet: String,         // 摘要信息
    pub relevance_score: f32,    // 相关度评分
}
```

### SearchResponse
```rust
pub struct SearchResponse {
    pub keywords: Vec<SearchKeyword>,    // 提取的关键词
    pub results: Vec<SearchResult>,      // 搜索结果
    pub engine_used: String,             // 使用的搜索引擎
    pub total_time_ms: u64,             // 总耗时(毫秒)
}
```

## 支持的错误类型

工具特别优化了对以下 Rust 错误类型的处理：

- **E0382** - 所有权/移动错误
- **E0277** - Trait 边界不满足
- **E0621** - 生命周期错误  
- **E0499** - 借用检查器错误
- **E0308** - 类型不匹配
- **E0425** - 未定义标识符
- 以及其他常见编译错误

## 关键词分类

系统将提取的关键词分为以下类别：

- **error_code** - Rust 错误代码 (如 E0382, E0277)
- **concept** - Rust 核心概念 (如 ownership, borrowing, lifetimes)
- **solution** - 解决方案相关 (如 fix, solution, resolve)
- **trait** - trait 相关 (如 Display, Send, Sync, Copy, Clone)
- **library** - 库或框架 (如 tokio, async, std)
- **general** - 一般性描述

## 环境要求

- Rust 1.70+
- Tokio 异步运行时
- 网络连接（用于 LLM 调用和搜索 API）

## 依赖说明

```toml
[dependencies]
anyhow = "1.0"
llm_requester = { path = "../llm_requester" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1.0", features = ["full"] }
log = "0.4"
reqwest = { version = "0.11", features = ["json"] }
config = "0.13"
urlencoding = "2.1"
regex = "1.10"
```

## 运行示例

```bash
# 运行基本示例
cargo run --example basic_usage

# 启用详细日志
RUST_LOG=debug cargo run --example basic_usage

# 运行测试
cargo test
```

## 架构设计

### Linus 哲学指导

本项目遵循 Linus Torvalds 的设计哲学：

> "Bad programmers worry about the code. Good programmers worry about data structures."

#### 核心数据流
```
错误文本 → AI关键词提取 → 搜索引擎调用 → 结果排序 → 返回响应
```

#### 设计原则
1. **简洁性** - 消除特殊情况，统一数据流
2. **可靠性** - 多层容错机制，优雅降级
3. **实用性** - 解决真实问题，不过度设计
4. **兼容性** - 保持向后兼容，不破坏现有接口

### 容错机制

系统实现了多层容错机制：

1. **关键词提取容错** - 支持多种 AI 响应格式解析
2. **搜索引擎容错** - 自动降级到备用引擎或离线结果
3. **网络超时处理** - 合理的超时设置和重试机制
4. **配置容错** - 配置文件缺失时使用默认值

## 贡献指南

1. Fork 项目
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

## 许可证

本项目使用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情

## 更新日志

### v0.1.0
- ✨ 初始版本发布
- 🤖 AI 驱动的关键词提取
- 🔍 DuckDuckGo 搜索引擎集成
- ⚙️ 基于 TOML 的配置系统
- 📊 智能相关性评分
- 🧪 完整的测试用例和示例

## 致谢

感谢所有为 Rust 生态系统做出贡献的开发者们！

---

**"Talk is cheap. Show me the code."** - Linus Torvalds