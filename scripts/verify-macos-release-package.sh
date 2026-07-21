#!/bin/bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This verifier is only supported on macOS." >&2
  exit 2
fi
if [[ "$#" -ne 1 ]]; then
  echo "Usage: $0 /absolute/path/to/FinalSub_<version>_universal.dmg" >&2
  exit 2
fi
if [[ -z "${APPLE_TEAM_ID:-}" ]]; then
  echo "APPLE_TEAM_ID is required for release verification." >&2
  exit 2
fi

DMG_PATH="$1"
BUNDLE_ID="com.gravitypoet.finalsub"
case "$DMG_PATH" in
  /*) ;;
  *)
    echo "FinalSub DMG path must be absolute: $DMG_PATH" >&2
    exit 2
    ;;
esac
if [[ ! -f "$DMG_PATH" ]]; then
  echo "Missing FinalSub DMG: $DMG_PATH" >&2
  exit 1
fi

MOUNT_DIR="$(/usr/bin/mktemp -d /tmp/finalsub-release-dmg.XXXXXX)"
MOUNTED=false
cleanup() {
  if [[ "$MOUNTED" == "true" ]]; then
    /usr/sbin/diskutil eject "$MOUNT_DIR" >/dev/null 2>&1 || true
  fi
  /bin/rmdir "$MOUNT_DIR" >/dev/null 2>&1 || true
}
trap cleanup EXIT

verify_developer_id_signature() {
  local item_path="$1"
  local label="$2"
  local require_runtime="$3"
  local details

  details="$(/usr/bin/codesign -dv --verbose=4 "$item_path" 2>&1)"
  if ! printf '%s\n' "$details" | /usr/bin/grep -F 'Authority=Developer ID Application:' >/dev/null; then
    echo "$label is not signed with a Developer ID Application certificate." >&2
    exit 1
  fi
  if ! printf '%s\n' "$details" | /usr/bin/grep -F "TeamIdentifier=$APPLE_TEAM_ID" >/dev/null; then
    echo "$label TeamIdentifier does not match APPLE_TEAM_ID." >&2
    exit 1
  fi
  if ! printf '%s\n' "$details" | /usr/bin/grep -F 'Timestamp=' >/dev/null; then
    echo "$label is missing a secure signing timestamp." >&2
    exit 1
  fi
  if [[ "$require_runtime" == "true" ]] \
    && ! printf '%s\n' "$details" | /usr/bin/grep -E 'flags=.*\(runtime\)' >/dev/null; then
    echo "$label does not enable Hardened Runtime." >&2
    exit 1
  fi
}

/usr/bin/hdiutil verify "$DMG_PATH"
/usr/bin/codesign --verify --strict --verbose=2 "$DMG_PATH"
verify_developer_id_signature "$DMG_PATH" "FinalSub DMG" false

/usr/sbin/diskutil image attach \
  --readOnly \
  --nobrowse \
  --mountPoint "$MOUNT_DIR" \
  "$DMG_PATH" >/dev/null
MOUNTED=true

APP_PATH="$MOUNT_DIR/FinalSub.app"
if [[ ! -d "$APP_PATH" ]]; then
  echo "FinalSub.app is missing from the release DMG." >&2
  exit 1
fi

/usr/bin/codesign --verify --deep --strict --verbose=2 "$APP_PATH"
actual_bundle_id="$(/usr/bin/plutil -extract CFBundleIdentifier raw "$APP_PATH/Contents/Info.plist")"
if [[ "$actual_bundle_id" != "$BUNDLE_ID" ]]; then
  echo "Unexpected FinalSub bundle identifier: $actual_bundle_id" >&2
  exit 1
fi
verify_developer_id_signature "$APP_PATH" "FinalSub app" true

for binary_name in finalsubtauri ffmpeg whisper-cli; do
  binary_path="$APP_PATH/Contents/MacOS/$binary_name"
  if [[ ! -f "$binary_path" ]]; then
    echo "Missing release binary: $binary_name" >&2
    exit 1
  fi
  verify_developer_id_signature "$binary_path" "$binary_name" true
  binary_arches="$(/usr/bin/lipo -archs "$binary_path")"
  if [[ " $binary_arches " != *" arm64 "* || " $binary_arches " != *" x86_64 "* ]]; then
    echo "$binary_name is not a Universal arm64 + x86_64 binary: $binary_arches" >&2
    exit 1
  fi
done

if [[ "$(/usr/bin/plutil -extract LSMinimumSystemVersion raw "$APP_PATH/Contents/Info.plist")" != "12.0" ]]; then
  echo "FinalSub release no longer targets macOS 12.0." >&2
  exit 1
fi

/usr/bin/xcrun stapler validate "$APP_PATH"
/usr/sbin/spctl --assess --type execute --verbose=4 "$APP_PATH"

printf 'bundle_id=%s\n' "$actual_bundle_id"
printf 'team_id=%s\n' "$APPLE_TEAM_ID"
printf 'notarization=stapled-and-gatekeeper-accepted\n'
