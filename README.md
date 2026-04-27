# ApiBump

ApiBump detects semantic public API changes and recommends the correct SemVer bump.

V1 is intentionally focused: a Rust CLI, a GitHub Action, and a Python backend powered by [Griffe](https://mkdocstrings.github.io/griffe/). The goal is to give maintainers a useful PR signal before expanding into Go, Java, Rust, and other ecosystems.

## Local Usage

Install the CLI from this repository:

```bash
cargo install --path crates/apibump
```

Install Griffe in the Python environment ApiBump should use:

```bash
python -m pip install "griffe>=1,<2"
```

Run a check:

```bash
apibump check \
  --language python \
  --package my_pkg \
  --search src \
  --base origin/main \
  --head HEAD \
  --format markdown \
  --fail-on breaking
```

Stable JSON output is available with `--format json`:

```json
{
  "schema_version": "0.1",
  "recommendation": "major",
  "summary": {
    "breaking": 1,
    "additive": 0,
    "internal": 0,
    "unknown": 0
  },
  "changes": [
    {
      "severity": "breaking",
      "kind": "parameter_removed",
      "symbol": "my_pkg.api.create_user",
      "file": "src/my_pkg/api.py",
      "line": 42,
      "message": "Parameter was removed",
      "backend": "griffe"
    }
  ]
}
```

## GitHub Action

```yaml
name: API compatibility

on:
  pull_request:

permissions:
  contents: read
  pull-requests: write

jobs:
  apibump:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0

      - uses: anasm266/apibump@v1
        with:
          language: python
          package: my_pkg
          search: src
          base: ${{ github.event.pull_request.base.sha }}
          head: ${{ github.sha }}
          comment: true
```

The action writes a job summary and updates one sticky PR comment. PR comments use GitHub's issue comments API because pull requests are issues in the REST API.

## Exit Codes

- `0`: check completed and did not violate the configured `--fail-on` policy.
- `1`: check completed and violated `--fail-on`.
- `2`: CLI usage, I/O, serialization, or strict backend failure.

By default, backend failures become an `unknown` report and do not fail `--fail-on breaking`. Use `--strict` or `--fail-on unknown` for stricter CI.

## Scope

ApiBump currently supports Python only. It does not yet detect additive Python API changes, so Python reports are either `major`, `patch`, or `unknown`. Future language adapters should normalize their findings into the same JSON schema.
