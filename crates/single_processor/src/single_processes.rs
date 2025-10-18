use anyhow::Result;
use log::{info, warn};
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[allow(unused_imports)]
use agent::Agent;
// 导入各模块

// 本地定义，替代 ai_optimizer::OptimizedResult
#[derive(Debug, Clone)]
struct OptimizedResult {
    rust_code: String,
    cargo_crates: Vec<String>,
}
use crate::file_processor::process_c_h_files;
use crate::file_processor::{
    create_cargo_project_with_code_from_c, detect_project_type_from_c, write_rust_code_to_project,
};
use crate::pkg_config::get_config;
use crate::rust_verifier::{extract_key_errors, verify_compilation};

/// 阶段状态回调类型
pub type StageCallback = Arc<dyn Fn(&str) + Send + Sync>;

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
        // 使用 Agent 搜索错误解决方案并生成诊断报告
        let project_name = project_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("single_project")
            .to_string();

        let mut agent = match Agent::new(
            project_name,
            project_path.to_path_buf(),
            Some(project_path.to_path_buf()),
        )
        .await
        {
            Ok(a) => a,
            Err(e) => {
                warn!("初始化 Agent 失败: {}", e);
                notify(&format!("⚠️ AI诊断初始化失败: {}", e));
                let feedback_error_path = project_path.join("ai_failure_feedback_error.txt");
                fs::write(&feedback_error_path, format!("Agent init failed: {}", e))?;
                notify(&format!(
                    "✓ 错误详情已保存: {}",
                    feedback_error_path.display()
                ));
                return Err(anyhow::anyhow!("编译失败，且AI诊断初始化失败"));
            }
        };

        // 初始化非必需组件（尽力而为）
        let _ = agent.initialize_file_manager().await;

        match agent.search_error_solution(&final_key_errors).await {
            Ok(solution) => {
                let mut md = String::new();
                md.push_str("# AI 失败诊断报告\n\n");
                md.push_str("## 编译关键错误\n\n```\n");
                md.push_str(&final_key_errors);
                md.push_str("\n```\n\n");
                md.push_str("## 搜索解法概览\n\n");
                md.push_str(&format!(
                    "- 错误类别: {}\n- 方案数量: {}\n- 置信度: {:?}\n",
                    solution.error_info.error_category,
                    solution.solutions.len(),
                    solution.metadata.confidence_level
                ));
                if let Some(first) = solution.solutions.first() {
                    md.push_str(&format!("- Top 方案: {}\n", first.title));
                }
                let feedback_path = project_path.join("ai_failure_feedback.md");
                fs::write(&feedback_path, &md)?;
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

/// 主翻译处理器
pub struct TranslationProcessor {
    callback: Option<StageCallback>,
    verifier: CompilationVerifier,
}

impl TranslationProcessor {
    pub async fn new(callback: Option<StageCallback>) -> Result<Self> {
        let config = get_config()?;
        let max_retries = config.max_retry_attempts;
        let verifier = CompilationVerifier::new(max_retries.try_into().unwrap());

        Ok(Self { callback, verifier })
    }

    /// 在指定项目目录执行 `cargo add` 添加依赖，并在进度回调中展示
    fn add_cargo_deps_with_progress(&self, project_dir: &Path, crates: &[String]) -> Result<()> {
        if crates.is_empty() {
            return Ok(());
        }
        self.notify("📦 检测到需要添加的依赖，开始执行 cargo add …");
        for (idx, krate) in crates.iter().enumerate() {
            self.notify(&format!(
                "📦 ({}/{}) cargo add {}",
                idx + 1,
                crates.len(),
                krate
            ));
            // 在项目目录运行 cargo add <crate>
            let output = std::process::Command::new("cargo")
                .arg("add")
                .arg(krate)
                .current_dir(project_dir)
                .output();
            match output {
                Ok(out) => {
                    if out.status.success() {
                        self.notify(&format!("✅ 已添加: {}", krate));
                    } else {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        warn!("cargo add {} 失败: {}\n{}", krate, stderr, stdout);
                        self.notify(&format!("⚠️ 添加依赖失败: {} (已跳过)", krate));
                    }
                }
                Err(e) => {
                    warn!("执行 cargo add {} 出错: {}", krate, e);
                    self.notify(&format!("⚠️ 执行 cargo add 出错: {} (已跳过)", krate));
                }
            }
        }
        Ok(())
    }

    /// 通知回调
    fn notify(&self, msg: &str) {
        if let Some(ref cb) = self.callback {
            cb(msg);
        }
    }

    async fn try_agent_optimize(
        &self,
        project_dir: &Path,
        source_c_file: &Path,
        compile_errors: Option<&str>,
    ) -> Result<OptimizedResult> {
        // 优先使用统一的 Agent 流程进行 AI 翻译
        let project_name = source_c_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("single_project")
            .to_string();

        let mut agent = Agent::new(
            project_name,
            project_dir.to_path_buf(),
            Some(project_dir.to_path_buf()),
        )
        .await?;

        // 尽力初始化，不作为硬失败条件
        let _ = agent.initialize_file_manager().await;
        let _ = agent.initialize_prompt_builder().await;

        let result = agent.translate_code(source_c_file, compile_errors).await?;

        Ok(OptimizedResult {
            rust_code: result.rust_code,
            cargo_crates: result.cargo_dependencies,

        })
    }

    /// 处理单个文件 - 纯 AI 翻译模式
    pub async fn process_single_file(&self, file_path: &Path) -> Result<()> {
        self.notify("🚀 开始处理单个文件（纯AI翻译模式）");
        info!("开始处理路径: {:?}", file_path);
        self.notify("🔄 【阶段 2/2】AI优化与编译验证");
        info!("🔄 第二阶段：AI 代码优化 + 编译验证");

        // 规范化：将输入统一为“项目目录”，并预处理 C/H 文件，获取待处理的 C 文件路径
        let project_dir = if file_path.is_dir() {
            file_path.to_path_buf()
        } else {
            file_path
                .parent()
                .map(|p| p.to_path_buf())
                .ok_or_else(|| anyhow::anyhow!("无法确定项目目录: {}", file_path.display()))?
        };
        self.notify("📝 正在预处理C文件...");
        let processed_c_file = process_c_h_files(&project_dir)?;
        info!("要翻译的C文件: {:?}", processed_c_file);
        self.notify(&format!(
            "✓ C文件预处理完成: {}",
            processed_c_file.display()
        ));

        self.notify("📁 正在创建最终输出目录...");
        let final_dir = project_dir.join("final-output");

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
            // 使用预处理后的 C 文件作为原始上下文，纯 AI 翻译
            let optimized = match self
                .try_agent_optimize(
                    &final_dir,
                    processed_c_file.as_path(),
                    compile_errors.as_deref(),
                )
                .await
            {
                Ok(res) => res,
                Err(err) => {
                    warn!("Agent 优化失败: {}", err);
                    return Err(err.into());
                }
            };

            // 第一次迭代：使用 cargo new 进行项目初始化（根据 C 文件是否包含 main 判断 bin/lib）
            // 后续迭代：仅覆盖对应 src 文件
            let proj_type = detect_project_type_from_c(&processed_c_file);
            let optimized_rust_path = if !final_dir.exists() || final_dir.read_dir().is_err() {
                // 初始化并写入
                create_cargo_project_with_code_from_c(
                    &final_dir,
                    &optimized.rust_code,
                    &processed_c_file,
                )?
            } else {
                // 已存在项目，直接覆盖源码文件
                write_rust_code_to_project(&final_dir, &optimized.rust_code, proj_type)?
            };
            // 处理 cargo 依赖添加
            self.add_cargo_deps_with_progress(&final_dir, &optimized.cargo_crates)?;
            self.notify(&format!(
                "✓ AI优化完成，代码长度: {} 字符",
                optimized.rust_code.len()
            ));
            info!("✅ AI优化代码已保存: {:?}", optimized_rust_path);
            self.notify(&format!("💾 代码已保存: {}", optimized_rust_path.display()));

            self.notify("🔨 正在编译验证...");
            // 编译验证
            match self
                .verifier
                .verify_with_retry(
                    &final_dir,
                    file_path,
                    &optimized_rust_path,
                    self.callback.as_ref(),
                )
                .await
            {
                Ok(_) => {
                    self.notify("🎉 编译验证通过！");
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

    fn _save_rust_project(&self, project_path: &Path, rust_code: &str) -> Result<()> {
        use crate::file_processor::{
            RustFileType, create_rust_project_structure_with_type, detect_rust_file_type,
        };
        self.notify("📁 正在创建Rust项目结构...");
        let rust_file_path = create_rust_project_structure_with_type(project_path, rust_code)?;
        self.notify("✓ 项目结构创建完成");

        let file_type = detect_rust_file_type(rust_code);
        let file_type_str = match file_type {
            RustFileType::Package => "package (main.rs)",
            RustFileType::Lib => "lib (lib.rs)",
        };
        self.notify(&format!("💾 文件类型自动识别为: {}", file_type_str));
        self.notify(&format!("💾 正在写入文件: {}", rust_file_path.display()));
        // 文件已由 create_rust_project_structure_with_type 写入，无需重复写入
        info!("转换结果已保存到: {:?}", rust_file_path);
        self.notify(&format!("✓ 文件保存成功 ({} 字节)", rust_code.len()));
        Ok(())
    }


}


pub async fn singlefile_processor(file_path: &Path, callback: Option<StageCallback>) -> Result<()> {
    let processor = TranslationProcessor::new(callback).await?;
    processor.process_single_file(file_path).await
}
