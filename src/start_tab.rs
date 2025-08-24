use dioxus::prelude::*;

#[component]
pub fn StartTab(on_start: EventHandler<()>) -> Element {
    rsx! {
        div { class: "start-container",
            h2 { "欢迎使用 C2Rust Agent" }
            p { "这是一个将C代码转换为Rust代码的智能工具" }

            div { class: "features",
                div { class: "feature",
                    h3 { "代码转换" }
                    p { "将C代码智能转换为等效的Rust代码" }
                }
                div { class: "feature",
                    h3 { "项目分析" }
                    p { "分析C项目结构和依赖关系" }
                }
                div { class: "feature",
                    h3 { "LLM集成" }
                    p { "利用大型语言模型辅助代码转换" }
                }
            }

            button {
                class: "start-button",
                onclick: move |_| on_start.call(()),
                "开始使用"
            }
        }
    }
}