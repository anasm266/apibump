use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use thiserror::Error;

use crate::model::Diagnostic;

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
    breaking_changes: Vec<BridgeChange>,
    #[serde(default)]
    old_snapshot: Vec<SymbolSnapshot>,
    #[serde(default)]
    new_snapshot: Vec<SymbolSnapshot>,
    #[serde(default)]
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct BridgeChange {
    kind: String,
    symbol: String,
    file: Option<String>,
    line: Option<usize>,
    message: String,
    #[serde(default = "default_griffe_backend")]
    backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonBackendResult {
    pub breaking_changes: Vec<BreakingChange>,
    pub old_snapshot: Vec<SymbolSnapshot>,
    pub new_snapshot: Vec<SymbolSnapshot>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Module,
    Class,
    Function,
    Method,
    Attribute,
    Alias,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ParameterSnapshot {
    pub name: String,
    pub kind: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SymbolSnapshot {
    pub path: String,
    pub parent_path: String,
    pub kind: SymbolKind,
    #[serde(default)]
    pub parameters: Vec<ParameterSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakingChange {
    pub kind: String,
    pub symbol: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub message: String,
    pub backend: String,
}

pub fn run_python_backend(
    options: &PythonBackendOptions,
) -> Result<PythonBackendResult, BackendError> {
    let _ = options.strict;

    if let Some(fake_path) = env::var("APIBUMP_FAKE_BACKEND_JSON").ok() {
        return load_fake_backend(Path::new(&fake_path), &options.package);
    }

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

    Ok(PythonBackendResult {
        breaking_changes: bridge_report
            .breaking_changes
            .into_iter()
            .map(|change| BreakingChange {
                kind: change.kind,
                symbol: change.symbol,
                file: change.file,
                line: change.line,
                message: change.message,
                backend: change.backend,
            })
            .collect(),
        old_snapshot: bridge_report.old_snapshot,
        new_snapshot: bridge_report.new_snapshot,
        diagnostics: bridge_report.diagnostics,
    })
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

fn load_fake_backend(root: &Path, package: &str) -> Result<PythonBackendResult, BackendError> {
    let path = if root.is_dir() {
        root.join(format!("{package}.json"))
    } else {
        root.to_path_buf()
    };
    let bytes = fs::read(&path).map_err(BackendError::BridgeWrite)?;
    let bridge_report: BridgeReport =
        serde_json::from_slice(&bytes).map_err(BackendError::InvalidJson)?;
    Ok(PythonBackendResult {
        breaking_changes: bridge_report
            .breaking_changes
            .into_iter()
            .map(|change| BreakingChange {
                kind: change.kind,
                symbol: change.symbol,
                file: change.file,
                line: change.line,
                message: change.message,
                backend: change.backend,
            })
            .collect(),
        old_snapshot: bridge_report.old_snapshot,
        new_snapshot: bridge_report.new_snapshot,
        diagnostics: bridge_report.diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_symbol_snapshot_kind() {
        let snapshot: SymbolSnapshot = serde_json::from_str(
            r#"{"path":"pkg.api.create_user","parent_path":"pkg.api","kind":"function","parameters":[]}"#,
        )
        .unwrap();

        assert!(matches!(snapshot.kind, SymbolKind::Function));
    }

    #[test]
    fn deserializes_bridge_report_shape() {
        let report: BridgeReport = serde_json::from_str(
            r#"{
                "breaking_changes":[
                    {
                        "kind":"parameter_removed",
                        "symbol":"pkg.api.create_user",
                        "file":"src/pkg/api.py",
                        "line":1,
                        "message":"Parameter was removed",
                        "backend":"griffe"
                    }
                ],
                "old_snapshot":[
                    {
                        "path":"pkg.api.create_user",
                        "parent_path":"pkg.api",
                        "kind":"function",
                        "parameters":[{"name":"name","kind":"ParameterKind.positional_or_keyword","required":true}]
                    }
                ],
                "new_snapshot":[],
                "diagnostics":[]
            }"#,
        )
        .unwrap();

        assert_eq!(report.breaking_changes.len(), 1);
        assert_eq!(report.old_snapshot.len(), 1);
    }
}
