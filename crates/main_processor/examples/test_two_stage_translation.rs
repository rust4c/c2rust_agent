use main_processor::process_single_path;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Test path - modify to actual test project path
    let test_path = Path::new("./test-projects/translate_chibicc/src");

    if !test_path.exists() {
        eprintln!("❌ Test path does not exist: {}", test_path.display());
        eprintln!("Please modify test_path to actual directory containing C files");
        return Ok(());
    }

    println!("🚀 Starting two-stage translation functionality test");
    println!("📁 Test path: {}", test_path.display());
    println!("🔄 Process: C2Rust automatic translation → AI code optimization");
    println!("⏱️  This may take a few minutes...");

    match process_single_path(test_path).await {
        Ok(()) => {
            println!("✅ Two-stage translation test completed successfully!");
            println!("📄 Please check the results in the output directory:");
            println!("   - two-stage-translation/c2rust-output/     (C2Rust original output)");
            println!("   - two-stage-translation/final-output/      (AI optimized results)");
            println!("   - two-stage-translation/final-output/c2rust_original.rs (C2Rust backup)");
        }
        Err(e) => {
            eprintln!("❌ Two-stage translation test failed: {}", e);
            eprintln!("Please check:");
            eprintln!("  1. Is C2Rust tool installed (cargo install c2rust)");
            eprintln!("  2. Does test directory contain .c or .h files");
            eprintln!("  3. Is LLM API configuration correct");
        }
    }

    Ok(())
}
