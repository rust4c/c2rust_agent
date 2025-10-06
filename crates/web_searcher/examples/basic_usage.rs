//! Basic usage example for the web_searcher crate
//!
//! This example demonstrates how to use the WebSearcher to automatically
//! extract keywords from Rust compilation errors and search for solutions.

use tokio;
use web_searcher::searcher::{search_rust_error, SearchRequest, WebSearcher};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🔍 Web Searcher Example - Intelligent Rust Error Search\n");

    // Example 1: Simple error search using convenience function
    println!("=== Example 1: Simple Error Search ===");
    let error_message = r#"
error[E0382]: use of moved value: `v`
  --> src/main.rs:5:9
   |
3  | let v = vec![1,2,3];
   |     - move occurs because `v` has type `Vec<i32>`, which does not implement the `Copy` trait
4  | let v2 = v;
   |         - value moved here
5  | println!("{:?}", v);
   |                  ^ value used here after move
"#;

    let code_context = r#"
fn main() {
    let v = vec![1,2,3];
    let v2 = v;
    println!("{:?}", v);
}
"#;

    match search_rust_error(error_message.to_string(), Some(code_context.to_string())).await {
        Ok(response) => {
            println!("✅ Search completed in {}ms", response.total_time_ms);
            println!("🔑 Extracted {} keywords:", response.keywords.len());
            for (i, keyword) in response.keywords.iter().enumerate() {
                println!(
                    "  {}. {} ({}): {:.2}",
                    i + 1,
                    keyword.keyword,
                    keyword.category,
                    keyword.relevance
                );
            }

            println!("\n📋 Found {} search results:", response.results.len());
            for (i, result) in response.results.iter().enumerate() {
                println!("  {}. {}", i + 1, result.title);
                println!("     URL: {}", result.url);
                println!(
                    "     Snippet: {}",
                    result.snippet.chars().take(100).collect::<String>()
                );
                if result.snippet.len() > 100 {
                    println!("...");
                }
                println!("     Relevance: {:.2}\n", result.relevance_score);
            }
        }
        Err(e) => {
            eprintln!("❌ Search failed: {}", e);
        }
    }

    // Example 2: Advanced usage with WebSearcher instance
    println!("\n=== Example 2: Advanced Usage ===");
    match WebSearcher::new().await {
        Ok(searcher) => {
            let request = SearchRequest {
                error_message: r#"
error[E0277]: `std::rc::Rc<std::cell::RefCell<i32>>` cannot be sent between threads safely
  --> src/main.rs:10:5
   |
10 |     thread::spawn(move || {
   |     ^^^^^^^^^^^^^ `std::rc::Rc<std::cell::RefCell<i32>>` cannot be sent between threads safely
   |
   = help: within `[closure@src/main.rs:10:19: 12:6]`, the trait `Send` is not implemented for `std::rc::Rc<std::cell::RefCell<i32>>`
"#.to_string(),
                code_context: Some(r#"
use std::rc::Rc;
use std::cell::RefCell;
use std::thread;

fn main() {
    let data = Rc::new(RefCell::new(42));
    let data_clone = data.clone();

    thread::spawn(move || {
        *data_clone.borrow_mut() = 100;
    });
}
"#.to_string()),
                project_context: Some("Using Rc<RefCell<T>> in multi-threaded context".to_string()),
            };

            match searcher.search_error(request).await {
                Ok(response) => {
                    println!("✅ Advanced search completed successfully!");
                    println!("🔍 Engine used: {}", response.engine_used);
                    println!("⏱️  Time taken: {}ms", response.total_time_ms);

                    println!("\n🎯 Top keywords identified:");
                    for keyword in response.keywords.iter().take(3) {
                        println!(
                            "  • {} ({}) - relevance: {:.2}",
                            keyword.keyword, keyword.category, keyword.relevance
                        );
                    }

                    if !response.results.is_empty() {
                        println!("\n🔗 Best search result:");
                        let best_result = &response.results[0];
                        println!("  Title: {}", best_result.title);
                        println!("  URL: {}", best_result.url);
                        println!("  Relevance: {:.2}", best_result.relevance_score);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Advanced search failed: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Failed to initialize WebSearcher: {}", e);
        }
    }

    // Example 3: Demonstrating different error types
    println!("\n=== Example 3: Different Error Types ===");
    let error_examples = vec![
        (
            "Lifetime Error",
            r#"error[E0621]: explicit lifetime required in the return type"#,
            "fn get_str() -> &str { \"hello\" }",
        ),
        (
            "Trait Bound Error",
            r#"error[E0277]: the trait bound `T: std::fmt::Display` is not satisfied"#,
            "fn print_it<T>(x: T) { println!(\"{}\", x); }",
        ),
        (
            "Async Error",
            r#"error[E0277]: `dyn Future` cannot be sent between threads safely"#,
            "async fn example() { tokio::spawn(async { /* ... */ }); }",
        ),
    ];

    for (name, error, code) in error_examples {
        println!("🧪 Testing: {}", name);
        match search_rust_error(error.to_string(), Some(code.to_string())).await {
            Ok(response) => {
                println!(
                    "  ✅ Found {} keywords in {}ms",
                    response.keywords.len(),
                    response.total_time_ms
                );
                if let Some(first_keyword) = response.keywords.first() {
                    println!(
                        "  🔑 Top keyword: {} ({})",
                        first_keyword.keyword, first_keyword.category
                    );
                }
            }
            Err(e) => {
                println!("  ❌ Failed: {}", e);
            }
        }
    }

    println!("\n🎉 All examples completed!");
    Ok(())
}
