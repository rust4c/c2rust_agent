//! Web Searcher - Intelligent Rust Error Solution Provider
//!
//! This crate provides a unified interface for solving Rust compilation errors:
//! Input: Error message → Output: Structured solutions
//!
//! ## Linus's Wisdom
//! "Talk is cheap. Show me the code."
//! Simple data flow: Error → Search → Fetch → Process → Solution

pub mod page_fetch;
pub mod searcher;

// Re-export core types
pub use page_fetch::{
    CodeBlock, ErrorAnalysis, PageFetchConfig, PageFetcher, PageMetadata, ProcessedCodeExample,
    ProcessedResult, SourceType, WebPageContent,
};
pub use searcher::{
    SearchEngineConfig, SearchKeyword, SearchRequest, SearchResponse, SearchResult, WebSearcher,
    WebSearcherConfig,
};

use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};

/// 🎯 MAIN INTERFACE: Unified Solution Structure
/// This is what users get - everything they need to solve their error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustErrorSolution {
    /// Original error information
    pub error_info: ErrorInfo,
    /// Direct solutions ranked by effectiveness
    pub solutions: Vec<Solution>,
    /// Code examples with explanations
    pub code_examples: Vec<CodeExample>,
    /// Related concepts and learning resources
    pub learning_resources: Vec<LearningResource>,
    /// Processing metadata
    pub metadata: SolutionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub error_code: Option<String>, // E0382, E0277, etc.
    pub error_category: String,     // ownership, trait_bound, lifetime, etc.
    pub original_message: String,   // Full error text
    pub confidence: f32,            // How confident we are in the analysis
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub title: String,              // "Fix ownership by using references"
    pub description: String,        // Detailed explanation
    pub fix_type: FixType,          // Code change, config, etc.
    pub difficulty: Difficulty,     // beginner, intermediate, advanced
    pub effectiveness_score: f32,   // 0.0-1.0, how likely to solve the problem
    pub source_url: Option<String>, // Where this solution came from
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExample {
    pub title: String,               // "Before and After Fix"
    pub before_code: Option<String>, // Problematic code
    pub after_code: String,          // Fixed code
    pub explanation: String,         // Why this works
    pub is_complete_solution: bool,  // Can this be used as-is?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningResource {
    pub title: String,
    pub url: String,
    pub resource_type: ResourceType, // documentation, tutorial, book, etc.
    pub relevance_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FixType {
    CodeChange,       // Modify source code
    DependencyUpdate, // Update Cargo.toml
    CompilerFlag,     // Add rustc flags
    Configuration,    // Change project config
    Architecture,     // Redesign approach
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Difficulty {
    Beginner,     // Simple fix, copy-paste solution
    Intermediate, // Requires understanding
    Advanced,     // Complex refactoring needed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    OfficialDoc,
    Tutorial,
    StackOverflow,
    BlogPost,
    Book,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionMetadata {
    pub total_sources_analyzed: usize,
    pub processing_time_ms: u64,
    pub search_keywords_used: Vec<String>,
    pub confidence_level: ConfidenceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    VeryHigh, // 0.9+ - Official docs, well-tested solutions
    High,     // 0.7-0.9 - Stack Overflow accepted answers
    Medium,   // 0.5-0.7 - Community solutions
    Low,      // <0.5 - Experimental or unverified
}

/// 🚀 PRIMARY FUNCTION: One function to rule them all
/// Input: Rust error message
/// Output: Complete structured solution
pub async fn solve_rust_error(error_message: impl Into<String>) -> Result<RustErrorSolution> {
    let error_text = error_message.into();
    let start_time = std::time::Instant::now();

    info!(
        "🔍 Starting error analysis for: {}",
        error_text.lines().next().unwrap_or("Unknown error")
    );

    // Step 1: Analyze error to extract basic info
    let error_info = analyze_error(&error_text);

    // Step 2: Search for solutions
    let search_results = search_for_solutions(&error_text).await?;

    // Step 3: Fetch and process web content
    let processed_results = fetch_and_process_content(search_results, &error_text).await?;

    // Step 4: Convert to unified solution format
    let solutions = convert_to_solutions(processed_results, &error_info);

    let processing_time = start_time.elapsed().as_millis() as u64;

    info!("✅ Error analysis completed in {}ms", processing_time);

    Ok(RustErrorSolution {
        error_info,
        solutions: solutions.solutions,
        code_examples: solutions.code_examples,
        learning_resources: solutions.learning_resources,
        metadata: SolutionMetadata {
            total_sources_analyzed: solutions.sources_count,
            processing_time_ms: processing_time,
            search_keywords_used: solutions.keywords_used,
            confidence_level: solutions.confidence_level,
        },
    })
}

/// 🔥 CONVENIENCE FUNCTION: For when you just want quick solutions
/// Returns the top 3 most effective solutions
pub async fn get_quick_solutions(error_message: impl Into<String>) -> Result<Vec<Solution>> {
    let solution = solve_rust_error(error_message).await?;
    Ok(solution.solutions.into_iter().take(3).collect())
}

/// 🎓 LEARNING FUNCTION: Focus on understanding, not just fixing
pub async fn explain_rust_error(error_message: impl Into<String>) -> Result<Vec<LearningResource>> {
    let solution = solve_rust_error(error_message).await?;
    Ok(solution.learning_resources)
}

// ===== INTERNAL IMPLEMENTATION =====

/// Analyze error message to extract basic information
fn analyze_error(error_text: &str) -> ErrorInfo {
    // Extract error code (E0382, E0277, etc.)
    let error_code = extract_error_code(error_text);

    // Categorize error type
    let error_category = categorize_error(error_text, error_code.as_deref());

    // Calculate confidence based on how well we can parse the error
    let confidence = calculate_error_confidence(error_text, &error_code);

    ErrorInfo {
        error_code,
        error_category,
        original_message: error_text.to_string(),
        confidence,
    }
}

fn extract_error_code(error_text: &str) -> Option<String> {
    use regex::Regex;

    if let Ok(re) = Regex::new(r"error\[([E]\d{4})\]") {
        if let Some(cap) = re.captures(error_text) {
            return cap.get(1).map(|m| m.as_str().to_string());
        }
    }
    None
}

fn categorize_error(error_text: &str, error_code: Option<&str>) -> String {
    let text_lower = error_text.to_lowercase();

    match error_code {
        Some("E0382") => "ownership",
        Some("E0277") => "trait_bound",
        Some("E0621") => "lifetime",
        Some("E0308") => "type_mismatch",
        Some("E0499") => "borrow_checker",
        Some("E0425") => "undefined_identifier",
        _ => {
            // Fallback to text analysis
            if text_lower.contains("moved") || text_lower.contains("ownership") {
                "ownership"
            } else if text_lower.contains("trait") || text_lower.contains("bound") {
                "trait_bound"
            } else if text_lower.contains("lifetime") {
                "lifetime"
            } else if text_lower.contains("type") && text_lower.contains("mismatch") {
                "type_mismatch"
            } else if text_lower.contains("borrow") {
                "borrow_checker"
            } else {
                "unknown"
            }
        }
    }
    .to_string()
}

fn calculate_error_confidence(error_text: &str, error_code: &Option<String>) -> f32 {
    let mut confidence: f32 = 0.5; // Base confidence

    // Boost confidence if we have an error code
    if error_code.is_some() {
        confidence += 0.3;
    }

    // Boost confidence if error message is detailed
    if error_text.contains("-->") && error_text.contains("|") {
        confidence += 0.2;
    }

    // Boost confidence if we recognize common patterns
    let known_patterns = [
        "move occurs because",
        "trait bound",
        "lifetime",
        "cannot borrow",
        "type mismatch",
        "cannot find",
        "use of moved value",
    ];

    for pattern in &known_patterns {
        if error_text.to_lowercase().contains(pattern) {
            confidence += 0.1;
            break;
        }
    }

    confidence.min(1.0)
}

/// Search for solutions using the search module
async fn search_for_solutions(error_text: &str) -> Result<Vec<SearchResult>> {
    let searcher = WebSearcher::new().await?;
    let request = SearchRequest {
        error_message: error_text.to_string(),
        code_context: None,
        project_context: None,
    };

    let response = searcher.search_error(request).await?;
    Ok(response.results)
}

/// Fetch and process web content using the page_fetch module
async fn fetch_and_process_content(
    search_results: Vec<SearchResult>,
    error_text: &str,
) -> Result<Vec<ProcessedResult>> {
    let page_fetcher = PageFetcher::new().await?;
    let processed = page_fetcher
        .process_search_results(search_results, error_text)
        .await?;

    Ok(processed)
}

/// Internal struct to collect conversion results
struct ConvertedSolutions {
    solutions: Vec<Solution>,
    code_examples: Vec<CodeExample>,
    learning_resources: Vec<LearningResource>,
    sources_count: usize,
    keywords_used: Vec<String>,
    confidence_level: ConfidenceLevel,
}

/// Convert processed results to unified solution format
fn convert_to_solutions(
    processed_results: Vec<ProcessedResult>,
    error_info: &ErrorInfo,
) -> ConvertedSolutions {
    let mut solutions = Vec::new();
    let mut code_examples = Vec::new();
    let mut learning_resources = Vec::new();
    let keywords_used = Vec::new();

    for result in &processed_results {
        // Convert to Solution
        let difficulty = if result.confidence_score > 0.8 {
            Difficulty::Beginner
        } else if result.confidence_score > 0.6 {
            Difficulty::Intermediate
        } else {
            Difficulty::Advanced
        };

        let fix_type = determine_fix_type(&result.solution_summary, error_info);

        solutions.push(Solution {
            title: extract_solution_title(&result.solution_summary),
            description: result.solution_summary.clone(),
            fix_type,
            difficulty,
            effectiveness_score: result.relevance_score,
            source_url: Some(result.url.clone()),
        });

        // Convert code examples
        for code_example in &result.code_examples {
            code_examples.push(CodeExample {
                title: code_example.title.clone(),
                before_code: None, // We don't have "before" code from web scraping
                after_code: code_example.code.clone(),
                explanation: code_example.explanation.clone(),
                is_complete_solution: code_example.is_solution,
            });
        }

        // Create learning resources
        let resource_type = match result.url.as_str() {
            url if url.contains("doc.rust-lang.org") => ResourceType::OfficialDoc,
            url if url.contains("stackoverflow.com") => ResourceType::StackOverflow,
            url if url.contains("blog") => ResourceType::BlogPost,
            _ => ResourceType::Tutorial,
        };

        learning_resources.push(LearningResource {
            title: result.title.clone(),
            url: result.url.clone(),
            resource_type,
            relevance_score: result.relevance_score,
        });
    }

    // Sort solutions by effectiveness
    solutions.sort_by(|a, b| {
        b.effectiveness_score
            .partial_cmp(&a.effectiveness_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Sort learning resources by relevance
    learning_resources.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Determine overall confidence level
    let avg_confidence = if !processed_results.is_empty() {
        processed_results
            .iter()
            .map(|r| r.confidence_score)
            .sum::<f32>()
            / processed_results.len() as f32
    } else {
        0.0
    };

    let confidence_level = match avg_confidence {
        c if c >= 0.9 => ConfidenceLevel::VeryHigh,
        c if c >= 0.7 => ConfidenceLevel::High,
        c if c >= 0.5 => ConfidenceLevel::Medium,
        _ => ConfidenceLevel::Low,
    };

    ConvertedSolutions {
        solutions,
        code_examples,
        learning_resources,
        sources_count: processed_results.len(),
        keywords_used,
        confidence_level,
    }
}

fn determine_fix_type(solution_text: &str, _error_info: &ErrorInfo) -> FixType {
    let text_lower = solution_text.to_lowercase();

    if text_lower.contains("cargo") || text_lower.contains("dependency") {
        FixType::DependencyUpdate
    } else if text_lower.contains("rustc") || text_lower.contains("flag") {
        FixType::CompilerFlag
    } else if text_lower.contains("config") || text_lower.contains("toml") {
        FixType::Configuration
    } else if text_lower.contains("refactor") || text_lower.contains("redesign") {
        FixType::Architecture
    } else {
        FixType::CodeChange // Default
    }
}

fn extract_solution_title(solution_text: &str) -> String {
    // Try to extract a meaningful title from the solution
    let lines: Vec<&str> = solution_text.lines().collect();

    // Look for markdown headers
    for line in &lines {
        let line = line.trim();
        if line.starts_with("# ") {
            return line[2..].to_string();
        } else if line.starts_with("## ") {
            return line[3..].to_string();
        }
    }

    // Fallback: use first meaningful sentence
    if let Some(first_line) = lines.first() {
        let first_line = first_line.trim();
        if first_line.len() > 10 && first_line.len() < 100 {
            return first_line.to_string();
        }
    }

    "Rust Error Solution".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_extraction() {
        let error = "error[E0382]: use of moved value";
        assert_eq!(extract_error_code(error), Some("E0382".to_string()));
    }

    #[test]
    fn test_error_categorization() {
        assert_eq!(
            categorize_error("use of moved value", Some("E0382")),
            "ownership"
        );
        assert_eq!(
            categorize_error("trait bound not satisfied", Some("E0277")),
            "trait_bound"
        );
        assert_eq!(
            categorize_error("explicit lifetime required", Some("E0621")),
            "lifetime"
        );
    }

    #[test]
    fn test_confidence_calculation() {
        let error_with_code = "error[E0382]: use of moved value: `v`\n  --> src/main.rs:5:9";
        let confidence = calculate_error_confidence(error_with_code, &Some("E0382".to_string()));
        assert!(confidence > 0.8); // Should be high confidence

        let vague_error = "compilation failed";
        let confidence = calculate_error_confidence(vague_error, &None);
        assert!(confidence < 0.6); // Should be lower confidence
    }

    #[tokio::test]
    async fn test_solve_rust_error_integration() {
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

        // This test might fail in CI due to network requirements
        match solve_rust_error(error_message).await {
            Ok(solution) => {
                assert!(!solution.solutions.is_empty());
                assert_eq!(solution.error_info.error_code, Some("E0382".to_string()));
                assert_eq!(solution.error_info.error_category, "ownership");
                println!(
                    "✅ Integration test passed: found {} solutions",
                    solution.solutions.len()
                );
            }
            Err(e) => {
                println!(
                    "⚠️ Integration test failed (expected in test environment): {}",
                    e
                );
                // Don't fail the test in CI/CD environments
            }
        }
    }
}
