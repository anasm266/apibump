use crate::model::{ApiChange, ApiReport, PackageReport, Recommendation, Severity};

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
        "Summary: {} breaking, {} additive, {} internal, {} unknown, {} suppressed\n",
        report.summary.breaking,
        report.summary.additive,
        report.summary.internal,
        report.summary.unknown,
        report.summary.suppressed
    ));

    if !report.packages.is_empty() {
        output.push('\n');
        for package in &report.packages {
            output.push_str(&format!(
                "Package {}: {} ({} breaking, {} additive, {} internal, {} unknown, {} suppressed)\n",
                package.package,
                package.recommendation,
                package.summary.breaking,
                package.summary.additive,
                package.summary.internal,
                package.summary.unknown,
                package.summary.suppressed
            ));
        }
    }

    if report.changes.is_empty() {
        output.push_str("\nNo public API changes detected.\n");
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

    if !report.suppressed_changes.is_empty() {
        output.push_str("\nSuppressed changes:\n");
        for change in &report.suppressed_changes {
            output.push_str(&format!(
                "- [{}] {} {}: {}\n",
                change.severity, change.kind, change.symbol, change.message
            ));
        }
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
        "**Summary:** `{}` breaking, `{}` additive, `{}` internal, `{}` unknown, `{}` suppressed\n\n",
        report.summary.breaking,
        report.summary.additive,
        report.summary.internal,
        report.summary.unknown,
        report.summary.suppressed
    ));

    if !report.packages.is_empty() {
        output.push_str("### Packages\n\n");
        output.push_str("| Package | Recommendation | Breaking | Additive | Internal | Unknown | Suppressed |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        for package in &report.packages {
            output.push_str(&package_summary_row(package));
        }
        output.push('\n');
    }

    if report.changes.is_empty() {
        output.push_str("No public API changes detected.\n");
        return output;
    }

    let include_package = report.packages.len() > 1;
    if include_package {
        output.push_str("| Package | Severity | Kind | Symbol | Location | Message |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- |\n");
    } else {
        output.push_str("| Severity | Kind | Symbol | Location | Message |\n");
        output.push_str("| --- | --- | --- | --- | --- |\n");
    }
    for change in &report.changes {
        if include_package {
            output.push_str(&format!(
                "| `{}` | `{}` | `{}` | `{}` | {} | {} |\n",
                escape_markdown_table(change.package.as_deref().unwrap_or("-")),
                change.severity,
                escape_markdown_table(&change.kind),
                escape_markdown_table(&change.symbol),
                escape_markdown_table(&location(change)),
                escape_markdown_table(&change.message)
            ));
        } else {
            output.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} | {} |\n",
                change.severity,
                escape_markdown_table(&change.kind),
                escape_markdown_table(&change.symbol),
                escape_markdown_table(&location(change)),
                escape_markdown_table(&change.message)
            ));
        }
    }

    if !report.suppressed_changes.is_empty() {
        output.push_str("\n### Suppressed Changes\n\n");
        output.push_str("| Severity | Kind | Symbol | Message |\n");
        output.push_str("| --- | --- | --- | --- |\n");
        for change in &report.suppressed_changes {
            output.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} |\n",
                change.severity,
                escape_markdown_table(&change.kind),
                escape_markdown_table(&change.symbol),
                escape_markdown_table(&change.message)
            ));
        }
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

fn package_summary_row(package: &PackageReport) -> String {
    format!(
        "| `{}` | `{}` | `{}` | `{}` | `{}` | `{}` | `{}` |\n",
        escape_markdown_table(&package.package),
        package.recommendation,
        package.summary.breaking,
        package.summary.additive,
        package.summary.internal,
        package.summary.unknown,
        package.summary.suppressed
    )
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

    use crate::model::{ApiChange, ApiReport, PackageReport, Severity};

    use super::*;

    #[test]
    fn renders_stable_markdown_summary() {
        let report = ApiReport::from_packages(
            vec![PackageReport::new(
                "pkg",
                vec![ApiChange {
                    package: Some("pkg".to_string()),
                    severity: Severity::Breaking,
                    kind: "parameter_removed".to_string(),
                    symbol: "pkg.api.create_user".to_string(),
                    file: Some("src/pkg/api.py".to_string()),
                    line: Some(42),
                    message: "Parameter was removed".to_string(),
                    backend: "griffe".to_string(),
                }],
                vec![],
            )],
            vec![],
        );

        assert_eq!(
            render_markdown(&report),
            "<!-- apibump-comment -->\n\
## ApiBump API Compatibility Report\n\n\
**Recommendation:** `major`\n\n\
**Summary:** `1` breaking, `0` additive, `0` internal, `0` unknown, `0` suppressed\n\n\
### Packages\n\n\
| Package | Recommendation | Breaking | Additive | Internal | Unknown | Suppressed |\n\
| --- | --- | --- | --- | --- | --- | --- |\n\
| `pkg` | `major` | `1` | `0` | `0` | `0` | `0` |\n\n\
| Severity | Kind | Symbol | Location | Message |\n\
| --- | --- | --- | --- | --- |\n\
| `breaking` | `parameter_removed` | `pkg.api.create_user` | src/pkg/api.py:42 | Parameter was removed |\n"
        );
    }
}
