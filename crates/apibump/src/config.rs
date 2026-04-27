use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    Changed,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigLanguage {
    Python,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFailOn {
    Breaking,
    Unknown,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApibumpConfig {
    pub path: PathBuf,
    pub version: u32,
    pub defaults: ConfigDefaults,
    pub packages: Vec<ConfigPackage>,
    pub ignore: Vec<IgnoreRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigDefaults {
    pub language: Option<ConfigLanguage>,
    pub selection: Option<SelectionMode>,
    pub fail_on: Option<ConfigFailOn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPackage {
    pub package: String,
    pub search: Vec<PathBuf>,
    pub roots: Vec<PathBuf>,
}

impl ConfigPackage {
    pub fn default_roots(search: &[PathBuf]) -> Vec<PathBuf> {
        search.to_vec()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreRule {
    pub symbol: String,
    pub kind: Option<String>,
    pub package: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    version: u32,
    #[serde(default)]
    defaults: RawDefaults,
    #[serde(default)]
    packages: Vec<RawPackage>,
    #[serde(default)]
    ignore: Vec<RawIgnoreRule>,
}

#[derive(Debug, Default, Deserialize)]
struct RawDefaults {
    language: Option<ConfigLanguage>,
    selection: Option<SelectionMode>,
    fail_on: Option<ConfigFailOn>,
}

#[derive(Debug, Deserialize)]
struct RawPackage {
    package: String,
    search: Vec<PathBuf>,
    roots: Option<Vec<PathBuf>>,
}

#[derive(Debug, Deserialize)]
struct RawIgnoreRule {
    symbol: String,
    kind: Option<String>,
    package: Option<String>,
    reason: Option<String>,
}

pub fn load_config(repo: &Path, explicit: Option<&Path>) -> anyhow::Result<Option<ApibumpConfig>> {
    let path = match explicit {
        Some(path) => repo.join(path),
        None => repo.join("apibump.toml"),
    };

    if !path.exists() {
        return if explicit.is_some() {
            Err(anyhow::anyhow!("config file not found: {}", path.display()))
        } else {
            Ok(None)
        };
    }

    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let raw: RawConfig =
        toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))?;

    if raw.version != 1 {
        bail!(
            "unsupported apibump.toml version {} in {}",
            raw.version,
            path.display()
        );
    }

    let base_dir = path.parent().unwrap_or(repo);
    let packages = raw
        .packages
        .into_iter()
        .map(|package| {
            if package.search.is_empty() {
                bail!(
                    "package {} in {} must declare at least one search path",
                    package.package,
                    path.display()
                );
            }

            let search = normalize_rel_paths(base_dir, repo, package.search)?;
            let roots = normalize_rel_paths(
                base_dir,
                repo,
                package
                    .roots
                    .unwrap_or_else(|| ConfigPackage::default_roots(&search)),
            )?;

            Ok(ConfigPackage {
                package: package.package,
                search,
                roots,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let ignore = raw
        .ignore
        .into_iter()
        .map(|rule| IgnoreRule {
            symbol: rule.symbol,
            kind: rule.kind,
            package: rule.package,
            reason: rule.reason,
        })
        .collect();

    Ok(Some(ApibumpConfig {
        path,
        version: raw.version,
        defaults: ConfigDefaults {
            language: raw.defaults.language,
            selection: raw.defaults.selection,
            fail_on: raw.defaults.fail_on,
        },
        packages,
        ignore,
    }))
}

fn normalize_rel_paths(base_dir: &Path, repo: &Path, paths: Vec<PathBuf>) -> anyhow::Result<Vec<PathBuf>> {
    paths
        .into_iter()
        .map(|path| normalize_rel_path(base_dir, repo, &path))
        .collect()
}

pub fn normalize_rel_path(base_dir: &Path, repo: &Path, path: &Path) -> anyhow::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    };

    let normalized = absolute
        .strip_prefix(repo)
        .with_context(|| format!("path {} is outside repo {}", absolute.display(), repo.display()))?;

    Ok(normalized.components().collect())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn parses_config_and_defaults_roots_from_search() {
        let repo = tempdir().unwrap();
        let config_path = repo.path().join("apibump.toml");
        fs::write(
            &config_path,
            r#"
version = 1

[defaults]
language = "python"
selection = "changed"
fail_on = "breaking"

[[packages]]
package = "demo_pkg"
search = ["src"]

[[ignore]]
symbol = "demo_pkg.legacy.*"
kind = "parameter_removed"
"#,
        )
        .unwrap();

        let config = load_config(repo.path(), None).unwrap().unwrap();
        assert_eq!(config.version, 1);
        assert_eq!(config.defaults.language, Some(ConfigLanguage::Python));
        assert_eq!(config.defaults.selection, Some(SelectionMode::Changed));
        assert_eq!(config.defaults.fail_on, Some(ConfigFailOn::Breaking));
        assert_eq!(config.packages[0].roots, vec![PathBuf::from("src")]);
        assert_eq!(config.ignore[0].symbol, "demo_pkg.legacy.*");
    }

    #[test]
    fn explicit_missing_config_errors() {
        let repo = tempdir().unwrap();
        let error = load_config(repo.path(), Some(Path::new("missing.toml"))).unwrap_err();
        assert!(error.to_string().contains("config file not found"));
    }

    #[test]
    fn rejects_paths_outside_repo() {
        let repo = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let error = normalize_rel_path(repo.path(), repo.path(), outside.path()).unwrap_err();
        assert!(error.to_string().contains("outside repo"));
    }
}
