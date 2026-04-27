# ApiBump Configuration

ApiBump reads `apibump.toml` from the repository root by default. You can also pass `--config path/to/apibump.toml`.

## Format

```toml
version = 1

[defaults]
language = "python"
selection = "changed"
fail_on = "breaking"

[[packages]]
package = "demo_pkg"
search = ["src"]
roots = ["src/demo_pkg"]

[[ignore]]
symbol = "demo_pkg.legacy.*"
kind = "parameter_removed"
reason = "intentional removal"
```

## Semantics

- `defaults.selection = "changed"` checks only packages whose `roots` intersect the git diff.
- `defaults.selection = "all"` checks every configured package.
- `packages[*].package` is the Python import root.
- `packages[*].search` is passed to Griffe and is relative to the repository root.
- `packages[*].roots` defines which changed files belong to that package. If omitted, ApiBump defaults `roots` from `search`.
- `ignore` rules suppress matching changes after classification and before recommendation.

## Ignore Rules

Ignore rules currently support:

- `symbol`: glob pattern such as `payments.legacy.*`
- `kind`: optional exact change kind such as `parameter_removed`
- `package`: optional package name filter

Suppressed changes are counted in the report but do not contribute to the SemVer recommendation.

## Autodetect

If there is no explicit `--package` and no `packages` list in `apibump.toml`, ApiBump scans `pyproject.toml` files and tries to detect Python packages automatically.

- One valid candidate: use it automatically.
- Multiple candidates and exactly one changed package: use the changed package automatically.
- Multiple ambiguous candidates: fail and ask for `apibump.toml` or `--package`.

Namespace-package autodetection is intentionally out of scope for this iteration.

