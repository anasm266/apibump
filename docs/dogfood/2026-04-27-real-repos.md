# Real Repo Dogfood: April 27, 2026

This run used four public Python libraries in forks under `anasm266` with draft pull requests opened against those forks. The goal was to verify zero-config package detection, the released CLI, and the GitHub Action comment flow on real repositories instead of fixtures.

## Scope

This dogfood pass happened in two stages:

- `v0.2.1` exposed setup blockers before the PR runs:
  - the Windows CLI passed refs like `origin/main` through to Griffe
  - zero-config autodetect missed real package layouts in `PyJWT` and `tomlkit`
- `v0.2.2` made the first real PR runs possible, but exposed two accuracy issues:
  - package-root re-export removals were missed
  - duplicate `unknown` signature changes were emitted next to a real breaking change

Those accuracy issues were fixed in `v0.2.4`, and the fork PRs below were rerun against `anasm266/apibump@v1` after moving `v1` to the `v0.2.4` release.

## Final Results

| Repo | Intentional change | Expected | ApiBump result | Notes | PR |
| --- | --- | --- | --- | --- | --- |
| `itsdangerous` | Remove the package-root export `base64_decode` | `major` | `major` | Correct after the `v0.2.4` rerun. Reported as `object_removed` on `itsdangerous.base64_decode`. | [anasm266/itsdangerous#1](https://github.com/anasm266/itsdangerous/pull/1) |
| `pyjwt` | Add `jwt.get_version()` at the package root | `minor` | `minor` | Correct. Zero-config autodetect resolved `jwt` even though the distribution name is `PyJWT`. | [anasm266/pyjwt#1](https://github.com/anasm266/pyjwt/pull/1) |
| `referencing` | Add a private `_DOGFOOD_TOKEN` in `_core.py` | `patch` | `patch` | Correct. Classified as `internal_only`. | [anasm266/referencing#1](https://github.com/anasm266/referencing/pull/1) |
| `tomlkit` | Add a required `mode` parameter to `tomlkit.parse()` | `major` | `major` | Correct after the `v0.2.4` rerun. The duplicate `unknown` signature changes are gone. | [anasm266/tomlkit#1](https://github.com/anasm266/tomlkit/pull/1) |

## What Worked

- The released Windows CLI now runs successfully against real git histories after resolving refs to commit IDs first.
- The GitHub Action posted sticky PR comments correctly on all four fork PRs.
- Zero-config autodetect now works on:
  - `src/` layout packages like `itsdangerous`
  - flat-layout packages like `referencing`
  - `tool.poetry` projects like `tomlkit`
  - repos where the import package and distribution name differ, like `PyJWT` -> `jwt`
- Real additive, internal-only, removed-export, and required-parameter breaking changes were all detected at least once.

## Takeaways

- The release binary, zero-config discovery, and PR comment workflow all survived a real-repo pass on four public Python libraries.
- Setup friction is much better than it was at the start of the day.
- Accuracy is materially better after the `v0.2.4` rerun, but broader trust still depends on repeating this exercise on more real repositories.
