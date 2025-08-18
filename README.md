# C2Rust Agent

An intelligent LLM-powered tool for converting C projects to idiomatic Rust code.

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()

## Overview

C2Rust Agent is an advanced code translation system that leverages Large Language Models (LLMs) to convert C projects into safe, idiomatic Rust code. Unlike simple transpilers, this tool understands code semantics and produces human-readable, maintainable Rust code following best practices.

## Key Features

- **Intelligent Translation**: Uses LLM to understand C code semantics and generate idiomatic Rust
- **Project-Level Analysis**: Processes entire C projects, maintaining structure and relationships
- **Context-Aware**: Leverages database-stored code relationships for better translation quality
- **Iterative Refinement**: Automatically retries translation with error feedback until code compiles
- **Multiple Project Types**: Handles single files, paired header/source files, and multi-module projects
- **LSP Integration**: Provides Language Server Protocol support for IDE integration
- **Database Backend**: Stores code analysis and translation history

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   C Project     │───▶│  LSP Services   │───▶│   Database      │
│   Analysis      │    │  (Code Index)   │    │  (SQLite+Qdrant)│
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                                              │
         ▼                                              ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  Preprocessor   │───▶│ Main Processor  │◀───│ Prompt Builder  │
│  (Cache & Map)  │    │ (Translation)   │    │ (Context Gen)   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │
         ▼                       ▼
┌─────────────────┐    ┌─────────────────┐
│  File Scanner   │    │   LLM Service   │
│  (Discovery)    │    │  (Translation)  │
└─────────────────┘    └─────────────────┘
                                │
                                ▼
                     ┌─────────────────┐
                     │  Rust Checker   │
                     │ (Validation)    │
                     └─────────────────┘
```

### Core Components

- **LSP Services**: Analyzes C code structure and relationships
- **Database Services**: Stores code analysis in SQLite + Qdrant vector database
- **Preprocessor**: Generates cache and file mappings, splits into compilation units
- **Main Processor**: Orchestrates translation workflow with retry logic
- **Prompt Builder**: Generates context-aware prompts for LLM translation
- **LLM Requester**: Interfaces with language models for code translation
- **Rust Checker**: Validates generated Rust code compilation

## Installation

### Prerequisites

1. **Rust**: Install from [rustup.rs](https://rustup.rs/)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Qdrant Vector Database**: Required for semantic code search
   ```bash
   # Using Docker
   docker run -p 6333:6333 qdrant/qdrant
   
   # Or install locally
   # See: https://qdrant.tech/documentation/guides/installation/
   ```

3. **LLM API Access**: Configure access to OpenAI GPT, Claude, or other compatible models

### Build from Source

```bash
# Clone the repository
git clone https://github.com/rust4c/c2rust_agent.git
cd c2rust_agent

# Build all components
cargo build --release

# Run tests
cargo test
```

## Configuration

Create `config/config.toml`:

```toml
[database]
sqlite_path = "data/c2rust.db"
qdrant_url = "http://localhost:6333"

[llm]
provider = "openai"  # or "claude", "local"
api_key = "your-api-key-here"
model = "gpt-4"

[translation]
max_retries = 3
concurrent_limit = 4
cache_dir = "cache"

[logging]
level = "info"
file = "logs/c2rust.log"
```

## Usage

### Command Line Interface

```bash
# Analyze and translate a C project
c2rust-agent translate /path/to/c/project --output /path/to/rust/project

# With database context
c2rust-agent translate /path/to/c/project --with-db --output ./rust_output

# Dry run (analysis only)
c2rust-agent analyze /path/to/c/project --dry-run
```

### Programmatic API

```rust
use main_processor::{MainProcessor, ProjectInfo, ProjectType};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize processor
    let processor = MainProcessor::new("./cache").await?;
    
    // Create project info
    let project = ProjectInfo {
        name: "my_c_project".to_string(),
        path: "/path/to/c/project".into(),
        project_type: ProjectType::PairedFiles,
    };
    
    // Run translation
    let stats = processor.run_translation_workflow().await?;
    println!("Translation completed: {:?}", stats);
    
    Ok(())
}
```

## Translation Process

1. **Discovery**: Scans C project structure and identifies compilation units
2. **Analysis**: Uses LSP services to understand code relationships and dependencies
3. **Caching**: Preprocessor creates optimized cache and file mappings
4. **Context Building**: Generates rich context prompts using database knowledge
5. **Translation**: LLM converts C code to Rust with semantic understanding
6. **Validation**: Rust compiler checks generated code
7. **Refinement**: Automatic retry with error feedback if compilation fails

### Supported Project Types

- **Single File**: Simple C programs (main.c → main.rs)
- **Paired Files**: Header/source pairs (.h/.c → lib.rs + modules)
- **Multi-Module**: Complex projects with multiple independent modules

### Translation Features

- Memory safety through Rust ownership system
- Error handling with `Result<T, E>` types
- Proper use of `Option<T>` for nullable pointers
- Idiomatic Rust patterns (iterators, pattern matching)
- Automatic `unsafe` block annotation where needed
- `#[repr(C)]` for C-compatible structs

## Examples

### Input C Code
```c
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int x, y;
} Point;

Point* create_point(int x, int y) {
    Point* p = malloc(sizeof(Point));
    if (p == NULL) return NULL;
    p->x = x;
    p->y = y;
    return p;
}

void free_point(Point* p) {
    free(p);
}
```

### Generated Rust Code
```rust
#[repr(C)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }
}

pub fn create_point(x: i32, y: i32) -> Option<Box<Point>> {
    Some(Box::new(Point::new(x, y)))
}

// Note: free_point not needed - Rust handles memory automatically
```

## Development

### Project Structure

```
c2rust_agent/
├── crates/
│   ├── commandline_tool/     # CLI interface
│   ├── cproject_analy/       # C project analysis
│   ├── db_services/          # Database operations
│   ├── env_checker/          # Environment validation
│   ├── file_scanner/         # File discovery
│   ├── llm_requester/        # LLM API interface
│   ├── lsp_services/         # Language server integration
│   ├── main_processor/       # Core translation logic
│   ├── prompt_builder/       # Context-aware prompt generation
│   ├── rust_checker/         # Rust code validation
│   └── ui_main/             # GUI interface
├── config/                   # Configuration files
├── test-projects/           # Test cases
└── target/                  # Build artifacts
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p main_processor

# Run with logging
RUST_LOG=debug cargo test
```

### Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Commit changes: `git commit -m 'Add amazing feature'`
4. Push to branch: `git push origin feature/amazing-feature`
5. Open a Pull Request

## Limitations

- Currently supports C99 standard (C11/C18 features in development)
- Complex macros may require manual adjustment
- Inline assembly is not automatically translated
- Some platform-specific code may need manual review

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [c2rust](https://github.com/immunant/c2rust) - Inspiration for transpilation approach
- [tree-sitter](https://tree-sitter.github.io/) - Code parsing technology
- [Qdrant](https://qdrant.tech/) - Vector database for semantic search
- Rust community for excellent tooling and libraries

## Support

- 📖 [Documentation](https://github.com/rust4c/c2rust_agent/wiki)
- 🐛 [Issues](https://github.com/rust4c/c2rust_agent/issues)
- 💬 [Discussions](https://github.com/rust4c/c2rust_agent/discussions)
- 📧 Contact: rust4c@example.com