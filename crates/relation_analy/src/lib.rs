use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Dependency graph node: a C/C++ source file or header file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileNode {
    pub path: PathBuf,
    /// Directly dependent local files (actual paths resolved through quoted includes)
    pub local_includes: BTreeSet<PathBuf>,
    /// Directly dependent system/third-party headers (through angle bracket includes or unresolvable quoted includes)
    pub system_includes: BTreeSet<String>,
}

/// Project-level dependency information (from compile_commands.json)
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BuildDeps {
    /// -I include directories
    pub include_dirs: BTreeSet<PathBuf>,
    /// -l link libraries (e.g. m, pthread)
    pub link_libs: BTreeSet<String>,
    /// -L link directories
    pub link_dirs: BTreeSet<PathBuf>,
}

/// Complete relationship file format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelationFile {
    pub workspace: PathBuf,
    pub files: BTreeMap<PathBuf, FileNode>,
    pub build: BuildDeps,
    pub generated_at: String,
}

/// Contract description
/// Input: workspace root path
/// Output: write relation_graph.json in workspace root directory, return RelationFile memory object
pub fn generate_c_dependency_graph(workspace_root: &Path) -> Result<RelationFile> {
    let workspace_root = workspace_root
        .canonicalize()
        .with_context(|| format!("Invalid workspace path: {}", workspace_root.display()))?;

    // 1) Enumerate C/C++ related files in the project
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

    // 2) Include parsing: regex scan #include lines in each file
    let re = Regex::new(r#"(?m)^\s*#\s*include\s*([<"])([^>"]+)[>"]"#).unwrap();
    let mut nodes: BTreeMap<PathBuf, FileNode> = BTreeMap::new();

    // Candidate local search paths: workspace root + subdirectories (will be merged with -I from compile_commands later)
    let mut local_search_roots: Vec<PathBuf> = vec![workspace_root.clone()];

    for file in &all_files {
        // Record parent directories for nearby resolution of relative includes
        if let Some(parent) = file.parent() {
            if !local_search_roots.contains(&parent.to_path_buf()) {
                local_search_roots.push(parent.to_path_buf());
            }
        }
    }

    let _files_set: BTreeSet<PathBuf> = all_files.iter().cloned().collect();

    for file in &all_files {
        let content = fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {}", file.display()))?;
        let mut local_includes: BTreeSet<PathBuf> = BTreeSet::new();
        let mut system_includes: BTreeSet<String> = BTreeSet::new();

        for cap in re.captures_iter(&content) {
            let delimiter = cap.get(1).unwrap().as_str();
            let target = cap.get(2).unwrap().as_str().trim();

            match delimiter {
                "\"" => {
                    // First resolve as relative path nearby
                    let resolved = resolve_local_include(file, target, &local_search_roots);
                    if let Some(p) = resolved {
                        local_includes.insert(p);
                    } else {
                        // Cannot resolve within project, treat as third-party/system header
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

    // 3) Parse compile_commands.json (if exists), aggregate -I, -l, -L, and extend local search paths
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

    // After extending search paths, try to complete previously unresolved local includes (optional, keep simple without second scan)
    // To keep complexity manageable, no second parsing is done here to avoid loops and multiple IO operations.

    // 4) Write JSON file
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
    // Relative to current file
    if let Some(parent) = current_file.parent() {
        let p = parent.join(target);
        if p.exists() {
            return Some(p);
        }
    }
    // Other search roots
    for root in search_roots {
        let p = root.join(target);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn parse_build_flags(cmd: &str, build: &mut BuildDeps, workspace: &Path) {
    // Crude but clear enough parsing: split by whitespace, handle -I -L -l three categories
    // This is not shell-level parsing, but sufficient to cover common compile_commands
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
    // Simplified shell tokenization: handle quoted wrapping and escaped spaces
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

        // Construct simple project
        let a_h = root.join("include/a.h");
        let b_h = root.join("src/b.h");
        let c_c = root.join("src/c.c");
        write(&a_h, "#pragma once\n");
        write(&b_h, "#pragma once\n");
        write(
            &c_c,
            "#include \"b.h\"\n#include <stdio.h>\n#include \"a.h\"\nint main(){return 0;}\n",
        );

        // compile_commands.json, provide -I include
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
        // Written file exists
        assert!(root.join("relation_graph.json").exists());

        // Verify build information
        assert!(
            rel.build
                .include_dirs
                .iter()
                .any(|p| p.ends_with("include"))
        );
        assert!(rel.build.link_libs.contains("m"));

        // Verify file nodes
        let key = c_c.canonicalize().unwrap();
        let node = rel
            .files
            .get(&key)
            .or_else(|| {
                // Some platforms store relative paths as keys after strip_prefix; try relative key
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
