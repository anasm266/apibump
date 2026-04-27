use std::fmt;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Breaking,
    Additive,
    Internal,
    Unknown,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Breaking => "breaking",
            Self::Additive => "additive",
            Self::Internal => "internal",
            Self::Unknown => "unknown",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    Major,
    Minor,
    Patch,
    Unknown,
}

impl fmt::Display for Recommendation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Patch => "patch",
            Self::Unknown => "unknown",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Summary {
    pub breaking: usize,
    pub additive: usize,
    pub internal: usize,
    pub unknown: usize,
}

impl Summary {
    pub fn from_changes(changes: &[ApiChange]) -> Self {
        let mut summary = Self::default();
        for change in changes {
            match change.severity {
                Severity::Breaking => summary.breaking += 1,
                Severity::Additive => summary.additive += 1,
                Severity::Internal => summary.internal += 1,
                Severity::Unknown => summary.unknown += 1,
            }
        }
        summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiChange {
    pub severity: Severity,
    pub kind: String,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiReport {
    pub schema_version: String,
    pub recommendation: Recommendation,
    pub summary: Summary,
    pub changes: Vec<ApiChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
}

impl ApiReport {
    pub fn new(changes: Vec<ApiChange>, diagnostics: Vec<Diagnostic>) -> Self {
        let summary = Summary::from_changes(&changes);
        let recommendation = recommend(&summary);

        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            recommendation,
            summary,
            changes,
            diagnostics,
        }
    }

    pub fn backend_unknown(message: impl Into<String>, backend: impl Into<String>) -> Self {
        let backend = backend.into();
        Self::new(
            vec![ApiChange {
                severity: Severity::Unknown,
                kind: "backend_error".to_string(),
                symbol: "<backend>".to_string(),
                file: None,
                line: None,
                message: message.into(),
                backend: backend.clone(),
            }],
            vec![Diagnostic {
                level: "error".to_string(),
                message: format!("{backend} backend failed"),
            }],
        )
    }
}

pub fn recommend(summary: &Summary) -> Recommendation {
    if summary.breaking > 0 {
        Recommendation::Major
    } else if summary.unknown > 0 {
        Recommendation::Unknown
    } else if summary.additive > 0 {
        Recommendation::Minor
    } else {
        Recommendation::Patch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(severity: Severity) -> ApiChange {
        ApiChange {
            severity,
            kind: "object_removed".to_string(),
            symbol: "pkg.api.symbol".to_string(),
            file: Some("src/pkg/api.py".to_string()),
            line: Some(12),
            message: "Public object was removed".to_string(),
            backend: "griffe".to_string(),
        }
    }

    #[test]
    fn recommends_major_when_any_breaking_change_exists() {
        let report = ApiReport::new(
            vec![change(Severity::Additive), change(Severity::Breaking)],
            vec![],
        );

        assert_eq!(report.recommendation, Recommendation::Major);
        assert_eq!(report.summary.breaking, 1);
        assert_eq!(report.summary.additive, 1);
    }

    #[test]
    fn recommends_minor_for_additive_only_changes() {
        let report = ApiReport::new(vec![change(Severity::Additive)], vec![]);

        assert_eq!(report.recommendation, Recommendation::Minor);
    }

    #[test]
    fn recommends_patch_for_empty_reports() {
        let report = ApiReport::new(vec![], vec![]);

        assert_eq!(report.recommendation, Recommendation::Patch);
    }

    #[test]
    fn recommends_unknown_for_backend_uncertainty() {
        let report = ApiReport::backend_unknown("griffe is missing", "griffe");

        assert_eq!(report.recommendation, Recommendation::Unknown);
        assert_eq!(report.summary.unknown, 1);
    }
}
