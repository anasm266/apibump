# Python Breaking Change Fixture

This fixture demonstrates the PR shape ApiBump is meant to catch.

- `base/` exposes `create_user(name, email)`.
- `head/` removes the public `email` parameter.
- Expected result: `parameter_removed`, one breaking change, `major`.

