use std::{fs, path::PathBuf};

use anyhow::Context;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::{
    backend::{run_python_backend, PythonBackendOptions},
    model::ApiReport,
    report::{render, render_markdown, OutputFormat},
};

#[derive(Debug, Parser)]
#[command(name = "apibump")]
#[command(about = "Detect public API breakages and recommend SemVer bumps.")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Check(CheckArgs),
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(long, value_enum, default_value_t = Language::Python)]
    language: Language,

    #[arg(long = "package")]
    package: String,

    #[arg(long = "search", value_name = "PATH")]
    search_paths: Vec<PathBuf>,

    #[arg(long, default_value = "origin/main")]
    base: String,

    #[arg(long, default_value = "HEAD")]
    head: String,

    #[arg(long, default_value = ".")]
    repo: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(long, value_enum, default_value_t = FailOn::Breaking)]
    fail_on: FailOn,

    #[arg(long)]
    strict: bool,

    #[arg(long, value_name = "PATH")]
    json_output: Option<PathBuf>,

    #[arg(long, value_name = "PATH")]
    markdown_output: Option<PathBuf>,

    #[arg(long, value_name = "COMMAND")]
    python: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Language {
    Python,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FailOn {
    Breaking,
    Unknown,
    Never,
}

pub fn run(cli: Cli) -> anyhow::Result<u8> {
    match cli.command {
        Commands::Check(args) => check(args),
    }
}

fn check(args: CheckArgs) -> anyhow::Result<u8> {
    let report = match args.language {
        Language::Python => run_python_backend(&PythonBackendOptions {
            package: args.package,
            search_paths: args.search_paths,
            base: args.base,
            head: args.head,
            repo: args.repo,
            python: args.python,
            strict: args.strict,
        })?,
    };

    write_optional_outputs(&report, args.json_output, args.markdown_output)?;

    println!("{}", render(&report, args.format)?);

    Ok(if should_fail(&report, args.fail_on) {
        1
    } else {
        0
    })
}

fn write_optional_outputs(
    report: &ApiReport,
    json_output: Option<PathBuf>,
    markdown_output: Option<PathBuf>,
) -> anyhow::Result<()> {
    if let Some(path) = json_output {
        let json = serde_json::to_string_pretty(report)?;
        fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    }

    if let Some(path) = markdown_output {
        fs::write(&path, render_markdown(report))
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    Ok(())
}

fn should_fail(report: &ApiReport, fail_on: FailOn) -> bool {
    match fail_on {
        FailOn::Never => false,
        FailOn::Breaking => report.summary.breaking > 0,
        FailOn::Unknown => report.summary.breaking > 0 || report.summary.unknown > 0,
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{ApiChange, ApiReport, Severity};

    use super::*;

    #[test]
    fn fail_on_breaking_ignores_unknown_reports() {
        let report = ApiReport::backend_unknown("missing griffe", "griffe");

        assert!(!should_fail(&report, FailOn::Breaking));
        assert!(should_fail(&report, FailOn::Unknown));
    }

    #[test]
    fn fail_on_breaking_fails_for_breakages() {
        let report = ApiReport::new(
            vec![ApiChange {
                severity: Severity::Breaking,
                kind: "object_removed".to_string(),
                symbol: "pkg.removed".to_string(),
                file: None,
                line: None,
                message: "Public object was removed".to_string(),
                backend: "griffe".to_string(),
            }],
            vec![],
        );

        assert!(should_fail(&report, FailOn::Breaking));
    }
}
