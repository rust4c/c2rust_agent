use anyhow::{Context, Result, anyhow};
use log::{debug, info, warn};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Known Python package mirrors that we try sequentially when installing via `uv`.
#[derive(Debug, Clone)]
pub struct MirrorSource {
    pub name: String,
    pub index_url: Option<String>,
    pub extra_index_url: Option<String>,
}

impl MirrorSource {
    pub fn new<N, I, E>(name: N, index_url: Option<I>, extra_index_url: Option<E>) -> Self
    where
        N: Into<String>,
        I: Into<String>,
        E: Into<String>,
    {
        Self {
            name: name.into(),
            index_url: index_url.map(Into::into),
            extra_index_url: extra_index_url.map(Into::into),
        }
    }
}

/// Default mirror list: PyPI followed by a few popular mainland China mirrors.
pub fn default_uv_mirrors() -> Vec<MirrorSource> {
    vec![
        MirrorSource::new("PyPI", None::<&str>, None::<&str>),
        MirrorSource::new(
            "Tsinghua",
            Some("https://pypi.tuna.tsinghua.edu.cn/simple"),
            None::<&str>,
        ),
        MirrorSource::new(
            "Aliyun",
            Some("https://mirrors.aliyun.com/pypi/simple"),
            None::<&str>,
        ),
        MirrorSource::new(
            "USTC",
            Some("https://pypi.mirrors.ustc.edu.cn/simple"),
            None::<&str>,
        ),
    ]
}

/// Handles `compiledb` availability within a user-provided virtual environment.
#[derive(Debug, Clone)]
pub struct CompiledbInstaller {
    uv_command: PathBuf,
    mirrors: Vec<MirrorSource>,
    package_spec: &'static str,
}

impl Default for CompiledbInstaller {
    fn default() -> Self {
        Self {
            uv_command: PathBuf::from("uv"),
            mirrors: default_uv_mirrors(),
            package_spec: "compiledb",
        }
    }
}

impl CompiledbInstaller {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_uv_command(mut self, uv_command: impl Into<PathBuf>) -> Self {
        self.uv_command = uv_command.into();
        self
    }

    pub fn with_mirrors<I>(mut self, mirrors: I) -> Self
    where
        I: IntoIterator<Item = MirrorSource>,
    {
        self.mirrors = mirrors.into_iter().collect();
        self
    }

    pub fn with_package_spec(mut self, spec: &'static str) -> Self {
        self.package_spec = spec;
        self
    }

    pub fn ensure_installed<P: AsRef<Path>>(&self, venv_path: P) -> Result<()> {
        if self.is_installed(&venv_path)? {
            info!("compiledb already present in {:?}", venv_path.as_ref());
            return Ok(());
        }

        self.install(venv_path)
    }

    pub fn is_installed<P: AsRef<Path>>(&self, venv_path: P) -> Result<bool> {
        let python_path = resolve_python_path(venv_path.as_ref())?;
        Ok(run_uv_pip_show(
            &self.uv_command,
            &python_path,
            self.package_spec,
        )?)
    }

    pub fn install<P: AsRef<Path>>(&self, venv_path: P) -> Result<()> {
        let python_path = resolve_python_path(venv_path.as_ref())?;

        let mut last_error: Option<anyhow::Error> = None;
        for mirror in &self.mirrors {
            match install_with_mirror(&self.uv_command, &python_path, self.package_spec, mirror) {
                Ok(()) => {
                    info!(
                        "compiledb installed successfully via {} mirror",
                        mirror.name
                    );
                    return Ok(());
                }
                Err(err) => {
                    debug!(
                        "uv installation attempt using {} mirror failed: {err:?}",
                        mirror.name
                    );

                    // If it's an error of uv command not found, and we've already tried installing uv, return error directly
                    if self.is_uv_not_found_error(&err) {
                        // Try to install uv tool
                        if let Err(install_err) = self.try_install_uv() {
                            warn!("Automatic uv installation failed: {}", install_err);
                            return Err(err);
                        }

                        // Retry installing compiledb
                        info!("uv installation completed, retrying compiledb installation...");
                        match install_with_mirror(
                            &self.uv_command,
                            &python_path,
                            self.package_spec,
                            mirror,
                        ) {
                            Ok(()) => {
                                info!(
                                    "compiledb installed successfully via {} mirror after installing uv",
                                    mirror.name
                                );
                                return Ok(());
                            }
                            Err(retry_err) => {
                                warn!("Retry installing compiledb failed: {}", retry_err);
                                return Err(retry_err);
                            }
                        }
                    }

                    warn!("Switching uv mirror from {} after failure", mirror.name);
                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            anyhow!("failed to install compiledb via uv: no mirror attempts succeeded")
        }))
    }

    /// Try to install uv tool itself
    fn try_install_uv(&self) -> Result<()> {
        info!("Attempting to install uv tool via pip...");

        let mut command = Command::new("pip");
        command
            .arg("install")
            .arg("uv")
            .arg("--break-system-packages");

        let output = command
            .output()
            .with_context(|| "Unable to execute pip command to install uv")?;

        if output.status.success() {
            info!("uv tool installation successful");
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            "pip installation of uv failed: {}, attempting to use mirror sources...",
            stderr
        );

        // Try to install uv using mirror sources
        for mirror in &self.mirrors {
            if let Some(index_url) = &mirror.index_url {
                let mut mirror_command = Command::new("pip");
                mirror_command
                    .arg("install")
                    .arg("uv")
                    .arg("--break-system-packages")
                    .arg("--index-url")
                    .arg(index_url);

                match mirror_command.output() {
                    Ok(output) if output.status.success() => {
                        info!("通过{}镜像源成功安装uv工具", mirror.name);
                        return Ok(());
                    }
                    Ok(output) => {
                        debug!(
                            "通过{}镜像源安装uv失败: {}",
                            mirror.name,
                            String::from_utf8_lossy(&output.stderr)
                        );
                    }
                    Err(err) => {
                        debug!("执行pip命令失败 ({}镜像源): {}", mirror.name, err);
                    }
                }
            }
        }

        Err(anyhow!(
            "Unable to install uv tool via pip, please install manually"
        ))
    }

    /// Check if error is caused by uv command not found
    fn is_uv_not_found_error(&self, err: &anyhow::Error) -> bool {
        err.chain().any(|cause| {
            if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                return io_err.kind() == ErrorKind::NotFound;
            }
            false
        })
    }
}

pub fn ensure_compiledb_installed<P: AsRef<Path>>(venv_path: P) -> Result<()> {
    CompiledbInstaller::default().ensure_installed(venv_path)
}

pub fn is_compiledb_available<P: AsRef<Path>>(venv_path: P) -> Result<bool> {
    CompiledbInstaller::default().is_installed(venv_path)
}

fn resolve_python_path(venv_path: &Path) -> Result<PathBuf> {
    let unix_candidate = venv_path.join("bin/python");
    if unix_candidate.exists() {
        return Ok(unix_candidate);
    }

    let windows_candidate = venv_path.join("Scripts/python.exe");
    if windows_candidate.exists() {
        return Ok(windows_candidate);
    }

    Err(anyhow!(
        "no Python interpreter found under virtual environment {:?}",
        venv_path
    ))
}

fn run_uv_pip_show(uv_command: &Path, python_path: &Path, package: &str) -> Result<bool> {
    let output = Command::new(uv_command)
        .arg("pip")
        .arg("show")
        .arg(package)
        .arg("--python")
        .arg(python_path)
        .output()
        .with_context(|| format!("failed to invoke uv to inspect {package}"))?;

    if output.status.success() {
        return Ok(true);
    }

    debug!(
        "uv pip show {package} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(false)
}

fn install_with_mirror(
    uv_command: &Path,
    python_path: &Path,
    package: &str,
    mirror: &MirrorSource,
) -> Result<()> {
    fn try_install_command(
        uv_command: &Path,
        python_path: &Path,
        package: &str,
        mirror: &MirrorSource,
    ) -> Result<()> {
        let mut command = Command::new(uv_command);
        command
            .arg("pip")
            .arg("install")
            .arg(package)
            .arg("--python")
            .arg(python_path)
            .arg("--upgrade");

        if let Some(index_url) = &mirror.index_url {
            command.arg("--index-url").arg(index_url);
        }

        if let Some(extra_index_url) = &mirror.extra_index_url {
            command.arg("--extra-index-url").arg(extra_index_url);
        }

        let output = command
            .output()
            .with_context(|| format!("failed to invoke uv using {} mirror", mirror.name))?;

        if output.status.success() {
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut message = format!(
            "uv pip install {package} exited with code {:?} via {} mirror",
            output.status.code(),
            mirror.name
        );

        if !stdout.trim().is_empty() {
            message.push_str("\nstdout:\n");
            message.push_str(stdout.trim());
        }

        if !stderr.trim().is_empty() {
            message.push_str("\nstderr:\n");
            message.push_str(stderr.trim());
        }

        Err(anyhow!(message))
    }

    // First attempt
    match try_install_command(uv_command, python_path, package, mirror) {
        Ok(()) => return Ok(()),
        Err(err) => {
            // Check if it's an error of uv command not found
            if err.chain().any(|cause| {
                if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                    return io_err.kind() == ErrorKind::NotFound;
                }
                false
            }) {
                warn!("Detected uv command not found, attempting to auto-install uv...");

                // Try to install uv
                if let Err(install_err) = try_install_uv_with_mirror(mirror) {
                    warn!("Automatic uv installation failed: {}", install_err);
                    return Err(err);
                }

                // Retry original command
                info!(
                    "uv installation completed, retrying installation of {}...",
                    package
                );
                return try_install_command(uv_command, python_path, package, mirror);
            }

            return Err(err);
        }
    }
}

fn try_install_uv_with_mirror(mirror: &MirrorSource) -> Result<()> {
    info!(
        "Attempting to install uv tool via {} mirror source...",
        mirror.name
    );

    let mut command = Command::new("pip");
    command
        .arg("install")
        .arg("uv")
        .arg("--break-system-packages");

    if let Some(index_url) = &mirror.index_url {
        command.arg("--index-url").arg(index_url);
    }

    let output = command.output().with_context(|| {
        format!(
            "Unable to execute pip command to install uv ({} mirror source)",
            mirror.name
        )
    })?;

    if output.status.success() {
        info!(
            "Successfully installed uv tool via {} mirror source",
            mirror.name
        );
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!(
        "Failed to install uv via {} mirror source: {}",
        mirror.name,
        stderr
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    #[cfg(unix)]
    fn resolve_python_path_finds_unix_layout() {
        let tmp_dir = tempdir().unwrap();
        let bin_dir = tmp_dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let python_path = bin_dir.join("python");
        std::fs::write(&python_path, b"").unwrap();

        let resolved = resolve_python_path(tmp_dir.path()).unwrap();
        assert_eq!(resolved, python_path);
    }

    #[test]
    fn resolve_python_path_errors_when_missing() {
        let tmp_dir = tempdir().unwrap();
        let error = resolve_python_path(tmp_dir.path()).unwrap_err();
        assert!(
            error.to_string().contains("no Python interpreter"),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn test_is_uv_not_found_error() {
        let installer = CompiledbInstaller::new();

        // Create a NotFound error
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "command not found");
        let anyhow_error = anyhow::Error::new(io_error);

        assert!(installer.is_uv_not_found_error(&anyhow_error));

        // Create a non-NotFound error
        let other_error = anyhow::anyhow!("some other error");
        assert!(!installer.is_uv_not_found_error(&other_error));
    }

    #[test]
    fn test_try_install_uv_with_no_mirrors() {
        let installer = CompiledbInstaller::new().with_mirrors(vec![]);

        // Since there are no mirror sources and pip install may fail, this test mainly verifies that the function doesn't panic
        // In actual environment, this function will try pip install, but in test environment we only verify the logic
        let result = installer.try_install_uv();

        // Whether successful or failed, it should not panic
        // If pip is unavailable or uv is already installed, this is expected behavior
        match result {
            Ok(_) => println!("uv installation succeeded"),
            Err(e) => println!("uv installation failed as expected: {}", e),
        }
    }
}
