slint::include_modules!();

use commandline_tool::{init_services, run_analyze, run_preprocess, run_translate};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;

fn main() {
    // Build a multi-threaded tokio runtime for async operations triggered by UI
    let rt = Arc::new(Runtime::new().expect("Failed to create tokio runtime"));

    let ui = MainWindow::new().expect("failed to load UI");

    // Install a panic hook to surface panics via dialog and append to log
    {
        let ui_weak = ui.as_weak();
        std::panic::set_hook(Box::new(move |panic_info| {
            let payload = panic_info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "Unknown panic".to_string());
            let location = panic_info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_else(|| "<unknown location>".to_string());
            let msg = format!("Panic at {}\n{}\n", location, payload);

            // Update UI on main thread
            let ui_weak2 = ui_weak.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak2.upgrade() {
                    // Append to log area
                    let old: String = ui.get_log_text().into();
                    ui.set_log_text(format!("{}{}", old, msg).into());

                    // Show panic dialog
                    if let Ok(dlg) = PanicDialog::new() {
                        dlg.set_message(msg.clone().into());
                        let _ = dlg.show();
                    }
                }
            })
            .ok();
        }));
    }

    // Initialize services once with debug=false for UI; could be toggled later
    {
        let rt2 = rt.clone();
        rt2.block_on(async move {
            let _ = init_services(false).await; // ignore errors but services will log
        });
    }

    let ui_handle = ui.as_weak();
    let rt_analyze = rt.clone();
    ui.on_analyze_click(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let input = ui.get_input_dir();
            let ui2 = ui.as_weak();
            rt_analyze.spawn(async move {
                let input_path = PathBuf::from(input.to_string());
                let mut buf = String::new();
                if input_path.as_os_str().is_empty() {
                    buf.push_str("错误：请输入输入目录\n");
                } else {
                    buf.push_str(&format!("开始分析: {}\n", input_path.display()));
                    if let Err(e) = run_analyze(&input_path).await {
                        buf.push_str(&format!("分析失败: {}\n", e));
                    } else {
                        buf.push_str("分析完成\n");
                    }
                }
                let text_to_set = buf;
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui2.upgrade() {
                        let old: String = ui.get_log_text().into();
                        ui.set_log_text(format!("{}{}", old, text_to_set).into());
                    }
                })
                .ok();
            });
        }
    });

    let ui_handle = ui.as_weak();
    let rt_pre = rt.clone();
    ui.on_preprocess_click(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let input = ui.get_input_dir();
            let output = ui.get_output_dir();
            let ui2 = ui.as_weak();
            rt_pre.spawn(async move {
                let input_path = PathBuf::from(input.to_string());
                let output_path = if output.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(output.to_string()))
                };
                let mut buf = String::new();
                if input_path.as_os_str().is_empty() {
                    buf.push_str("错误：请输入输入目录\n");
                } else {
                    buf.push_str(&format!("开始预处理: {}\n", input_path.display()));
                    match run_preprocess(&input_path, output_path.as_deref()).await {
                        Ok(cache_dir) => {
                            buf.push_str(&format!("预处理完成: {}\n", cache_dir.display()))
                        }
                        Err(e) => buf.push_str(&format!("预处理失败: {}\n", e)),
                    }
                }
                let text_to_set = buf;
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui2.upgrade() {
                        let old: String = ui.get_log_text().into();
                        ui.set_log_text(format!("{}{}", old, text_to_set).into());
                    }
                })
                .ok();
            });
        }
    });

    let ui_handle = ui.as_weak();
    let rt_trans = rt.clone();
    ui.on_translate_click(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let input = ui.get_input_dir();
            let output = ui.get_output_dir();
            let ui2 = ui.as_weak();
            rt_trans.spawn(async move {
                let input_path = PathBuf::from(input.to_string());
                let output_path = if output.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(output.to_string()))
                };
                let mut buf = String::new();
                if input_path.as_os_str().is_empty() {
                    buf.push_str("错误：请输入输入目录\n");
                } else {
                    buf.push_str(&format!("开始转换: {}\n", input_path.display()));
                    match run_translate(&input_path, output_path.as_deref()).await {
                        Ok(()) => buf.push_str("转换完成\n"),
                        Err(e) => buf.push_str(&format!("转换失败: {}\n", e)),
                    }
                }
                let text_to_set = buf;
                slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui2.upgrade() {
                        let old: String = ui.get_log_text().into();
                        ui.set_log_text(format!("{}{}", old, text_to_set).into());
                    }
                })
                .ok();
            });
        }
    });

    ui.run().unwrap();
}
