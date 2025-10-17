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

                    if err
                        .chain()
                        .any(|cause| matches!(cause.downcast_ref::<std::io::Error>(), Some(io) if io.kind() == ErrorKind::NotFound))
                    {
                        return Err(err);
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
}
