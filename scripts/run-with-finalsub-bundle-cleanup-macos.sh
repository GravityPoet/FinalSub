#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This build wrapper is only supported on macOS." >&2
  exit 1
fi

if [ "$#" -eq 0 ]; then
  echo "Usage: $0 <npm-script> [<npm-script> ...]" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLEANUP_SCRIPT="$REPO_ROOT/scripts/cleanup-finalsub-bundle-apps-macos.sh"

finish() {
  status=$?
  cleanup_status=0
  trap - EXIT INT TERM
  bash "$CLEANUP_SCRIPT" || cleanup_status=$?
  if [ "$status" -eq 0 ] && [ "$cleanup_status" -ne 0 ]; then
    status=$cleanup_status
  fi
  exit "$status"
}

trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cd "$REPO_ROOT"
bash "$CLEANUP_SCRIPT"
for npm_script in "$@"; do
  npm run "$npm_script"
done
