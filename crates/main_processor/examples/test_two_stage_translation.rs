use main_processor::process_single_path;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    env_logger::init();
    
    // 测试路径 - 修改为实际的测试项目路径
    let test_path = Path::new("./test-projects/translate_chibicc/src");
    
    if !test_path.exists() {
        eprintln!("❌ 测试路径不存在: {}", test_path.display());
        eprintln!("请修改 test_path 为实际的包含 C 文件的目录");
        return Ok(());
    }
    
    println!("🚀 开始测试两阶段翻译功能");
    println!("📁 测试路径: {}", test_path.display());
    println!("🔄 流程: C2Rust 自动翻译 → AI 代码优化");
    println!("⏱️  这可能需要几分钟时间...");
    
    match process_single_path(test_path).await {
        Ok(()) => {
            println!("✅ 两阶段翻译测试成功完成!");
            println!("📄 请检查输出目录中的结果:");
            println!("   - two-stage-translation/c2rust-output/     (C2Rust 原始输出)");
            println!("   - two-stage-translation/final-output/      (AI 优化后的结果)");
            println!("   - two-stage-translation/final-output/c2rust_original.rs (C2Rust 备份)");
        }
        Err(e) => {
            eprintln!("❌ 两阶段翻译测试失败: {}", e);
            eprintln!("请检查:");
            eprintln!("  1. C2Rust 工具是否已安装 (cargo install c2rust)");
            eprintln!("  2. 测试目录中是否包含 .c 或 .h 文件");
            eprintln!("  3. LLM API 配置是否正确");
        }
    }
    
    Ok(())
}