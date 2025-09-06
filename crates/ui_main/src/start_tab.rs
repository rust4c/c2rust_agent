use dioxus::prelude::*;

#[component]
pub fn StartTab(on_start: EventHandler<()>) -> Element {
    rsx! {
        div { class: "start-container",
            div { class: "start-content",
                h1 { "C2Rust Agent" }
                p { class: "version", "版本: 0.0.8" }
                p { class: "description", "将C代码转换为Rust代码的智能工具" }
                div { class: "features",
                    div { class: "feature",
                        h3 { "分析" }
                        p { "深度分析C代码结构" }
                    }
                    div { class: "feature",
                        h3 { "转换" }
                        p { "智能转换C到Rust代码" }
                    }
                    div { class: "feature",
                        h3 { "优化" }
                        p { "优化生成代码质量" }
                    }
                }
                button { class: "start-button", onclick: move |_| on_start.call(()),
                    "开始使用"
                }
            }
        }
    }
}


