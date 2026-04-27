use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use apibump::cli::{run, Cli};
use clap::Parser;
use serde_json::Value;
use tempfile::tempdir;

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

fn copy_dir(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let dest_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir(&source_path, &dest_path);
        } else {
            fs::create_dir_all(dest_path.parent().unwrap()).unwrap();
            fs::copy(&source_path, &dest_path).unwrap();
        }
    }
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {:?} failed", args);
}

#[test]
fn autodetects_changed_package_in_a_monorepo() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    copy_dir(&fixture_path("fixtures/python-monorepo/base"), &repo);

    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "ApiBump Test"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "base"]);
    let base = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    copy_dir(&fixture_path("fixtures/python-monorepo/head"), &repo);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "head"]);
    let head = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let json_output = repo.join("apibump-report.json");
    let fake_backend = fixture_path("fixtures/python-monorepo/backend");
    unsafe {
        std::env::set_var("APIBUMP_FAKE_BACKEND_JSON", fake_backend);
    }
    let cli = Cli::parse_from([
        "apibump",
        "check",
        "--repo",
        repo.to_str().unwrap(),
        "--base",
        &base,
        "--head",
        &head,
        "--format",
        "json",
        "--json-output",
        json_output.to_str().unwrap(),
    ]);
    let exit_code = run(cli).unwrap();
    unsafe {
        std::env::remove_var("APIBUMP_FAKE_BACKEND_JSON");
    }

    assert_eq!(exit_code, 0);
    let report: Value = serde_json::from_str(&fs::read_to_string(json_output).unwrap()).unwrap();
    assert_eq!(report["recommendation"], "minor");
    assert_eq!(report["packages"].as_array().unwrap().len(), 1);
    assert_eq!(report["packages"][0]["package"], "payments");
    assert_eq!(report["summary"]["additive"], 2);
}
