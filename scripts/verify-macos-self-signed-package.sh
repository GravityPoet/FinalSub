#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This verifier is only supported on macOS." >&2
  exit 2
fi

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 /absolute/path/to/FinalSub-<version>-macos-universal-self-signed.dmg" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DMG_PATH="$1"
BUNDLE_ID="com.gravitypoet.finalsub"
IDENTITY="ChordVox Local Code Signing"
EXPECTED_VERSION="$(node -p 'require(process.argv[1]).version' "$REPO_ROOT/package.json")"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-self-signed-verify.XXXXXX")"
MOUNT_PATH="$WORK_DIR/mount"
DEVICE=""

case "$DMG_PATH" in
  /*) ;;
  *)
    echo "DMG path must be absolute: $DMG_PATH" >&2
    exit 2
    ;;
esac

if [ ! -f "$DMG_PATH" ]; then
  echo "Missing self-signed DMG: $DMG_PATH" >&2
  exit 1
fi

cleanup() {
  status=$?
  trap - EXIT INT TERM
  if [ -d "$MOUNT_PATH/FinalSub.app" ]; then
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
      -u "$MOUNT_PATH/FinalSub.app" >/dev/null 2>&1 || true
  fi
  if [ -n "$DEVICE" ]; then
    diskutil eject "$DEVICE" >/dev/null 2>&1 || true
  fi
  case "$WORK_DIR" in
    "${TMPDIR:-/tmp}"/finalsub-self-signed-verify.*)
      find "$WORK_DIR" -depth -delete 2>/dev/null || true
      ;;
    *)
      echo "Refusing to clean unexpected verifier path: $WORK_DIR" >&2
      status=1
      ;;
  esac
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

signing_state="$(bash "$REPO_ROOT/scripts/ensure-macos-signing-identity.sh")"
expected_requirement="$(printf '%s\n' "$signing_state" | sed -n 's/^requirement=//p' | head -n 1)"
expected_fingerprint="$(printf '%s\n' "$signing_state" | sed -n 's/^certificate_sha256=//p' | head -n 1)"
if [ -z "$expected_requirement" ] || [ -z "$expected_fingerprint" ]; then
  echo "Could not derive FinalSub's pinned self-signed identity." >&2
  exit 1
fi

certificate_fingerprint() {
  signed_path="$1"
  output_dir="$2"
  mkdir -p "$output_dir"
  (
    cd "$output_dir"
    codesign -d --extract-certificates "$signed_path" >/dev/null 2>&1
  )
  if [ ! -f "$output_dir/codesign0" ]; then
    echo "Could not extract the signing certificate from: $signed_path" >&2
    return 1
  fi
  /usr/bin/openssl x509 -inform DER -in "$output_dir/codesign0" -noout -fingerprint -sha256 \
    | sed 's/^[^=]*=//; s/://g' \
    | tr '[:lower:]' '[:upper:]'
}

hdiutil verify "$DMG_PATH"
codesign --verify --strict --verbose=2 "$DMG_PATH"
dmg_signature="$(codesign -dv --verbose=4 "$DMG_PATH" 2>&1)"
if ! printf '%s\n' "$dmg_signature" | grep -F "Authority=$IDENTITY" >/dev/null; then
  echo "DMG is not signed with FinalSub's pinned self-signed identity." >&2
  exit 1
fi
if printf '%s\n' "$dmg_signature" | grep -F 'Authority=Developer ID' >/dev/null; then
  echo "Developer ID packages must use the formal notarized release channel." >&2
  exit 1
fi
dmg_fingerprint="$(certificate_fingerprint "$DMG_PATH" "$WORK_DIR/dmg-certificate")"
if [ "$dmg_fingerprint" != "$expected_fingerprint" ]; then
  echo "DMG certificate does not match the pinned FinalSub certificate." >&2
  exit 1
fi

if xcrun stapler validate "$DMG_PATH" >/dev/null 2>&1; then
  echo "A notarized DMG must use the formal Developer ID release channel." >&2
  exit 1
fi

set +e
gatekeeper_output="$(spctl --assess --type open --context context:primary-signature -vv "$DMG_PATH" 2>&1)"
gatekeeper_status=$?
set -e
if [ "$gatekeeper_status" -eq 0 ]; then
  echo "Self-signed channel unexpectedly passed the formal Gatekeeper assessment." >&2
  exit 1
fi
if ! printf '%s\n' "$gatekeeper_output" | grep -F "origin=$IDENTITY" >/dev/null; then
  echo "Gatekeeper rejection did not identify the pinned FinalSub authority." >&2
  printf '%s\n' "$gatekeeper_output" >&2
  exit 1
fi

mkdir -p "$MOUNT_PATH"
attach_output="$(diskutil image attach --readOnly --mountOptions nobrowse --mountPoint "$MOUNT_PATH" "$DMG_PATH")"
DEVICE="$(printf '%s\n' "$attach_output" | awk '/^\/dev\// { print $1; exit }')"
if [ -z "$DEVICE" ] || [ ! -d "$MOUNT_PATH/FinalSub.app" ]; then
  echo "DMG did not mount with FinalSub.app." >&2
  exit 1
fi
if [ ! -L "$MOUNT_PATH/Applications" ] || [ "$(readlink "$MOUNT_PATH/Applications")" != "/Applications" ]; then
  echo "DMG is missing the Applications drag target." >&2
  exit 1
fi

APP_PATH="$MOUNT_PATH/FinalSub.app"
bash "$REPO_ROOT/scripts/verify-finalsub-macos-app.sh" "$APP_PATH"
actual_version="$(plutil -extract CFBundleShortVersionString raw "$APP_PATH/Contents/Info.plist")"
actual_bundle_id="$(plutil -extract CFBundleIdentifier raw "$APP_PATH/Contents/Info.plist")"
if [ "$actual_version" != "$EXPECTED_VERSION" ] || [ "$actual_bundle_id" != "$BUNDLE_ID" ]; then
  echo "DMG app metadata does not match the repository release target." >&2
  printf 'expected_version=%s actual_version=%s\n' "$EXPECTED_VERSION" "$actual_version" >&2
  printf 'expected_bundle_id=%s actual_bundle_id=%s\n' "$BUNDLE_ID" "$actual_bundle_id" >&2
  exit 1
fi

actual_requirement="$(codesign -d -r- "$APP_PATH" 2>&1 | sed -n 's/^designated => //p' | head -n 1)"
if [ "$actual_requirement" != "$expected_requirement" ]; then
  echo "DMG app designated requirement changed." >&2
  exit 1
fi
app_fingerprint="$(certificate_fingerprint "$APP_PATH" "$WORK_DIR/app-certificate")"
if [ "$app_fingerprint" != "$expected_fingerprint" ]; then
  echo "DMG app certificate does not match the pinned FinalSub certificate." >&2
  exit 1
fi

for binary in finalsubtauri ffmpeg whisper-cli; do
  binary_path="$APP_PATH/Contents/MacOS/$binary"
  lipo "$binary_path" -verify_arch arm64 x86_64
done
if [ "$(xcrun vtool -show-build "$APP_PATH/Contents/MacOS/finalsubtauri" | awk '/minos/{print $2}' | sort -u)" != "12.0" ]; then
  echo "FinalSub minimum macOS deployment target changed." >&2
  exit 1
fi
if [ "$(plutil -extract LSMinimumSystemVersion raw "$APP_PATH/Contents/Info.plist")" != "12.0" ]; then
  echo "FinalSub Info.plist minimum system version changed." >&2
  exit 1
fi

filters="$("$APP_PATH/Contents/MacOS/ffmpeg" -hide_banner -filters 2>&1)"
encoders="$("$APP_PATH/Contents/MacOS/ffmpeg" -hide_banner -encoders 2>&1)"
printf '%s\n' "$filters" | grep -F ' subtitles ' >/dev/null
printf '%s\n' "$encoders" | grep -F 'libx264' >/dev/null
test -f "$APP_PATH/Contents/Resources/licenses/ffmpeg-GPLv3.txt"
test -f "$APP_PATH/Contents/Resources/licenses/whisper.cpp-LICENSE.txt"

printf 'release_channel=self-signed-macos\n'
printf 'version=%s\n' "$actual_version"
printf 'bundle_id=%s\n' "$actual_bundle_id"
printf 'identity=%s\n' "$IDENTITY"
printf 'certificate_sha256=%s\n' "$app_fingerprint"
printf 'requirement=%s\n' "$actual_requirement"
printf 'notarized=0\n'
printf 'gatekeeper=manual-first-launch-approval-required\n'
printf 'updater=manual-release-page\n'
