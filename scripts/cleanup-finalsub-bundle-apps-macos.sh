#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This cleanup is only supported on macOS." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="$REPO_ROOT/src-tauri/target"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
REMOVED=0

if [ -d "$TARGET_DIR" ]; then
  while IFS= read -r generated_app; do
    [ -n "$generated_app" ] || continue
    "$LSREGISTER" -u "$generated_app" >/dev/null 2>&1 || true
    rm -rf "$generated_app"
    REMOVED=$((REMOVED + 1))
  done <<EOF
$(find "$TARGET_DIR" -type d -path '*/bundle/macos/FinalSub.app' -prune -print | sort)
EOF
fi

REMAINING="$(find "$TARGET_DIR" -type d -path '*/bundle/macos/FinalSub.app' -prune -print 2>/dev/null || true)"
if [ -n "$REMAINING" ]; then
  echo "Failed to remove generated FinalSub app bundles:" >&2
  printf '%s\n' "$REMAINING" >&2
  exit 1
fi

printf 'REMOVED_GENERATED_APPS=%s\nREMAINING_GENERATED_APPS=0\n' "$REMOVED"
