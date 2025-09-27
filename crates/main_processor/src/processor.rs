use anyhow::{anyhow, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{debug, info};
use single_processor::singlefile_processor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;

use crate::pkg_config::MainProcessorConfig;

// Docker 风格的进度条样式
fn progress_style_docker_step() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.blue} [{elapsed_precise}] {spinner:.green} {msg}")
        .unwrap()
        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
}

fn progress_style_docker_overall() -> ProgressStyle {
    ProgressStyle::with_template(
        "{prefix:.bold.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg} ({percent}%)",
    )
    .unwrap()
}

fn progress_style_docker_completed() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.green} [{elapsed_precise}] ✓ {msg}").unwrap()
}

fn progress_style_docker_failed() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.red} [{elapsed_precise}] ✗ {msg}").unwrap()
}

// 获取当前时间戳字符串
fn get_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let hours = (now / 3600) % 24;
    let minutes = (now / 60) % 60;
    let seconds = now % 60;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

// 生成步骤前缀 (类似 Docker 的 [1/4] RUN 格式)
fn format_step_prefix(current: usize, total: usize, action: &str) -> String {
    format!("[{}/{}] {}", current, total, action)
}

pub async fn process_single_path(path: &Path) -> Result<()> {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let timestamp = get_timestamp();

    info!("🚀 [{}] PROC [1/1] 开始处理: {}", timestamp, file_name);
    debug!("完整路径: {}", path.display());

    match singlefile_processor(path).await {
        Ok(_) => {
            let timestamp = get_timestamp();
            info!("✅ [{}] DONE [1/1] 成功处理: {}", timestamp, file_name);
            Ok(())
        }
        Err(err) => {
            let timestamp = get_timestamp();
            info!(
                "❌ [{}] ERROR [1/1] 处理失败: {} - {}",
                timestamp, file_name, err
            );
            Err(err)
        }
    }
}

/// 遍历 src_cache 目录，收集可处理的目标目录
/// 参考结构:
/// src_cache/
///   ├── individual_files/   <- 这里的每个子目录都是一个可处理单元
///   ├── mapping.json        <- 可选，暂不使用
///   ├── paired_files/       <- 预留，暂不使用
///   └── unrelated_files/    <- 忽略
pub async fn discover_src_cache_projects(root: &Path) -> Result<Vec<PathBuf>> {
    use tokio::fs;

    let timestamp = get_timestamp();
    info!(
        "🔍 [{}] SCAN 开始扫描 src_cache 目录: {}",
        timestamp,
        root.display()
    );

    if !root.exists() {
        return Err(anyhow!("❌ 路径不存在: {}", root.display()));
    }

    let individual = root.join("individual_files");
    if !individual.exists() {
        return Err(anyhow!(
            "❌ src_cache 目录缺少 individual_files: {}",
            individual.display()
        ));
    }

    let mut out = Vec::new();
    let mut entries = fs::read_dir(&individual).await?;
    let mut scanned_dirs = 0;
    let mut valid_dirs = 0;

    while let Some(entry) = entries.next_entry().await? {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }

        scanned_dirs += 1;

        // 仅挑选包含 .c/.h 文件的目录，避免无效任务
        let mut has_ch = false;
        let mut c_files = 0;
        let mut h_files = 0;

        let mut sub = fs::read_dir(&p).await?;
        while let Some(se) = sub.next_entry().await? {
            let fp = se.path();
            if fp.is_file() {
                if let Some(ext) = fp.extension() {
                    match ext.to_str() {
                        Some("c") => {
                            c_files += 1;
                            has_ch = true;
                        }
                        Some("h") => {
                            h_files += 1;
                            has_ch = true;
                        }
                        _ => {}
                    }
                }
            }
        }

        if has_ch {
            debug!(
                "📁 发现有效目录: {} ({} .c 文件, {} .h 文件)",
                p.file_name().unwrap_or_default().to_string_lossy(),
                c_files,
                h_files
            );
            out.push(p);
            valid_dirs += 1;
        }
    }

    // 稳定排序，便于可重复性
    out.sort();

    let timestamp = get_timestamp();
    info!(
        "✅ [{}] SCAN 扫描完成: 发现 {} 个有效目录 (共扫描 {} 个目录)",
        timestamp, valid_dirs, scanned_dirs
    );

    Ok(out)
}

// 批量并发处理：Docker 风格的进度显示
pub async fn process_batch_paths(cfg: MainProcessorConfig, paths: Vec<PathBuf>) -> Result<()> {
    info!("🚀 开始批量处理 C2Rust 转换任务");

    let concurrent = if cfg.concurrent_limit == 0 {
        1
    } else {
        cfg.concurrent_limit
    };

    let total_tasks = paths.len();
    let m = MultiProgress::new();

    // 总体进度条，类似 Docker 的整体构建进度
    let overall = m.add(ProgressBar::new(total_tasks as u64));
    overall.set_style(progress_style_docker_overall());
    overall.set_prefix("BATCH");
    overall.set_message("正在处理 C2Rust 转换任务");

    let sem = Arc::new(Semaphore::new(concurrent));
    let mut handles = Vec::with_capacity(total_tasks);

    for (index, p) in paths.into_iter().enumerate() {
        let step_number = index + 1;
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_style(progress_style_docker_step());
        pb.set_prefix(format_step_prefix(step_number, total_tasks, "PROC"));
        pb.enable_steady_tick(Duration::from_millis(120));
        pb.set_message(format!(
            "等待处理: {}",
            p.file_name().unwrap_or_default().to_string_lossy()
        ));

        // 为该任务创建一个滚动日志小窗口
        let log_pb = m.add(ProgressBar::new_spinner());
        log_pb.set_style(ProgressStyle::with_template("{prefix:.dim} {msg}").unwrap());
        log_pb.set_prefix("日志");
        log_pb.enable_steady_tick(Duration::from_millis(300));

        let permit = sem.clone();
        let pb_clone = pb.clone();
        let log_pb_clone = log_pb.clone();
        let overall_clone = overall.clone();
        let max_retries = cfg.max_retry_attempts.max(1);
        let path_buf = p.clone();

        let handle = tokio::task::spawn_blocking(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let _permit = permit
                    .acquire_owned()
                    .await
                    .map_err(|e| anyhow!("获取信号量失败: {}", e))?;

                let file_name = path_buf.file_name().unwrap_or_default().to_string_lossy();
                let mut attempt = 0usize;
                use std::collections::VecDeque;
                let mut log_buf: VecDeque<String> = VecDeque::with_capacity(6);
                let mut update_log_window = |line: String| {
                    if log_buf.len() == log_buf.capacity() {
                        log_buf.pop_front();
                    }
                    log_buf.push_back(line);
                    let combined = log_buf.iter().cloned().collect::<Vec<_>>().join("\n");
                    log_pb_clone.set_message(combined);
                };

                loop {
                    attempt += 1;

                    // 更新进度显示，类似 Docker 的运行状态
                    if attempt == 1 {
                        pb_clone.set_message(format!(
                            "🔄 正在处理: {} (第 {} 次尝试)",
                            file_name, attempt
                        ));
                        update_log_window(format!("{} 开始处理", file_name));
                    } else {
                        pb_clone.set_message(format!(
                            "🔄 重新尝试: {} (第 {}/{} 次)",
                            file_name, attempt, max_retries
                        ));
                        update_log_window(format!("重试第 {}/{} 次", attempt, max_retries));
                    }

                    match singlefile_processor(&path_buf).await {
                        Ok(_) => {
                            // 成功完成
                            pb_clone.set_style(progress_style_docker_completed());
                            pb_clone.finish_with_message(format!("✅ 成功处理: {}", file_name));
                            update_log_window("处理完成".to_string());
                            log_pb_clone.set_style(progress_style_docker_completed());
                            log_pb_clone.finish_and_clear();
                            overall_clone.inc(1);
                            break Ok(());
                        }
                        Err(err) if attempt < max_retries => {
                            // 需要重试
                            let err_short = err.to_string().chars().take(80).collect::<String>();
                            pb_clone.set_message(format!(
                                "⚠️  第 {} 次尝试失败: {} - {}",
                                attempt, file_name, err_short
                            ));
                            update_log_window(format!("失败: {}", err_short));
                            // 短暂延迟后重试
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        Err(err) => {
                            // 最终失败
                            pb_clone.set_style(progress_style_docker_failed());
                            pb_clone.finish_with_message(format!(
                                "❌ 处理失败: {} - {}",
                                file_name,
                                err.to_string().chars().take(80).collect::<String>()
                            ));
                            update_log_window("已达到最大重试次数".to_string());
                            log_pb_clone.set_style(progress_style_docker_failed());
                            overall_clone.inc(1);
                            break Err(err);
                        }
                    }
                }
            })
        });
        handles.push(handle);
    }

    // 等待所有任务完成并统计结果
    let mut successes = 0usize;
    let mut failures = 0usize;

    for h in handles {
        match h.await {
            Ok(Ok(())) => successes += 1,
            Ok(Err(_)) => failures += 1,
            Err(_) => failures += 1,
        }
    }

    // 完成总体进度显示
    if failures == 0 {
        overall.set_style(progress_style_docker_completed());
        overall.finish_with_message(format!("🎉 全部任务完成! 成功处理 {} 个文件", successes));
        info!(
            "✅ 批量处理完成: 成功 {} 个，失败 {} 个",
            successes, failures
        );
        Ok(())
    } else {
        overall.set_style(progress_style_docker_failed());
        overall.finish_with_message(format!(
            "⚠️  批量处理完成: 成功 {} 个，失败 {} 个",
            successes, failures
        ));
        info!(
            "⚠️  批量处理完成: 成功 {} 个，失败 {} 个",
            successes, failures
        );
        Err(anyhow!("批量处理完成，但有 {} 个任务失败", failures))
    }
}
