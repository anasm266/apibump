#!/usr/bin/env bash
set -euo pipefail

json_file="${RUNNER_TEMP:-/tmp}/apibump-report.json"
markdown_file="${RUNNER_TEMP:-/tmp}/apibump-report.md"
venv_dir="${RUNNER_TEMP:-/tmp}/apibump-venv"

uv venv "$venv_dir"

if [[ "${RUNNER_OS:-Linux}" == "Windows" ]]; then
  python_bin="$venv_dir/Scripts/python.exe"
else
  python_bin="$venv_dir/bin/python"
fi

uv pip install --python "$python_bin" "griffe>=1,<2"

args=(
  check
  --language "${INPUT_LANGUAGE:-python}"
  --base "$INPUT_BASE"
  --head "${INPUT_HEAD:-HEAD}"
  --format "${INPUT_FORMAT:-github}"
  --fail-on "${INPUT_FAIL_ON:-breaking}"
  --python "$python_bin"
  --json-output "$json_file"
  --markdown-output "$markdown_file"
)

if [[ -n "${INPUT_PACKAGE:-}" ]]; then
  args+=(--package "$INPUT_PACKAGE")
fi

if [[ -n "${INPUT_CONFIG:-}" ]]; then
  args+=(--config "$INPUT_CONFIG")
fi

if [[ "${INPUT_ALL_PACKAGES:-false}" == "true" ]]; then
  args+=(--all-packages)
fi

if [[ -n "${INPUT_SEARCH:-}" ]]; then
  search_value="${INPUT_SEARCH//,/ }"
  for search_path in $search_value; do
    args+=(--search "$search_path")
  done
fi

set +e
APIBUMP_PYTHON="$python_bin" apibump "${args[@]}"
status=$?
set -e

if [[ -f "$markdown_file" && -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  cat "$markdown_file" >> "$GITHUB_STEP_SUMMARY"
fi

if [[ -f "$json_file" ]]; then
  recommendation="$("$python_bin" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["recommendation"])' "$json_file")"
  breaking="$("$python_bin" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["summary"]["breaking"])' "$json_file")"
  unknown="$("$python_bin" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["summary"]["unknown"])' "$json_file")"

  {
    echo "recommendation=$recommendation"
    echo "breaking=$breaking"
    echo "unknown=$unknown"
    echo "json_file=$json_file"
    echo "markdown_file=$markdown_file"
  } >> "$GITHUB_OUTPUT"
fi

echo "exit_code=$status" >> "$GITHUB_OUTPUT"
exit 0
