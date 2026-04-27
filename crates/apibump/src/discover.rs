use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{bail, Context};
use serde::Deserialize;
use walkdir::{DirEntry, WalkDir};

use crate::config::SelectionMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPackage {
    pub package: String,
    pub search: Vec<PathBuf>,
    pub roots: Vec<PathBuf>,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageDetection {
    Explicit(DiscoveredPackage),
    Configured(Vec<DiscoveredPackage>),
    AutoDetected(DiscoveredPackage),
}

#[derive(Debug, Deserialize)]
struct PyProject {
    project: Option<PyProjectProject>,
    tool: Option<PyProjectTool>,
}

#[derive(Debug, Deserialize)]
struct PyProjectProject {
    name: Option<String>,
    #[serde(rename = "import-names")]
    import_names: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct PyProjectTool {
    poetry: Option<PyProjectPoetry>,
    flit: Option<PyProjectFlit>,
}

#[derive(Debug, Deserialize)]
struct PyProjectPoetry {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PyProjectFlit {
    module: Option<PyProjectFlitModule>,
}

#[derive(Debug, Deserialize)]
struct PyProjectFlitModule {
    name: Option<String>,
}

pub fn discover_python_packages(repo: &Path) -> anyhow::Result<Vec<DiscoveredPackage>> {
    let mut packages = Vec::new();

    for entry in WalkDir::new(repo)
        .into_iter()
        .filter_entry(|entry| should_walk(entry))
    {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.file_name() != "pyproject.toml" {
            continue;
        }

        let manifest_path = entry.path();
        let manifest_dir = manifest_path.parent().unwrap_or(repo);
        let contents = fs::read_to_string(manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?;
        let pyproject: PyProject = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
        let mut resolved = false;
        for import_name in pyproject_import_names(&pyproject) {
            if let Some(package) =
                resolve_candidate(repo, manifest_path, manifest_dir, &import_name)?
            {
                packages.push(package);
                resolved = true;
            }
        }

        if !resolved {
            if let Some(package) = infer_single_layout_candidate(repo, manifest_path, manifest_dir)?
            {
                packages.push(package);
            }
        }
    }

    packages.sort_by(|left, right| left.package.cmp(&right.package));
    packages.dedup_by(|left, right| left.package == right.package && left.roots == right.roots);
    Ok(packages)
}

pub fn git_changed_files(repo: &Path, base: &str, head: &str) -> anyhow::Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("diff")
        .arg("--name-only")
        .arg(format!("{base}..{head}"))
        .output()
        .with_context(|| "failed to run git diff")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git diff failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect())
}

pub fn select_changed_packages(
    packages: &[DiscoveredPackage],
    changed_files: &[PathBuf],
    selection: SelectionMode,
    all_packages: bool,
) -> Vec<DiscoveredPackage> {
    if all_packages || selection == SelectionMode::All {
        return packages.to_vec();
    }

    packages
        .iter()
        .filter(|package| package_has_changed(&package.roots, changed_files))
        .cloned()
        .collect()
}

pub fn package_has_changed(roots: &[PathBuf], changed_files: &[PathBuf]) -> bool {
    changed_files
        .iter()
        .any(|changed| roots.iter().any(|root| path_is_within(changed, root)))
}

pub fn path_is_within(path: &Path, root: &Path) -> bool {
    if root.as_os_str().is_empty() || root == Path::new(".") {
        return true;
    }
    path == root || path.starts_with(root)
}

fn should_walk(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }

    !matches!(
        entry.file_name().to_string_lossy().as_ref(),
        ".git" | "target" | ".venv" | "__pycache__"
    )
}

fn pyproject_import_names(pyproject: &PyProject) -> Vec<String> {
    let mut names = BTreeSet::new();

    if let Some(project) = &pyproject.project {
        if let Some(import_names) = &project.import_names {
            for name in import_names {
                let sanitized = sanitize_import_name(name);
                if !sanitized.is_empty() {
                    names.insert(sanitized);
                }
            }
        }

        if let Some(name) = &project.name {
            let normalized = normalize_project_name(name);
            if !normalized.is_empty() {
                names.insert(normalized);
            }
        }
    }

    if let Some(tool) = &pyproject.tool {
        if let Some(poetry) = &tool.poetry {
            if let Some(name) = &poetry.name {
                let normalized = normalize_project_name(name);
                if !normalized.is_empty() {
                    names.insert(normalized);
                }
            }
        }

        if let Some(flit) = &tool.flit {
            if let Some(module) = &flit.module {
                if let Some(name) = &module.name {
                    let sanitized = sanitize_import_name(name);
                    if !sanitized.is_empty() {
                        names.insert(sanitized);
                    }
                }
            }
        }
    }

    names.into_iter().collect()
}

fn sanitize_import_name(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn normalize_project_name(value: &str) -> String {
    value.trim().to_lowercase().replace(['-', '.', ' '], "_")
}

fn resolve_candidate(
    repo: &Path,
    manifest_path: &Path,
    manifest_dir: &Path,
    import_name: &str,
) -> anyhow::Result<Option<DiscoveredPackage>> {
    let layouts = [
        (
            manifest_dir.join("src"),
            manifest_dir
                .join("src")
                .join(import_name)
                .join("__init__.py"),
        ),
        (
            manifest_dir.to_path_buf(),
            manifest_dir.join(import_name).join("__init__.py"),
        ),
    ];

    for (search_dir, init_file) in layouts {
        if !init_file.exists() {
            continue;
        }

        let search = vec![repo_relative(repo, &search_dir)?];
        let roots = vec![repo_relative(
            repo,
            init_file.parent().unwrap_or(&search_dir),
        )?];
        return Ok(Some(DiscoveredPackage {
            package: import_name.to_string(),
            search,
            roots,
            manifest_path: repo_relative(repo, manifest_path)?,
        }));
    }

    Ok(None)
}

fn infer_single_layout_candidate(
    repo: &Path,
    manifest_path: &Path,
    manifest_dir: &Path,
) -> anyhow::Result<Option<DiscoveredPackage>> {
    let mut candidates = BTreeSet::new();

    for search_dir in [manifest_dir.join("src"), manifest_dir.to_path_buf()] {
        if !search_dir.is_dir() {
            continue;
        }

        let search = repo_relative(repo, &search_dir)?;
        for entry in fs::read_dir(&search_dir)
            .with_context(|| format!("failed to read {}", search_dir.display()))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let package_name = entry.file_name().to_string_lossy().to_string();
            if !is_likely_package_name(&package_name) {
                continue;
            }

            let package_root = entry.path();
            if !package_root.join("__init__.py").exists() {
                continue;
            }

            candidates.insert((
                package_name,
                search.clone(),
                repo_relative(repo, &package_root)?,
            ));
        }
    }

    let Some((package, search, root)) = only_candidate(candidates) else {
        return Ok(None);
    };

    Ok(Some(DiscoveredPackage {
        package,
        search: vec![search],
        roots: vec![root],
        manifest_path: repo_relative(repo, manifest_path)?,
    }))
}

fn only_candidate(
    candidates: BTreeSet<(String, PathBuf, PathBuf)>,
) -> Option<(String, PathBuf, PathBuf)> {
    if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        None
    }
}

fn is_likely_package_name(value: &str) -> bool {
    if value.starts_with('_')
        || value.starts_with('.')
        || matches!(
            value,
            "__pycache__"
                | "tests"
                | "test"
                | "docs"
                | "doc"
                | "examples"
                | "example"
                | "scripts"
                | "tasks"
                | "benchmarks"
        )
    {
        return false;
    }

    let mut chars = value.chars();
    match chars.next() {
        Some(character) if character.is_ascii_alphabetic() => {}
        _ => return false,
    }

    chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn repo_relative(repo: &Path, path: &Path) -> anyhow::Result<PathBuf> {
    Ok(path
        .strip_prefix(repo)
        .with_context(|| format!("{} is outside {}", path.display(), repo.display()))?
        .components()
        .collect())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn discovers_src_layout_package() {
        let repo = tempdir().unwrap();
        let manifest_dir = repo.path().join("pkg");
        fs::create_dir_all(manifest_dir.join("src/demo_pkg")).unwrap();
        fs::write(
            manifest_dir.join("pyproject.toml"),
            "[project]\nname = \"demo-pkg\"\n",
        )
        .unwrap();
        fs::write(manifest_dir.join("src/demo_pkg/__init__.py"), "").unwrap();

        let packages = discover_python_packages(repo.path()).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package, "demo_pkg");
        assert_eq!(packages[0].search, vec![PathBuf::from("pkg/src")]);
        assert_eq!(packages[0].roots, vec![PathBuf::from("pkg/src/demo_pkg")]);
    }

    #[test]
    fn discovers_flat_layout_package_from_import_names() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("flat_pkg")).unwrap();
        fs::write(
            repo.path().join("pyproject.toml"),
            "[project]\nname = \"flat-pkg\"\nimport-names = [\"flat_pkg\"]\n",
        )
        .unwrap();
        fs::write(repo.path().join("flat_pkg/__init__.py"), "").unwrap();

        let packages = discover_python_packages(repo.path()).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].search, vec![PathBuf::from("")]);
        assert_eq!(packages[0].roots, vec![PathBuf::from("flat_pkg")]);
    }

    #[test]
    fn discovers_tool_poetry_package() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("tomlkit")).unwrap();
        fs::write(
            repo.path().join("pyproject.toml"),
            "[tool.poetry]\nname = \"tomlkit\"\n",
        )
        .unwrap();
        fs::write(repo.path().join("tomlkit/__init__.py"), "").unwrap();

        let packages = discover_python_packages(repo.path()).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package, "tomlkit");
        assert_eq!(packages[0].search, vec![PathBuf::from("")]);
        assert_eq!(packages[0].roots, vec![PathBuf::from("tomlkit")]);
    }

    #[test]
    fn falls_back_to_the_only_package_directory() {
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join("jwt")).unwrap();
        fs::write(
            repo.path().join("pyproject.toml"),
            "[project]\nname = \"PyJWT\"\n",
        )
        .unwrap();
        fs::write(repo.path().join("jwt/__init__.py"), "").unwrap();

        let packages = discover_python_packages(repo.path()).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package, "jwt");
        assert_eq!(packages[0].search, vec![PathBuf::from("")]);
        assert_eq!(packages[0].roots, vec![PathBuf::from("jwt")]);
    }

    #[test]
    fn package_change_detection_uses_roots() {
        let package = DiscoveredPackage {
            package: "demo_pkg".to_string(),
            search: vec![PathBuf::from("src")],
            roots: vec![PathBuf::from("src/demo_pkg")],
            manifest_path: PathBuf::from("pyproject.toml"),
        };

        assert!(package_has_changed(
            &package.roots,
            &[PathBuf::from("src/demo_pkg/api.py")]
        ));
        assert!(!package_has_changed(
            &package.roots,
            &[PathBuf::from("other/file.py")]
        ));
    }
}
