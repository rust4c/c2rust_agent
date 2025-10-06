//! Unified Interface Example - One Function to Solve All Rust Errors
//!
//! This example demonstrates the main unified interface:
//! solve_rust_error() - Input error message, get complete solution
//!
//! ## Linus's Philosophy
//! "The best interface is no interface" - One function does everything you need.

use tokio;
use web_searcher::{
    explain_rust_error, get_quick_solutions, solve_rust_error, ConfidenceLevel, Difficulty,
    FixType, ResourceType,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("🎯 Unified Interface Demo - One Function to Rule Them All\n");

    // Example 1: Complete solution for ownership error
    println!("=== Example 1: Complete Ownership Error Solution ===");
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

    match solve_rust_error(ownership_error).await {
        Ok(solution) => {
            println!("✅ COMPLETE SOLUTION FOUND");

            // Error Analysis
            println!("\n🔍 ERROR ANALYSIS:");
            println!("  Error Code: {:?}", solution.error_info.error_code);
            println!("  Category: {}", solution.error_info.error_category);
            println!(
                "  Analysis Confidence: {:.1}%",
                solution.error_info.confidence * 100.0
            );

            // Solutions
            println!("\n🛠️ SOLUTIONS ({} found):", solution.solutions.len());
            for (i, sol) in solution.solutions.iter().enumerate().take(3) {
                println!("  {}. {} ({:?})", i + 1, sol.title, sol.difficulty);
                println!("     Fix Type: {:?}", sol.fix_type);
                println!(
                    "     Effectiveness: {:.1}%",
                    sol.effectiveness_score * 100.0
                );

                let description = if sol.description.len() > 100 {
                    let mut end = 100;
                    while end > 0 && !sol.description.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}...", &sol.description[..end])
                } else {
                    sol.description.clone()
                };
                println!("     Description: {}", description);
                println!();
            }

            // Code Examples
            if !solution.code_examples.is_empty() {
                println!("💻 CODE EXAMPLES ({}):", solution.code_examples.len());
                for (i, example) in solution.code_examples.iter().enumerate().take(2) {
                    println!("  {}. {}", i + 1, example.title);
                    println!("     Complete Solution: {}", example.is_complete_solution);
                    if example.after_code.len() > 80 {
                        let mut end = 80;
                        while end > 0 && !example.after_code.is_char_boundary(end) {
                            end -= 1;
                        }
                        println!("     Code: {}...", &example.after_code[..end]);
                    } else {
                        println!("     Code: {}", example.after_code);
                    }
                    println!();
                }
            }

            // Learning Resources
            if !solution.learning_resources.is_empty() {
                println!(
                    "📚 LEARNING RESOURCES ({}):",
                    solution.learning_resources.len()
                );
                for (i, resource) in solution.learning_resources.iter().enumerate().take(3) {
                    println!(
                        "  {}. {} ({:?})",
                        i + 1,
                        resource.title,
                        resource.resource_type
                    );
                    println!("     URL: {}", resource.url);
                    println!("     Relevance: {:.1}%", resource.relevance_score * 100.0);
                    println!();
                }
            }

            // Metadata
            println!("📊 PROCESSING METADATA:");
            println!(
                "  Sources Analyzed: {}",
                solution.metadata.total_sources_analyzed
            );
            println!(
                "  Processing Time: {}ms",
                solution.metadata.processing_time_ms
            );
            println!(
                "  Confidence Level: {:?}",
                solution.metadata.confidence_level
            );
            if !solution.metadata.search_keywords_used.is_empty() {
                println!(
                    "  Keywords Used: {:?}",
                    solution.metadata.search_keywords_used
                );
            }
        }
        Err(e) => {
            println!("❌ Failed to solve error: {}", e);
        }
    }

    // Example 2: Quick solutions for trait bound error
    println!("\n\n=== Example 2: Quick Solutions for Trait Bound Error ===");
    let trait_error = r#"
error[E0277]: the trait bound `T: std::fmt::Display` is not satisfied
  --> src/main.rs:2:20
   |
1  | fn print_value<T>(value: T) {
   |                - help: consider restricting this bound: `T: std::fmt::Display`
2  |     println!("{}", value);
   |                    ^^^^^ `T` cannot be formatted with the default formatter
"#;

    match get_quick_solutions(trait_error).await {
        Ok(quick_solutions) => {
            println!("⚡ QUICK SOLUTIONS ({} found):", quick_solutions.len());
            for (i, solution) in quick_solutions.iter().enumerate() {
                let difficulty_icon = match solution.difficulty {
                    Difficulty::Beginner => "🟢",
                    Difficulty::Intermediate => "🟡",
                    Difficulty::Advanced => "🔴",
                };

                let fix_type_icon = match solution.fix_type {
                    FixType::CodeChange => "💻",
                    FixType::DependencyUpdate => "📦",
                    FixType::CompilerFlag => "🚩",
                    FixType::Configuration => "⚙️",
                    FixType::Architecture => "🏗️",
                };

                println!(
                    "  {}. {} {} {}",
                    i + 1,
                    difficulty_icon,
                    fix_type_icon,
                    solution.title
                );
                println!(
                    "     Effectiveness: {:.1}%",
                    solution.effectiveness_score * 100.0
                );

                // Show first sentence of description
                let first_sentence = solution
                    .description
                    .split('.')
                    .next()
                    .unwrap_or(&solution.description);
                println!("     Summary: {}", first_sentence);
                println!();
            }
        }
        Err(e) => {
            println!("❌ Failed to get quick solutions: {}", e);
        }
    }

    // Example 3: Learning resources for async error
    println!("\n\n=== Example 3: Learning Resources for Async Error ===");
    let async_error = r#"
error[E0277]: `Rc<RefCell<i32>>` cannot be sent between threads safely
  --> src/main.rs:10:5
   |
10 |     tokio::spawn(async move {
   |     ^^^^^^^^^^^^
   |
   = help: within `[closure@src/main.rs:10:19: 12:6]`, the trait `Send` is not implemented for `Rc<RefCell<i32>>`
"#;

    match explain_rust_error(async_error).await {
        Ok(resources) => {
            println!("🎓 LEARNING RESOURCES ({} found):", resources.len());
            for (i, resource) in resources.iter().enumerate().take(5) {
                let type_icon = match resource.resource_type {
                    ResourceType::OfficialDoc => "📜",
                    ResourceType::StackOverflow => "📚",
                    ResourceType::Tutorial => "🎯",
                    ResourceType::BlogPost => "📝",
                    ResourceType::Book => "📖",
                    ResourceType::Video => "🎥",
                };

                println!(
                    "  {}. {} {} (Relevance: {:.1}%)",
                    i + 1,
                    type_icon,
                    resource.title,
                    resource.relevance_score * 100.0
                );
                println!("     {}", resource.url);
                println!();
            }
        }
        Err(e) => {
            println!("❌ Failed to get learning resources: {}", e);
        }
    }

    // Example 4: Demonstrate different error types
    println!("\n\n=== Example 4: Various Error Types Analysis ===");
    let test_errors = vec![
        (
            "Lifetime Error",
            "error[E0621]: explicit lifetime required in the return type",
        ),
        (
            "Type Mismatch",
            "error[E0308]: mismatched types\nexpected `i32`, found `&str`",
        ),
        (
            "Undefined Function",
            "error[E0425]: cannot find function `undefined_func` in this scope",
        ),
        (
            "Borrow Checker",
            "error[E0499]: cannot borrow `x` as mutable more than once at a time",
        ),
    ];

    for (name, error) in test_errors {
        println!("🔍 Analyzing: {}", name);
        match solve_rust_error(error).await {
            Ok(solution) => {
                println!(
                    "  ✅ Category: {} | Confidence: {:.1}% | Solutions: {}",
                    solution.error_info.error_category,
                    solution.error_info.confidence * 100.0,
                    solution.solutions.len()
                );

                // Show confidence level interpretation
                match solution.metadata.confidence_level {
                    ConfidenceLevel::VeryHigh => {
                        println!("  🟢 Very High Confidence - Official solutions available")
                    }
                    ConfidenceLevel::High => {
                        println!("  🟡 High Confidence - Well-tested community solutions")
                    }
                    ConfidenceLevel::Medium => {
                        println!("  🟠 Medium Confidence - Multiple approaches available")
                    }
                    ConfidenceLevel::Low => {
                        println!("  🔴 Low Confidence - Limited or experimental solutions")
                    }
                }
            }
            Err(e) => {
                println!("  ❌ Analysis failed: {}", e);
            }
        }
        println!();
    }

    println!("🎉 Unified Interface Demo Complete!");
    println!("\n💡 KEY BENEFITS:");
    println!("  ✅ One function call solves any Rust error");
    println!("  ✅ Structured, actionable solutions");
    println!("  ✅ Multiple difficulty levels");
    println!("  ✅ Learning resources for understanding");
    println!("  ✅ Confidence scoring for reliability");
    println!("  ✅ Processing metadata for transparency");

    println!("\n🚀 INTEGRATION EXAMPLES:");
    println!("  // Basic usage");
    println!("  let solution = solve_rust_error(error_message).await?;");
    println!();
    println!("  // Quick fixes only");
    println!("  let fixes = get_quick_solutions(error_message).await?;");
    println!();
    println!("  // Learning focused");
    println!("  let resources = explain_rust_error(error_message).await?;");

    Ok(())
}
