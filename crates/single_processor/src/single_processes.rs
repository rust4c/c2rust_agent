use anyhow::Result;
use log::{info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// 导入各模块
use crate::ai_optimizer::{ai_analyze_final_failure, ai_optimize_rust_code};
use crate::c2rust_translator::c2rust_translate;
use crate::file_processor::create_rust_project_structure_with_type;
use crate::file_processor::process_c_h_files;
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
            let optimized = ai_optimize_rust_code(
                None,
                processed_c_file.as_path(),
                &final_dir,
                compile_errors.as_deref(),
            )
            .await?;

            // 自动识别类型并命名
            let optimized_rust_path =
                create_rust_project_structure_with_type(&final_dir, &optimized.rust_code)?;
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
        let (work_dir, c2rust_output) =
            match self.execute_stage1(&processed_c_file, file_path).await {
                Ok(res) => res,
                Err(_) => {
                    warn!("C2Rust翻译失败，切换到纯AI翻译模式");
                    self.notify("⚠️ C2Rust翻译失败，自动切换到纯AI翻译模式");
                    self.notify("🔄 正在启动纯AI翻译流程...");
                    return self.process_single_file(file_path).await;
                }
            };

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

        use crate::file_processor::create_rust_project_structure_with_type;
        let c2rust_code = fs::read_to_string(c2rust_output)?;
        create_rust_project_structure_with_type(&final_dir, &c2rust_code)?;
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
            let optimized = ai_optimize_rust_code(
                Some(&c2rust_output.to_path_buf()),
                processed_c_file,
                &final_dir,
                compile_errors.as_deref(),
            )
            .await?;

            // 自动识别类型并命名
            let optimized_rust_path =
                create_rust_project_structure_with_type(&final_dir, &optimized.rust_code)?;
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
                    processed_c_file,
                    &optimized_rust_path,
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
