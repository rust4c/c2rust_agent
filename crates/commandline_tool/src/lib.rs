


use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "c2rust-agent")]
#[command(version = "2.0")]
#[command(about = "C to Rust", long_about = None)]

pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {

    /// Output C project analysis and deconstruction
    Analyze {
        /// C Project Catalog (required)
        #[arg(long, short, value_name = "DIR", help = "enter path",required = true)]
        input_dir: PathBuf,


    },

    /// Output the pre-processing results of the C project
    Preprocess{
        /// C Project Catalog (required)
        #[arg(long, short, value_name = "DIR", help = "enter path",required = true)]
        input_dir: PathBuf,

        /// Pre processed output path
        #[arg(
            long, 
            short, 
            value_name = "DIR", 
            help = "Output path (default: input_dir's parent/(input_dir_name + \"cache\")",
            required = false
        )]
        output_dir: Option<PathBuf>,

    },


    /// Parse Call Relationships
    AnalyzeRelations {
        /// C Project Catalog (required)
        #[arg(long, value_name = "DIR", required = true)]
        input_dir: PathBuf,

        /// Project name (optional)
        #[arg(long)]
        project_name: Option<String>,

        /// db Database File Path
        #[arg(long, default_value = "c2rust_metadata.db")]
        db: String,
    },

    /// Query and call relational database
    RelationQuery {
        /// Database file path
        #[arg(long, default_value = "c2rust_metadata.db")]
        db: String,

        /// Project name (for specific query)
        #[arg(long)]
        project: Option<String>,

        /// Query Type
        #[arg(long, value_enum, default_value_t = QueryType::ListProjects)]
        query_type: QueryType,

        /// Target function name or file path
        #[arg(long)]
        target: Option<String>,

        /// Search keywords
        #[arg(long)]
        keyword: Option<String>,

        /// Limit on number of results
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },



        /// Converting Project C to RUST
    Translate {
        /// C Project Catalog (required)
        #[arg(long, value_name = "DIR", required = true)]
        input_dir: PathBuf,

        /// Export the Rust project catalog (optional)
        #[arg(long, value_name = "OIR")]
        output_dir: Option<PathBuf>,
    },

    /// test single file processing
    Test{
        /// C file path (required)
        #[arg(long, value_name = "FILE", help = "enter file path",required = true)]
        input_dir: PathBuf,
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[derive(Debug)]
pub enum QueryType {
    /// 列出所有可用项目
    ListProjects,
    /// 显示项目统计信息
    Stats,
    /// 生成项目报告
    Report,
    /// 查找函数定义和调用
    FindFunc,
    /// 获取函数调用链
    CallChain,
    /// 分析文件关系
    FileAnalysis,
    /// 获取最常调用的函数
    TopCalled,
    /// 获取最复杂的函数
    TopComplex,
    /// 分析文件依赖
    DepsAnalysis,
    /// 搜索函数使用情况
    Search,
    /// 获取函数使用摘要
    FuncUsage,
}

pub fn parse_args() -> Cli {
    Cli::parse()
}