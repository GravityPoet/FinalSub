#!/bin/bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This verifier is only supported on macOS." >&2
  exit 2
fi

if [[ "$#" -ne 1 ]]; then
  echo "Usage: $0 /absolute/path/to/FinalSub.app" >&2
  exit 2
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="$1"
BUNDLE_ID="com.gravitypoet.finalsub"
IDENTITY="ChordVox Local Code Signing"

case "$APP_PATH" in
  /*) ;;
  *)
    echo "FinalSub app path must be absolute: $APP_PATH" >&2
    exit 2
    ;;
esac

if [[ ! -d "$APP_PATH" ]]; then
  echo "Missing FinalSub app: $APP_PATH" >&2
  exit 1
fi

signing_state="$(/bin/bash "$ROOT_DIR/scripts/ensure-macos-signing-identity.sh")"
expected_requirement="$(printf '%s\n' "$signing_state" | /usr/bin/sed -n 's/^requirement=//p' | /usr/bin/head -n 1)"
if [[ -z "$expected_requirement" ]]; then
  echo "Could not derive the pinned FinalSub signing requirement." >&2
  exit 1
fi

/usr/bin/codesign --verify --deep --strict --verbose=2 "$APP_PATH"

actual_bundle_id="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$APP_PATH/Contents/Info.plist")"
if [[ "$actual_bundle_id" != "$BUNDLE_ID" ]]; then
  echo "Unexpected FinalSub bundle identifier: $actual_bundle_id" >&2
  exit 1
fi

native_execution="$(/usr/bin/plutil -extract LSRequiresNativeExecution raw "$APP_PATH/Contents/Info.plist")"
if [[ "$native_execution" != "true" ]]; then
  echo "FinalSub must require native execution on Apple silicon." >&2
  exit 1
fi

actual_requirement="$(/usr/bin/codesign -d -r- "$APP_PATH" 2>&1 \
  | /usr/bin/sed -n 's/^designated => //p' \
  | /usr/bin/head -n 1)"
if [[ "$actual_requirement" != "$expected_requirement" ]]; then
  echo "FinalSub was not signed with the pinned stable identity." >&2
  printf 'expected_requirement=%s\n' "$expected_requirement" >&2
  printf 'actual_requirement=%s\n' "${actual_requirement:-<none>}" >&2
  exit 1
fi

signature_details="$(/usr/bin/codesign -dv --verbose=4 "$APP_PATH" 2>&1)"
if printf '%s\n' "$signature_details" | /usr/bin/grep -F 'Signature=adhoc' >/dev/null; then
  echo "FinalSub unexpectedly fell back to ad-hoc signing." >&2
  exit 1
fi
if ! printf '%s\n' "$signature_details" | /usr/bin/grep -F "Authority=$IDENTITY" >/dev/null; then
  echo "FinalSub signing authority does not match '$IDENTITY'." >&2
  exit 1
fi
if ! printf '%s\n' "$signature_details" | /usr/bin/grep -E 'flags=.*\(runtime\)' >/dev/null; then
  echo "FinalSub Hardened Runtime is not enabled." >&2
  exit 1
fi

printf 'bundle_id=%s\n' "$actual_bundle_id"
printf 'native_execution=%s\n' "$native_execution"
printf 'identity=%s\n' "$IDENTITY"
printf 'requirement=%s\n' "$actual_requirement"
