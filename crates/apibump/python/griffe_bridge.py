#!/usr/bin/env python3
"""Griffe bridge for ApiBump.

This script is intentionally small and JSON-only. Rust owns the public report
schema; Python only adapts Griffe objects into a backend-neutral shape.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


def main() -> int:
    parser = argparse.ArgumentParser(description="Run Griffe API breakage detection.")
    parser.add_argument("--package", required=True)
    parser.add_argument("--base", required=True, help="Older git ref to compare from.")
    parser.add_argument("--head", required=True, help="Newer git ref to compare to.")
    parser.add_argument("--repo", default=".")
    parser.add_argument("--search", action="append", default=[])
    args = parser.parse_args()

    try:
        import griffe
    except Exception as error:  # pragma: no cover - depends on caller env
        print(f"failed to import griffe: {error}", file=sys.stderr)
        return 2

    try:
        search_paths = args.search or ["."]
        old_api = griffe.load_git(
            args.package,
            ref=args.base,
            repo=args.repo,
            search_paths=search_paths,
            resolve_aliases=True,
        )
        new_api = griffe.load_git(
            args.package,
            ref=args.head,
            repo=args.repo,
            search_paths=search_paths,
            resolve_aliases=True,
        )
        changes = [
            normalize_breakage(breakage, griffe, search_paths)
            for breakage in griffe.find_breaking_changes(old_api, new_api)
        ]
    except Exception as error:
        print(f"griffe check failed: {error}", file=sys.stderr)
        return 3

    json.dump({"changes": changes, "diagnostics": []}, sys.stdout, sort_keys=True)
    sys.stdout.write("\n")
    return 0


def normalize_breakage(breakage: Any, griffe: Any, search_paths: list[str]) -> dict[str, Any]:
    data = as_dict(breakage)
    obj = getattr(breakage, "obj", None)
    kind = enum_value(getattr(breakage, "kind", None)) or data.get("kind") or type(breakage).__name__

    return {
        "kind": normalize_kind(str(kind)),
        "symbol": symbol_for(obj, data),
        "file": file_for(obj, data, search_paths),
        "line": line_for(obj, data),
        "message": message_for(breakage, griffe),
        "backend": "griffe",
    }


def as_dict(breakage: Any) -> dict[str, Any]:
    method = getattr(breakage, "as_dict", None)
    if method is None:
        return {}
    try:
        value = method(full=True)
    except TypeError:
        value = method()
    return value if isinstance(value, dict) else {}


def enum_value(value: Any) -> str | None:
    if value is None:
        return None
    return getattr(value, "value", None) or str(value)


def symbol_for(obj: Any, data: dict[str, Any]) -> str:
    for candidate in (
        getattr(obj, "path", None),
        getattr(obj, "canonical_path", None),
        data.get("object_path"),
        data.get("path"),
        data.get("name"),
    ):
        if candidate:
            return str(candidate)
    return "<unknown>"


def file_for(obj: Any, data: dict[str, Any], search_paths: list[str]) -> str | None:
    for candidate in (
        getattr(obj, "filepath", None),
        getattr(obj, "relative_filepath", None),
        data.get("filepath"),
        data.get("relative_filepath"),
    ):
        if candidate:
            return relativize_path(normalize_path(candidate), search_paths)
    return None


def line_for(obj: Any, data: dict[str, Any]) -> int | None:
    for candidate in (getattr(obj, "lineno", None), data.get("lineno"), data.get("line")):
        if candidate is None:
            continue
        try:
            return int(candidate)
        except (TypeError, ValueError):
            continue
    return None


def message_for(breakage: Any, griffe: Any) -> str:
    explain = getattr(breakage, "explain", None)
    if explain is None:
        return type(breakage).__name__

    styles = []
    explanation_style = getattr(griffe, "ExplanationStyle", None)
    if explanation_style is not None:
        styles.extend(
            [
                getattr(explanation_style, "ONE_LINE", None),
                getattr(explanation_style, "ONELINE", None),
            ]
        )
    styles.extend(["oneline", "one-line", None])

    for style in styles:
        try:
            if style is None:
                return strip_ansi(str(explain()))
            return strip_ansi(str(explain(style=style)))
        except Exception:
            continue

    return type(breakage).__name__


def normalize_path(value: Any) -> str:
    return Path(str(value)).as_posix()


def relativize_path(path: str, search_paths: list[str]) -> str:
    for search_path in search_paths:
        normalized = normalize_path(search_path).strip("/")
        if not normalized or normalized == ".":
            continue
        needle = f"/{normalized}/"
        index = path.rfind(needle)
        if index >= 0:
            return path[index + 1 :]
    return path


def normalize_kind(value: str) -> str:
    normalized = snake_case(value)
    aliases = {
        "positional_parameter_was_moved": "parameter_moved",
        "parameter_was_removed": "parameter_removed",
        "parameter_kind_was_changed": "parameter_changed_kind",
        "parameter_default_was_changed": "parameter_changed_default",
        "parameter_is_now_required": "parameter_changed_required",
        "parameter_was_added_as_required": "parameter_added_required",
        "return_types_are_incompatible": "return_changed_type",
        "public_object_was_removed": "object_removed",
        "public_object_points_to_a_different_kind_of_object": "object_changed_kind",
        "attribute_types_are_incompatible": "attribute_changed_type",
        "attribute_value_was_changed": "attribute_changed_value",
        "base_class_was_removed": "class_removed_base",
    }
    return aliases.get(normalized, normalized)


def snake_case(value: str) -> str:
    value = value.split(".")[-1]
    value = value.replace("-", "_").replace(" ", "_")
    value = re.sub(r"(?<!^)(?=[A-Z])", "_", value)
    value = re.sub(r"[^0-9A-Za-z_]+", "_", value)
    value = re.sub(r"_+", "_", value)
    return value.strip("_").lower()


def strip_ansi(value: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", value)


if __name__ == "__main__":
    raise SystemExit(main())
