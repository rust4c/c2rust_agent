use slint_build::CompilerConfiguration;

fn main() {
    let c = CompilerConfiguration::new().with_style("cupertino".to_string());
    slint_build::compile_with_config("src/app.slint", c).unwrap();
}
