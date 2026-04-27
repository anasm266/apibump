# Changelog

## v0.2.2

- Resolve Git refs to commit IDs before calling Griffe so local Windows runs do not trip over branch names like `origin/main`.
- Widen Python autodetection to handle `tool.poetry` projects and single-package repos whose import package does not match the distribution name.

## v0.2.1

- Fix Python bridge symbol kind normalization for real Griffe output.
- Add a regression test that catches `Kind.MODULE` style enum string handling.

## v0.2.0

- Add `apibump.toml` configuration support.
- Add Python package autodetection from `pyproject.toml`.
- Add config-first monorepo package selection.
- Add conservative additive classification for Python API changes.
- Add suppressed change reporting and package-level summaries.
- Add zero-config docs, monorepo docs, and a Griffe differentiation page.

## v0.1.0

- Initial Rust CLI with `apibump check`.
- Python public API breakage detection powered by Griffe.
- Stable JSON report schema version `0.1`.
- Human, Markdown, JSON, and GitHub annotation output.
- Composite GitHub Action with PR summary/comment support.
