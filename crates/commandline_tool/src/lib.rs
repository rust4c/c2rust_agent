


use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "c2rust-agent")]
#[command(version = "1.0")]
#[command(about = "C 到 Rust 代码转换工具", long_about = None)]

pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 分析 C 项目
    Analyze {
        /// C 项目目录（必需）
        #[arg(long, short, value_name = "DIR", help = "输入路径",required = true)]
        input_dir: PathBuf,


    },

    /// C项目预处理
    Preprocess{
        //C 项目目录（必需）
        #[arg(long, short, value_name = "DIR", help = "输入路径",required = true)]
        input_dir: PathBuf,

        /// 预处理后输出路径
        #[arg(long, short, value_name = "DIR", help = "输入路径",required = true)]
        output_dir: PathBuf,

    },

    /// 将 C 项目转换为 Rust
    Translate {
        /// C 项目目录（必需）
        #[arg(long, value_name = "DIR", required = true)]
        input_dir: PathBuf,

        /// 输出 Rust 项目目录（可选）
        #[arg(long, value_name = "DIR")]
        output_dir: Option<PathBuf>,
    },

    /// 分析调用关系
    AnalyzeRelations {
        /// C 项目目录（必需）
        #[arg(long, value_name = "DIR", required = true)]
        input_dir: PathBuf,

        /// 项目名称（可选）
        #[arg(long)]
        project_name: Option<String>,

        /// 数据库文件路径
        #[arg(long, default_value = "relation_analysis.db")]
        db: String,
    },

    /// 查询调用关系数据库
    RelationQuery {
        /// 数据库文件路径
        #[arg(long, default_value = "relation_analysis.db")]
        db: String,

        /// 项目名称（用于具体查询）
        #[arg(long)]
        project: Option<String>,

        /// 查询类型
        #[arg(long, value_enum, default_value_t = QueryType::ListProjects)]
        query_type: QueryType,

        /// 目标函数名或文件路径
        #[arg(long)]
        target: Option<String>,

        /// 搜索关键词
        #[arg(long)]
        keyword: Option<String>,

        /// 结果数量限制
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
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