#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This updater build is only supported on macOS." >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PUBLIC_KEY_PATH="$REPO_ROOT/src-tauri/signing/finalsub-updater-root-v1.pub"
DEFAULT_PRIVATE_KEY="$HOME/Library/Application Support/GravityPoet/ReleaseKeys/FinalSub/updater/root-v1/finalsub-updater-root-v1.key"

if [ ! -s "$PUBLIC_KEY_PATH" ]; then
  echo "Missing tracked updater public key: $PUBLIC_KEY_PATH" >&2
  exit 1
fi

TRACKED_PUBLIC_KEY="$(tr -d '\r\n' < "$PUBLIC_KEY_PATH")"
if [ -n "${FINALSUB_UPDATER_PUBLIC_KEY:-}" ] && [ "$FINALSUB_UPDATER_PUBLIC_KEY" != "$TRACKED_PUBLIC_KEY" ]; then
  echo "FINALSUB_UPDATER_PUBLIC_KEY does not match the tracked FinalSub updater root." >&2
  exit 1
fi
export FINALSUB_UPDATER_PUBLIC_KEY="$TRACKED_PUBLIC_KEY"

if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && [ -s "$DEFAULT_PRIVATE_KEY" ]; then
  export TAURI_SIGNING_PRIVATE_KEY="$DEFAULT_PRIVATE_KEY"
fi
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  echo "Missing Tauri updater signing private key." >&2
  exit 1
fi
if [ -z "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD+x}" ]; then
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
fi

exec bash "$REPO_ROOT/scripts/run-with-finalsub-bundle-cleanup-macos.sh" \
  build:universal:updater:bundle
