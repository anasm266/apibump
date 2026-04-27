#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${GH_TOKEN:-}" ]]; then
  echo "ApiBump comment skipped: GH_TOKEN is empty" >&2
  exit 0
fi

if [[ -z "${GITHUB_REPOSITORY:-}" || -z "${PR_NUMBER:-}" || -z "${COMMENT_FILE:-}" ]]; then
  echo "ApiBump comment skipped: missing repository, PR number, or comment file" >&2
  exit 0
fi

python - "$GITHUB_REPOSITORY" "$PR_NUMBER" "$COMMENT_FILE" <<'PY'
from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request

repo, pr_number, comment_file = sys.argv[1:4]
api_url = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")
token = os.environ["GH_TOKEN"]
marker = "<!-- apibump-comment -->"

with open(comment_file, encoding="utf-8") as handle:
    body = handle.read()

if marker not in body:
    body = marker + "\n" + body


def request(method: str, path: str, payload: dict | None = None):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        f"{api_url}{path}",
        data=data,
        method=method,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    try:
        with urllib.request.urlopen(req) as response:
            raw = response.read().decode("utf-8")
            return json.loads(raw) if raw else None
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise SystemExit(f"GitHub API request failed: HTTP {error.code}: {detail}") from error


comments = request("GET", f"/repos/{repo}/issues/{pr_number}/comments?per_page=100")
existing = next((comment for comment in comments if marker in comment.get("body", "")), None)

if existing:
    request("PATCH", f"/repos/{repo}/issues/comments/{existing['id']}", {"body": body})
    print(f"Updated ApiBump PR comment #{existing['id']}")
else:
    created = request("POST", f"/repos/{repo}/issues/{pr_number}/comments", {"body": body})
    print(f"Created ApiBump PR comment #{created['id']}")
PY

