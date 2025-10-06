//! Page Fetch - Web page content extraction and AI processing
//!
//! This module provides functionality to fetch web page content from search results
//! and process it using AI to extract relevant solutions for Rust compilation errors.
//!
//! ## Linus's Wisdom
//! "Good programmers worry about data structures."
//! Simple data flow: SearchResult → WebContent → AI Processing → ProcessedResult

use anyhow::{anyhow, Result};
use llm_requester::llm_request_with_prompt;
use log::{debug, info, warn};
use regex::Regex;
use reqwest;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::fs;

use crate::SearchResult;

/// Configuration for page fetching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFetchConfig {
    pub max_content_length: usize,
    pub timeout_seconds: u64,
    pub max_concurrent_fetches: usize,
    pub user_agent: String,
    pub extract_code_blocks: bool,
    pub extract_solutions: bool,
}

impl Default for PageFetchConfig {
    fn default() -> Self {
        Self {
            max_content_length: 50000, // 50KB max content
            timeout_seconds: 30,
            max_concurrent_fetches: 5,
            user_agent: "Mozilla/5.0 (compatible; RustErrorSearcher/1.0)".to_string(),
            extract_code_blocks: true,
            extract_solutions: true,
        }
    }
}

/// Raw web page content extracted from HTML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebPageContent {
    pub url: String,
    pub title: String,
    pub main_content: String,
    pub code_blocks: Vec<CodeBlock>,
    pub metadata: PageMetadata,
    pub fetch_time_ms: u64,
}

/// Code block extracted from web page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub code: String,
    pub context: Option<String>, // Surrounding text for context
}

/// Page metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMetadata {
    pub source_type: SourceType,
    pub author: Option<String>,
    pub published_date: Option<String>,
    pub vote_score: Option<i32>, // For Stack Overflow, Reddit, etc.
    pub tags: Vec<String>,
}

/// Type of content source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    StackOverflow,
    RustDoc,
    GitHub,
    Blog,
    Documentation,
    Forum,
    Unknown,
}

/// AI-processed result from web page content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedResult {
    pub url: String,
    pub title: String,
    pub relevance_score: f32,
    pub solution_summary: String,
    pub code_examples: Vec<ProcessedCodeExample>,
    pub key_insights: Vec<String>,
    pub error_analysis: Option<ErrorAnalysis>,
    pub confidence_score: f32,
    pub processing_time_ms: u64,
}

/// Processed code example with explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedCodeExample {
    pub title: String,
    pub code: String,
    pub explanation: String,
    pub is_solution: bool,
    pub language: String,
}

/// Analysis of the original error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorAnalysis {
    pub error_category: String,
    pub root_cause: String,
    pub fix_approach: String,
    pub common_mistakes: Vec<String>,
}

/// Main page fetcher struct
pub struct PageFetcher {
    config: PageFetchConfig,
    client: reqwest::Client,
    prompt_template: String,
}

impl PageFetcher {
    /// Create a new PageFetcher instance
    pub async fn new() -> Result<Self> {
        let config = PageFetchConfig::default();
        let prompt_template = Self::load_search_result_prompt().await?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .user_agent(&config.user_agent)
            .build()?;

        info!(
            "PageFetcher initialized with timeout {}s",
            config.timeout_seconds
        );

        Ok(Self {
            config,
            client,
            prompt_template,
        })
    }

    /// Process multiple search results concurrently
    pub async fn process_search_results(
        &self,
        search_results: Vec<SearchResult>,
        original_error: &str,
    ) -> Result<Vec<ProcessedResult>> {
        let start_time = std::time::Instant::now();
        let mut processed_results = Vec::new();

        // Limit concurrent fetches
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(
            self.config.max_concurrent_fetches,
        ));

        let mut tasks = Vec::new();

        for result in search_results
            .into_iter()
            .take(self.config.max_concurrent_fetches)
        {
            let permit = semaphore.clone().acquire_owned().await?;
            let client = self.client.clone();
            let config = self.config.clone();
            let prompt = self.prompt_template.clone();
            let error = original_error.to_string();

            let task = tokio::spawn(async move {
                let _permit = permit; // Hold permit until task completes
                Self::fetch_and_process_single(&client, &config, &prompt, result, &error).await
            });

            tasks.push(task);
        }

        // Wait for all tasks to complete
        for task in tasks {
            match task.await {
                Ok(Ok(processed)) => processed_results.push(processed),
                Ok(Err(e)) => warn!("Failed to process search result: {}", e),
                Err(e) => warn!("Task failed: {}", e),
            }
        }

        // Sort by relevance score (descending)
        processed_results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        info!(
            "Processed {} search results in {}ms",
            processed_results.len(),
            start_time.elapsed().as_millis()
        );

        Ok(processed_results)
    }

    /// Fetch and process a single search result
    async fn fetch_and_process_single(
        client: &reqwest::Client,
        config: &PageFetchConfig,
        prompt_template: &str,
        search_result: SearchResult,
        original_error: &str,
    ) -> Result<ProcessedResult> {
        let fetch_start = std::time::Instant::now();

        // Step 1: Fetch web page content
        let web_content = Self::fetch_web_content(client, config, &search_result).await?;

        // Step 2: Process with AI
        let processed =
            Self::process_with_ai(prompt_template, &web_content, original_error).await?;

        debug!(
            "Processed {} in {}ms",
            web_content.url,
            fetch_start.elapsed().as_millis()
        );

        Ok(processed)
    }

    /// Fetch and parse web page content
    async fn fetch_web_content(
        client: &reqwest::Client,
        config: &PageFetchConfig,
        search_result: &SearchResult,
    ) -> Result<WebPageContent> {
        let start_time = std::time::Instant::now();

        // Fetch the page
        let response = client.get(&search_result.url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP error {}: {}",
                response.status(),
                search_result.url
            ));
        }

        let html_content = response.text().await?;

        // Limit content length
        let html_content = if html_content.len() > config.max_content_length {
            html_content
                .chars()
                .take(config.max_content_length)
                .collect()
        } else {
            html_content
        };

        // Parse HTML content
        let document = Html::parse_document(&html_content);

        // Extract content based on source type
        let source_type = Self::detect_source_type(&search_result.url);
        let (title, main_content, code_blocks, metadata) =
            Self::extract_content_by_source(&document, &source_type, &search_result.url)?;

        let fetch_time = start_time.elapsed().as_millis() as u64;

        Ok(WebPageContent {
            url: search_result.url.clone(),
            title,
            main_content,
            code_blocks,
            metadata,
            fetch_time_ms: fetch_time,
        })
    }

    /// Detect the type of content source
    fn detect_source_type(url: &str) -> SourceType {
        let url_lower = url.to_lowercase();

        if url_lower.contains("stackoverflow.com") {
            SourceType::StackOverflow
        } else if url_lower.contains("doc.rust-lang.org") {
            SourceType::RustDoc
        } else if url_lower.contains("github.com") {
            SourceType::GitHub
        } else if url_lower.contains("reddit.com") || url_lower.contains("discourse") {
            SourceType::Forum
        } else if url_lower.contains("blog") || url_lower.contains("medium.com") {
            SourceType::Blog
        } else {
            SourceType::Unknown
        }
    }

    /// Extract content based on source type
    fn extract_content_by_source(
        document: &Html,
        source_type: &SourceType,
        url: &str,
    ) -> Result<(String, String, Vec<CodeBlock>, PageMetadata)> {
        match source_type {
            SourceType::StackOverflow => Self::extract_stackoverflow_content(document, url),
            SourceType::RustDoc => Self::extract_rustdoc_content(document, url),
            SourceType::GitHub => Self::extract_github_content(document, url),
            _ => Self::extract_generic_content(document, url),
        }
    }

    /// Extract content from Stack Overflow pages
    fn extract_stackoverflow_content(
        document: &Html,
        _url: &str,
    ) -> Result<(String, String, Vec<CodeBlock>, PageMetadata)> {
        // Title
        let title_selector = Selector::parse("h1 a, .question-hyperlink").unwrap();
        let title = document
            .select(&title_selector)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_else(|| "Stack Overflow Question".to_string());

        // Main content (question + accepted answer)
        let question_selector = Selector::parse(".js-post-body, .s-prose").unwrap();
        let mut main_content = String::new();

        for element in document.select(&question_selector).take(3) {
            // Question + top 2 answers
            main_content.push_str(&element.text().collect::<Vec<_>>().join(" "));
            main_content.push_str("\n\n");
        }

        // Code blocks
        let code_blocks = Self::extract_code_blocks(document);

        // Metadata
        let vote_selector = Selector::parse(".js-vote-count").unwrap();
        let vote_score = document
            .select(&vote_selector)
            .next()
            .and_then(|el| el.text().next())
            .and_then(|text| text.parse::<i32>().ok());

        let tag_selector = Selector::parse(".post-tag").unwrap();
        let tags: Vec<String> = document
            .select(&tag_selector)
            .map(|el| el.text().collect::<String>())
            .collect();

        let metadata = PageMetadata {
            source_type: SourceType::StackOverflow,
            author: None, // Could extract user info if needed
            published_date: None,
            vote_score,
            tags,
        };

        Ok((title, main_content, code_blocks, metadata))
    }

    /// Extract content from Rust documentation
    fn extract_rustdoc_content(
        document: &Html,
        _url: &str,
    ) -> Result<(String, String, Vec<CodeBlock>, PageMetadata)> {
        // Title
        let title_selector = Selector::parse("h1.fqn, h1.main-heading").unwrap();
        let title = document
            .select(&title_selector)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_else(|| "Rust Documentation".to_string());

        // Main content
        let content_selector = Selector::parse(".docblock, .rustdoc").unwrap();
        let mut main_content = String::new();

        for element in document.select(&content_selector) {
            main_content.push_str(&element.text().collect::<Vec<_>>().join(" "));
            main_content.push_str("\n\n");
        }

        // Code blocks
        let code_blocks = Self::extract_code_blocks(document);

        let metadata = PageMetadata {
            source_type: SourceType::RustDoc,
            author: Some("Rust Team".to_string()),
            published_date: None,
            vote_score: None,
            tags: vec!["rust".to_string(), "documentation".to_string()],
        };

        Ok((title, main_content, code_blocks, metadata))
    }

    /// Extract content from GitHub
    fn extract_github_content(
        document: &Html,
        _url: &str,
    ) -> Result<(String, String, Vec<CodeBlock>, PageMetadata)> {
        let title_selector = Selector::parse("h1, .js-issue-title").unwrap();
        let title = document
            .select(&title_selector)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_else(|| "GitHub Content".to_string());

        let content_selector = Selector::parse(".comment-body, .markdown-body").unwrap();
        let mut main_content = String::new();

        for element in document.select(&content_selector).take(5) {
            main_content.push_str(&element.text().collect::<Vec<_>>().join(" "));
            main_content.push_str("\n\n");
        }

        let code_blocks = Self::extract_code_blocks(document);

        let metadata = PageMetadata {
            source_type: SourceType::GitHub,
            author: None,
            published_date: None,
            vote_score: None,
            tags: vec!["github".to_string()],
        };

        Ok((title, main_content, code_blocks, metadata))
    }

    /// Generic content extraction for unknown sources
    fn extract_generic_content(
        document: &Html,
        _url: &str,
    ) -> Result<(String, String, Vec<CodeBlock>, PageMetadata)> {
        // Try common title selectors
        let title_selectors = ["h1", "title", ".title", "#title"];
        let mut title = String::new();

        for selector_str in &title_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = document.select(&selector).next() {
                    title = element.text().collect::<String>();
                    break;
                }
            }
        }

        if title.is_empty() {
            title = "Web Content".to_string();
        }

        // Try common content selectors
        let content_selectors = ["main", "article", ".content", ".post", ".entry", "body"];
        let mut main_content = String::new();

        for selector_str in &content_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                if let Some(element) = document.select(&selector).next() {
                    main_content = element.text().collect::<Vec<_>>().join(" ");
                    break;
                }
            }
        }

        // Fallback to body content
        if main_content.is_empty() {
            main_content = document.root_element().text().collect::<Vec<_>>().join(" ");
        }

        let code_blocks = Self::extract_code_blocks(document);

        let metadata = PageMetadata {
            source_type: SourceType::Unknown,
            author: None,
            published_date: None,
            vote_score: None,
            tags: Vec::new(),
        };

        Ok((title, main_content, code_blocks, metadata))
    }

    /// Extract code blocks from HTML document
    fn extract_code_blocks(document: &Html) -> Vec<CodeBlock> {
        let mut code_blocks = Vec::new();

        // Common code block selectors
        let code_selectors = [
            ("pre code", None),
            ("pre", None),
            (".highlight", None),
            (".language-rust", Some("rust")),
            (".language-rs", Some("rust")),
            ("code[class*=\"language-\"]", None),
        ];

        for (selector_str, default_lang) in &code_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    let code_text = element.text().collect::<String>();
                    if code_text.trim().len() > 10 {
                        // Only include substantial code blocks
                        let language = default_lang
                            .map(|s| s.to_string())
                            .or_else(|| {
                                Self::detect_language_from_class(
                                    element.value().attr("class").unwrap_or(""),
                                )
                            })
                            .or_else(|| Self::detect_language_from_content(&code_text));

                        code_blocks.push(CodeBlock {
                            language,
                            code: code_text.trim().to_string(),
                            context: None, // Could extract surrounding text
                        });
                    }
                }
            }
        }

        code_blocks
    }

    /// Detect programming language from CSS class
    fn detect_language_from_class(class: &str) -> Option<String> {
        let class_lower = class.to_lowercase();
        if class_lower.contains("rust") || class_lower.contains("rs") {
            Some("rust".to_string())
        } else if class_lower.contains("javascript") || class_lower.contains("js") {
            Some("javascript".to_string())
        } else if class_lower.contains("python") || class_lower.contains("py") {
            Some("python".to_string())
        } else if class_lower.contains("cpp") || class_lower.contains("c++") {
            Some("cpp".to_string())
        } else if class_lower.contains("language-") {
            // Extract language from "language-xxx" pattern
            if let Some(lang_start) = class_lower.find("language-") {
                let lang_part = &class_lower[lang_start + 9..];
                if let Some(space_pos) = lang_part.find(' ') {
                    Some(lang_part[..space_pos].to_string())
                } else {
                    Some(lang_part.to_string())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Detect programming language from code content
    fn detect_language_from_content(code: &str) -> Option<String> {
        let code_lower = code.to_lowercase();

        if code_lower.contains("fn ")
            || code_lower.contains("let ")
            || code_lower.contains("mut ")
            || code_lower.contains("impl ")
            || code_lower.contains("use ")
            || code_lower.contains("struct ")
            || code_lower.contains("enum ")
        {
            Some("rust".to_string())
        } else if code_lower.contains("function")
            || code_lower.contains("const ")
            || code_lower.contains("var ")
        {
            Some("javascript".to_string())
        } else if code_lower.contains("def ")
            || code_lower.contains("import ")
            || code_lower.contains("class ")
        {
            Some("python".to_string())
        } else {
            None
        }
    }

    /// Process web content using AI
    async fn process_with_ai(
        prompt_template: &str,
        web_content: &WebPageContent,
        original_error: &str,
    ) -> Result<ProcessedResult> {
        let start_time = std::time::Instant::now();

        // Build context for AI processing
        let mut context_parts = vec![
            format!("原始错误:\n{}", original_error),
            format!("网页标题: {}", web_content.title),
            format!("网页URL: {}", web_content.url),
            format!("来源类型: {:?}", web_content.metadata.source_type),
        ];

        // Add main content (truncated if too long)
        let content = if web_content.main_content.len() > 3000 {
            format!("{}...", &web_content.main_content[..3000])
        } else {
            web_content.main_content.clone()
        };
        context_parts.push(format!("网页内容:\n{}", content));

        // Add code blocks
        if !web_content.code_blocks.is_empty() {
            let mut code_section = String::from("代码示例:\n");
            for (i, block) in web_content.code_blocks.iter().enumerate().take(5) {
                code_section.push_str(&format!(
                    "\n代码块 {} ({}):  \n```\n{}\n```\n",
                    i + 1,
                    block.language.as_deref().unwrap_or("unknown"),
                    block.code
                ));
            }
            context_parts.push(code_section);
        }

        let full_context = context_parts.join("\n\n");

        // Call LLM
        let ai_response =
            llm_request_with_prompt(vec![full_context], prompt_template.to_string()).await?;

        // Parse AI response
        let processed = Self::parse_ai_response(&ai_response, web_content)?;

        let processing_time = start_time.elapsed().as_millis() as u64;

        Ok(ProcessedResult {
            processing_time_ms: processing_time,
            ..processed
        })
    }

    /// Parse AI response into structured result
    fn parse_ai_response(
        ai_response: &str,
        web_content: &WebPageContent,
    ) -> Result<ProcessedResult> {
        // This is a simplified parser - in real implementation, this would be more sophisticated
        let mut solution_summary = String::new();
        let mut code_examples = Vec::new();
        let mut key_insights = Vec::new();
        let mut confidence_score = 0.7; // Default confidence
        let mut relevance_score = 0.8; // Default relevance

        // Extract sections from AI response
        let lines: Vec<&str> = ai_response.lines().collect();
        let mut current_section = "";
        let mut current_content = String::new();

        for line in lines {
            let line = line.trim();

            if line.starts_with("## ") || line.starts_with("# ") {
                // Process previous section
                Self::process_section(
                    current_section,
                    &current_content,
                    &mut solution_summary,
                    &mut code_examples,
                    &mut key_insights,
                );

                // Start new section
                current_section = line;
                current_content.clear();
            } else {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        // Process final section
        Self::process_section(
            current_section,
            &current_content,
            &mut solution_summary,
            &mut code_examples,
            &mut key_insights,
        );

        // If no structured response, use the entire response as summary
        if solution_summary.is_empty() {
            solution_summary = ai_response.chars().take(500).collect();
        }

        // Extract confidence and relevance scores if mentioned in response
        if let Ok(conf_regex) = Regex::new(r"(?i)confidence[:\s]*([0-9]*\.?[0-9]+)") {
            if let Some(cap) = conf_regex.captures(ai_response) {
                if let Some(score_str) = cap.get(1) {
                    if let Ok(score) = score_str.as_str().parse::<f32>() {
                        confidence_score = score.min(1.0).max(0.0);
                    }
                }
            }
        }

        if let Ok(rel_regex) = Regex::new(r"(?i)relevance[:\s]*([0-9]*\.?[0-9]+)") {
            if let Some(cap) = rel_regex.captures(ai_response) {
                if let Some(score_str) = cap.get(1) {
                    if let Ok(score) = score_str.as_str().parse::<f32>() {
                        relevance_score = score.min(1.0).max(0.0);
                    }
                }
            }
        }

        let error_analysis = Self::extract_error_analysis(ai_response);

        Ok(ProcessedResult {
            url: web_content.url.clone(),
            title: web_content.title.clone(),
            relevance_score,
            solution_summary,
            code_examples,
            key_insights,
            error_analysis,
            confidence_score,
            processing_time_ms: 0, // Will be set by caller
        })
    }

    /// Process individual sections from AI response
    fn process_section(
        section_header: &str,
        content: &str,
        solution_summary: &mut String,
        code_examples: &mut Vec<ProcessedCodeExample>,
        key_insights: &mut Vec<String>,
    ) {
        let header_lower = section_header.to_lowercase();

        if header_lower.contains("solution") || header_lower.contains("解决") {
            if solution_summary.is_empty() {
                *solution_summary = content.trim().to_string();
            }
        } else if header_lower.contains("code")
            || header_lower.contains("example")
            || header_lower.contains("代码")
        {
            // Extract code blocks from content
            if let Ok(code_regex) = Regex::new(r"```(\w+)?\n([\s\S]*?)\n```") {
                for cap in code_regex.captures_iter(content) {
                    let language = cap
                        .get(1)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_else(|| "rust".to_string());
                    let code = cap
                        .get(2)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();

                    if !code.trim().is_empty() {
                        code_examples.push(ProcessedCodeExample {
                            title: format!("Code Example {}", code_examples.len() + 1),
                            code: code.trim().to_string(),
                            explanation: "From AI processing".to_string(),
                            is_solution: header_lower.contains("solution"),
                            language,
                        });
                    }
                }
            }
        } else if header_lower.contains("insight")
            || header_lower.contains("key")
            || header_lower.contains("要点")
        {
            // Split insights by bullet points or newlines
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty()
                    && (line.starts_with("- ") || line.starts_with("• ") || line.starts_with("*"))
                {
                    let cleaned_line = line
                        .trim_start_matches('-')
                        .trim_start_matches('•')
                        .trim_start_matches('*')
                        .trim();
                    key_insights.push(cleaned_line.to_string());
                } else if !line.is_empty() && key_insights.is_empty() {
                    key_insights.push(line.to_string());
                }
            }
        }
    }

    /// Extract error analysis from AI response
    fn extract_error_analysis(ai_response: &str) -> Option<ErrorAnalysis> {
        // Simple extraction - could be more sophisticated
        let response_lower = ai_response.to_lowercase();

        if response_lower.contains("error") || response_lower.contains("错误") {
            Some(ErrorAnalysis {
                error_category: "Compilation Error".to_string(),
                root_cause: "See AI analysis".to_string(),
                fix_approach: "Apply suggested solutions".to_string(),
                common_mistakes: vec!["Check AI insights".to_string()],
            })
        } else {
            None
        }
    }

    /// Load search result processing prompt from config/prompts/search_result.md
    async fn load_search_result_prompt() -> Result<String> {
        let possible_paths = [
            "config/prompts/search_result.md",
            "../config/prompts/search_result.md",
            "../../config/prompts/search_result.md",
            "../../../config/prompts/search_result.md",
        ];

        for path in &possible_paths {
            match fs::read_to_string(path).await {
                Ok(content) => {
                    info!("Loaded search result prompt from {}", path);
                    return Ok(content);
                }
                Err(_) => continue,
            }
        }

        warn!("Failed to load search result prompt from any location, using default");
        Ok(Self::default_search_result_prompt())
    }

    /// Default search result processing prompt
    fn default_search_result_prompt() -> String {
        r#"你是一位资深的 Rust 编程专家，擅长分析网页内容并提取对解决 Rust 编译错误有用的信息。

请分析提供的网页内容，并按以下格式输出结构化的解决方案：

## 解决方案总结
[简洁明了地总结如何解决原始错误，100-200字]

## 关键要点
- [要点1: 核心概念或原因]
- [要点2: 具体解决步骤]
- [要点3: 注意事项或最佳实践]

## 代码示例
如果网页包含相关代码，请提取并解释：

```rust
// 示例代码
[代码内容]
```

**解释**: [代码的作用和如何解决问题]

## 相关度评分
请评估这个网页内容对解决原始错误的相关程度：
- Relevance: [0.0-1.0的评分]
- Confidence: [0.0-1.0的置信度]

## 错误分析 (如适用)
- **错误类别**: [错误的类型分类]
- **根本原因**: [导致错误的根本原因]
- **修复方法**: [推荐的修复方法]

请确保输出内容准确、简洁，重点关注实用性。"#
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_page_fetcher_creation() {
        let fetcher = PageFetcher::new().await;
        assert!(fetcher.is_ok());
    }

    #[test]
    fn test_source_type_detection() {
        assert_eq!(
            PageFetcher::detect_source_type("https://stackoverflow.com/questions/123"),
            SourceType::StackOverflow
        );
        assert_eq!(
            PageFetcher::detect_source_type("https://doc.rust-lang.org/std/"),
            SourceType::RustDoc
        );
        assert_eq!(
            PageFetcher::detect_source_type("https://github.com/rust-lang/rust"),
            SourceType::GitHub
        );
        assert_eq!(
            PageFetcher::detect_source_type("https://blog.rust-lang.org/"),
            SourceType::Blog
        );
        assert_eq!(
            PageFetcher::detect_source_type("https://unknown-site.com/"),
            SourceType::Unknown
        );
    }

    #[test]
    fn test_language_detection_from_class() {
        assert_eq!(
            PageFetcher::detect_language_from_class("language-rust"),
            Some("rust".to_string())
        );
        assert_eq!(
            PageFetcher::detect_language_from_class("language-javascript"),
            Some("javascript".to_string())
        );
        assert_eq!(
            PageFetcher::detect_language_from_class("highlight rust"),
            Some("rust".to_string())
        );
        assert_eq!(PageFetcher::detect_language_from_class(""), None);
    }

    #[test]
    fn test_language_detection_from_content() {
        assert_eq!(
            PageFetcher::detect_language_from_content("fn main() { let x = 5; }"),
            Some("rust".to_string())
        );
        assert_eq!(
            PageFetcher::detect_language_from_content("function test() { var x = 5; }"),
            Some("javascript".to_string())
        );
        assert_eq!(
            PageFetcher::detect_language_from_content("def test(): pass"),
            Some("python".to_string())
        );
        assert_eq!(
            PageFetcher::detect_language_from_content("hello world"),
            None
        );
    }

    #[tokio::test]
    async fn test_prompt_loading() {
        let prompt = PageFetcher::load_search_result_prompt().await;
        assert!(prompt.is_ok());
        let prompt_content = prompt.unwrap();
        assert!(prompt_content.contains("Rust"));
        assert!(prompt_content.len() > 100);
    }
}
