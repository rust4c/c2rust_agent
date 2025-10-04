use anyhow::{anyhow, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::error;
use single_processor::two_stage_processor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::pkg_config::MainProcessorConfig;

/// 进度条样式 - 总体进度
fn progress_style_overall() -> ProgressStyle {
    ProgressStyle::with_template(
        "{prefix:.bold.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg} ({percent}%)",
    )
    .unwrap()
}

/// 进度条样式 - 完成状态
fn progress_style_completed() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.green} [{elapsed_precise}] ✓ {msg}").unwrap()
}

/// 进度条样式 - 失败状态
fn progress_style_failed() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.red} [{elapsed_precise}] ✗ {msg}").unwrap()
}

/// 进度条样式 - 任务阶段
fn progress_style_task() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.blue} [{elapsed_precise}] {spinner:.green} {msg}")
        .unwrap()
}

/// 生成任务前缀
fn format_task_prefix(current: usize, total: usize) -> String {
    format!("[{}/{}]", current, total)
}

/// 处理单个路径 - 两阶段翻译
pub async fn process_single_path(path: &Path) -> Result<()> {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();

    println!("🚀 开始两阶段翻译: {}", file_name);

    match two_stage_processor(path).await {
        Ok(_) => {
            println!("✅ 两阶段翻译成功: {}", file_name);
            Ok(())
        }
        Err(err) => {
            error!("两阶段翻译失败: {} - {}", file_name, err);
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

        // 只挑选包含 .c/.h 文件的目录
        let mut has_ch = false;

        let mut sub = fs::read_dir(&p).await?;
        while let Some(se) = sub.next_entry().await? {
            let fp = se.path();
            if fp.is_file() {
                if let Some(ext) = fp.extension() {
                    if ext == "c" || ext == "h" {
                        has_ch = true;
                        break;
                    }
                }
            }
        }

        if has_ch {
            projects.push(p);
            valid_dirs += 1;
        }
    }

    Ok((projects, scanned_dirs, valid_dirs))
}

/// 遍历 src_cache 目录，收集可处理的目标目录
pub async fn discover_src_cache_projects(root: &Path) -> Result<Vec<PathBuf>> {
    println!("🔍 扫描 src_cache 目录: {}", root.display());

    if !root.exists() {
        return Err(anyhow!("路径不存在: {}", root.display()));
    }

    let individual = root.join("individual_files");
    let paired = root.join("paired_files");

    if !individual.exists() && !paired.exists() {
        return Err(anyhow!(
            "src_cache 目录缺少 individual_files 和 paired_files: {}",
            root.display()
        ));
    }

    let mut out = Vec::new();
    let mut total_valid_dirs = 0;

    // 扫描 individual_files
    if individual.exists() {
        let (mut individual_projects, _, valid) = scan_directory_for_projects(&individual).await?;
        out.append(&mut individual_projects);
        total_valid_dirs += valid;
        println!("  📂 individual_files: {} 个有效目录", valid);
    }

    // 扫描 paired_files
    if paired.exists() {
        let (mut paired_projects, _, valid) = scan_directory_for_projects(&paired).await?;
        out.append(&mut paired_projects);
        total_valid_dirs += valid;
        println!("  📂 paired_files: {} 个有效目录", valid);
    }

    out.sort();
    println!("✅ 扫描完成: 总共 {} 个有效目录\n", total_valid_dirs);

    Ok(out)
}

/// 批量并发处理：使用两阶段翻译（C2Rust + AI优化 + 编译验证）
pub async fn process_batch_paths(cfg: MainProcessorConfig, paths: Vec<PathBuf>) -> Result<()> {
    let concurrent = if cfg.concurrent_limit == 0 {
        1
    } else {
        cfg.concurrent_limit
    };
    let total_tasks = paths.len();

    println!("🚀 开始批量两阶段翻译");
    println!("   任务总数: {}", total_tasks);
    println!("   并发度: {}", concurrent);
    println!("   流程: C2Rust 自动翻译 → AI 代码优化 → 编译验证\n");

    let m = MultiProgress::new();
    let overall = m.add(ProgressBar::new(total_tasks as u64));
    overall.set_style(progress_style_overall());
    overall.set_prefix("总进度");
    overall.set_message("处理中...");

    let sem = Arc::new(Semaphore::new(concurrent));
    let mut handles = Vec::with_capacity(total_tasks);

    for (index, p) in paths.into_iter().enumerate() {
        let step_number = index + 1;
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_style(progress_style_task());
        pb.set_prefix(format_task_prefix(step_number, total_tasks));

        let file_name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        pb.set_message(format!("{} - 等待开始", file_name));

        let permit = sem.clone();
        let pb_clone = pb.clone();
        let overall_clone = overall.clone();
        let max_retries = cfg.max_retry_attempts.max(1);

        let handle = tokio::task::spawn_blocking(move || {
            // 使用 block_on 来执行 async 代码
            tokio::runtime::Handle::current().block_on(async {
                let _permit = permit
                    .acquire_owned()
                    .await
                    .map_err(|e| anyhow!("获取信号量失败: {}", e))?;

                let mut attempt = 0;
                loop {
                    attempt += 1;

                    pb_clone.set_message(format!(
                        "{} - 处理中 (尝试 {}/{})",
                        file_name, attempt, max_retries
                    ));
                    pb_clone.enable_steady_tick(Duration::from_millis(100));

                    // 直接调用 two_stage_processor，内部已包含所有阶段
                    match two_stage_processor(&p).await {
                        Ok(()) => {
                            pb_clone.set_style(progress_style_completed());
                            pb_clone.finish_with_message(format!("✅ {}", file_name));
                            overall_clone.inc(1);
                            return Ok(());
                        }
                        Err(e) => {
                            if attempt < max_retries {
                                pb_clone.set_message(format!(
                                    "{} - 重试中 ({}/{})",
                                    file_name, attempt, max_retries
                                ));
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                continue;
                            } else {
                                error!("任务失败: {} - {}", file_name, e);
                                pb_clone.set_style(progress_style_failed());
                                pb_clone.finish_with_message(format!("❌ {}", file_name));
                                overall_clone.inc(1);
                                return Err(e);
                            }
                        }
                    }
                }
            })
        });
        handles.push(handle);
    }

    // 等待所有任务完成
    let mut successes = 0;
    let mut failures = 0;

    for h in handles {
        match h.await {
            Ok(Ok(())) => successes += 1,
            Ok(Err(_)) => failures += 1,
            Err(_) => failures += 1,
        }
    }

    // 完成总体进度
    if failures == 0 {
        overall.set_style(progress_style_completed());
        overall.finish_with_message(format!("🎉 全部完成! 成功 {} 个", successes));
        println!(
            "\n✅ 批量处理完成: 成功 {} 个，失败 {} 个",
            successes, failures
        );
        Ok(())
    } else {
        overall.set_style(progress_style_failed());
        overall.finish_with_message(format!(
            "⚠️ 完成: 成功 {} 个，失败 {} 个",
            successes, failures
        ));
        println!(
            "\n⚠️ 批量处理完成: 成功 {} 个，失败 {} 个",
            successes, failures
        );
        Err(anyhow!("批量处理完成，但有 {} 个任务失败", failures))
    }
}
