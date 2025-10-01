use anyhow::{anyhow, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{debug, info};
use single_processor::two_stage_processor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;

use crate::pkg_config::MainProcessorConfig;

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

    info!("🚀 [{}] PROC [1/1] 开始两阶段翻译处理: {}", timestamp, file_name);
    debug!("完整路径: {}", path.display());

    match two_stage_processor(path).await {
        Ok(_) => {
            let timestamp = get_timestamp();
            info!("✅ [{}] DONE [1/1] 两阶段翻译成功: {}", timestamp, file_name);
            Ok(())
        }
        Err(err) => {
            let timestamp = get_timestamp();
            info!(
                "❌ [{}] ERROR [1/1] 两阶段翻译失败: {} - {}",
                timestamp, file_name, err
            );
            Err(err)
        }
    }
}

/// 扫描指定目录，收集包含 .c/.h 文件的子目录
async fn scan_directory_for_projects(dir_path: &Path) -> Result<(Vec<PathBuf>, usize, usize)> {
    use tokio::fs;
    
    let mut projects = Vec::new();
    let mut scanned_dirs = 0;
    let mut valid_dirs = 0;

    if !dir_path.exists() {
        return Ok((projects, scanned_dirs, valid_dirs));
    }

    let mut entries = fs::read_dir(dir_path).await?;
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
            projects.push(p);
            valid_dirs += 1;
        }
    }

    Ok((projects, scanned_dirs, valid_dirs))
}

/// 遍历 src_cache 目录，收集可处理的目标目录
/// 参考结构:
/// src_cache/
///   ├── individual_files/   <- 这里的每个子目录都是一个可处理单元
///   ├── paired_files/       <- 这里的每个子目录都是一个可处理单元
///   ├── mapping.json        <- 可选，暂不使用
///   └── unrelated_files/    <- 忽略
pub async fn discover_src_cache_projects(root: &Path) -> Result<Vec<PathBuf>> {
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
    let paired = root.join("paired_files");
    
    // 检查至少存在一个目录
    if !individual.exists() && !paired.exists() {
        return Err(anyhow!(
            "❌ src_cache 目录缺少 individual_files 和 paired_files: {}",
            root.display()
        ));
    }

    let mut out = Vec::new();
    let mut total_scanned_dirs = 0;
    let mut total_valid_dirs = 0;

    // 扫描 individual_files 目录
    if individual.exists() {
        info!("🔍 扫描 individual_files 目录...");
        let (mut individual_projects, scanned, valid) = scan_directory_for_projects(&individual).await?;
        out.append(&mut individual_projects);
        total_scanned_dirs += scanned;
        total_valid_dirs += valid;
        info!("📂 individual_files: 发现 {} 个有效目录 (共扫描 {} 个)", valid, scanned);
    } else {
        info!("⚠️  跳过不存在的 individual_files 目录");
    }

    // 扫描 paired_files 目录
    if paired.exists() {
        info!("🔍 扫描 paired_files 目录...");
        let (mut paired_projects, scanned, valid) = scan_directory_for_projects(&paired).await?;
        out.append(&mut paired_projects);
        total_scanned_dirs += scanned;
        total_valid_dirs += valid;
        info!("📂 paired_files: 发现 {} 个有效目录 (共扫描 {} 个)", valid, scanned);
    } else {
        info!("⚠️  跳过不存在的 paired_files 目录");
    }

    // 稳定排序，便于可重复性
    out.sort();

    let timestamp = get_timestamp();
    info!(
        "✅ [{}] SCAN 扫描完成: 总共发现 {} 个有效目录 (共扫描 {} 个目录)",
        timestamp, total_valid_dirs, total_scanned_dirs
    );

    Ok(out)
}

// 批量并发处理：Docker 风格的进度显示
pub async fn process_batch_paths(cfg: MainProcessorConfig, paths: Vec<PathBuf>) -> Result<()> {
    // 使用 progress bar 的 suspend 包裹日志，避免打断进度条渲染
    // 参考示例：通过 suspend 在进度条上方输出日志
    // 由于 overall 进度条稍后才创建，这里先直接打印一次启动日志
    info!("🚀 开始批量处理两阶段 C2Rust 翻译任务");

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
    overall.set_message("正在处理两阶段 C2Rust 翻译任务");

    // 从这里开始，所有日志尽量通过 suspend 包裹，避免与进度条冲突
    overall.suspend(|| {
        info!(
            "📦 两阶段翻译任务数: {}，并发度: {} (0 表示串行，已规范为至少 1)",
            total_tasks, concurrent
        );
        info!("🔄 翻译流程: C2Rust 自动翻译 → AI 代码优化");
    });

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
                    .map_err(|e| anyhow!("获取信号量失败: {}", e))?;

                let file_name = path_buf.file_name().unwrap_or_default().to_string_lossy();
                let mut attempt = 0usize;
                // 最近一条细节信息，展示在进度条消息尾部，避免额外的日志栏位
                let mut _last_detail: Option<String> = None;
                let set_status =
                    |status: &str, last_detail: &Option<String>| match last_detail.as_ref() {
                        Some(detail) => pb_clone.set_message(format!("{} | {}", status, detail)),
                        None => pb_clone.set_message(status.to_string()),
                    };

                loop {
                    attempt += 1;

                    // 更新进度显示，类似 Docker 的运行状态
                    if attempt == 1 {
                        _last_detail = Some("两阶段翻译中".to_string());
                        let status = format!("🔄 两阶段翻译: {} (第 {} 次尝试)", file_name, attempt);
                        set_status(&status, &_last_detail);
                    } else {
                        _last_detail = Some(format!("重试第 {}/{} 次", attempt, max_retries));
                        let status = format!(
                            "🔄 重新尝试翻译: {} (第 {}/{} 次)",
                            file_name, attempt, max_retries
                        );
                        set_status(&status, &_last_detail);
                    }

                    match two_stage_processor(&path_buf).await {
                        Ok(_) => {
                            // 成功完成
                            pb_clone.set_style(progress_style_docker_completed());
                            pb_clone.finish_with_message(format!("✅ 两阶段翻译成功: {}", file_name));
                            overall_clone.inc(1);
                            break Ok(());
                        }
                        Err(err) if attempt < max_retries => {
                            // 需要重试
                            let err_short = err.to_string().chars().take(80).collect::<String>();
                            _last_detail = Some(format!("失败: {}", err_short));
                            let status = format!("⚠️  第 {} 次尝试失败: {}", attempt, file_name);
                            set_status(&status, &_last_detail);
                            // 短暂延迟后重试
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                        Err(err) => {
                            // 最终失败
                            pb_clone.set_style(progress_style_docker_failed());
                            pb_clone.finish_with_message(format!(
                                "❌ 翻译失败: {} - {}",
                                file_name,
                                err.to_string().chars().take(80).collect::<String>()
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
        overall.finish_with_message(format!("🎉 全部两阶段翻译任务完成! 成功翻译 {} 个文件", successes));
        overall.suspend(|| {
            info!(
                "✅ 两阶段翻译批量处理完成: 成功 {} 个，失败 {} 个",
                successes, failures
            );
        });
        Ok(())
    } else {
        overall.set_style(progress_style_docker_failed());
        overall.finish_with_message(format!(
            "⚠️  两阶段翻译批量处理完成: 成功 {} 个，失败 {} 个",
            successes, failures
        ));
        overall.suspend(|| {
            info!(
                "⚠️  两阶段翻译批量处理完成: 成功 {} 个，失败 {} 个",
                successes, failures
            );
        });
        Err(anyhow!("两阶段翻译批量处理完成，但有 {} 个任务失败", failures))
    }
}
