use relation_analy::generate_c_dependency_graph;
use std::env;
use std::path::PathBuf;

fn print_usage() {
    eprintln!("Usage: relation_analyzer [WORKSPACE_DIR]\n\n");
    eprintln!(
        "Generate C/C++ file dependency graph and write relation_graph.json at workspace root."
    );
    eprintln!("- WORKSPACE_DIR: optional, defaults to current directory (.)\n");
    eprintln!("Examples:");
    eprintln!("  cargo run -p relation_analy --example relation_analyzer -- .");
    eprintln!("  cargo run -p relation_analy --example relation_analyzer -- /path/to/workspace");
}

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return;
    }

    let workspace = if let Some(p) = args.get(0) {
        PathBuf::from(p)
    } else {
        PathBuf::from(".")
    };

    match generate_c_dependency_graph(&workspace) {
        Ok(rel) => {
            println!(
                "✅ Relation graph generated at: {}",
                rel.workspace.join("relation_graph.json").display()
            );
            println!("Files: {}", rel.files.len());
            let include_edges: usize = rel
                .files
                .values()
                .map(|n| n.local_includes.len() + n.system_includes.len())
                .sum();
            println!("Include edges: {}", include_edges);
            println!(
                "Include dirs: {} | Link libs: {} | Link dirs: {}",
                rel.build.include_dirs.len(),
                rel.build.link_libs.len(),
                rel.build.link_dirs.len()
            );
        }
        Err(e) => {
            eprintln!("❌ Failed to generate relation graph: {e}");
            std::process::exit(1);
        }
    }
}
