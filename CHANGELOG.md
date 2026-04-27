# Changelog

## v0.2.4

- Ignore non-package import aliases such as `__future__.annotations` and stdlib imports when deriving the public API snapshot for modules without `__all__`.
- Keep package-root re-exports inside the same package visible so removed exports still surface as public API changes.

## v0.2.3

- Treat package-root imported aliases according to ApiBump's own public API rules when `__all__` is absent.
- Emit breaking `object_removed` changes for removed public snapshot symbols that are not already covered by a stronger backend breakage.
- Suppress duplicate unknown signature-change findings when an exact or canonical public symbol is already covered by a breaking change.

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
