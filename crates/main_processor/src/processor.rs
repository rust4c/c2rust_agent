use anyhow::{anyhow, Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use log::{error, info};
use single_processor::{singlefile_processor, StageCallback};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::pkg_config::MainProcessorConfig;

/// Progress bar style - overall progress
fn progress_style_overall() -> ProgressStyle {
    ProgressStyle::with_template(
        "{prefix:.bold.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg} ({percent}%)",
    )
    .unwrap()
}

/// Progress bar style - completed state
fn progress_style_completed() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.green} [{elapsed_precise}] ✓ {msg}").unwrap()
}

/// Progress bar style - failed state
fn progress_style_failed() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.red} [{elapsed_precise}] ✗ {msg}").unwrap()
}

/// Progress bar style - task phase
fn progress_style_task() -> ProgressStyle {
    ProgressStyle::with_template("{prefix:.bold.blue} [{elapsed_precise}] {spinner:.green} {msg}")
        .unwrap()
}

/// Generate task prefix
fn format_task_prefix(current: usize, total: usize) -> String {
    format!("[{}/{}]", current, total)
}

/// Process single path - two-stage translation
pub async fn process_single_path(path: &Path) -> Result<()> {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();

    println!("🚀 Starting two-stage translation: {}", file_name);

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

    match singlefile_processor(path, Some(callback)).await {
        Ok(_) => {
            pb.finish_with_message(format!(
                "✅ Two-stage translation successful: {}",
                file_name
            ));
            println!("✅ Two-stage translation successful: {}", file_name);
            Ok(())
        }
        Err(err) => {
            pb.finish_with_message(format!("✗ Translation failed: {}", file_name));
            error!("Two-stage translation failed: {} - {}", file_name, err);
            Err(err)
        }
    }
}

/// Scan specified directory, collect subdirectories containing .c/.h files
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

        // Only select directories containing .c/.h files
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

/// Traverse src_cache directory, collect processable target directories
pub async fn discover_src_cache_projects(root: &Path) -> Result<Vec<PathBuf>> {
    println!("🔍 Scanning src_cache directory: {}", root.display());

    if !root.exists() {
        return Err(anyhow!("Path does not exist: {}", root.display()));
    }

    let individual = root.join("individual_files");
    let paired = root.join("paired_files");

    if !individual.exists() && !paired.exists() {
        return Err(anyhow!(
            "src_cache directory missing individual_files and paired_files: {}",
            root.display()
        ));
    }

    let mut out = Vec::new();
    let mut total_valid_dirs = 0;

    // Scan individual_files
    if individual.exists() {
        let (mut individual_projects, _, valid) = scan_directory_for_projects(&individual).await?;
        out.append(&mut individual_projects);
        total_valid_dirs += valid;
        println!("  📂 individual_files: {} valid directories", valid);
    }

    // Scan paired_files
    if paired.exists() {
        let (mut paired_projects, _, valid) = scan_directory_for_projects(&paired).await?;
        out.append(&mut paired_projects);
        total_valid_dirs += valid;
        println!("  📂 paired_files: {} valid directories", valid);
    }

    out.sort();
    println!(
        "✅ Scanning completed: {} valid directories total\n",
        total_valid_dirs
    );

    Ok(out)
}

/// Batch concurrent processing: using two-stage translation (C2Rust + AI optimization + compilation verification)
pub async fn process_batch_paths(cfg: MainProcessorConfig, paths: Vec<PathBuf>) -> Result<()> {
    let concurrent = if cfg.concurrent_limit == 0 {
        1
    } else {
        cfg.concurrent_limit
    };
    let total_tasks = paths.len();

    println!("🚀 Starting batch two-stage translation");
    println!("   Total tasks: {}", total_tasks);
    println!("   Concurrency: {}", concurrent);
    println!("   Process: C2Rust automatic translation → AI code optimization → compilation verification\n");

    let m = MultiProgress::new();
    let overall = m.add(ProgressBar::new(total_tasks as u64));
    overall.set_style(progress_style_overall());
    overall.set_prefix("Total Progress");
    overall.set_message("Processing...");

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
        pb.set_message(format!("{} - Waiting to start", file_name));

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
                    .map_err(|e| anyhow!("Failed to acquire semaphore: {}", e))?;

                let mut attempt = 0;
                loop {
                    attempt += 1;

                    // 创建阶段回调，更新进度条显示详细阶段信息
                    let pb_callback = pb_clone.clone();
                    let file_name_clone = file_name.clone();
                    let callback: StageCallback = Arc::new(move |stage_msg: &str| {
                        let message = format!(
                            "{} - {} (attempt {}/{})",
                            file_name_clone, stage_msg, attempt, max_retries
                        );
                        pb_callback.set_message(message.clone());
                        info!("{}", message);
                    });

                    pb_clone.set_message(format!(
                        "{} - Starting processing (attempt {}/{})",
                        file_name, attempt, max_retries
                    ));
                    pb_clone.enable_steady_tick(Duration::from_millis(100));

                    // 使用带回调的两阶段处理器
                    match singlefile_processor(&p, Some(callback)).await {
                        Ok(()) => {
                            pb_clone.set_style(progress_style_completed());
                            pb_clone.finish_with_message(format!("✅ {}", file_name));
                            overall_clone.inc(1);
                            return Ok(());
                        }
                        Err(e) => {
                            if attempt < max_retries {
                                pb_clone.set_message(format!(
                                    "{} - Retrying ({}/{})",
                                    file_name, attempt, max_retries
                                ));
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                continue;
                            } else {
                                error!("Task failed: {} - {}", file_name, e);
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

    // Wait for all tasks to complete
    let mut successes = 0;
    let mut failures = 0;

    for h in handles {
        match h.await {
            Ok(Ok(())) => successes += 1,
            Ok(Err(_)) => failures += 1,
            Err(_) => failures += 1,
        }
    }

    // Complete overall progress
    if failures == 0 {
        overall.set_style(progress_style_completed());
        overall.finish_with_message(format!("🎉 All completed! {} successful", successes));
        println!(
            "\n✅ Batch processing completed: {} successful, {} failed",
            successes, failures
        );
        Ok(())
    } else {
        overall.set_style(progress_style_failed());
        overall.finish_with_message(format!(
            "⚠️ Completed: {} successful, {} failed",
            successes, failures
        ));
        println!(
            "\n⚠️ Batch processing completed: {} successful, {} failed",
            successes, failures
        );
        Err(anyhow!(
            "Batch processing completed, but {} tasks failed",
            failures
        ))
    }
}

/// Read relation_graph.json from given root directory (or user-specified absolute path),
/// elevate file-level dependencies to "directory-level dependencies", and schedule conversion tasks in "leaf-to-root" order.
/// Also limit concurrency, but allow below the upper limit to avoid invalid compilation due to missing dependencies.
pub async fn process_with_dependency_graph(
    cfg: MainProcessorConfig,
    relation_graph_path: &Path,
    cache_root_hint: Option<&Path>,
) -> Result<()> {
    // 1) Read relation graph
    let text = tokio::fs::read_to_string(relation_graph_path)
        .await
        .with_context(|| {
            format!(
                "Failed to read relation_graph.json: {}",
                relation_graph_path.display()
            )
        })?;
    let rel: relation_analy::RelationFile =
        serde_json::from_str(&text).with_context(|| "Failed to parse relation_graph.json")?;

    // workspace root (workspace recorded in relation_graph.json)
    let workspace = rel.workspace.clone();
    let cache_root = cache_root_hint
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| workspace.clone());

    // We only treat each subdirectory under src_cache/individual_files and paired_files as a "project node"
    // Map file dependencies to directory dependencies: if A.c depends on B.h/B.c, then A's directory depends on B's directory.
    let mut dir_of: HashMap<PathBuf, PathBuf> = HashMap::new(); // file(relative) -> directory(absolute)
    let mut projects: HashSet<PathBuf> = HashSet::new();
    // Two roots
    let indiv = cache_root.join("individual_files");
    let paired = cache_root.join("paired_files");

    for (_key, node) in &rel.files {
        // Map relative paths in relation to actual paths under cache_root
        let abs = if node.path.is_absolute() {
            node.path.clone()
        } else {
            cache_root.join(&node.path)
        };
        // We only care about direct project subdirectories of individual_files/*/* or paired_files/*/*
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

    // Directory-level dependency graph: dir -> its dependent dirs
    let mut deps: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
    let mut rdeps: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new(); // Reverse: which directories depend on it
    for (_key, node) in &rel.files {
        // Target directory (directory that owns this file)
        let Some(dir_a) = dir_of.get(&node.path).cloned() else {
            continue;
        };

        // Local includes (only within this project)
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

    // Ensure all project nodes have entries in the graph
    for p in &projects {
        deps.entry(p.clone()).or_default();
        rdeps.entry(p.clone()).or_default();
    }

    // Calculate in-degree (dependency count): a directory must wait for all its dependent directories to complete before processing
    let mut indeg: HashMap<PathBuf, usize> = HashMap::new();
    for p in &projects {
        let d = deps.get(p).map(|s| s.len()).unwrap_or(0);
        indeg.insert(p.clone(), d);
    }

    // Ready queue: all nodes with indeg==0 (leaf layer), these are the "endpoints"
    let mut ready: VecDeque<PathBuf> = indeg
        .iter()
        .filter(|(_, &v)| v == 0)
        .map(|(k, _)| k.clone())
        .collect();

    let total_tasks = projects.len();
    println!("🚀 Dependency-aware batch translation");
    println!("   Projects discovered: {}", total_tasks);
    println!("   Scheduling strategy: leaf-to-root, dependency-ready priority\n");

    let concurrent = if cfg.concurrent_limit == 0 {
        1
    } else {
        cfg.concurrent_limit
    };
    let m = MultiProgress::new();
    let overall = m.add(ProgressBar::new(total_tasks as u64));
    overall.set_style(progress_style_overall());
    overall.set_prefix("Total Progress");
    overall.set_message("Waiting for ready...");

    // We use a semaphore to limit concurrency, but won't force fill it, wait if no ready tasks
    let sem = Arc::new(Semaphore::new(concurrent));
    let mut join_set: tokio::task::JoinSet<(PathBuf, Result<()>)> = tokio::task::JoinSet::new();
    let mut running_dirs: HashSet<PathBuf> = HashSet::new();
    let mut completed_ok: HashSet<PathBuf> = HashSet::new();
    let mut completed_err: HashSet<PathBuf> = HashSet::new();

    // Utility: create a task for directory (avoid closure capturing mutable borrow, use function parameters)
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
        pb.set_message("Queuing...");

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
                        "{} - {} (attempt {}/{})",
                        name2, stage, attempt, max_retries
                    ));
                });
                match singlefile_processor(&dir_clone, Some(callback)).await {
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

    // Main loop: submit tasks if ready; otherwise wait for any task to complete and advance the graph
    while completed_ok.len() + completed_err.len() < total_tasks {
        // Submit ready tasks as much as possible, until concurrency limit or no ready tasks
        while running_dirs.len() < concurrent && !ready.is_empty() {
            if let Some(dir) = ready.pop_front() {
                // Failed dependencies will block central nodes, but we still allow other branches to continue;
                // Here we continue submitting leaf nodes.
                spawn_task_in(&mut join_set, &mut running_dirs, dir, &cfg, &m, sem.clone());
            }
        }

        if join_set.is_empty() {
            // No ready tasks and no running tasks, indicating a cycle in the graph or all remaining nodes blocked by failed dependencies.
            // To avoid deadlock, break directly.
            break;
        }

        // Wait for any task to complete
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

        // Decrease in-degree of nodes that depend on it by 1 (only successful completion unlocks dependencies, failure does not)
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
        overall.finish_with_message(format!(
            "🎉 All completed! {} successful",
            completed_ok.len()
        ));
        Ok(())
    } else {
        overall.set_style(progress_style_failed());
        overall.finish_with_message(format!(
            "⚠️ Completed: {} successful, {} failed",
            completed_ok.len(),
            completed_err.len()
        ));
        Err(anyhow!(
            "Dependency-aware processing completed, but {} tasks failed",
            completed_err.len()
        ))
    }
}
