<!-- apibump-comment -->
## ApiBump API Compatibility Report

**Recommendation:** `major`

**Summary:** `1` breaking, `0` additive, `0` internal, `0` unknown, `0` suppressed

### Packages

| Package | Recommendation | Breaking | Additive | Internal | Unknown | Suppressed |
| --- | --- | --- | --- | --- | --- | --- |
| `demo_pkg` | `major` | `1` | `0` | `0` | `0` | `0` |

| Severity | Kind | Symbol | Location | Message |
| --- | --- | --- | --- | --- |
| `breaking` | `parameter_removed` | `demo_pkg.api.create_user` | src/demo_pkg/api.py:1 | src/demo_pkg/api.py:1: create_user(email): Parameter was removed |
