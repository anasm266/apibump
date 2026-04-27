# Contributing

Thanks for helping improve ApiBump.

## Development

Install Rust stable and run:

```bash
cargo fmt --check
cargo test --locked
```

The Python backend uses Griffe. For local bridge testing:

```bash
python -m pip install "griffe>=1,<2"
python crates/apibump/python/griffe_bridge.py --help
```

## Pull Requests

- Keep changes small and focused.
- Add or update tests for behavior changes.
- Preserve the JSON report schema unless the change is explicitly versioned.
- Prefer clear examples over abstract feature descriptions.

## Language Backends

V1 supports Python through Griffe. New language backends should normalize their findings into the existing report model before adding backend-specific options.

