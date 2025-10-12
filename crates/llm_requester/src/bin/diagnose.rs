//! LLM Configuration Diagnostic Tool
//!
//! This tool helps diagnose common configuration issues with the LLM requester.
//! Run with: cargo run --bin diagnose

use llm_requester::{
    diagnose_config_issues, llm_request_with_retry, print_setup_instructions, test_llm_connection,
    validate_llm_config,
};
use std::env;
use std::process;
use tokio::time::{Duration, timeout};

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
                println!("🔗 Testing LLM connection...");
                run_connection_test().await;
                return;
            }
            "--validate" => {
                println!("✅ Validating configuration...");
                match validate_llm_config().await {
                    Ok(_) => println!("✓ Configuration validation successful!"),
                    Err(e) => {
                        println!("✗ Configuration validation failed: {}", e);
                        process::exit(1);
                    }
                }
                return;
            }
            "--network" => {
                println!("🌐 Running network diagnostics...");
                run_network_diagnostics().await;
                return;
            }
            "--full-test" => {
                println!("🧪 Running comprehensive LLM test...");
                run_full_test().await;
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

            // If there are any ✗ marks, suggest next steps
            if report.contains("✗") {
                println!("\n❗ Issues detected. Available options:");
                println!("  --setup     Show setup instructions");
                println!("  --network   Test network connectivity");
                println!("  --test      Test LLM connection");
                println!("  --full-test Run comprehensive test");
                process::exit(1);
            } else {
                println!("\n✅ All checks passed! Try --full-test for a comprehensive test.");
            }
        }
        Err(e) => {
            println!("❌ Failed to run diagnostics: {}", e);
            println!("\nRun with --setup for setup instructions.");
            process::exit(1);
        }
    }
}

async fn run_connection_test() {
    match timeout(Duration::from_secs(30), test_llm_connection()).await {
        Ok(Ok(_)) => {
            println!("✓ Connection test successful!");
            println!("✓ Your LLM provider is working correctly");
        }
        Ok(Err(e)) => {
            println!("✗ Connection test failed:");
            analyze_connection_error(&e).await;
            process::exit(1);
        }
        Err(_) => {
            println!("✗ Connection test timed out after 30 seconds");
            println!("💡 This suggests network connectivity issues or API service problems");
            process::exit(1);
        }
    }
}

async fn run_network_diagnostics() {
    println!("Testing network connectivity...\n");

    // Test basic internet connectivity
    println!("📡 Testing internet connectivity...");
    if test_internet_connectivity().await {
        println!("✓ Internet connection working");
    } else {
        println!("✗ No internet connection detected");
        println!("💡 Please check your network connection");
        return;
    }

    // Test DNS resolution
    println!("🔍 Testing DNS resolution...");
    test_dns_resolution().await;

    // Test API endpoints
    println!("🌐 Testing API endpoint accessibility...");
    test_api_endpoints().await;
}

async fn run_full_test() {
    println!("Running comprehensive LLM functionality test...\n");

    // First validate config
    match validate_llm_config().await {
        Ok(_) => println!("✓ Configuration validation passed"),
        Err(e) => {
            println!("✗ Configuration validation failed: {}", e);
            process::exit(1);
        }
    }

    // Test simple request
    println!("🔤 Testing simple LLM request...");
    let test_messages = vec!["Say 'Hello World' in response.".to_string()];

    match timeout(
        Duration::from_secs(60),
        llm_request_with_retry(test_messages, 2),
    )
    .await
    {
        Ok(Ok(response)) => {
            println!("✓ Simple request successful!");
            println!(
                "📝 Response: {}",
                response.chars().take(100).collect::<String>()
            );
            if response.len() > 100 {
                println!("   ... (truncated)");
            }
        }
        Ok(Err(e)) => {
            println!("✗ Simple request failed:");
            analyze_connection_error(&e).await;
            process::exit(1);
        }
        Err(_) => {
            println!("✗ Request timed out after 60 seconds");
            process::exit(1);
        }
    }

    println!("\n🎉 All tests passed! Your LLM setup is working correctly.");
}

async fn analyze_connection_error(error: &anyhow::Error) {
    let error_str = error.to_string();

    println!("  Error: {}", error_str);
    println!("\n🔍 Analysis:");

    if error_str.contains("error decoding response body") {
        println!("💡 Response decoding error suggests:");
        println!("   • Network interruption during request");
        println!("   • Invalid or expired API key");
        println!("   • API service returning unexpected format");
        println!("   • Rate limiting or quota exceeded");
        println!("\n🛠️  Suggested fixes:");
        println!("   1. Check your API key is valid and not expired");
        println!("   2. Try again in a few minutes (rate limiting)");
        println!("   3. Check your network connection stability");
        println!("   4. Verify API key format (DeepSeek: sk-xxx, OpenAI: sk-xxx)");
    } else if error_str.contains("401") {
        println!("💡 Authentication error (401):");
        println!("   • Invalid API key");
        println!("   • Expired API key");
        println!("   • Incorrect API key format");
        println!("\n🛠️  Fix: Update your API key in config/config.toml");
    } else if error_str.contains("429") {
        println!("💡 Rate limit exceeded (429):");
        println!("   • Too many requests in a short time");
        println!("   • API quota exceeded");
        println!("\n🛠️  Fix: Wait a few minutes before retrying");
    } else if error_str.contains("timeout") {
        println!("💡 Request timeout:");
        println!("   • Slow network connection");
        println!("   • API service overloaded");
        println!("   • Firewall blocking connection");
        println!("\n🛠️  Fix: Check network connection and firewall settings");
    } else {
        println!("💡 General troubleshooting:");
        println!("   • Verify network connectivity");
        println!("   • Check API key configuration");
        println!("   • Try a different provider (e.g., switch to Ollama for local testing)");
    }
}

async fn test_internet_connectivity() -> bool {
    match timeout(
        Duration::from_secs(10),
        tokio::process::Command::new("ping")
            .arg("-c")
            .arg("1")
            .arg("8.8.8.8")
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => output.status.success(),
        _ => false,
    }
}

async fn test_dns_resolution() {
    let hosts = ["api.deepseek.com", "api.openai.com", "www.google.com"];

    for host in hosts {
        match timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host(format!("{}:443", host)),
        )
        .await
        {
            Ok(Ok(_)) => println!("✓ DNS resolution for {} working", host),
            Ok(Err(_)) => println!("✗ DNS resolution failed for {}", host),
            Err(_) => println!("⏱️  DNS lookup timed out for {}", host),
        }
    }
}

async fn test_api_endpoints() {
    println!("🔗 Testing HTTPS connectivity to API endpoints...");

    let endpoints = [
        ("DeepSeek API", "https://api.deepseek.com"),
        ("OpenAI API", "https://api.openai.com"),
    ];

    for (name, url) in endpoints {
        match timeout(Duration::from_secs(10), reqwest::get(url)).await {
            Ok(Ok(response)) => {
                println!("✓ {} accessible (status: {})", name, response.status());
            }
            Ok(Err(e)) => {
                println!("✗ {} not accessible: {}", name, e);
            }
            Err(_) => {
                println!("⏱️  {} connection timed out", name);
            }
        }
    }
}

fn print_help() {
    println!("🔧 LLM Configuration Diagnostic Tool");
    println!();
    println!("USAGE:");
    println!("    cargo run --bin diagnose [OPTION]");
    println!();
    println!("OPTIONS:");
    println!("    (none)       Run full configuration diagnostics");
    println!("    --setup      Show detailed setup instructions");
    println!("    --validate   Validate configuration only");
    println!("    --test       Test connection to LLM provider");
    println!("    --network    Run network connectivity diagnostics");
    println!("    --full-test  Run comprehensive LLM functionality test");
    println!("    --help, -h   Show this help message");
    println!();
    println!("EXAMPLES:");
    println!("    cargo run --bin diagnose             # Quick diagnostics");
    println!("    cargo run --bin diagnose --setup     # Setup guide");
    println!("    cargo run --bin diagnose --network   # Network tests");
    println!("    cargo run --bin diagnose --full-test # Complete test");
    println!();
    println!("TROUBLESHOOTING:");
    println!("    If you see 'error decoding response body':");
    println!("    1. Run --network to check connectivity");
    println!("    2. Verify your API key is valid");
    println!("    3. Try --full-test for detailed analysis");
}
