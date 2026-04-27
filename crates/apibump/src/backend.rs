use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use thiserror::Error;

use crate::model::{ApiChange, ApiReport, Diagnostic, Severity};

const GRIFFE_BRIDGE: &str = include_str!("../python/griffe_bridge.py");

#[derive(Debug, Clone)]
pub struct PythonBackendOptions {
    pub package: String,
    pub search_paths: Vec<PathBuf>,
    pub base: String,
    pub head: String,
    pub repo: PathBuf,
    pub python: Option<String>,
    pub strict: bool,
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("failed to create temporary Griffe bridge script: {0}")]
    BridgeTemp(std::io::Error),
    #[error("failed to write temporary Griffe bridge script: {0}")]
    BridgeWrite(std::io::Error),
    #[error("failed to run Python backend: {0}")]
    Command(std::io::Error),
    #[error("Python backend failed: {0}")]
    Failed(String),
    #[error("Python backend returned invalid JSON: {0}")]
    InvalidJson(serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct BridgeReport {
    #[serde(default)]
    changes: Vec<BridgeChange>,
    #[serde(default)]
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Deserialize)]
struct BridgeChange {
    kind: String,
    symbol: String,
    file: Option<String>,
    line: Option<usize>,
    message: String,
    #[serde(default = "default_griffe_backend")]
    backend: String,
}

pub fn run_python_backend(options: &PythonBackendOptions) -> Result<ApiReport, BackendError> {
    match run_python_backend_strict(options) {
        Ok(report) => Ok(report),
        Err(error) if !options.strict => {
            Ok(ApiReport::backend_unknown(error.to_string(), "griffe"))
        }
        Err(error) => Err(error),
    }
}

fn run_python_backend_strict(options: &PythonBackendOptions) -> Result<ApiReport, BackendError> {
    let bridge_dir = tempfile::tempdir().map_err(BackendError::BridgeTemp)?;
    let bridge_path = bridge_dir.path().join("griffe_bridge.py");
    fs::write(&bridge_path, GRIFFE_BRIDGE).map_err(BackendError::BridgeWrite)?;

    let python = options
        .python
        .clone()
        .or_else(|| env::var("APIBUMP_PYTHON").ok())
        .unwrap_or_else(default_python_command);

    let mut command = Command::new(python);
    command
        .arg(&bridge_path)
        .arg("--package")
        .arg(&options.package)
        .arg("--base")
        .arg(&options.base)
        .arg("--head")
        .arg(&options.head)
        .arg("--repo")
        .arg(&options.repo);

    for search_path in normalized_search_paths(&options.search_paths) {
        command.arg("--search").arg(search_path);
    }

    let output = command.output().map_err(BackendError::Command)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = match (stderr.is_empty(), stdout.is_empty()) {
            (false, _) => stderr,
            (true, false) => stdout,
            (true, true) => format!("process exited with status {}", output.status),
        };
        return Err(BackendError::Failed(message));
    }

    let bridge_report: BridgeReport =
        serde_json::from_slice(&output.stdout).map_err(BackendError::InvalidJson)?;
    let changes = bridge_report
        .changes
        .into_iter()
        .map(|change| ApiChange {
            severity: Severity::Breaking,
            kind: change.kind,
            symbol: change.symbol,
            file: change.file,
            line: change.line,
            message: change.message,
            backend: change.backend,
        })
        .collect();

    Ok(ApiReport::new(changes, bridge_report.diagnostics))
}

fn normalized_search_paths(search_paths: &[PathBuf]) -> Vec<&Path> {
    if search_paths.is_empty() {
        vec![Path::new(".")]
    } else {
        search_paths.iter().map(PathBuf::as_path).collect()
    }
}

fn default_python_command() -> String {
    if cfg!(windows) {
        "python".to_string()
    } else {
        "python3".to_string()
    }
}

fn default_griffe_backend() -> String {
    "griffe".to_string()
}
