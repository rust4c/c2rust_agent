//! Web Searcher - Intelligent error search with AI-powered keyword extraction
//!
//! This crate provides functionality to automatically extract search keywords from
//! Rust compilation errors using AI and perform web searches to find solutions.
//!
//! ## Linus's Wisdom
//! "Talk is cheap. Show me the code."
//! Simple data flow: Error → Keywords → Search Results. No special cases.

use anyhow::{anyhow, Result};
use llm_requester::llm_request_with_prompt;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::fs;

/// Search engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEngineConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub max_results: usize,
    pub enabled: bool,
}

/// Web searcher configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearcherConfig {
    pub default_engine: String,
    pub max_keywords: usize,
    pub engines: HashMap<String, SearchEngineConfig>,
}

/// Search request structure
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub error_message: String,
    pub code_context: Option<String>,
    pub project_context: Option<String>,
}

/// Search keyword with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchKeyword {
    pub keyword: String,
    pub relevance: f32,
    pub category: String, // "error_code", "concept", "solution", etc.
}

/// Search result item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub relevance_score: f32,
}

/// Complete search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub keywords: Vec<SearchKeyword>,
    pub results: Vec<SearchResult>,
    pub engine_used: String,
    pub total_time_ms: u64,
}

/// Main web searcher struct
pub struct WebSearcher {
    config: WebSearcherConfig,
    prompt_template: String,
}

impl WebSearcher {
    /// Create a new WebSearcher instance
    pub async fn new() -> Result<Self> {
        let config = Self::load_config().await?;
        let prompt_template = Self::load_search_prompt().await?;

        info!(
            "WebSearcher initialized with {} engines",
            config.engines.len()
        );

        Ok(Self {
            config,
            prompt_template,
        })
    }

    /// Perform intelligent search for Rust errors
    pub async fn search_error(&self, request: SearchRequest) -> Result<SearchResponse> {
        let start_time = std::time::Instant::now();

        // Step 1: Extract keywords using AI
        let keywords = self.extract_keywords(&request).await?;
        debug!("Extracted {} keywords", keywords.len());

        // Step 2: Perform web search
        let results = self.perform_search(&keywords).await?;

        let total_time = start_time.elapsed().as_millis() as u64;

        Ok(SearchResponse {
            keywords,
            results,
            engine_used: self.config.default_engine.clone(),
            total_time_ms: total_time,
        })
    }

    /// Extract search keywords from error using AI
    async fn extract_keywords(&self, request: &SearchRequest) -> Result<Vec<SearchKeyword>> {
        let mut messages = vec![request.error_message.clone()];

        // Add context if available
        if let Some(code) = &request.code_context {
            messages.push(format!("相关代码:\n```rust\n{}\n```", code));
        }

        if let Some(project) = &request.project_context {
            messages.push(format!("项目上下文: {}", project));
        }

        // Call AI to extract keywords
        let response = llm_request_with_prompt(messages, self.prompt_template.clone()).await?;

        // Parse AI response to extract structured keywords
        self.parse_keywords_response(&response)
    }

    /// Parse AI response to extract structured keywords
    fn parse_keywords_response(&self, response: &str) -> Result<Vec<SearchKeyword>> {
        let mut keywords = Vec::new();

        // Try multiple parsing strategies

        // Strategy 1: Structured format "keyword|category|relevance"
        for line in response.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }

            if let Some((keyword_part, rest)) = line.split_once('|') {
                let keyword = keyword_part.trim().trim_matches('"').trim_matches('`');
                if keyword.is_empty() {
                    continue;
                }

                if let Some((category_part, relevance_part)) = rest.split_once('|') {
                    let category = category_part.trim();
                    let relevance: f32 = relevance_part.trim().parse().unwrap_or(1.0);

                    keywords.push(SearchKeyword {
                        keyword: keyword.to_string(),
                        relevance,
                        category: category.to_string(),
                    });
                    continue;
                }
            }
        }

        // Strategy 2: If no structured format found, extract from quoted strings
        if keywords.is_empty() {
            let quote_patterns = [r#""([^"]+)""#, r#"`([^`]+)`"#];

            for pattern in &quote_patterns {
                if let Ok(regex) = regex::Regex::new(pattern) {
                    for cap in regex.captures_iter(response) {
                        if let Some(keyword) = cap.get(1) {
                            let keyword_text = keyword.as_str().trim();
                            if !keyword_text.is_empty() && keyword_text.len() > 3 {
                                // Categorize based on content
                                let category = self.categorize_keyword(keyword_text);
                                let relevance = self.calculate_relevance(keyword_text);

                                keywords.push(SearchKeyword {
                                    keyword: keyword_text.to_string(),
                                    relevance,
                                    category,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Strategy 3: If still no keywords, extract key technical terms
        if keywords.is_empty() {
            keywords.extend(self.extract_technical_terms(response));
        }

        // Deduplicate and sort
        keywords.sort_by(|a, b| a.keyword.cmp(&b.keyword));
        keywords.dedup_by(|a, b| a.keyword == b.keyword);

        keywords.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        keywords.truncate(self.config.max_keywords);

        // Final fallback
        if keywords.is_empty() {
            warn!("No keywords extracted from AI response, using fallback");
            keywords.push(SearchKeyword {
                keyword: "Rust compilation error".to_string(),
                relevance: 0.5,
                category: "fallback".to_string(),
            });
        }

        debug!(
            "Extracted {} keywords: {:?}",
            keywords.len(),
            keywords.iter().map(|k| &k.keyword).collect::<Vec<_>>()
        );
        Ok(keywords)
    }

    /// Categorize a keyword based on its content
    fn categorize_keyword(&self, keyword: &str) -> String {
        let keyword_lower = keyword.to_lowercase();

        if keyword_lower.starts_with("e0") || keyword_lower.contains("error[") {
            "error_code".to_string()
        } else if keyword_lower.contains("trait")
            || ["send", "sync", "display", "debug", "clone", "copy"]
                .iter()
                .any(|&t| keyword_lower.contains(t))
        {
            "trait".to_string()
        } else if ["ownership", "borrow", "lifetime", "move", "reference"]
            .iter()
            .any(|&c| keyword_lower.contains(c))
        {
            "concept".to_string()
        } else if ["fix", "solve", "solution", "resolve", "how to"]
            .iter()
            .any(|&s| keyword_lower.contains(s))
        {
            "solution".to_string()
        } else if ["tokio", "async", "await", "std", "vec", "string"]
            .iter()
            .any(|&l| keyword_lower.contains(l))
        {
            "library".to_string()
        } else {
            "general".to_string()
        }
    }

    /// Calculate relevance score based on keyword characteristics
    fn calculate_relevance(&self, keyword: &str) -> f32 {
        let keyword_lower = keyword.to_lowercase();
        let mut score: f32 = 0.7; // base score

        // Boost for error codes
        if keyword_lower.starts_with("e0") {
            score += 0.3;
        }

        // Boost for core Rust concepts
        if ["ownership", "borrow", "lifetime", "trait"]
            .iter()
            .any(|&c| keyword_lower.contains(c))
        {
            score += 0.2;
        }

        // Boost for solution-oriented terms
        if ["fix", "solve", "solution"]
            .iter()
            .any(|&s| keyword_lower.contains(s))
        {
            score += 0.1;
        }

        // Penalty for very long phrases
        if keyword.len() > 50 {
            score -= 0.2;
        }

        score.min(1.0).max(0.1)
    }

    /// Extract technical terms when structured parsing fails
    fn extract_technical_terms(&self, text: &str) -> Vec<SearchKeyword> {
        let mut keywords = Vec::new();
        let technical_terms = [
            ("E0382", "error_code", 1.0),
            ("E0277", "error_code", 1.0),
            ("E0621", "error_code", 1.0),
            ("ownership", "concept", 0.9),
            ("borrow checker", "concept", 0.9),
            ("lifetime", "concept", 0.9),
            ("trait bound", "trait", 0.9),
            ("Send trait", "trait", 0.8),
            ("Display trait", "trait", 0.8),
            ("move semantics", "concept", 0.8),
            ("Rust error", "general", 0.7),
            ("compilation error", "general", 0.7),
        ];

        for (term, category, relevance) in &technical_terms {
            if text.to_lowercase().contains(&term.to_lowercase()) {
                keywords.push(SearchKeyword {
                    keyword: term.to_string(),
                    relevance: *relevance,
                    category: category.to_string(),
                });
            }
        }

        keywords
    }

    /// Perform actual web search using configured engine
    async fn perform_search(&self, keywords: &[SearchKeyword]) -> Result<Vec<SearchResult>> {
        let engine_name = &self.config.default_engine;
        let engine = self
            .config
            .engines
            .get(engine_name)
            .ok_or_else(|| anyhow!("Search engine '{}' not configured", engine_name))?;

        if !engine.enabled {
            return Err(anyhow!("Search engine '{}' is disabled", engine_name));
        }

        // Build search query from keywords
        let query = self.build_search_query(keywords);
        info!("Searching with query: {}", query);

        // Perform actual search using the configured engine
        self.perform_actual_search(&query, engine).await
    }

    /// Build search query string from keywords
    fn build_search_query(&self, keywords: &[SearchKeyword]) -> String {
        let mut query_parts = Vec::new();

        for keyword in keywords.iter().take(5) {
            // Use top 5 keywords
            match keyword.category.as_str() {
                "error_code" => query_parts.push(format!("\"{}\"", keyword.keyword)),
                "concept" => query_parts.push(keyword.keyword.clone()),
                "solution" => query_parts.push(format!("\"{}\" solution", keyword.keyword)),
                _ => query_parts.push(keyword.keyword.clone()),
            }
        }

        query_parts.join(" ")
    }

    /// Perform actual search using the configured engine
    async fn perform_actual_search(
        &self,
        query: &str,
        engine: &SearchEngineConfig,
    ) -> Result<Vec<SearchResult>> {
        match engine.name.as_str() {
            "DuckDuckGo" => self.duckduckgo_search(query, engine).await,
            "Bing" => self.bing_search(query, engine).await,
            "Google" => self.google_search(query, engine).await,
            _ => {
                warn!("Unsupported search engine: {}", engine.name);
                self.fallback_search_results(query).await
            }
        }
    }

    /// DuckDuckGo search implementation
    async fn duckduckgo_search(
        &self,
        query: &str,
        engine: &SearchEngineConfig,
    ) -> Result<Vec<SearchResult>> {
        use reqwest;

        let client = reqwest::Client::new();
        let url = format!(
            "{}?q={}&format=json&no_html=1&skip_disambig=1",
            engine.base_url,
            urlencoding::encode(query)
        );

        match client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<Value>().await {
                        Ok(json) => {
                            self.parse_duckduckgo_results(&json, engine.max_results)
                                .await
                        }
                        Err(e) => {
                            warn!("Failed to parse DuckDuckGo response: {}", e);
                            self.fallback_search_results(query).await
                        }
                    }
                } else {
                    warn!("DuckDuckGo API returned status: {}", response.status());
                    self.fallback_search_results(query).await
                }
            }
            Err(e) => {
                warn!("Failed to call DuckDuckGo API: {}", e);
                self.fallback_search_results(query).await
            }
        }
    }

    /// Parse DuckDuckGo JSON response
    async fn parse_duckduckgo_results(
        &self,
        json: &Value,
        max_results: usize,
    ) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        // DuckDuckGo returns results in multiple sections
        let sections = ["AbstractText", "RelatedTopics", "Results"];

        for section in &sections {
            if let Some(section_data) = json.get(section) {
                match *section {
                    "AbstractText" => {
                        if let Some(text) = section_data.as_str() {
                            if !text.is_empty() {
                                results.push(SearchResult {
                                    title: "DuckDuckGo Abstract".to_string(),
                                    url: json
                                        .get("AbstractURL")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("https://duckduckgo.com")
                                        .to_string(),
                                    snippet: text.to_string(),
                                    relevance_score: 0.9,
                                });
                            }
                        }
                    }
                    "RelatedTopics" => {
                        if let Some(topics) = section_data.as_array() {
                            for topic in topics.iter().take(max_results / 2) {
                                if let (Some(text), Some(url)) = (
                                    topic.get("Text").and_then(|v| v.as_str()),
                                    topic.get("FirstURL").and_then(|v| v.as_str()),
                                ) {
                                    results.push(SearchResult {
                                        title: format!(
                                            "Related: {}",
                                            text.split(" - ").next().unwrap_or(text)
                                        ),
                                        url: url.to_string(),
                                        snippet: text.to_string(),
                                        relevance_score: 0.7,
                                    });
                                }
                            }
                        }
                    }
                    "Results" => {
                        if let Some(search_results) = section_data.as_array() {
                            for result in search_results.iter().take(max_results / 2) {
                                if let (Some(text), Some(url)) = (
                                    result.get("Text").and_then(|v| v.as_str()),
                                    result.get("FirstURL").and_then(|v| v.as_str()),
                                ) {
                                    results.push(SearchResult {
                                        title: text.split(" - ").next().unwrap_or(text).to_string(),
                                        url: url.to_string(),
                                        snippet: text.to_string(),
                                        relevance_score: 0.8,
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if results.is_empty() {
            return self.fallback_search_results("DuckDuckGo search").await;
        }

        // Sort by relevance and limit results
        results.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(max_results);

        Ok(results)
    }

    /// Bing search implementation (placeholder)
    async fn bing_search(
        &self,
        query: &str,
        engine: &SearchEngineConfig,
    ) -> Result<Vec<SearchResult>> {
        if let Some(_api_key) = &engine.api_key {
            // TODO: Implement Bing search API
            warn!("Bing search not yet implemented, falling back");
        }
        self.fallback_search_results(query).await
    }

    /// Google search implementation (placeholder)
    async fn google_search(
        &self,
        query: &str,
        engine: &SearchEngineConfig,
    ) -> Result<Vec<SearchResult>> {
        if let Some(_api_key) = &engine.api_key {
            // TODO: Implement Google Custom Search API
            warn!("Google search not yet implemented, falling back");
        }
        self.fallback_search_results(query).await
    }

    /// Fallback search results when API fails
    async fn fallback_search_results(&self, query: &str) -> Result<Vec<SearchResult>> {
        Ok(vec![
            SearchResult {
                title: format!("Rust Error Documentation: {}", query),
                url: "https://doc.rust-lang.org/error-index.html".to_string(),
                snippet:
                    "Official Rust error documentation with detailed explanations and solutions."
                        .to_string(),
                relevance_score: 0.9,
            },
            SearchResult {
                title: format!("Stack Overflow Rust Questions: {}", query),
                url: "https://stackoverflow.com/questions/tagged/rust".to_string(),
                snippet:
                    "Community discussions and solutions for Rust compilation errors and issues."
                        .to_string(),
                relevance_score: 0.8,
            },
            SearchResult {
                title: "The Rust Programming Language Book".to_string(),
                url: "https://doc.rust-lang.org/book/".to_string(),
                snippet: "Comprehensive guide to Rust programming concepts and error handling."
                    .to_string(),
                relevance_score: 0.7,
            },
        ])
    }

    /// Load configuration from config.toml
    async fn load_config() -> Result<WebSearcherConfig> {
        use config::{Config, File};
        use serde::Deserialize;

        #[derive(Debug, Deserialize)]
        struct ConfigFile {
            web_searcher: Option<WebSearcherSection>,
        }

        #[derive(Debug, Deserialize)]
        struct WebSearcherSection {
            default_engine: Option<String>,
            max_keywords: Option<usize>,
            engines: Option<HashMap<String, EngineConfig>>,
        }

        #[derive(Debug, Deserialize)]
        struct EngineConfig {
            name: String,
            base_url: String,
            api_key: Option<String>,
            max_results: Option<usize>,
            enabled: Option<bool>,
        }

        // Try multiple possible paths for the config file
        let possible_paths = [
            "config/config.toml",
            "../config/config.toml",
            "../../config/config.toml",
            "../../../config/config.toml",
        ];

        let mut config_builder = Config::builder();
        let mut found_config = false;

        for path in &possible_paths {
            if std::path::Path::new(path).exists() {
                config_builder = config_builder.add_source(File::with_name(path));
                found_config = true;
                break;
            }
        }

        let mut engines = HashMap::new();

        if found_config {
            match config_builder
                .build()
                .and_then(|c| c.try_deserialize::<ConfigFile>())
            {
                Ok(config_file) => {
                    if let Some(web_config) = config_file.web_searcher {
                        // Load engines from config
                        if let Some(engine_configs) = web_config.engines {
                            for (name, engine_config) in engine_configs {
                                engines.insert(
                                    name,
                                    SearchEngineConfig {
                                        name: engine_config.name,
                                        base_url: engine_config.base_url,
                                        api_key: engine_config.api_key,
                                        max_results: engine_config.max_results.unwrap_or(10),
                                        enabled: engine_config.enabled.unwrap_or(true),
                                    },
                                );
                            }
                        }

                        return Ok(WebSearcherConfig {
                            default_engine: web_config
                                .default_engine
                                .unwrap_or_else(|| "duckduckgo".to_string()),
                            max_keywords: web_config.max_keywords.unwrap_or(10),
                            engines,
                        });
                    }
                }
                Err(e) => {
                    warn!("Failed to parse config file: {}", e);
                }
            }
        }

        // Fallback to default configuration
        engines.insert(
            "duckduckgo".to_string(),
            SearchEngineConfig {
                name: "DuckDuckGo".to_string(),
                base_url: "https://api.duckduckgo.com/".to_string(),
                api_key: None,
                max_results: 10,
                enabled: true,
            },
        );

        engines.insert(
            "bing".to_string(),
            SearchEngineConfig {
                name: "Bing".to_string(),
                base_url: "https://api.bing.microsoft.com/v7.0/search".to_string(),
                api_key: std::env::var("BING_API_KEY").ok(),
                max_results: 10,
                enabled: std::env::var("BING_API_KEY").is_ok(),
            },
        );

        info!("Using default web searcher configuration");
        Ok(WebSearcherConfig {
            default_engine: "duckduckgo".to_string(),
            max_keywords: 10,
            engines,
        })
    }

    /// Load search prompt template from config/prompts/web_search.md
    async fn load_search_prompt() -> Result<String> {
        let possible_paths = [
            "config/prompts/web_search.md",
            "../config/prompts/web_search.md",
            "../../config/prompts/web_search.md",
            "../../../config/prompts/web_search.md",
        ];

        for path in &possible_paths {
            match fs::read_to_string(path).await {
                Ok(content) => {
                    info!("Loaded search prompt from {}", path);
                    return Ok(content);
                }
                Err(_) => continue,
            }
        }

        warn!("Failed to load search prompt from any location, using default");
        Ok(Self::default_search_prompt())
    }

    /// Default search prompt if file loading fails
    fn default_search_prompt() -> String {
        r#"请分析以下Rust编译错误，提取适合网络搜索的关键词。

输出格式：每行一个关键词，格式为 "关键词|类型|相关度"
- 类型可以是：error_code, concept, solution, library, general
- 相关度是0.0到1.0的浮点数，1.0表示最相关

示例输出：
"E0382|error_code|1.0
"borrow checker|concept|0.9
"move semantics solution|solution|0.8

请基于提供的错误信息生成关键词："#
            .to_string()
    }
}

/// Convenience function to perform a quick error search
pub async fn search_rust_error(
    error_message: String,
    code_context: Option<String>,
) -> Result<SearchResponse> {
    let searcher = WebSearcher::new().await?;
    let request = SearchRequest {
        error_message,
        code_context,
        project_context: None,
    };
    searcher.search_error(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_keyword_parsing() {
        let searcher = WebSearcher::new().await.unwrap();
        let response = r#"
"E0382|error_code|1.0
"borrow checker|concept|0.9
"move after use|general|0.8
        "#;

        let keywords = searcher.parse_keywords_response(response).unwrap();
        assert_eq!(keywords.len(), 3);
        assert_eq!(keywords[0].keyword, "E0382");
        assert_eq!(keywords[0].category, "error_code");
        assert_eq!(keywords[0].relevance, 1.0);
    }

    #[tokio::test]
    async fn test_search_query_building() {
        let searcher = WebSearcher::new().await.unwrap();
        let keywords = vec![
            SearchKeyword {
                keyword: "E0382".to_string(),
                relevance: 1.0,
                category: "error_code".to_string(),
            },
            SearchKeyword {
                keyword: "borrow checker".to_string(),
                relevance: 0.9,
                category: "concept".to_string(),
            },
        ];

        let query = searcher.build_search_query(&keywords);
        assert!(query.contains("E0382"));
        assert!(query.contains("borrow checker"));
    }
}
