use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// 依赖图节点：一个 C/C++ 源文件或头文件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileNode {
    pub path: PathBuf,
    /// 直接依赖的本地文件（通过引号 include 解析到的实际路径）
    pub local_includes: BTreeSet<PathBuf>,
    /// 直接依赖的系统/第三方头（通过尖括号 include 或无法解析的引号 include）
    pub system_includes: BTreeSet<String>,
}

/// 工程级别的依赖信息（来自 compile_commands.json）
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BuildDeps {
    /// -I 包含目录
    pub include_dirs: BTreeSet<PathBuf>,
    /// -l 链接库（如 m, pthread）
    pub link_libs: BTreeSet<String>,
    /// -L 链接目录
    pub link_dirs: BTreeSet<PathBuf>,
}

/// 完整关系文件格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationFile {
    pub workspace: PathBuf,
    pub files: BTreeMap<PathBuf, FileNode>,
    pub build: BuildDeps,
    pub generated_at: String,
}

/// 合同说明
/// 输入: workspace 根路径
/// 输出: 在 workspace 根目录写入 relation_graph.json，返回 RelationFile 内存对象
pub fn generate_c_dependency_graph(workspace_root: &Path) -> Result<RelationFile> {
    let workspace_root = workspace_root
        .canonicalize()
        .with_context(|| format!("工作区路径无效: {}", workspace_root.display()))?;

    // 1) 枚举工程内的 C/C 相关文件
    let mut all_files: Vec<PathBuf> = Vec::new();
    let exts = ["c", "cc", "cpp", "cxx", "h", "hpp", "hxx"];
    for entry in WalkDir::new(&workspace_root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                if exts.contains(&ext) {
                    all_files.push(entry.path().to_path_buf());
                }
            }
        }
    }

    // 2) include 解析：正则扫描每个文件的 #include 行
    let re = Regex::new(r#"(?m)^\s*#\s*include\s*([<"])([^>"]+)[>"]"#).unwrap();
    let mut nodes: BTreeMap<PathBuf, FileNode> = BTreeMap::new();

    // 候选本地搜索路径：workspace 根 + 子目录（后面会并入 compile_commands 的 -I）
    let mut local_search_roots: Vec<PathBuf> = vec![workspace_root.clone()];

    for file in &all_files {
        // 记录上层目录，便于相对 include 先就近解析
        if let Some(parent) = file.parent() {
            if !local_search_roots.contains(&parent.to_path_buf()) {
                local_search_roots.push(parent.to_path_buf());
            }
        }
    }

    let _files_set: BTreeSet<PathBuf> = all_files.iter().cloned().collect();

    for file in &all_files {
        let content = fs::read_to_string(file)
            .with_context(|| format!("读取文件失败: {}", file.display()))?;
        let mut local_includes: BTreeSet<PathBuf> = BTreeSet::new();
        let mut system_includes: BTreeSet<String> = BTreeSet::new();

        for cap in re.captures_iter(&content) {
            let delimiter = cap.get(1).unwrap().as_str();
            let target = cap.get(2).unwrap().as_str().trim();

            match delimiter {
                "\"" => {
                    // 先按相对路径就近解析
                    let resolved = resolve_local_include(file, target, &local_search_roots);
                    if let Some(p) = resolved {
                        local_includes.insert(p);
                    } else {
                        // 无法在工程内解析，视为第三方/系统头
                        system_includes.insert(target.to_string());
                    }
                }
                "<" => {
                    system_includes.insert(target.to_string());
                }
                _ => {}
            }
        }

        nodes.insert(
            file.clone(),
            FileNode {
                path: file
                    .strip_prefix(&workspace_root)
                    .unwrap_or(file)
                    .to_path_buf(),
                local_includes: local_includes
                    .into_iter()
                    .map(|p| p.strip_prefix(&workspace_root).unwrap_or(&p).to_path_buf())
                    .collect(),
                system_includes,
            },
        );
    }

    // 3) 解析 compile_commands.json（若存在），汇总 -I、-l、-L，并扩展本地搜索路径
    let mut build = BuildDeps::default();
    let cc = workspace_root.join("compile_commands.json");
    if cc.exists() {
        if let Ok(text) = fs::read_to_string(&cc) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(arr) = v.as_array() {
                    for item in arr {
                        let cmd_opt: Option<String> = if let Some(cmd) =
                            item.get("command").and_then(|x| x.as_str())
                        {
                            Some(cmd.to_string())
                        } else if let Some(args) = item.get("arguments").and_then(|x| x.as_array())
                        {
                            Some(
                                args.iter()
                                    .filter_map(|e| e.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            )
                        } else {
                            None
                        };
                        if let Some(cmd) = cmd_opt {
                            parse_build_flags(&cmd, &mut build, &workspace_root);
                        }
                        if let Some(dir) = item.get("directory").and_then(|x| x.as_str()) {
                            let p = PathBuf::from(dir);
                            if p.is_dir() {
                                build.include_dirs.insert(p.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    // 扩展搜索路径后，再尝试补全此前未解析的本地 include（可选，保持简单不做二次扫描）
    // 为保持复杂度可控，这里不做二次解析，避免循环和多次 IO。

    // 4) 写 JSON 文件
    let relation = RelationFile {
        workspace: workspace_root.clone(),
        files: nodes,
        build,
        generated_at: chrono::Utc::now().to_rfc3339(),
    };

    let out_path = workspace_root.join("relation_graph.json");
    let json_text = serde_json::to_string_pretty(&relation)?;
    fs::write(&out_path, json_text)?;

    Ok(relation)
}

fn resolve_local_include(
    current_file: &Path,
    target: &str,
    search_roots: &[PathBuf],
) -> Option<PathBuf> {
    // 相对当前文件
    if let Some(parent) = current_file.parent() {
        let p = parent.join(target);
        if p.exists() {
            return Some(p);
        }
    }
    // 其它搜索根
    for root in search_roots {
        let p = root.join(target);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn parse_build_flags(cmd: &str, build: &mut BuildDeps, workspace: &Path) {
    // 粗暴但足够清晰的解析：按空白分割，处理 -I -L -l 三类
    // 这不是 shell 级解析，已足够覆盖常见 compile_commands
    let parts = shell_like_split(cmd);
    let mut iter = parts.iter();
    while let Some(tok) = iter.next() {
        if tok.starts_with("-I") {
            let dir = if tok == "-I" {
                iter.next().cloned()
            } else {
                Some(tok.trim_start_matches("-I").to_string())
            };
            if let Some(d) = dir {
                let p = PathBuf::from(d);
                build.include_dirs.insert(if p.is_absolute() {
                    p
                } else {
                    workspace.join(p)
                });
            }
        } else if tok.starts_with("-L") {
            let dir = if tok == "-L" {
                iter.next().cloned()
            } else {
                Some(tok.trim_start_matches("-L").to_string())
            };
            if let Some(d) = dir {
                let p = PathBuf::from(d);
                build.link_dirs.insert(if p.is_absolute() {
                    p
                } else {
                    workspace.join(p)
                });
            }
        } else if tok.starts_with("-l") {
            let lib = if tok == "-l" {
                iter.next().cloned()
            } else {
                Some(tok.trim_start_matches("-l").to_string())
            };
            if let Some(l) = lib {
                build.link_libs.insert(l);
            }
        }
    }
}

fn shell_like_split(s: &str) -> Vec<String> {
    // 简化的 shell 分词：处理引号包裹和转义空格
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_s = false;
    let mut in_d = false;
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            cur.push(ch);
            esc = false;
            continue;
        }
        match ch {
            '\\' => {
                esc = true;
            }
            '\'' => {
                if !in_d {
                    in_s = !in_s;
                } else {
                    cur.push(ch);
                }
            }
            '"' => {
                if !in_s {
                    in_d = !in_d;
                } else {
                    cur.push(ch);
                }
            }
            c if c.is_whitespace() && !in_s && !in_d => {
                if !cur.is_empty() {
                    out.push(cur.clone());
                    cur.clear();
                }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    // no extra imports needed

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn test_generate_relation_graph_basic() {
        let td = TempDir::new().unwrap();
        let root = td.path();

        // 构造简单工程
        let a_h = root.join("include/a.h");
        let b_h = root.join("src/b.h");
        let c_c = root.join("src/c.c");
        write(&a_h, "#pragma once\n");
        write(&b_h, "#pragma once\n");
        write(
            &c_c,
            "#include \"b.h\"\n#include <stdio.h>\n#include \"a.h\"\nint main(){return 0;}\n",
        );

        // compile_commands.json，提供 -I include
        let cc = serde_json::json!([{
            "directory": root.to_string_lossy(),
            "file": c_c.to_string_lossy(),
            "command": format!("clang -I {} -L /usr/lib -lm -c {}", root.join("include").to_string_lossy(), c_c.to_string_lossy())
        }]);
        write(
            &root.join("compile_commands.json"),
            &serde_json::to_string_pretty(&cc).unwrap(),
        );

        let rel = generate_c_dependency_graph(root).unwrap();
        // 写出的文件存在
        assert!(root.join("relation_graph.json").exists());

        // 校验构建信息
        assert!(
            rel.build
                .include_dirs
                .iter()
                .any(|p| p.ends_with("include"))
        );
        assert!(rel.build.link_libs.contains("m"));

        // 校验文件节点
        let key = c_c.canonicalize().unwrap();
        let node = rel
            .files
            .get(&key)
            .or_else(|| {
                // 有些平台 strip_prefix 后作为 key 存相对路径；尝试相对键
                let rel_key = key.strip_prefix(&rel.workspace).unwrap().to_path_buf();
                rel.files.get(&rel_key)
            })
            .expect("node for c.c");

        // 本地包含应包含 a.h b.h
        let locals: Vec<String> = node
            .local_includes
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert!(locals.contains(&"a.h".to_string()));
        assert!(locals.contains(&"b.h".to_string()));
        // 系统包含包含 stdio.h
        assert!(
            node.system_includes.iter().any(|s| s.contains("stdio.h"))
                || node.system_includes.contains("stdio.h")
        );
    }
}
