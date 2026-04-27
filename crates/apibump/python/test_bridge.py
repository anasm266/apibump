from __future__ import annotations

import importlib.util
import tempfile
from pathlib import Path


def load_bridge():
    bridge_path = Path(__file__).with_name("griffe_bridge.py")
    spec = importlib.util.spec_from_file_location("griffe_bridge", bridge_path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


class FakeKind:
    value = "module"

    def __str__(self) -> str:
        return "Kind.MODULE"


def main() -> None:
    bridge = load_bridge()
    import griffe

    class FakeObject:
        kind = FakeKind()
        is_alias = False

    assert bridge.normalize_enum_kind(FakeKind()) == "module"
    assert bridge.symbol_kind(FakeObject(), "module") == "module"

    with tempfile.TemporaryDirectory() as temp_dir:
        root = Path(temp_dir)
        package_dir = root / "src" / "demo_pkg"
        package_dir.mkdir(parents=True)
        (package_dir / "__init__.py").write_text(
            "from __future__ import annotations\nfrom .api import public_fn as public_fn\n"
        )
        (package_dir / "api.py").write_text("def public_fn() -> None:\n    return None\n")

        module = griffe.load("demo_pkg", search_paths=[str(root / "src")], resolve_aliases=True)
        snapshot = bridge.snapshot_public_api(module)

        assert any(item["path"] == "demo_pkg.public_fn" for item in snapshot)
        assert not any(item["path"] == "demo_pkg.annotations" for item in snapshot)


if __name__ == "__main__":
    main()
