use commandline_tool::{Commands, QueryType};
use dioxus::prelude::*;
use llm_requester::llm_request;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::runtime::Runtime;

mod start_tab;
use start_tab::StartTab;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const START_CSS: Asset = asset!("/assets/start.css");
const HEADER_SVG: Asset = asset!("/assets/header.svg");

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tab {
    CliTool,
    LlmRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum CliCommand {
    Analyze,
    Preprocess,
    Translate,
    AnalyzeRelations,
    RelationQuery,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut show_start = use_signal(|| true);  // 默认显示开始界面

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: START_CSS }
        div { id: "app",
            img { src: HEADER_SVG, id: "header" }
            h1 { "C2Rust Agent UI" }
            p { "版本: 0.0.3" }

            // 根据状态显示不同界面
            if *show_start.read() {
                StartTab { on_start: move |_| show_start.set(false) }
            } else {
                Tabs {}
            }
        }
    }
}

#[component]
fn Tabs() -> Element {
    let mut active_tab = use_signal(|| Tab::CliTool);

    rsx! {
        div { class: "tab-container",
            div {
                class: if *active_tab.read() == Tab::CliTool { "tab active" } else { "tab" },
                onclick: move |_| active_tab.set(Tab::CliTool),
                "命令行工具"
            }
            div {
                class: if *active_tab.read() == Tab::LlmRequest { "tab active" } else { "tab" },
                onclick: move |_| active_tab.set(Tab::LlmRequest),
                "LLM请求"
            }
        }

        div {
            class: if *active_tab.read() == Tab::CliTool { "tab-content active" } else { "tab-content" },
            CliToolTab {}
        }

        div {
            class: if *active_tab.read() == Tab::LlmRequest { "tab-content active" } else { "tab-content" },
            LlmRequestTab {}
        }
    }
}

#[component]
fn CliToolTab() -> Element {
    let mut selected_command = use_signal(|| CliCommand::Analyze);
    let mut input_dir = use_signal(|| String::new());
    let mut output_dir = use_signal(|| String::new());
    let mut project_name = use_signal(|| String::new());
    let mut db_path = use_signal(|| String::from("relation_analysis.db"));
    let mut query_type = use_signal(|| QueryType::ListProjects);
    let mut target = use_signal(|| String::new());
    let mut keyword = use_signal(|| String::new());
    let mut limit = use_signal(|| 10);
    let mut result = use_signal(|| String::new());
    let mut is_loading = use_signal(|| false);

    let execute_command = move |_| {
        is_loading.set(true);
        result.set("正在执行...".to_string());

        let cmd = selected_command.clone();
        let input = input_dir.clone();
        let output = output_dir.clone();
        let project = project_name.clone();
        let db = db_path.clone();
        let q_type = query_type.clone();
        let tgt = target.clone();
        let kw = keyword.clone();
        let lim = limit.clone();

        // 克隆需要异步修改的信号 - 这里添加了mut关键字
        let mut result_clone = result.clone();
        let mut loading_clone = is_loading.clone();

        spawn(async move {
            let command_result = match *cmd.read() {
                CliCommand::Analyze => {
                    if input.read().is_empty() {
                        "错误：请输入输入目录".to_string()
                    } else {
                        format!("执行分析命令:\n输入目录: {}", input.read())
                    }
                }
                // ...其他命令处理逻辑...
                _ => "命令执行完成".to_string(),
            };

            result_clone.set(command_result);
            loading_clone.set(false);
        });
    };

    rsx! {
        div {
            h2 { "命令行工具界面" }

            div { class: "form-group",
                label { "选择命令:" }
                select {
                    value: "{selected_command.read():?}",
                    onchange: move |e| {
                        if let Ok(cmd) = e.value().parse::<String>() {
                            match cmd.as_str() {
                                "Analyze" => selected_command.set(CliCommand::Analyze),
                                "Preprocess" => selected_command.set(CliCommand::Preprocess),
                                "Translate" => selected_command.set(CliCommand::Translate),
                                "AnalyzeRelations" => selected_command.set(CliCommand::AnalyzeRelations),
                                "RelationQuery" => selected_command.set(CliCommand::RelationQuery),
                                _ => {}
                            }
                        }
                    },
                    option { value: "Analyze", "分析C项目" }
                    option { value: "Preprocess", "预处理C项目" }
                    option { value: "Translate", "转换C到Rust" }
                    option { value: "AnalyzeRelations", "分析调用关系" }
                    option { value: "RelationQuery", "查询关系数据库" }
                }
            }

            div { class: "form-group",
                label { "输入目录:" }
                input {
                    value: "{input_dir}",
                    placeholder: "输入C项目目录路径",
                    oninput: move |e| input_dir.set(e.value())
                }
            }

            if *selected_command.read() == CliCommand::Preprocess || *selected_command.read() == CliCommand::Translate {
                div { class: "form-group",
                    label { "输出目录:" }
                    input {
                        value: "{output_dir}",
                        placeholder: "输出目录路径（可选）",
                        oninput: move |e| output_dir.set(e.value())
                    }
                }
            }

            if *selected_command.read() == CliCommand::AnalyzeRelations || *selected_command.read() == CliCommand::RelationQuery {
                div { class: "form-group",
                    label { "项目名称:" }
                    input {
                        value: "{project_name}",
                        placeholder: "项目名称（可选）",
                        oninput: move |e| project_name.set(e.value())
                    }
                }
            }

            if *selected_command.read() == CliCommand::AnalyzeRelations || *selected_command.read() == CliCommand::RelationQuery {
                div { class: "form-group",
                    label { "数据库路径:" }
                    input {
                        value: "{db_path}",
                        oninput: move |e| db_path.set(e.value())
                    }
                }
            }

            if *selected_command.read() == CliCommand::RelationQuery {
                div { class: "form-group",
                    label { "查询类型:" }
                    select {
                        value: "{query_type.read():?}",
                        onchange: move |e| {
                            if let Ok(qt) = e.value().parse::<String>() {
                                match qt.as_str() {
                                    "ListProjects" => query_type.set(QueryType::ListProjects),
                                    "Stats" => query_type.set(QueryType::Stats),
                                    "Report" => query_type.set(QueryType::Report),
                                    "FindFunc" => query_type.set(QueryType::FindFunc),
                                    "CallChain" => query_type.set(QueryType::CallChain),
                                    "FileAnalysis" => query_type.set(QueryType::FileAnalysis),
                                    "TopCalled" => query_type.set(QueryType::TopCalled),
                                    "TopComplex" => query_type.set(QueryType::TopComplex),
                                    "DepsAnalysis" => query_type.set(QueryType::DepsAnalysis),
                                    "Search" => query_type.set(QueryType::Search),
                                    "FuncUsage" => query_type.set(QueryType::FuncUsage),
                                    _ => {}
                                }
                            }
                        },
                        option { value: "ListProjects", "列出所有项目" }
                        option { value: "Stats", "显示项目统计" }
                        option { value: "Report", "生成项目报告" }
                        option { value: "FindFunc", "查找函数定义和调用" }
                        option { value: "CallChain", "获取函数调用链" }
                        option { value: "FileAnalysis", "分析文件关系" }
                        option { value: "TopCalled", "获取最常调用函数" }
                        option { value: "TopComplex", "获取最复杂函数" }
                        option { value: "DepsAnalysis", "分析文件依赖" }
                        option { value: "Search", "搜索函数使用" }
                        option { value: "FuncUsage", "获取函数使用摘要" }
                    }
                }

                div { class: "form-group",
                    label { "目标:" }
                    input {
                        value: "{target}",
                        placeholder: "目标函数名或文件路径",
                        oninput: move |e| target.set(e.value())
                    }
                }

                div { class: "form-group",
                    label { "关键词:" }
                    input {
                        value: "{keyword}",
                        placeholder: "搜索关键词",
                        oninput: move |e| keyword.set(e.value())
                    }
                }

                div { class: "form-group",
                    label { "结果限制:" }
                    input {
                        r#type: "number",
                        value: "{limit}",
                        oninput: move |e| {
                            if let Ok(num) = e.value().parse::<usize>() {
                                limit.set(num);
                            }
                        }
                    }
                }
            }

            button {
                onclick: execute_command,
                disabled: *is_loading.read(),
                "执行命令"
                if *is_loading.read() {
                    span { class: "loading" }
                }
            }

            if !result.read().is_empty() {
                div { class: "result-container",
                    "{result.read()}"
                }
            }
        }
    }
}

#[component]
fn LlmRequestTab() -> Element {
    let mut messages = use_signal(|| String::new());
    let mut result = use_signal(|| String::new());
    let mut is_loading = use_signal(|| false);

    let send_request = move |_| {
        if messages.read().trim().is_empty() {
            result.set("错误：请输入消息内容".to_string());
            return;
        }

        is_loading.set(true);
        result.set("请求中...".to_string());

        let msg = messages.clone();
        let mut result_clone = result.clone();
        let mut loading_clone = is_loading.clone();

        spawn(async move {
            let msg_list = vec![msg.read().clone()];
            let request_result = match llm_request(msg_list).await {
                Ok(response) => format!("LLM响应:\n{}", response),
                Err(e) => format!("请求失败: {}", e),
            };

            result_clone.set(request_result);
            loading_clone.set(false);
        });
    };

    rsx! {
        div {
            h2 { "LLM请求界面" }

            div { class: "form-group",
                label { "消息内容:" }
                textarea {
                    value: "{messages}",
                    placeholder: "输入要发送给LLM的消息",
                    rows: 5,
                    oninput: move |e| messages.set(e.value())
                }
            }

            button {
                onclick: send_request,
                disabled: *is_loading.read(),
                "发送请求"
                if *is_loading.read() {
                    span { class: "loading" }
                }
            }

            if !result.read().is_empty() {
                div { class: "result-container",
                    "{result.read()}"
                }
            }
        }
    }
}