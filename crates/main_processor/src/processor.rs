use anyhow::{anyhow, Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{error, info};
use single_processor::{two_stage_processor_with_callback, StageCallback};
use std::collections::{HashMap, HashSet, VecDeque};
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

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let pb_clone = pb.clone();
    let file_name_for_log = file_name.to_string();
    let callback: StageCallback = Arc::new(move |msg: &str| {
        pb_clone.set_message(msg.to_string());
        info!("{} - {}", file_name_for_log, msg);
    });

    match two_stage_processor_with_callback(path, Some(callback)).await {
        Ok(_) => {
            pb.finish_with_message(format!("✅ 两阶段翻译成功: {}", file_name));
            println!("✅ 两阶段翻译成功: {}", file_name);
            Ok(())
        }
        Err(err) => {
            pb.finish_with_message(format!("✗ 翻译失败: {}", file_name));
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

                    // 创建阶段回调，更新进度条显示详细阶段信息
                    let pb_callback = pb_clone.clone();
                    let file_name_clone = file_name.clone();
                    let callback: StageCallback = Arc::new(move |stage_msg: &str| {
                        let message = format!(
                            "{} - {} (尝试 {}/{})",
                            file_name_clone, stage_msg, attempt, max_retries
                        );
                        pb_callback.set_message(message.clone());
                        info!("{}", message);
                    });

                    pb_clone.set_message(format!(
                        "{} - 开始处理 (尝试 {}/{})",
                        file_name, attempt, max_retries
                    ));
                    pb_clone.enable_steady_tick(Duration::from_millis(100));

                    // 使用带回调的两阶段处理器
                    match two_stage_processor_with_callback(&p, Some(callback)).await {
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

/// 读取给定根目录中的 relation_graph.json（或用户指定的绝对路径），
/// 将文件级依赖提升为“目录级依赖”，并按“叶到根”的顺序调度转换任务。
/// 同时限制并发数，但允许小于上限以避免依赖缺失导致的无效编译。
pub async fn process_with_dependency_graph(
    cfg: MainProcessorConfig,
    relation_graph_path: &Path,
    cache_root_hint: Option<&Path>,
) -> Result<()> {
    // 1) 读取关系图
    let text = tokio::fs::read_to_string(relation_graph_path)
        .await
        .with_context(|| {
            format!(
                "读取 relation_graph.json 失败: {}",
                relation_graph_path.display()
            )
        })?;
    let rel: relation_analy::RelationFile =
        serde_json::from_str(&text).with_context(|| "解析 relation_graph.json 失败")?;

    // workspace 根（relation_graph.json 内记录的 workspace）
    let workspace = rel.workspace.clone();
    let cache_root = cache_root_hint
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| workspace.clone());

    // 我们仅对 src_cache/individual_files 和 paired_files 下的每个子目录作为一个“项目节点”
    // 将文件依赖映射为目录依赖：如果 A.c 依赖 B.h/B.c，则 A 所在目录 依赖 B 所在目录。
    let mut dir_of: HashMap<PathBuf, PathBuf> = HashMap::new(); // 文件(相对) -> 目录(绝对)
    let mut projects: HashSet<PathBuf> = HashSet::new();
    // 两个根
    let indiv = cache_root.join("individual_files");
    let paired = cache_root.join("paired_files");

    for (_key, node) in &rel.files {
        // 把 relation 中的相对路径映射到 cache_root 下的实际路径
        let abs = if node.path.is_absolute() {
            node.path.clone()
        } else {
            cache_root.join(&node.path)
        };
        // 我们只关心 individual_files/*/* 或 paired_files/*/* 的直接项目子目录
        // 即 project_dir = indiv/<name> 或 paired/<name>
        let mut project_dir_opt: Option<PathBuf> = None;
        if abs.starts_with(&indiv) {
            if let Ok(relp) = abs.strip_prefix(&indiv) {
                if let Some(first) = relp.components().next() {
                    project_dir_opt = Some(indiv.join(first.as_os_str()));
                }
            }
        } else if abs.starts_with(&paired) {
            if let Ok(relp) = abs.strip_prefix(&paired) {
                if let Some(first) = relp.components().next() {
                    project_dir_opt = Some(paired.join(first.as_os_str()));
                }
            }
        }
        if let Some(project_dir) = project_dir_opt {
            projects.insert(project_dir.clone());
            dir_of.insert(node.path.clone(), project_dir);
        }
    }

    // 目录级依赖图：dir -> 其依赖的 dirs
    let mut deps: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
    let mut rdeps: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new(); // 反向：被哪些目录依赖
    for (_key, node) in &rel.files {
        // 目标目录（拥有该文件的目录）
        let Some(dir_a) = dir_of.get(&node.path).cloned() else {
            continue;
        };

        // 本地 include（只在本工程内）
        for inc in &node.local_includes {
            if let Some(dir_b) = dir_of.get(inc).cloned() {
                if dir_a != dir_b {
                    deps.entry(dir_a.clone()).or_default().insert(dir_b.clone());
                    rdeps
                        .entry(dir_b.clone())
                        .or_default()
                        .insert(dir_a.clone());
                    projects.insert(dir_a.clone());
                    projects.insert(dir_b.clone());
                }
            }
        }
    }

    // 确保所有项目节点在图中存在条目
    for p in &projects {
        deps.entry(p.clone()).or_default();
        rdeps.entry(p.clone()).or_default();
    }

    // 计算入度（依赖计数）：一个目录必须等其所有依赖目录完成后才能处理
    let mut indeg: HashMap<PathBuf, usize> = HashMap::new();
    for p in &projects {
        let d = deps.get(p).map(|s| s.len()).unwrap_or(0);
        indeg.insert(p.clone(), d);
    }

    // 就绪队列：所有 indeg==0 的节点（叶子层），这就是“末梢”
    let mut ready: VecDeque<PathBuf> = indeg
        .iter()
        .filter(|(_, &v)| v == 0)
        .map(|(k, _)| k.clone())
        .collect();

    let total_tasks = projects.len();
    println!("🚀 依赖感知批量翻译");
    println!("   发现项目数: {}", total_tasks);
    println!("   调度策略: 叶到根、依赖就绪优先\n");

    let concurrent = if cfg.concurrent_limit == 0 {
        1
    } else {
        cfg.concurrent_limit
    };
    let m = MultiProgress::new();
    let overall = m.add(ProgressBar::new(total_tasks as u64));
    overall.set_style(progress_style_overall());
    overall.set_prefix("总进度");
    overall.set_message("等待就绪...");

    // 我们用一个信号量限制并发，但不会强行填满，如果没有就绪任务则等待
    let sem = Arc::new(Semaphore::new(concurrent));
    let mut join_set: tokio::task::JoinSet<(PathBuf, Result<()>)> = tokio::task::JoinSet::new();
    let mut running_dirs: HashSet<PathBuf> = HashSet::new();
    let mut completed_ok: HashSet<PathBuf> = HashSet::new();
    let mut completed_err: HashSet<PathBuf> = HashSet::new();

    // 小工具：为目录创建一个任务（避免闭包捕获可变借用，改为函数传参）
    fn spawn_task_in(
        join_set: &mut tokio::task::JoinSet<(PathBuf, Result<()>)>,
        running_dirs: &mut HashSet<PathBuf>,
        dir: PathBuf,
        cfg: &MainProcessorConfig,
        m: &MultiProgress,
        sem: Arc<Semaphore>,
    ) {
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_style(progress_style_task());
        let name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        pb.set_prefix(format!("{}", name));
        pb.set_message("排队中...");

        let dir_clone = dir.clone();
        let max_retries = cfg.max_retry_attempts.max(1);
        join_set.spawn(async move {
            let _permit = sem.acquire_owned().await.unwrap();
            let mut attempt = 0;
            loop {
                attempt += 1;
                let pb_clone = pb.clone();
                let name2 = name.clone();
                let callback: StageCallback = Arc::new(move |stage| {
                    pb_clone.set_message(format!(
                        "{} - {} (尝试 {}/{})",
                        name2, stage, attempt, max_retries
                    ));
                });
                match two_stage_processor_with_callback(&dir_clone, Some(callback)).await {
                    Ok(()) => {
                        pb.set_style(progress_style_completed());
                        pb.finish_with_message(format!("✅ {}", name));
                        break (dir_clone.clone(), Ok(()));
                    }
                    Err(e) => {
                        if attempt < max_retries {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        } else {
                            pb.set_style(progress_style_failed());
                            pb.finish_with_message(format!("❌ {}", name));
                            break (dir_clone.clone(), Err(e));
                        }
                    }
                }
            }
        });
        running_dirs.insert(dir);
    }

    // 主循环：有就绪则提交任务；否则等待任一任务完成并推进图
    while completed_ok.len() + completed_err.len() < total_tasks {
        // 尽量提交就绪任务，直到并发上限或没有就绪
        while running_dirs.len() < concurrent && !ready.is_empty() {
            if let Some(dir) = ready.pop_front() {
                // 已失败的依赖会阻塞中心节点，但我们仍允许其它分支继续；
                // 这里继续提交叶节点。
                spawn_task_in(&mut join_set, &mut running_dirs, dir, &cfg, &m, sem.clone());
            }
        }

        if join_set.is_empty() {
            // 没有就绪任务也没有运行中的任务，说明图中存在循环或所有剩余节点被失败的依赖阻塞。
            // 为避免死等，直接中断。
            break;
        }

        // 等待任一个任务完成
        let Some(res_join) = join_set.join_next().await else {
            break;
        };
        let (dir_done, res) = res_join.unwrap();
        running_dirs.remove(&dir_done);
        match res {
            Ok(()) => {
                completed_ok.insert(dir_done.clone());
            }
            Err(_) => {
                completed_err.insert(dir_done.clone());
            }
        }
        overall.inc(1);

        // 将其作为依赖的节点入度-1（只有成功完成才解锁依赖，失败则不解锁）
        if completed_ok.contains(&dir_done) {
            if let Some(users) = rdeps.get(&dir_done) {
                for u in users {
                    if let Some(v) = indeg.get_mut(u) {
                        if *v > 0 {
                            *v -= 1;
                        }
                        if *v == 0 {
                            ready.push_back(u.clone());
                        }
                    }
                }
            }
        }
    }

    if completed_err.is_empty() {
        overall.set_style(progress_style_completed());
        overall.finish_with_message(format!("🎉 全部完成! 成功 {} 个", completed_ok.len()));
        Ok(())
    } else {
        overall.set_style(progress_style_failed());
        overall.finish_with_message(format!(
            "⚠️ 完成: 成功 {} 个，失败 {} 个",
            completed_ok.len(),
            completed_err.len()
        ));
        Err(anyhow!(
            "依赖感知处理完成，但有 {} 个任务失败",
            completed_err.len()
        ))
    }
}
