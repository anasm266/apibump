use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use clap::{Args, Parser, Subcommand, ValueEnum};
use globset::{Glob, GlobMatcher};

use crate::{
    backend::{
        run_python_backend, BreakingChange, ParameterSnapshot, PythonBackendOptions,
        PythonBackendResult, SymbolKind, SymbolSnapshot,
    },
    config::{load_config, ApibumpConfig, ConfigFailOn, ConfigPackage, IgnoreRule, SelectionMode},
    discover::{
        discover_python_packages, git_changed_files, package_has_changed, select_changed_packages,
        DiscoveredPackage,
    },
    model::{ApiChange, ApiReport, Diagnostic, PackageReport, Severity},
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
    package: Option<String>,

    #[arg(long = "search", value_name = "PATH")]
    search_paths: Vec<PathBuf>,

    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    #[arg(long)]
    all_packages: bool,

    #[arg(long, default_value = "origin/main")]
    base: String,

    #[arg(long, default_value = "HEAD")]
    head: String,

    #[arg(long, default_value = ".")]
    repo: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(long, value_enum)]
    fail_on: Option<FailOn>,

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

#[derive(Debug, Clone)]
struct SelectedPackage {
    package: String,
    search: Vec<PathBuf>,
    roots: Vec<PathBuf>,
    changed: bool,
}

#[derive(Debug)]
struct CompiledIgnoreRule {
    symbol: GlobMatcher,
    kind: Option<String>,
    package: Option<String>,
}

#[derive(Debug)]
struct EvaluatedPackage {
    report: PackageReport,
    diagnostics: Vec<Diagnostic>,
}

pub fn run(cli: Cli) -> anyhow::Result<u8> {
    match cli.command {
        Commands::Check(args) => check(args),
    }
}

fn check(args: CheckArgs) -> anyhow::Result<u8> {
    let repo = args.repo.canonicalize().unwrap_or(args.repo.clone());
    let config = load_config(&repo, args.config.as_deref())?;
    let changed_files = git_changed_files(&repo, &args.base, &args.head)?;
    let fail_on = effective_fail_on(args.fail_on, config.as_ref());
    let selection = config
        .as_ref()
        .and_then(|config| config.defaults.selection)
        .unwrap_or(SelectionMode::Changed);
    let ignore_rules = compile_ignore_rules(config.as_ref())?;

    let packages = resolve_packages(&repo, &args, config.as_ref(), &changed_files, selection)?;
    let evaluated = packages
        .into_iter()
        .map(|package| {
            evaluate_python_package(
                &repo,
                &package,
                &args.base,
                &args.head,
                args.python.clone(),
                &changed_files,
                &ignore_rules,
                args.strict,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let diagnostics = evaluated
        .iter()
        .flat_map(|package| package.diagnostics.clone())
        .collect::<Vec<_>>();
    let report = ApiReport::from_packages(
        evaluated
            .into_iter()
            .map(|package| package.report)
            .collect(),
        diagnostics,
    );

    write_optional_outputs(&report, args.json_output, args.markdown_output)?;
    println!("{}", render(&report, args.format)?);

    Ok(if should_fail(&report, fail_on) { 1 } else { 0 })
}

fn effective_fail_on(cli_fail_on: Option<FailOn>, config: Option<&ApibumpConfig>) -> FailOn {
    cli_fail_on
        .or_else(|| config.and_then(|config| config.defaults.fail_on.map(map_config_fail_on)))
        .unwrap_or(FailOn::Breaking)
}

fn map_config_fail_on(value: ConfigFailOn) -> FailOn {
    match value {
        ConfigFailOn::Breaking => FailOn::Breaking,
        ConfigFailOn::Unknown => FailOn::Unknown,
        ConfigFailOn::Never => FailOn::Never,
    }
}

fn resolve_packages(
    repo: &Path,
    args: &CheckArgs,
    config: Option<&ApibumpConfig>,
    changed_files: &[PathBuf],
    selection: SelectionMode,
) -> anyhow::Result<Vec<SelectedPackage>> {
    if let Some(package) = &args.package {
        let search = if args.search_paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            args.search_paths.clone()
        };
        let roots = search.clone();
        let changed = package_has_changed(&roots, changed_files);
        return Ok(vec![SelectedPackage {
            package: package.clone(),
            search,
            roots,
            changed,
        }]);
    }

    if let Some(config) = config {
        if !config.packages.is_empty() {
            let configured = config
                .packages
                .iter()
                .map(|package| to_discovered_package(&config.path, package))
                .collect::<Vec<_>>();
            return Ok(select_packages(
                configured,
                changed_files,
                selection,
                args.all_packages,
            ));
        }
    }

    let discovered = discover_python_packages(repo)?;
    if discovered.is_empty() {
        bail!("could not detect a Python package; pass --package or add apibump.toml");
    }

    if discovered.len() == 1 {
        return Ok(select_packages(
            discovered,
            changed_files,
            selection,
            args.all_packages,
        ));
    }

    let changed_candidates =
        select_changed_packages(&discovered, changed_files, SelectionMode::Changed, false);
    if changed_candidates.len() == 1 {
        return Ok(select_packages(
            changed_candidates,
            changed_files,
            selection,
            args.all_packages,
        ));
    }

    let candidates = discovered
        .iter()
        .map(|candidate| {
            format!(
                "{} ({})",
                candidate.package,
                candidate.manifest_path.display()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "multiple Python package candidates detected: {candidates}. Add apibump.toml or pass --package."
    );
}

fn select_packages(
    packages: Vec<DiscoveredPackage>,
    changed_files: &[PathBuf],
    selection: SelectionMode,
    all_packages: bool,
) -> Vec<SelectedPackage> {
    let packages = if all_packages || selection == SelectionMode::All {
        packages
    } else {
        select_changed_packages(&packages, changed_files, selection, all_packages)
    };

    packages
        .into_iter()
        .map(|package| {
            let changed = package_has_changed(&package.roots, changed_files);
            SelectedPackage {
                package: package.package,
                search: package.search,
                roots: package.roots,
                changed,
            }
        })
        .collect()
}

fn to_discovered_package(config_path: &Path, package: &ConfigPackage) -> DiscoveredPackage {
    DiscoveredPackage {
        package: package.package.clone(),
        search: package.search.clone(),
        roots: package.roots.clone(),
        manifest_path: config_path.to_path_buf(),
    }
}

fn compile_ignore_rules(config: Option<&ApibumpConfig>) -> anyhow::Result<Vec<CompiledIgnoreRule>> {
    config
        .into_iter()
        .flat_map(|config| config.ignore.iter())
        .map(compile_ignore_rule)
        .collect()
}

fn compile_ignore_rule(rule: &IgnoreRule) -> anyhow::Result<CompiledIgnoreRule> {
    Ok(CompiledIgnoreRule {
        symbol: Glob::new(&rule.symbol)
            .with_context(|| format!("invalid ignore symbol pattern {}", rule.symbol))?
            .compile_matcher(),
        kind: rule.kind.clone(),
        package: rule.package.clone(),
    })
}

fn evaluate_python_package(
    repo: &Path,
    package: &SelectedPackage,
    base: &str,
    head: &str,
    python: Option<String>,
    changed_files: &[PathBuf],
    ignore_rules: &[CompiledIgnoreRule],
    strict: bool,
) -> anyhow::Result<EvaluatedPackage> {
    match run_python_backend(&PythonBackendOptions {
        package: package.package.clone(),
        search_paths: package.search.clone(),
        base: base.to_string(),
        head: head.to_string(),
        repo: repo.to_path_buf(),
        python,
        strict,
    }) {
        Ok(result) => {
            let diagnostics = result.diagnostics.clone();
            Ok(EvaluatedPackage {
                report: build_package_report(package, result, changed_files, ignore_rules),
                diagnostics,
            })
        }
        Err(error) if !strict => {
            let message = error.to_string();
            Ok(EvaluatedPackage {
                report: PackageReport::new(
                    package.package.clone(),
                    vec![ApiChange {
                        package: Some(package.package.clone()),
                        severity: Severity::Unknown,
                        kind: "backend_error".to_string(),
                        symbol: "<backend>".to_string(),
                        file: None,
                        line: None,
                        message: message.clone(),
                        backend: "griffe".to_string(),
                    }],
                    vec![],
                ),
                diagnostics: vec![Diagnostic {
                    level: "error".to_string(),
                    message: format!("griffe backend failed for {}: {}", package.package, message),
                }],
            })
        }
        Err(error) => Err(error.into()),
    }
}

fn build_package_report(
    package: &SelectedPackage,
    result: PythonBackendResult,
    changed_files: &[PathBuf],
    ignore_rules: &[CompiledIgnoreRule],
) -> PackageReport {
    let mut changes = result
        .breaking_changes
        .into_iter()
        .map(|change| to_breaking_api_change(&package.package, change))
        .collect::<Vec<_>>();

    let breaking_paths = changes
        .iter()
        .map(|change| change.symbol.clone())
        .collect::<BTreeSet<_>>();
    changes.extend(classify_snapshot_changes(
        &package.package,
        &result.old_snapshot,
        &result.new_snapshot,
        &breaking_paths,
    ));

    changes.sort_by(|left, right| {
        left.symbol
            .cmp(&right.symbol)
            .then(left.kind.cmp(&right.kind))
            .then(left.message.cmp(&right.message))
    });

    let (mut active_changes, suppressed_changes) = apply_ignore_rules(changes, ignore_rules);
    if package.changed
        && !active_changes
            .iter()
            .any(|change| !matches!(change.severity, Severity::Internal))
        && package_has_changed(&package.roots, changed_files)
    {
        active_changes.push(ApiChange {
            package: Some(package.package.clone()),
            severity: Severity::Internal,
            kind: "internal_only".to_string(),
            symbol: package.package.clone(),
            file: None,
            line: None,
            message: "Files changed without a public API change".to_string(),
            backend: "apibump".to_string(),
        });
    }

    PackageReport::new(package.package.clone(), active_changes, suppressed_changes)
}

fn to_breaking_api_change(package: &str, change: BreakingChange) -> ApiChange {
    ApiChange {
        package: Some(package.to_string()),
        severity: Severity::Breaking,
        kind: change.kind,
        symbol: change.symbol,
        file: change.file,
        line: change.line,
        message: change.message,
        backend: change.backend,
    }
}

fn classify_snapshot_changes(
    package: &str,
    old_snapshot: &[SymbolSnapshot],
    new_snapshot: &[SymbolSnapshot],
    breaking_paths: &BTreeSet<String>,
) -> Vec<ApiChange> {
    let old_by_path = old_snapshot
        .iter()
        .map(|symbol| (symbol.path.as_str(), symbol))
        .collect::<BTreeMap<_, _>>();
    let new_by_path = new_snapshot
        .iter()
        .map(|symbol| (symbol.path.as_str(), symbol))
        .collect::<BTreeMap<_, _>>();

    let mut changes = Vec::new();

    let added_paths = new_by_path
        .keys()
        .filter(|path| !old_by_path.contains_key(**path))
        .copied()
        .collect::<BTreeSet<_>>();
    for path in &added_paths {
        if has_added_ancestor(path, &added_paths) {
            continue;
        }
        let symbol = new_by_path[path];
        changes.push(ApiChange {
            package: Some(package.to_string()),
            severity: Severity::Additive,
            kind: "object_added".to_string(),
            symbol: symbol.path.clone(),
            file: None,
            line: None,
            message: format!("Public {} was added", symbol_kind_label(&symbol.kind)),
            backend: "apibump".to_string(),
        });
    }

    for path in old_by_path
        .keys()
        .filter(|path| new_by_path.contains_key(**path))
    {
        if has_breaking_ancestor(path, breaking_paths) {
            continue;
        }

        let old_symbol = old_by_path[path];
        let new_symbol = new_by_path[path];

        if old_symbol.kind != new_symbol.kind || old_symbol.parent_path != new_symbol.parent_path {
            changes.push(ApiChange {
                package: Some(package.to_string()),
                severity: Severity::Unknown,
                kind: "object_changed".to_string(),
                symbol: old_symbol.path.clone(),
                file: None,
                line: None,
                message: "Public object changed in a non-breaking but unclassified way".to_string(),
                backend: "apibump".to_string(),
            });
            continue;
        }

        if matches!(
            new_symbol.kind,
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Alias
        ) {
            changes.extend(classify_parameter_changes(
                package,
                &old_symbol.path,
                &old_symbol.parameters,
                &new_symbol.parameters,
            ));
        }
    }

    changes
}

fn classify_parameter_changes(
    package: &str,
    symbol: &str,
    old_params: &[ParameterSnapshot],
    new_params: &[ParameterSnapshot],
) -> Vec<ApiChange> {
    if old_params == new_params {
        return Vec::new();
    }

    let mut changes = Vec::new();
    let old_by_name = old_params
        .iter()
        .map(|parameter| (parameter.name.as_str(), parameter))
        .collect::<BTreeMap<_, _>>();
    let new_by_name = new_params
        .iter()
        .map(|parameter| (parameter.name.as_str(), parameter))
        .collect::<BTreeMap<_, _>>();

    for old_parameter in old_params {
        let Some(new_parameter) = new_by_name.get(old_parameter.name.as_str()) else {
            return vec![unknown_signature_change(package, symbol)];
        };

        if old_parameter.kind != new_parameter.kind {
            return vec![unknown_signature_change(package, symbol)];
        }

        if old_parameter.required && !new_parameter.required {
            changes.push(ApiChange {
                package: Some(package.to_string()),
                severity: Severity::Additive,
                kind: "parameter_became_optional".to_string(),
                symbol: symbol.to_string(),
                file: None,
                line: None,
                message: format!("Parameter {} became optional", old_parameter.name),
                backend: "apibump".to_string(),
            });
        } else if old_parameter.required != new_parameter.required {
            return vec![unknown_signature_change(package, symbol)];
        }
    }

    if new_params.len() < old_params.len() {
        return vec![unknown_signature_change(package, symbol)];
    }

    let old_names = old_params
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<Vec<_>>();
    let new_names = new_params
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<Vec<_>>();
    if old_names != new_names[..old_names.len()] {
        let added_count = new_names
            .iter()
            .filter(|name| !old_by_name.contains_key(**name))
            .count();
        if added_count > 0 {
            return vec![unknown_signature_change(package, symbol)];
        }
    }

    for new_parameter in &new_params[old_params.len()..] {
        if new_parameter.required {
            return vec![unknown_signature_change(package, symbol)];
        }

        if !matches!(
            new_parameter.kind.as_str(),
            "ParameterKind.keyword_only" | "ParameterKind.positional_or_keyword"
        ) {
            return vec![unknown_signature_change(package, symbol)];
        }

        changes.push(ApiChange {
            package: Some(package.to_string()),
            severity: Severity::Additive,
            kind: "parameter_added_optional".to_string(),
            symbol: symbol.to_string(),
            file: None,
            line: None,
            message: format!("Optional parameter {} was added", new_parameter.name),
            backend: "apibump".to_string(),
        });
    }

    changes
}

fn unknown_signature_change(package: &str, symbol: &str) -> ApiChange {
    ApiChange {
        package: Some(package.to_string()),
        severity: Severity::Unknown,
        kind: "signature_changed".to_string(),
        symbol: symbol.to_string(),
        file: None,
        line: None,
        message: "Signature changed in a non-breaking but unclassified way".to_string(),
        backend: "apibump".to_string(),
    }
}

fn symbol_kind_label(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Module => "module",
        SymbolKind::Class => "class",
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Attribute => "attribute",
        SymbolKind::Alias => "alias",
    }
}

fn has_added_ancestor(path: &str, added_paths: &BTreeSet<&str>) -> bool {
    ancestor_paths(path).any(|ancestor| added_paths.contains(ancestor))
}

fn has_breaking_ancestor(path: &str, breaking_paths: &BTreeSet<String>) -> bool {
    ancestor_paths(path).any(|ancestor| breaking_paths.contains(ancestor))
}

fn ancestor_paths(path: &str) -> impl Iterator<Item = &str> {
    let mut indices = path
        .match_indices('.')
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    indices.reverse();
    indices.into_iter().map(move |index| &path[..index])
}

fn apply_ignore_rules(
    changes: Vec<ApiChange>,
    ignore_rules: &[CompiledIgnoreRule],
) -> (Vec<ApiChange>, Vec<ApiChange>) {
    let mut active = Vec::new();
    let mut suppressed = Vec::new();

    for change in changes {
        if ignore_rules
            .iter()
            .any(|rule| ignore_rule_matches(rule, &change))
        {
            suppressed.push(change);
        } else {
            active.push(change);
        }
    }

    (active, suppressed)
}

fn ignore_rule_matches(rule: &CompiledIgnoreRule, change: &ApiChange) -> bool {
    if !rule.symbol.is_match(&change.symbol) {
        return false;
    }

    if let Some(kind) = &rule.kind {
        if &change.kind != kind {
            return false;
        }
    }

    if let Some(package) = &rule.package {
        if change.package.as_deref() != Some(package.as_str()) {
            return false;
        }
    }

    true
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
    use std::path::PathBuf;

    use crate::{
        backend::{ParameterSnapshot, SymbolKind, SymbolSnapshot},
        model::{ApiChange, Severity},
    };

    use super::*;

    #[test]
    fn fail_on_breaking_ignores_unknown_reports() {
        let report = ApiReport::backend_unknown("missing griffe", "griffe");

        assert!(!should_fail(&report, FailOn::Breaking));
        assert!(should_fail(&report, FailOn::Unknown));
    }

    #[test]
    fn fail_on_breaking_fails_for_breakages() {
        let report = ApiReport::from_packages(
            vec![PackageReport::new(
                "pkg",
                vec![ApiChange {
                    package: Some("pkg".to_string()),
                    severity: Severity::Breaking,
                    kind: "object_removed".to_string(),
                    symbol: "pkg.removed".to_string(),
                    file: None,
                    line: None,
                    message: "Public object was removed".to_string(),
                    backend: "griffe".to_string(),
                }],
                vec![],
            )],
            vec![],
        );

        assert!(should_fail(&report, FailOn::Breaking));
    }

    #[test]
    fn classifies_optional_parameter_additions() {
        let changes = classify_parameter_changes(
            "pkg",
            "pkg.api.create_user",
            &[ParameterSnapshot {
                name: "name".to_string(),
                kind: "ParameterKind.positional_or_keyword".to_string(),
                required: true,
            }],
            &[
                ParameterSnapshot {
                    name: "name".to_string(),
                    kind: "ParameterKind.positional_or_keyword".to_string(),
                    required: true,
                },
                ParameterSnapshot {
                    name: "email".to_string(),
                    kind: "ParameterKind.keyword_only".to_string(),
                    required: false,
                },
            ],
        );

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, "parameter_added_optional");
        assert_eq!(changes[0].severity, Severity::Additive);
    }

    #[test]
    fn classifies_unknown_non_prefix_parameter_changes() {
        let changes = classify_parameter_changes(
            "pkg",
            "pkg.api.create_user",
            &[ParameterSnapshot {
                name: "name".to_string(),
                kind: "ParameterKind.positional_or_keyword".to_string(),
                required: true,
            }],
            &[
                ParameterSnapshot {
                    name: "email".to_string(),
                    kind: "ParameterKind.keyword_only".to_string(),
                    required: false,
                },
                ParameterSnapshot {
                    name: "name".to_string(),
                    kind: "ParameterKind.positional_or_keyword".to_string(),
                    required: true,
                },
            ],
        );

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, "signature_changed");
        assert_eq!(changes[0].severity, Severity::Unknown);
    }

    #[test]
    fn ignore_rules_suppress_matching_changes() {
        let rules = vec![CompiledIgnoreRule {
            symbol: Glob::new("pkg.api.*").unwrap().compile_matcher(),
            kind: Some("parameter_removed".to_string()),
            package: Some("pkg".to_string()),
        }];
        let change = ApiChange {
            package: Some("pkg".to_string()),
            severity: Severity::Breaking,
            kind: "parameter_removed".to_string(),
            symbol: "pkg.api.create_user".to_string(),
            file: None,
            line: None,
            message: "removed".to_string(),
            backend: "griffe".to_string(),
        };

        let (active, suppressed) = apply_ignore_rules(vec![change], &rules);
        assert!(active.is_empty());
        assert_eq!(suppressed.len(), 1);
    }

    #[test]
    fn snapshot_additions_skip_descendants_of_added_parents() {
        let changes = classify_snapshot_changes(
            "pkg",
            &[],
            &[
                SymbolSnapshot {
                    path: "pkg.models".to_string(),
                    parent_path: "pkg".to_string(),
                    kind: SymbolKind::Module,
                    parameters: vec![],
                },
                SymbolSnapshot {
                    path: "pkg.models.User".to_string(),
                    parent_path: "pkg.models".to_string(),
                    kind: SymbolKind::Class,
                    parameters: vec![],
                },
            ],
            &BTreeSet::new(),
        );

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].symbol, "pkg.models");
    }

    #[test]
    fn resolve_packages_autodetects_single_candidate() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join("src/demo_pkg")).unwrap();
        std::fs::write(
            repo.path().join("pyproject.toml"),
            "[project]\nname = \"demo-pkg\"\n",
        )
        .unwrap();
        std::fs::write(repo.path().join("src/demo_pkg/__init__.py"), "").unwrap();

        let args = CheckArgs {
            language: Language::Python,
            package: None,
            search_paths: vec![],
            config: None,
            all_packages: false,
            base: "HEAD~1".to_string(),
            head: "HEAD".to_string(),
            repo: PathBuf::from("."),
            format: OutputFormat::Human,
            fail_on: None,
            strict: false,
            json_output: None,
            markdown_output: None,
            python: None,
        };

        let packages = resolve_packages(
            repo.path(),
            &args,
            None,
            &[PathBuf::from("src/demo_pkg/api.py")],
            SelectionMode::Changed,
        )
        .unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package, "demo_pkg");
    }
}
