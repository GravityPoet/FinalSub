#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This verifier is only supported on macOS." >&2
  exit 2
fi
if [ "$#" -ne 2 ]; then
  echo "Usage: $0 <FinalSub.app.tar.gz> <FinalSub.app.tar.gz.sig>" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCHIVE="$1"
SIGNATURE="$2"
PUBLIC_KEY="$REPO_ROOT/src-tauri/signing/finalsub-updater-root-v1.pub"
EXPECTED_VERSION="$(node -p 'require(process.argv[1]).version' "$REPO_ROOT/package.json")"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-updater-verify.XXXXXX")"

cleanup() {
  status=$?
  trap - EXIT INT TERM
  case "$WORK_DIR" in
    "${TMPDIR:-/tmp}"/finalsub-updater-verify.*)
      find "$WORK_DIR" -depth -delete 2>/dev/null || true
      ;;
    *)
      echo "Refusing to clean unexpected updater verifier path: $WORK_DIR" >&2
      status=1
      ;;
  esac
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

test -s "$ARCHIVE"
test -s "$SIGNATURE"
test -s "$PUBLIC_KEY"
node "$REPO_ROOT/scripts/verify-updater-signature.mjs" "$ARCHIVE" "$SIGNATURE" "$PUBLIC_KEY"

tar -xzf "$ARCHIVE" -C "$WORK_DIR"
APP_PATHS=()
while IFS= read -r app_path; do
  APP_PATHS+=("$app_path")
done < <(find "$WORK_DIR" -type d -name FinalSub.app -prune -print)
if [ "${#APP_PATHS[@]}" -ne 1 ]; then
  echo "Updater archive must contain exactly one FinalSub.app." >&2
  exit 1
fi

APP_PATH="${APP_PATHS[0]}"
bash "$REPO_ROOT/scripts/verify-finalsub-macos-app.sh" "$APP_PATH"
ACTUAL_VERSION="$(plutil -extract CFBundleShortVersionString raw "$APP_PATH/Contents/Info.plist")"
if [ "$ACTUAL_VERSION" != "$EXPECTED_VERSION" ]; then
  echo "Updater app version mismatch: expected $EXPECTED_VERSION, got $ACTUAL_VERSION" >&2
  exit 1
fi
for binary in finalsubtauri ffmpeg whisper-cli; do
  lipo "$APP_PATH/Contents/MacOS/$binary" -verify_arch arm64 x86_64
done

printf 'updater_signature=verified\n'
printf 'version=%s\n' "$ACTUAL_VERSION"
printf 'architectures=x86_64 arm64\n'
