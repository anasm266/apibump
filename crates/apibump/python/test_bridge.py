from __future__ import annotations

import importlib.util
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

    class FakeObject:
        kind = FakeKind()
        is_alias = False

    assert bridge.normalize_enum_kind(FakeKind()) == "module"
    assert bridge.symbol_kind(FakeObject(), "module") == "module"


if __name__ == "__main__":
    main()
