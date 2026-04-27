# Real Repo Dogfood: April 27, 2026

This run used four public Python libraries in forks under `anasm266` with draft pull requests opened against those forks. The goal was to verify zero-config package detection, the released CLI, and the GitHub Action comment flow on real repositories instead of fixtures.

## Scope

Initial `v0.2.1` dogfood exposed two blockers before the PR runs:

- The Windows CLI passed refs like `origin/main` through to Griffe, which broke Griffe worktree temp paths on Windows.
- Zero-config autodetect missed real package layouts in `PyJWT` and `tomlkit`.

Both blockers were fixed in `v0.2.2` before opening the draft PRs below.

## Final Results

| Repo | Intentional change | Expected | ApiBump result | Notes | PR |
| --- | --- | --- | --- | --- | --- |
| `itsdangerous` | Remove the package-root export `base64_decode` | `major` | `patch` | False negative. ApiBump treated an implicit package re-export removal as `internal_only`. | [anasm266/itsdangerous#1](https://github.com/anasm266/itsdangerous/pull/1) |
| `pyjwt` | Add `jwt.get_version()` at the package root | `minor` | `minor` | Correct. Zero-config autodetect resolved `jwt` even though the distribution name is `PyJWT`. | [anasm266/pyjwt#1](https://github.com/anasm266/pyjwt/pull/1) |
| `referencing` | Add a private `_DOGFOOD_TOKEN` in `_core.py` | `patch` | `patch` | Correct. Classified as `internal_only`. | [anasm266/referencing#1](https://github.com/anasm266/referencing/pull/1) |
| `tomlkit` | Add a required `mode` parameter to `tomlkit.parse()` | `major` | `major` | Correct major recommendation, but noisy. ApiBump also emitted two extra `unknown` `signature_changed` entries for the same change. | [anasm266/tomlkit#1](https://github.com/anasm266/tomlkit/pull/1) |

## What Worked

- The released `v0.2.2` Windows CLI ran successfully against real git histories after resolving refs to commit IDs first.
- The GitHub Action posted sticky PR comments correctly on all four fork PRs.
- Zero-config autodetect now works on:
  - `src/` layout packages like `itsdangerous`
  - flat-layout packages like `referencing`
  - `tool.poetry` projects like `tomlkit`
  - repos where the import package and distribution name differ, like `PyJWT` -> `jwt`
- Real additive, internal-only, and required-parameter breaking changes were all detected at least once.

## What Still Needs Work

- Package-root implicit re-exports are under-modeled.
  - Removing `itsdangerous.base64_decode` from `src/itsdangerous/__init__.py` should have been a breaking public API removal.
- Signature-change deduplication is too noisy.
  - `tomlkit` correctly produced `parameter_added_required`, but ApiBump also reported two extra `unknown` signature changes on the underlying function and its alias.

## Takeaways

- The release binary, zero-config discovery, and PR comment workflow are credible enough for more dogfooding.
- The main adoption risk is accuracy, not setup friction.
- The next high-value fixes are:
  - model package-root re-export removals as public API surface
  - suppress duplicate `unknown` signature changes when a stronger breaking result already exists
