//! LLM Configuration Diagnostic Tool
//!
//! This tool helps diagnose common configuration issues with the LLM requester.
//! Run with: cargo run --bin diagnose

use llm_requester::{
    diagnose_config_issues, print_setup_instructions, test_llm_connection, validate_llm_config,
};
use std::env;

#[tokio::main]
async fn main() {
    // Initialize simple logging
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "--help" | "-h" => {
                print_help();
                return;
            }
            "--setup" => {
                print_setup_instructions();
                return;
            }
            "--test" => {
                println!("Testing LLM connection...");
                match test_llm_connection().await {
                    Ok(_) => println!("✓ Connection test successful!"),
                    Err(e) => {
                        println!("✗ Connection test failed: {}", e);
                        std::process::exit(1);
                    }
                }
                return;
            }
            "--validate" => {
                println!("Validating configuration...");
                match validate_llm_config().await {
                    Ok(_) => println!("✓ Configuration validation successful!"),
                    Err(e) => {
                        println!("✗ Configuration validation failed: {}", e);
                        std::process::exit(1);
                    }
                }
                return;
            }
            _ => {
                println!("Unknown option: {}", args[1]);
                print_help();
                return;
            }
        }
    }

    // Default: run full diagnostics
    println!("🔍 Running LLM configuration diagnostics...\n");

    match diagnose_config_issues().await {
        Ok(report) => {
            println!("{}", report);

            // If there are any ✗ marks, suggest setup
            if report.contains("✗") {
                println!("\n❗ Issues detected. Run with --setup for instructions.");
                std::process::exit(1);
            } else {
                println!("\n✅ All checks passed!");
            }
        }
        Err(e) => {
            println!("❌ Failed to run diagnostics: {}", e);
            println!("\nRun with --setup for setup instructions.");
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!("LLM Configuration Diagnostic Tool");
    println!();
    println!("USAGE:");
    println!("    cargo run --bin diagnose [OPTION]");
    println!();
    println!("OPTIONS:");
    println!("    (none)      Run full configuration diagnostics");
    println!("    --setup     Show setup instructions");
    println!("    --validate  Validate configuration only");
    println!("    --test      Test connection to LLM provider");
    println!("    --help, -h  Show this help message");
    println!();
    println!("EXAMPLES:");
    println!("    cargo run --bin diagnose          # Run full diagnostics");
    println!("    cargo run --bin diagnose --setup  # Show setup instructions");
    println!("    cargo run --bin diagnose --test   # Test connection");
}
