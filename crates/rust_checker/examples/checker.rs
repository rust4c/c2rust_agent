//! 使用方式:
//! ```bash
//! cargo run --package rust_checker --example checker -- /path/to/project
//! cargo run --package rust_checker --example checker -- --workspace /path/to/workspace
//! ```

use rust_checker::RustCodeCheck;

fn print_usage(program_name: &str) {
    eprintln!(
        "用法: {program_name} [选项] <项目路径>\n\
         选项:\n\
           -w, --workspace    构建整个 workspace\n\
           -h, --help         显示帮助信息"
    );
}

fn main() {
    let mut args = std::env::args();
    let program_name = args.next().unwrap_or_else(|| String::from("checker"));

    let mut workspace_mode = false;
    let mut project_dir: Option<String> = None;

    for arg in args {
        match arg.as_str() {
            "-w" | "--workspace" => workspace_mode = true,
            "-h" | "--help" => {
                print_usage(&program_name);
                return;
            }
            _ if arg.starts_with('-') => {
                eprintln!("未知选项: {arg}");
                print_usage(&program_name);
                std::process::exit(2);
            }
            _ => {
                if project_dir.is_some() {
                    eprintln!("仅支持一个项目路径参数");
                    print_usage(&program_name);
                    std::process::exit(2);
                }
                project_dir = Some(arg);
            }
        }
    }

    let project_dir = match project_dir {
        Some(dir) => dir,
        None => {
            eprintln!("Missing project path parameter");
            print_usage(&program_name);
            std::process::exit(1);
        }
    };

    let checker = RustCodeCheck::new(&project_dir);
    let result = if workspace_mode {
        checker.check_workspace()
    } else {
        checker.check_rust_project()
    };

    match result {
        Ok(_) => {
            if workspace_mode {
                println!("✅ Workspace check and build successful");
            } else {
                println!("✅ Project check and compilation successful");
            }
        }
        Err(e) => {
            eprintln!("❌ Check failed: {e}");
            std::process::exit(1);
        }
    }
}
