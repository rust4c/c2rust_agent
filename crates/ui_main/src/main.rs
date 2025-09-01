use commandline_tool::{Commands, QueryType};
use dioxus::prelude::*;
use manganis::asset;

use llm_requester::llm_request;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::runtime::Runtime;

mod start_tab;
use start_tab::StartTab;

const FAVICON: Asset = asset!("assets/logo.ico");
const MAIN_CSS: Asset = asset!("crates/ui_main/assets/main.css");
const START_CSS: Asset = asset!("assets/start.css");
const HEADER_SVG: Asset = asset!("assets/header.svg");

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

    let css_content = r#"
        /* App-wide styling */
        body {
            background: linear-gradient(135deg, #1a1f2e 0%, #0f1116 100%);
            color: #ffffff;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            margin: 20px;
            line-height: 1.6;
        }

        #app {
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
        }

        #hero {
            margin: 0;
            display: flex;
            flex-direction: column;
            justify-content: center;
            align-items: center;
        }

        #links {
            width: 400px;
            text-align: left;
            font-size: x-large;
            color: white;
            display: flex;
            flex-direction: column;
        }

        #links a {
            color: white;
            text-decoration: none;
            margin-top: 20px;
            margin: 10px 0px;
            border: white 1px solid;
            border-radius: 5px;
            padding: 10px;
        }

        #links a:hover {
            background-color: #1f1f1f;
            cursor: pointer;
        }

        #header {
            max-width: 1200px;
        }

        .tab-container {
            display: flex;
            margin-bottom: 20px;
            border-bottom: 1px solid #444;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
            background: rgba(31, 31, 31, 0.5);
            backdrop-filter: blur(10px);
        }

        .tab {
            padding: 10px 20px;
            cursor: pointer;
            background-color: rgba(31, 31, 31, 0.7);
            border: 1px solid #444;
            border-bottom: none;
            margin-right: 5px;
            border-radius: 8px 8px 0 0;
            transition: all 0.3s ease;
        }

        .tab:hover {
            background-color: rgba(47, 47, 47, 0.8);
            transform: translateY(-2px);
        }

        .tab.active {
            background: linear-gradient(135deg, #2f2f2f 0%, #3a3a3a 100%);
            border-bottom: 1px solid #2f2f2f;
            box-shadow: 0 -2px 5px rgba(0, 0, 0, 0.1);
        }

        .tab-content {
            display: none;
            padding: 20px;
            background-color: rgba(31, 31, 31, 0.7);
            border-radius: 8px;
            box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
            backdrop-filter: blur(5px);
            animation: fadeIn 0.3s ease-out;
        }

        .tab-content.active {
            display: block;
        }

        .form-group {
            margin-bottom: 15px;
        }

        .form-group label {
            display: block;
            margin-bottom: 5px;
            font-weight: 500;
        }

        .form-group input, .form-group select, .form-group textarea {
            width: 100%;
            padding: 10px;
            background-color: rgba(47, 47, 47, 0.7);
            border: 1px solid #444;
            color: white;
            border-radius: 6px;
            transition: all 0.3s ease;
        }

        .form-group input:focus, .form-group select:focus, .form-group textarea:focus {
            outline: none;
            border-color: #3498db;
            box-shadow: 0 0 0 2px rgba(52, 152, 219, 0.2);
        }

        button {
            background: linear-gradient(135deg, #4a4a4a 0%, #5a5a5a 100%);
            color: white;
            border: none;
            padding: 10px 15px;
            border-radius: 6px;
            cursor: pointer;
            transition: all 0.3s ease;
            font-weight: 500;
            box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
        }

        button:hover {
            background: linear-gradient(135deg, #5a5a5a 0%, #6a6a6a 100%);
            transform: translateY(-2px);
            box-shadow: 0 4px 8px rgba(0, 0, 0, 0.15);
        }

        button:active {
            transform: translateY(0);
            box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
        }

        .result-container {
            margin-top: 20px;
            padding: 15px;
            background-color: rgba(47, 47, 47, 0.7);
            border-radius: 8px;
            border: 1px solid #444;
            white-space: pre-wrap;
            box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
            backdrop-filter: blur(5px);
            animation: fadeIn 0.3s ease-out;
        }

        .loading {
            display: inline-block;
            width: 20px;
            height: 20px;
            border: 3px solid rgba(255,255,255,.3);
            border-radius: 50%;
            border-top-color: #fff;
            animation: spin 1s ease-in-out infinite;
        }

        @keyframes spin {
            to { transform: rotate(360deg); }
        }

        @keyframes fadeIn {
            from { 
                opacity: 0; 
                transform: translateY(10px); 
            }
            to { 
                opacity: 1; 
                transform: translateY(0); 
            }
        }

        /* 开始界面样式 */
        .start-container {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            padding: 2rem;
            text-align: center;
            max-width: 800px;
            margin: 0 auto;
            min-height: 70vh;
            background: linear-gradient(135deg, #f5f7fa 0%, #c3cfe2 100%);
            border-radius: 16px;
            box-shadow: 0 10px 30px rgba(0, 0, 0, 0.1);
            animation: fadeIn 0.5s ease-out;
        }

        .start-content {
            display: flex;
            flex-direction: column;
            align-items: center;
            width: 100%;
            animation: fadeIn 0.7s ease-out;
        }

        .start-container h1 {
            font-size: 3rem;
            margin-bottom: 0.5rem;
            color: #2c3e50;
            text-shadow: 0 2px 4px rgba(0, 0, 0, 0.05);
            font-weight: 700;
            letter-spacing: -0.5px;
        }

        .start-container .version {
            font-size: 1rem;
            margin-bottom: 1rem;
            color: #95a5a6;
            font-weight: 500;
            padding: 0.25rem 0.75rem;
            background: rgba(255, 255, 255, 0.5);
            border-radius: 20px;
            backdrop-filter: blur(5px);
        }

        .start-container .description {
            font-size: 1.4rem;
            margin-bottom: 2.5rem;
            color: #7f8c8d;
            max-width: 600px;
            line-height: 1.6;
            font-weight: 400;
        }

        .features {
            display: flex;
            justify-content: space-between;
            width: 100%;
            margin-bottom: 3rem;
            gap: 1rem;
        }

        .feature {
            flex: 1;
            padding: 1.5rem;
            margin: 0;
            background: rgba(255, 255, 255, 0.7);
            border-radius: 12px;
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
            transition: all 0.3s ease;
            backdrop-filter: blur(10px);
            border: 1px solid rgba(255, 255, 255, 0.3);
            animation: fadeIn 0.9s ease-out;
            animation-fill-mode: both;
        }

        .feature:nth-child(1) {
            animation-delay: 0.1s;
        }

        .feature:nth-child(2) {
            animation-delay: 0.2s;
        }

        .feature:nth-child(3) {
            animation-delay: 0.3s;
        }

        .feature:hover {
            transform: translateY(-8px);
            box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
            background: rgba(255, 255, 255, 0.85);
        }

        .feature h3 {
            font-size: 1.5rem;
            margin-bottom: 0.5rem;
            color: #3498db;
            font-weight: 600;
        }

        .feature p {
            font-size: 1rem;
            color: #7f8c8d;
            line-height: 1.6;
        }

        .start-button {
            padding: 1rem 2.5rem;
            font-size: 1.2rem;
            background: linear-gradient(135deg, #3498db 0%, #2980b9 100%);
            color: white;
            border: none;
            border-radius: 50px;
            cursor: pointer;
            transition: all 0.3s ease;
            font-weight: 600;
            box-shadow: 0 4px 15px rgba(52, 152, 219, 0.3);
            animation: fadeIn 1.1s ease-out;
            animation-fill-mode: both;
        }

        .start-button:hover {
            background: linear-gradient(135deg, #2980b9 0%, #21618c 100%);
            transform: translateY(-3px);
            box-shadow: 0 6px 20px rgba(52, 152, 219, 0.4);
        }

        .start-button:active {
            transform: translateY(1px);
            box-shadow: 0 2px 10px rgba(52, 152, 219, 0.3);
        }
    "#;

    rsx! {
        // 内联样式标签
        style { "{css_content}" }

        document::Link { rel: "icon", href: FAVICON }
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