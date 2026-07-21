#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-only
#
# This file is provided WITHOUT ANY WARRANTY;
# without even the implied warranty of MERCHANTABILITY
# or FITNESS FOR A PARTICULAR PURPOSE.

# Claude Code PostToolUse hook: formats a file just edited by an agent so formatting
# drift never reaches lint/pre-push. Reads the hook JSON payload on stdin and extracts
# tool_input.file_path. Always exits 0 — formatting is best-effort and must never block
# an edit.

set -u

file="$(node -e '
let d = "";
process.stdin.on("data", (c) => (d += c)).on("end", () => {
  try {
    process.stdout.write(JSON.parse(d).tool_input.file_path || "");
  } catch {}
});
' 2>/dev/null)"

[[ -z "$file" || ! -f "$file" ]] && exit 0

case "$file" in
  *.rs)
    rustfmt --edition 2021 "$file" >/dev/null 2>&1 || true
    ;;
  *.ts | *.tsx | *.js | *.jsx | *.mjs | *.cjs | *.json | *.md | *.mdx | *.yml | *.yaml | *.css | *.sol)
    npx prettier --write "$file" >/dev/null 2>&1 || true
    ;;
esac

exit 0
