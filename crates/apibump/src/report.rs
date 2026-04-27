use crate::model::{ApiChange, ApiReport, Recommendation, Severity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
    Markdown,
    Github,
}

pub fn render(report: &ApiReport, format: OutputFormat) -> anyhow::Result<String> {
    match format {
        OutputFormat::Human => Ok(render_human(report)),
        OutputFormat::Json => Ok(serde_json::to_string_pretty(report)?),
        OutputFormat::Markdown => Ok(render_markdown(report)),
        OutputFormat::Github => Ok(render_github(report)),
    }
}

pub fn render_human(report: &ApiReport) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "ApiBump recommendation: {}\n",
        report.recommendation
    ));
    output.push_str(&format!(
        "Summary: {} breaking, {} additive, {} internal, {} unknown\n",
        report.summary.breaking,
        report.summary.additive,
        report.summary.internal,
        report.summary.unknown
    ));

    if report.changes.is_empty() {
        output.push_str("No public API changes detected.\n");
        return output;
    }

    output.push('\n');
    for change in &report.changes {
        output.push_str(&format!(
            "- [{}] {} {}{}: {}\n",
            change.severity,
            change.kind,
            change.symbol,
            location_suffix(change),
            change.message
        ));
    }

    output
}

pub fn render_markdown(report: &ApiReport) -> String {
    let mut output = String::new();
    output.push_str("<!-- apibump-comment -->\n");
    output.push_str("## ApiBump API Compatibility Report\n\n");
    output.push_str(&format!(
        "**Recommendation:** `{}`\n\n",
        report.recommendation
    ));
    output.push_str(&format!(
        "**Summary:** `{}` breaking, `{}` additive, `{}` internal, `{}` unknown\n\n",
        report.summary.breaking,
        report.summary.additive,
        report.summary.internal,
        report.summary.unknown
    ));

    if report.changes.is_empty() {
        output.push_str("No public API changes detected.\n");
        return output;
    }

    output.push_str("| Severity | Kind | Symbol | Location | Message |\n");
    output.push_str("| --- | --- | --- | --- | --- |\n");
    for change in &report.changes {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {} |\n",
            change.severity,
            escape_markdown_table(&change.kind),
            escape_markdown_table(&change.symbol),
            escape_markdown_table(&location(change)),
            escape_markdown_table(&change.message)
        ));
    }

    output
}

pub fn render_github(report: &ApiReport) -> String {
    let mut output = String::new();

    for change in &report.changes {
        let annotation = match change.severity {
            Severity::Breaking => "error",
            Severity::Unknown => "warning",
            Severity::Additive | Severity::Internal => "notice",
        };
        let title = format!("{}: {}", change.severity, change.kind);
        let message = format!("{}: {}", change.symbol, change.message);

        output.push_str("::");
        output.push_str(annotation);
        if let Some(file) = &change.file {
            output.push_str(" file=");
            output.push_str(&escape_github_command_property(file));
            if let Some(line) = change.line {
                output.push_str(",line=");
                output.push_str(&line.to_string());
            }
        }
        output.push_str(",title=");
        output.push_str(&escape_github_command_property(&title));
        output.push_str("::");
        output.push_str(&escape_github_command_data(&message));
        output.push('\n');
    }

    if report.changes.is_empty() {
        output.push_str("::notice title=ApiBump::No public API changes detected\n");
    }

    output.push_str(&format!(
        "::notice title=ApiBump recommendation::{}\n",
        recommendation_sentence(report.recommendation)
    ));

    output
}

fn recommendation_sentence(recommendation: Recommendation) -> &'static str {
    match recommendation {
        Recommendation::Major => "major version bump required",
        Recommendation::Minor => "minor version bump recommended",
        Recommendation::Patch => "patch version is sufficient",
        Recommendation::Unknown => "unable to determine version bump",
    }
}

fn location_suffix(change: &ApiChange) -> String {
    let location = location(change);
    if location == "-" {
        String::new()
    } else {
        format!(" ({location})")
    }
}

fn location(change: &ApiChange) -> String {
    match (&change.file, change.line) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.clone(),
        (None, Some(line)) => format!("line {line}"),
        (None, None) => "-".to_string(),
    }
}

fn escape_markdown_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', "<br>")
}

fn escape_github_command_property(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

fn escape_github_command_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::model::{ApiChange, ApiReport, Severity};

    use super::*;

    #[test]
    fn renders_stable_markdown_summary() {
        let report = ApiReport::new(
            vec![ApiChange {
                severity: Severity::Breaking,
                kind: "parameter_removed".to_string(),
                symbol: "pkg.api.create_user".to_string(),
                file: Some("src/pkg/api.py".to_string()),
                line: Some(42),
                message: "Parameter was removed".to_string(),
                backend: "griffe".to_string(),
            }],
            vec![],
        );

        assert_eq!(
            render_markdown(&report),
            "<!-- apibump-comment -->\n\
## ApiBump API Compatibility Report\n\n\
**Recommendation:** `major`\n\n\
**Summary:** `1` breaking, `0` additive, `0` internal, `0` unknown\n\n\
| Severity | Kind | Symbol | Location | Message |\n\
| --- | --- | --- | --- | --- |\n\
| `breaking` | `parameter_removed` | `pkg.api.create_user` | src/pkg/api.py:42 | Parameter was removed |\n"
        );
    }
}
