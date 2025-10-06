//! Complete Workflow Example - Full Search and Page Processing Pipeline
//!
//! This example demonstrates the complete workflow:
//! 1. Search for Rust error solutions
//! 2. Fetch web page content
//! 3. Process content with AI
//! 4. Present structured results

use tokio;
use web_searcher::{solve_rust_error, PageFetcher, SearchRequest, WebSearcher};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🔄 Complete Workflow Demo - From Error to Solution\n");

    // Test case 1: Ownership error (E0382)
    println!("=== Test Case 1: Ownership Error (E0382) ===");
    let ownership_error = r#"
error[E0382]: use of moved value: `data`
  --> src/main.rs:8:20
   |
6  | let data = vec![1, 2, 3, 4, 5];
   |     ---- move occurs because `data` has type `Vec<i32>`, which does not implement the `Copy` trait
7  | let processed = process_data(data);
   |                              ---- value moved here
8  | println!("Original: {:?}", data);
   |                            ^^^^ value used here after move
"#;

    let _ownership_code = r#"
fn process_data(data: Vec<i32>) -> Vec<i32> {
    data.iter().map(|x| x * 2).collect()
}

fn main() {
    let data = vec![1, 2, 3, 4, 5];
    let processed = process_data(data);
    println!("Processed: {:?}", processed);
    println!("Original: {:?}", data);  // Error here!
}
"#;

    match solve_rust_error(ownership_error).await {
        Ok(solution) => {
            println!("✅ Found {} solutions", solution.solutions.len());

            for (i, result) in solution.solutions.iter().enumerate().take(3) {
                println!("\n--- Solution {} ---", i + 1);
                println!("📄 Title: {}", result.title);
                println!("🔗 URL: {:?}", result.source_url);
                println!("⭐ Effectiveness: {:.2}", result.effectiveness_score);
                println!("🎯 Difficulty: {:?}", result.difficulty);

                println!("\n📋 Summary:");
                let summary = if result.description.len() > 200 {
                    format!("{}...", &result.description[..200])
                } else {
                    result.description.clone()
                };
                println!("{}", summary);
            }
        }
        Err(e) => {
            println!("❌ Workflow failed: {}", e);
        }
    }

    // Test case 2: Trait bound error (E0277)
    println!("\n\n=== Test Case 2: Trait Bound Error (E0277) ===");
    let trait_error = r#"
error[E0277]: the trait bound `T: std::fmt::Display` is not satisfied
  --> src/main.rs:2:20
   |
1  | fn print_value<T>(value: T) {
   |                - help: consider restricting this bound: `T: std::fmt::Display`
2  |     println!("{}", value);
   |                    ^^^^^ `T` cannot be formatted with the default formatter
   |
   = help: the trait `std::fmt::Display` is not implemented for `T`
"#;

    let _trait_code = r#"
fn print_value<T>(value: T) {
    println!("{}", value);
}

fn main() {
    let number = 42;
    print_value(number);

    let my_struct = MyStruct { data: 100 };
    print_value(my_struct); // This will fail
}

struct MyStruct {
    data: i32,
}
"#;

    match solve_rust_error(trait_error).await {
        Ok(solution) => {
            println!("✅ Found {} solutions", solution.solutions.len());

            for (i, result) in solution.solutions.iter().enumerate().take(2) {
                println!("\n--- Solution {} ---", i + 1);
                println!("📄 Title: {}", result.title);
                println!(
                    "⭐ Effectiveness: {:.2} | 🎯 Difficulty: {:?}",
                    result.effectiveness_score, result.difficulty
                );

                println!("\n🔍 Error Analysis:");
                println!("  Category: {}", solution.error_info.error_category);
                println!(
                    "  Original Error: {}",
                    solution
                        .error_info
                        .original_message
                        .lines()
                        .next()
                        .unwrap_or("Unknown")
                );
            }
        }
        Err(e) => {
            println!("❌ Workflow failed: {}", e);
        }
    }

    // Test case 3: Manual step-by-step workflow
    println!("\n\n=== Test Case 3: Manual Step-by-Step Workflow ===");

    let async_error = r#"
error[E0277]: `dyn Future<Output = ()>` cannot be sent between threads safely
  --> src/main.rs:10:5
   |
10 |     tokio::spawn(async move {
   |     ^^^^^^^^^^^^
   |
   = help: the trait `Send` is not implemented for `dyn Future<Output = ()>`
"#;

    // Step 1: Search for solutions
    println!("Step 1: Searching for solutions...");
    let searcher = WebSearcher::new().await?;
    let search_request = SearchRequest {
        error_message: async_error.to_string(),
        code_context: Some("tokio::spawn(async move { /* async code */ });".to_string()),
        project_context: Some("Using tokio for async programming".to_string()),
    };

    let search_response = searcher.search_error(search_request).await?;
    println!(
        "✅ Found {} search results with {} keywords",
        search_response.results.len(),
        search_response.keywords.len()
    );

    println!("\n🔑 Top Keywords:");
    for (i, keyword) in search_response.keywords.iter().take(5).enumerate() {
        println!(
            "  {}. {} ({}) - {:.2}",
            i + 1,
            keyword.keyword,
            keyword.category,
            keyword.relevance
        );
    }

    // Step 2: Fetch and process pages
    println!("\nStep 2: Fetching and processing web pages...");
    let page_fetcher = PageFetcher::new().await?;
    let processed_results = page_fetcher
        .process_search_results(search_response.results, async_error)
        .await?;

    println!("✅ Processed {} web pages", processed_results.len());

    // Step 3: Present results
    println!("\nStep 3: Presenting structured results...");
    for (i, result) in processed_results.iter().enumerate() {
        println!("\n🌐 Result {} - {}", i + 1, result.title);
        println!("  URL: {}", result.url);
        println!("  Source Type: Based on URL analysis");
        println!(
            "  Relevance: {:.2} | Confidence: {:.2}",
            result.relevance_score, result.confidence_score
        );

        if !result.solution_summary.is_empty() {
            println!("\n  📋 Summary:");
            let lines: Vec<&str> = result.solution_summary.lines().take(3).collect();
            for line in lines {
                if !line.trim().is_empty() {
                    println!("    {}", line.trim());
                }
            }
        }
    }

    // Performance summary
    println!("\n\n=== Performance Summary ===");
    if !processed_results.is_empty() {
        let total_processing_time: u64 =
            processed_results.iter().map(|r| r.processing_time_ms).sum();
        let avg_processing_time = total_processing_time / processed_results.len() as u64;

        println!("Total pages processed: {}", processed_results.len());
        println!(
            "Average processing time: {}ms per page",
            avg_processing_time
        );
        println!("Total processing time: {}ms", total_processing_time);
    }

    // Best practices demonstration
    println!("\n\n=== Best Practices Demonstration ===");

    // Example: How to filter results by confidence
    let high_confidence_results: Vec<_> = processed_results
        .iter()
        .filter(|r| r.confidence_score > 0.7)
        .collect();
    println!(
        "High confidence results (>0.7): {}",
        high_confidence_results.len()
    );

    // Example: How to find code solutions
    let code_solutions: Vec<_> = processed_results
        .iter()
        .filter(|r| !r.code_examples.is_empty())
        .collect();
    println!("Results with code examples: {}", code_solutions.len());

    // Example: How to prioritize official documentation
    let official_docs: Vec<_> = processed_results
        .iter()
        .filter(|r| r.url.contains("doc.rust-lang.org"))
        .collect();
    println!(
        "Official Rust documentation results: {}",
        official_docs.len()
    );

    println!("\n🎉 Complete workflow demonstration finished!");
    println!("\n💡 Tips for integration:");
    println!("  • Use search_and_fetch_solutions() for simple cases");
    println!("  • Use manual step-by-step for fine-grained control");
    println!("  • Filter results by confidence_score and relevance_score");
    println!("  • Prioritize results with code_examples for practical solutions");
    println!("  • Cache processed results to avoid repeated API calls");

    Ok(())
}
