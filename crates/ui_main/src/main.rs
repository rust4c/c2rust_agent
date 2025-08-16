//代码环境在本地，兼容性是没有的
use dioxus::prelude::*;
use dioxus::logger::tracing;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // 状态管理：控制是否显示宽限消息
    let mut show_extension = use_signal(|| false);
    
    // 事件处理函数
    let handle_click = move |_| {
        show_extension.set(true);  // 设置为显示宽限消息
        tracing::info!("请求宽限几天");
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; 
                     align-items: center; justify-content: center; 
                     height: 100vh; font-family: Arial, sans-serif;",
            
            /* 核心文本 */
            h1 { "我写下代码代表我还活着，我还在坚持。" }
            
            /* 宽限按钮 */
            button {
                onclick: handle_click,  // 绑定点击事件
                style: "
                    padding: 12px 24px;
                    background-color: #4CAF50;
                    color: white;
                    border: none;
                    border-radius: 8px;
                    cursor: pointer;
                    font-size: 16px;
                    margin-top: 20px;
                    transition: background-color 0.3s;
                    
                    &:hover {
                        background-color: #45a049;
                    }
                ",
                "请求宽限几天"
            }
            
            /* 根据状态显示宽限消息 */
            if show_extension() {
                rsx! {
                    p {
                        style: "color: #FF5722; font-size: 18px; margin-top: 20px;",
                        "再宽限几天..."
                    }
                }
            }
        }
    }
}
    *
