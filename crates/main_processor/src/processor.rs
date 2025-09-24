use anyhow::{anyhow, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{debug, info};
use single_processor::singlefile_processor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

mod pkg_config;
use pkg_config::get_config;

// 进度条样式
fn progress_style_spinner() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.green} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn progress_style_bar() -> ProgressStyle {
    ProgressStyle::with_template("{bar:40.cyan/blue} {pos}/{len} {msg}").unwrap()
}

pub async fn process_single_path(path: &Path) -> Result<()> {
    debug!("处理文件: {}", path.display());
    singlefile_processor(path).await
}

// 批量并发处理：带进度条和重试
pub async fn process_batch_paths(paths: Vec<PathBuf>) -> Result<()> {
    info!("开始处理");
    let cfg = get_config()?;
    let concurrent = if cfg.concurrent_limit == 0 {
        1
    } else {
        cfg.concurrent_limit
    };

    let m = MultiProgress::new();
    let overall = m.add(ProgressBar::new(paths.len() as u64));
    overall.set_style(progress_style_bar());
    overall.set_message("总体进度");

    let sem = Arc::new(Semaphore::new(concurrent));

    let mut handles = Vec::with_capacity(paths.len());
    for p in paths {
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_style(progress_style_spinner());
        pb.enable_steady_tick(Duration::from_millis(100));
        pb.set_message(format!("排队中: {}", p.display()));

        let permit = sem.clone();
        let pb_clone = pb.clone();
        let overall_clone = overall.clone();
        let max_retries = cfg.max_retry_attempts.max(1);
        let path_buf = p.clone();

        let handle = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let _permit = permit
                    .acquire_owned()
                    .await
                    .map_err(|e| anyhow!("semaphore error: {}", e))?;
                let mut attempt = 0usize;
                loop {
                    attempt += 1;
                    pb_clone.set_message(format!(
                        "处理: {} (第 {}/{}) 次尝试",
                        path_buf.display(),
                        attempt,
                        max_retries
                    ));
                    match singlefile_processor(&path_buf).await {
                        Ok(_) => {
                            pb_clone.finish_with_message(format!("完成: {}", path_buf.display()));
                            overall_clone.inc(1);
                            break Ok(());
                        }
                        Err(err) if attempt < max_retries => {
                            pb_clone.set_message(format!(
                                "重试: {}，原因: {}",
                                path_buf.display(),
                                err
                            ));
                        }
                        Err(err) => {
                            pb_clone.abandon_with_message(format!(
                                "失败: {}，原因: {}",
                                path_buf.display(),
                                err
                            ));
                            overall_clone.inc(1);
                            break Err(err);
                        }
                    }
                }
            })
        });
        handles.push(handle);
    }

    let mut failures = 0usize;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(_e)) => failures += 1,
            Err(_join_err) => failures += 1,
        }
    }

    overall.finish_with_message("全部任务完成");

    if failures == 0 {
        Ok(())
    } else {
        Err(anyhow!("{} 个任务失败", failures))
    }
}
