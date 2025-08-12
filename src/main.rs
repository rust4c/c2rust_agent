use ::lsp_services::lsp_services::check_function_and_class_name;
use anyhow::Result;
use env_logger;
use log::LevelFilter;

fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(LevelFilter::Debug)
        .init();

    let project_path = "~/Documents/AppCode/Rust/contest/translate_chibicc";
    check_function_and_class_name(project_path, false)?;
    Ok(())
}
