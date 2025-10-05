use anyhow::Result;
use db_services::DatabaseManager;
use llm_requester::llm_request_with_prompt;
use log::{info, warn};
use prompt_builder::PromptBuilder;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::time::{Duration, timeout};

// 导入各模块
use crate::ai_optimizer::{ai_analyze_final_failure, ai_optimize_rust_code};
use crate::c2rust_translator::c2rust_translate;
use crate::code_splitter::{MAX_TOTAL_PROMPT_CHARS, make_messages_with_function_chunks, total_len};
use crate::file_processor::{create_rust_project_structure, process_c_h_files};
use crate::pkg_config::get_config;
use crate::rust_verifier::{extract_key_errors, verify_compilation};

/// 阶段状态回调类型
pub type StageCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Rust 代码提取器组件
struct RustCodeExtractor;

impl RustCodeExtractor {
    /// 提取 LLM 响应中的 Rust 代码
    fn extract_rust_code_from_response(llm_response: &str) -> Result<String> {
        let mut rust_code = None;

        // 方法1: 直接JSON格式
        if let Ok(json_response) = serde_json::from_str::<Value>(llm_response) {
            if let Some(code) = json_response["rust_code"].as_str() {
                info!("成功从JSON响应中提取rust_code字段");
                rust_code = Some(code.to_string());
            } else if let Some(choices) = json_response["choices"].as_array() {
                if let Some(first_choice) = choices.first() {
                    if let Some(message) = first_choice["message"].as_object() {
                        if let Some(content) = message["content"].as_str() {
                            info!("成功从OpenAI格式响应中提取内容");
                            rust_code = Some(content.to_string());
                        }
                    }
                }
            }
        }

        // 方法2: 处理被代码块包裹的JSON
        if rust_code.is_none() {
            rust_code = Self::extract_from_code_blocks(llm_response);
        }

        // 方法3: 尝试从不完整的JSON中提取rust_code
        if rust_code.is_none() {
            rust_code = Self::extract_from_incomplete_json(llm_response);
        }

        // 方法4: 整个响应作为兜底
        rust_code.ok_or_else(|| anyhow::anyhow!("无法从LLM响应中提取Rust代码"))
    }

    fn extract_from_code_blocks(llm_response: &str) -> Option<String> {
        let cleaned_response = llm_response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        if let Ok(json_response) = serde_json::from_str::<Value>(cleaned_response) {
            if let Some(code) = json_response["rust_code"].as_str() {
                info!("成功从清理后的JSON响应中提取rust_code字段");
                return Some(code.to_string());
            }
        }

        // 尝试 ```rust 代码块
        if let Some(start_idx) = llm_response.find("```rust") {
            let code_start = if llm_response[start_idx..].starts_with("```rust\n") {
                start_idx + 8
            } else {
                start_idx + 7
            };

            if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                let code_end = code_start + end_idx;
                info!("成功从```rust代码块中提取代码");
                return Some(llm_response[code_start..code_end].to_string());
            } else if let Some(end_idx) = llm_response[code_start..].find("```") {
                let code_end = code_start + end_idx;
                warn!("从```rust代码块中提取代码（无结束换行符）");
                return Some(llm_response[code_start..code_end].to_string());
            }
        }

        // 尝试通用代码块 ```
        if let Some(start_idx) = llm_response.find("```\n") {
            let code_start = start_idx + 4;
            if let Some(end_idx) = llm_response[code_start..].find("\n```") {
                let code_end = code_start + end_idx;
                info!("成功从通用代码块中提取代码");
                return Some(llm_response[code_start..code_end].to_string());
            }
        }

        None
    }

    fn extract_from_incomplete_json(llm_response: &str) -> Option<String> {
        if let Some(start_pos) = llm_response.find(r#""rust_code""#) {
            if let Some(colon_pos) = llm_response[start_pos..].find(':') {
                let value_start = start_pos + colon_pos + 1;
                let remaining = &llm_response[value_start..].trim_start();

                if remaining.starts_with('"') {
                    let content_start =
                        value_start + (llm_response[value_start..].len() - remaining.len()) + 1;
                    let bytes = llm_response.as_bytes();
                    let mut pos = content_start;
                    let mut escaped = false;

                    while pos < bytes.len() {
                        if escaped {
                            escaped = false;
                        } else if bytes[pos] == b'\\' {
                            escaped = true;
                        } else if bytes[pos] == b'"' {
                            if let Ok(json_str) =
                                String::from_utf8(bytes[content_start..pos].to_vec())
                            {
                                let unescaped = json_str
                                    .replace(r"\n", "\n")
                                    .replace(r"\t", "\t")
                                    .replace(r#"\""#, "\"")
                                    .replace(r"\\", "\\");
                                info!("从不完整JSON中成功提取并解码rust_code字段");
                                return Some(unescaped);
                            }
                        }
                        pos += 1;
                    }
                }
            }
        }
        None
    }
}

/// 编译验证器组件
struct CompilationVerifier {
    max_retries: usize,
}

impl CompilationVerifier {
    fn new(max_retries: usize) -> Self {
        Self { max_retries }
    }

    async fn verify_with_retry(
        &self,
        project_path: &Path,
        processed_c_file: &Path,
        rust_output_path: &Path,
        callback: Option<&StageCallback>,
    ) -> Result<()> {
        let notify = |msg: &str| {
            if let Some(cb) = callback {
                cb(msg);
            }
        };

        for attempt in 1..=self.max_retries {
            notify(&format!(
                "🔍 编译验证 (尝试 {}/{})",
                attempt, self.max_retries
            ));
            info!("🔍 开始编译验证（尝试 {}/{}）", attempt, self.max_retries);

            match verify_compilation(project_path) {
                Ok(_) => {
                    info!("🎉 编译验证通过！");
                    notify("🎉 编译通过！");
                    return Ok(());
                }
                Err(e) => {
                    if attempt < self.max_retries {
                        warn!(
                            "❌ 编译失败（尝试 {}/{}），准备重试",
                            attempt, self.max_retries
                        );
                        notify(&format!(
                            "❌ 编译失败，准备重试 ({}/{})",
                            attempt, self.max_retries
                        ));

                        let key_errors = extract_key_errors(&e.to_string());
                        info!("关键错误信息：\n{}", key_errors);

                        // 返回错误信息供调用者处理重试逻辑
                        return Err(anyhow::anyhow!("编译失败: {}", key_errors));
                    } else {
                        return self
                            .handle_final_failure(
                                e,
                                project_path,
                                processed_c_file,
                                rust_output_path,
                                callback,
                            )
                            .await;
                    }
                }
            }
        }

        Err(anyhow::anyhow!("未知的编译验证状态"))
    }

    async fn handle_final_failure(
        &self,
        error: anyhow::Error,
        project_path: &Path,
        processed_c_file: &Path,
        rust_output_path: &Path,
        callback: Option<&StageCallback>,
    ) -> Result<()> {
        let notify = |msg: &str| {
            if let Some(cb) = callback {
                cb(msg);
            }
        };

        warn!("❌ 编译验证失败，已达最大重试次数 {}", self.max_retries);
        warn!("最后的编译错误: {}", error);
        notify(&format!(
            "❌ 编译失败，已达重试上限 ({} 次)",
            self.max_retries
        ));

        notify("💾 正在保存错误日志...");
        let error_log_path = project_path.join("final_compile_errors.txt");
        fs::write(&error_log_path, error.to_string())?;
        info!("编译错误已保存到: {:?}", error_log_path);
        notify(&format!("✓ 错误日志已保存: {}", error_log_path.display()));

        let final_key_errors = extract_key_errors(&error.to_string());
        notify(&format!(
            "🔍 识别到 {} 个关键错误",
            final_key_errors.lines().count()
        ));

        notify("🤖 正在请求AI诊断分析（这可能需要几分钟）...");
        match ai_analyze_final_failure(processed_c_file, rust_output_path, &final_key_errors).await
        {
            Ok(feedback) => {
                let feedback_path = project_path.join("ai_failure_feedback.md");
                fs::write(&feedback_path, &feedback)?;
                info!("AI诊断建议已保存到: {:?}", feedback_path);
                notify(&format!("💡 AI诊断建议已生成: {}", feedback_path.display()));
                notify("📖 请查看诊断报告了解失败原因和建议");
            }
            Err(ai_err) => {
                warn!("AI 诊断失败: {}", ai_err);
                notify(&format!("⚠️ AI诊断失败: {}", ai_err));
                let feedback_error_path = project_path.join("ai_failure_feedback_error.txt");
                fs::write(&feedback_error_path, ai_err.to_string())?;
                notify(&format!(
                    "✓ 错误详情已保存: {}",
                    feedback_error_path.display()
                ));
            }
        }

        Err(anyhow::anyhow!(
            "两阶段翻译失败：AI优化后的代码经过 {} 次尝试仍无法编译通过",
            self.max_retries
        ))
    }
}

/// LLM 请求器组件
struct LLMRequester;

impl LLMRequester {
    async fn request_translation(
        prompt: &str,
        content: &str,
        timeout_seconds: u64,
    ) -> Result<String> {
        let enhanced_prompt = format!(
            "{}\n\n请将下面传输的 C 代码片段整体转换为一个可编译的 Rust main.rs（保持功能等价、可编译）。当你收到所有片段后再开始输出最终结果。",
            prompt
        );

        let messages = make_messages_with_function_chunks(
            &enhanced_prompt,
            "以下是处理后的 C 代码",
            content,
            true,
            MAX_TOTAL_PROMPT_CHARS,
        );

        info!(
            "生成的消息条数: {}，总长度: {} 字符",
            messages.len(),
            total_len(&messages)
        );

        let timeout_duration = Duration::from_secs(timeout_seconds);
        match timeout(
            timeout_duration,
            llm_request_with_prompt(
                messages,
                "你是一位C到Rust代码转换专家，特别擅长文件系统和FUSE相关的代码转换".to_string(),
            ),
        )
        .await
        {
            Ok(Ok(response)) => {
                info!("LLM响应接收成功，长度: {} 字符", response.len());
                Ok(response)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow::anyhow!(
                "LLM请求超时，未能在{}秒内获取响应",
                timeout_seconds
            )),
        }
    }
}

/// 主翻译处理器
pub struct TranslationProcessor {
    callback: Option<StageCallback>,
    db_manager: DatabaseManager,
    verifier: CompilationVerifier,
}

impl TranslationProcessor {
    pub async fn new(callback: Option<StageCallback>) -> Result<Self> {
        let db_manager = DatabaseManager::new_default().await?;
        let config = get_config()?;
        let max_retries = config.max_retry_attempts;
        let verifier = CompilationVerifier::new(max_retries.try_into().unwrap());

        Ok(Self {
            callback,
            db_manager,
            verifier,
        })
    }

    /// 通知回调
    fn notify(&self, msg: &str) {
        if let Some(ref cb) = self.callback {
            cb(msg);
        }
    }

    /// 处理单个文件 - 纯 AI 翻译模式
    pub async fn process_single_file(&self, file_path: &Path) -> Result<()> {
        self.notify("🚀 开始处理单个文件（纯AI翻译模式）");
        info!("开始处理文件: {:?}", file_path);

        self.notify(&format!("📂 正在分析文件: {}", file_path.display()));
        let prompt_builder = PromptBuilder::new(
            &self.db_manager,
            "c_project".to_string(),
            Some(file_path.to_path_buf()),
        )
        .await?;

        self.notify("🔍 正在构建上下文提示词...");
        let prompt = prompt_builder
            .build_file_context_prompt(file_path, None)
            .await?;
        self.notify("✓ 上下文提示词构建完成");

        self.notify("📝 正在预处理C/H文件...");
        let processed_file = process_c_h_files(file_path)?;
        let content = fs::read_to_string(&processed_file)?;
        self.notify(&format!(
            "✓ 文件预处理完成，代码长度: {} 字符",
            content.len()
        ));

        self.notify("🤖 正在请求AI翻译（这可能需要几分钟）...");
        let llm_response = LLMRequester::request_translation(&prompt, &content, 6000).await?;
        self.notify("✓ AI响应接收完成");

        self.notify("📦 正在提取Rust代码...");
        let rust_code = RustCodeExtractor::extract_rust_code_from_response(&llm_response)?;
        self.notify(&format!(
            "✓ 成功提取Rust代码，长度: {} 字符",
            rust_code.len()
        ));

        self.notify("💾 正在保存Rust项目...");
        let rust_project_path = file_path.join("rust-project");
        self.save_rust_project(&rust_project_path, &rust_code)?;

        info!("纯AI翻译完成，结果保存到: {:?}", rust_project_path);
        self.notify(&format!(
            "✅ 纯AI翻译完成！项目保存至: {}",
            rust_project_path.display()
        ));
        Ok(())
    }

    /// 两阶段翻译主函数
    pub async fn process_two_stage(&self, file_path: &Path) -> Result<()> {
        self.notify("🚀 开始两阶段翻译处理（C2Rust + AI优化模式）");
        info!("开始两阶段翻译处理: {:?}", file_path);

        self.notify(&format!("📂 目标文件: {}", file_path.display()));
        self.notify("📝 正在预处理C文件...");
        let processed_c_file = process_c_h_files(file_path)?;
        info!("要翻译的C文件: {:?}", processed_c_file);
        self.notify(&format!(
            "✓ C文件预处理完成: {}",
            processed_c_file.display()
        ));

        // 第一阶段：C2Rust 翻译
        self.notify("📍 开始第一阶段：C2Rust自动翻译");
        let (work_dir, c2rust_output) = self.execute_stage1(&processed_c_file, file_path).await?;

        // 第二阶段：AI 优化 + 编译验证
        self.notify("📍 开始第二阶段：AI优化与编译验证");
        self.execute_stage2(&work_dir, &c2rust_output, &processed_c_file)
            .await?;

        info!("✅ 两阶段翻译处理完成");
        self.notify(&format!(
            "🎉 两阶段翻译全部完成！工作目录: {}",
            work_dir.display()
        ));
        Ok(())
    }

    async fn execute_stage1(
        &self,
        processed_c_file: &Path,
        original_path: &Path,
    ) -> Result<(PathBuf, PathBuf)> {
        self.notify("🔄 【阶段 1/2】C2Rust自动翻译");
        info!("🔄 第一阶段：C2Rust 自动翻译");

        self.notify("📁 正在创建工作目录...");
        let work_dir = original_path.join("two-stage-translation");
        let c2rust_dir = work_dir.join("c2rust-output");
        fs::create_dir_all(&c2rust_dir)?;
        self.notify(&format!("✓ 工作目录创建完成: {}", work_dir.display()));

        self.notify("⚙️ 正在执行C2Rust翻译工具...");
        match c2rust_translate(processed_c_file, &c2rust_dir).await {
            Ok(path) => {
                info!("✅ C2Rust 翻译成功: {:?}", path);
                self.notify(&format!("✅ C2Rust翻译成功！输出: {}", path.display()));
                Ok((work_dir, path))
            }
            Err(e) => {
                warn!("⚠️ C2Rust 翻译失败: {}，将切换到纯AI模式", e);
                self.notify("⚠️ C2Rust翻译失败，自动切换到纯AI翻译模式");
                self.notify("🔄 正在启动纯AI翻译流程...");
                self.process_single_file(original_path).await?;
                Err(e)
            }
        }
    }

    async fn execute_stage2(
        &self,
        work_dir: &Path,
        c2rust_output: &Path,
        processed_c_file: &Path,
    ) -> Result<()> {
        self.notify("🔄 【阶段 2/2】AI优化与编译验证");
        info!("🔄 第二阶段：AI 代码优化 + 编译验证");

        self.notify("📁 正在创建最终输出目录...");
        let final_dir = work_dir.join("final-output");
        let final_output_path = final_dir.join("src").join("main.rs");

        create_rust_project_structure(&final_dir)?;
        self.notify(&format!("✓ 项目结构创建完成: {}", final_dir.display()));

        let mut compile_errors: Option<String> = None;

        for attempt in 1..=self.verifier.max_retries {
            self.notify(&format!(
                "🔄 【迭代 {}/{}】AI优化与编译验证",
                attempt, self.verifier.max_retries
            ));
            info!("🔄 AI优化尝试 {}/{}", attempt, self.verifier.max_retries);

            if let Some(ref errors) = compile_errors {
                self.notify(&format!(
                    "📋 上次编译错误: {} 个问题",
                    errors.lines().count()
                ));
            }

            self.notify("🤖 正在请求AI优化代码...");
            let optimized_code = ai_optimize_rust_code(
                c2rust_output,
                processed_c_file,
                &final_dir,
                compile_errors.as_deref(),
            )
            .await?;

            self.notify(&format!(
                "✓ AI优化完成，代码长度: {} 字符",
                optimized_code.len()
            ));
            fs::write(&final_output_path, &optimized_code)?;
            info!("✅ AI优化代码已保存: {:?}", final_output_path);
            self.notify(&format!("💾 代码已保存: {}", final_output_path.display()));

            self.notify("🔨 正在编译验证...");
            // 编译验证
            match self
                .verifier
                .verify_with_retry(
                    &final_dir,
                    processed_c_file,
                    &final_output_path,
                    self.callback.as_ref(),
                )
                .await
            {
                Ok(_) => {
                    self.notify("🎉 编译验证通过！");
                    // 备份原始C2Rust输出
                    self.notify("💾 正在备份C2Rust原始输出...");
                    self.backup_c2rust_output(c2rust_output, &final_dir)?;
                    self.notify("✓ 备份完成");
                    self.notify(&format!(
                        "✅ 第二阶段完成！最终项目: {}",
                        final_dir.display()
                    ));
                    return Ok(());
                }
                Err(e) => {
                    if attempt < self.verifier.max_retries {
                        compile_errors = Some(e.to_string());
                        self.notify(&format!("⚠️ 编译失败，将进行第 {} 次重试", attempt + 1));
                    } else {
                        self.notify("❌ 已达最大重试次数，编译验证失败");
                        return Err(e);
                    }
                }
            }
        }

        Err(anyhow::anyhow!("两阶段翻译失败：未知错误"))
    }

    fn save_rust_project(&self, project_path: &Path, rust_code: &str) -> Result<()> {
        self.notify("📁 正在创建Rust项目结构...");
        create_rust_project_structure(project_path)?;
        self.notify("✓ 项目结构创建完成");

        let output_file_path = project_path.join("src").join("main.rs");
        self.notify(&format!("💾 正在写入文件: {}", output_file_path.display()));
        let mut output_file = File::create(&output_file_path)?;
        write!(output_file, "{}", rust_code)?;
        info!("转换结果已保存到: {:?}", output_file_path);
        self.notify(&format!("✓ 文件保存成功 ({} 字节)", rust_code.len()));
        Ok(())
    }

    fn backup_c2rust_output(&self, c2rust_output: &Path, final_dir: &Path) -> Result<()> {
        let c2rust_backup_path = final_dir.join("c2rust_original.rs");
        if let Ok(c2rust_content) = fs::read_to_string(c2rust_output) {
            fs::write(&c2rust_backup_path, &c2rust_content)?;
            info!("📄 C2Rust 原始输出已备份到: {:?}", c2rust_backup_path);
            self.notify(&format!(
                "📄 C2Rust原始输出已备份: {}",
                c2rust_backup_path.display()
            ));
        }
        Ok(())
    }
}

pub async fn two_stage_processor_with_callback(
    file_path: &Path,
    callback: Option<StageCallback>,
) -> Result<()> {
    let processor = TranslationProcessor::new(callback).await?;
    processor.process_two_stage(file_path).await
}

pub async fn singlefile_processor(file_path: &Path, callback: Option<StageCallback>) -> Result<()> {
    let processor = TranslationProcessor::new(callback).await?;
    processor.process_single_file(file_path).await
}
