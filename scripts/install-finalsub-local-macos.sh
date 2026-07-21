#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This installer is only supported on macOS." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE_APP="${FINALSUB_SOURCE_APP:-$REPO_ROOT/src-tauri/target/universal-apple-darwin/release/bundle/macos/FinalSub.app}"
DEST_APP="/Applications/FinalSub.app"
TARGET_DIR="$REPO_ROOT/src-tauri/target"
BUNDLE_ID="com.gravitypoet.finalsub"
BINARY_NAME="finalsubtauri"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
PROCESS_PATTERN='^/Applications/FinalSub\.app/Contents/MacOS/finalsubtauri( |$)'
STAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR="$HOME/Library/Application Support/FinalSub/Backups/$STAMP"
BACKUP_ZIP="$BACKUP_DIR/FinalSub.app.zip"
STAGE_APP="/Applications/.FinalSub-stage-$$"
DISPLACED_APP="/Applications/.FinalSub-displaced-$$"
VERIFY_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-install-verify.XXXXXX")"
OLD_REQUIREMENT=""
NEW_REQUIREMENT=""

unregister_app_bundle() {
  app_bundle="$1"
  if [ -d "$app_bundle/Contents" ]; then
    while IFS= read -r -d '' nested_app; do
      "$LSREGISTER" -u "$nested_app" >/dev/null 2>&1 || true
    done < <(find "$app_bundle/Contents" -type d -name '*.app' -prune -print0 2>/dev/null)
  fi
  "$LSREGISTER" -u "$app_bundle" >/dev/null 2>&1 || true
}

dock_paths_for_bundle() {
  FINAL_APP_BUNDLE_ID="$BUNDLE_ID" /usr/bin/swift -e '
    import Foundation
    let bundleID = ProcessInfo.processInfo.environment["FINAL_APP_BUNDLE_ID"]!
    let plistURL = FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent("Library/Preferences/com.apple.dock.plist")
    guard let data = try? Data(contentsOf: plistURL),
          let root = try? PropertyListSerialization.propertyList(from: data, format: nil),
          let dictionary = root as? [String: Any],
          let apps = dictionary["persistent-apps"] as? [[String: Any]] else { exit(0) }
    for app in apps {
      guard let tile = app["tile-data"] as? [String: Any],
            tile["bundle-identifier"] as? String == bundleID,
            let file = tile["file-data"] as? [String: Any],
            let raw = file["_CFURLString"] as? String else { continue }
      if let url = URL(string: raw), url.isFileURL { print(url.path) } else { print(raw) }
    }
  ' | sort -u
}

cleanup_or_rollback() {
  status=$?
  trap - EXIT INT TERM
  if [ "$SOURCE_APP" != "$DEST_APP" ]; then
    unregister_app_bundle "$SOURCE_APP"
  fi
  rm -rf "$STAGE_APP" "$VERIFY_ROOT"
  if [ "$status" -ne 0 ] && [ -d "$DISPLACED_APP" ]; then
    unregister_app_bundle "$DEST_APP"
    rm -rf "$DEST_APP"
    mv "$DISPLACED_APP" "$DEST_APP"
    "$LSREGISTER" -f "$DEST_APP" >/dev/null 2>&1 || true
    open "$DEST_APP" >/dev/null 2>&1 || true
  elif [ "$status" -ne 0 ] && [ -d "$DEST_APP" ]; then
    "$LSREGISTER" -f "$DEST_APP" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup_or_rollback EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [ ! -d "$SOURCE_APP" ]; then
  echo "Missing source app: $SOURCE_APP" >&2
  exit 1
fi
if [ ! -d "$DEST_APP" ]; then
  echo "Missing installed app: $DEST_APP" >&2
  exit 1
fi

mkdir -p "$TARGET_DIR" "$BACKUP_DIR"
: > "$TARGET_DIR/.metadata_never_index"
: > "$VERIFY_ROOT/.metadata_never_index"
bash "$REPO_ROOT/scripts/verify-finalsub-macos-app.sh" "$SOURCE_APP"
if [ "$(plutil -extract CFBundleIdentifier raw "$SOURCE_APP/Contents/Info.plist")" != "$BUNDLE_ID" ]; then
  echo "Unexpected source bundle identifier." >&2
  exit 1
fi
for arch in arm64 x86_64; do
  lipo "$SOURCE_APP/Contents/MacOS/$BINARY_NAME" -verify_arch "$arch"
done

rm -rf "$STAGE_APP" "$DISPLACED_APP"
ditto --noextattr --noqtn "$SOURCE_APP" "$STAGE_APP"
xattr -cr "$STAGE_APP"
bash "$REPO_ROOT/scripts/verify-finalsub-macos-app.sh" "$STAGE_APP"
ditto -c -k --sequesterRsrc --keepParent "$DEST_APP" "$BACKUP_ZIP"
unzip -tq "$BACKUP_ZIP" >/dev/null
ditto -x -k "$BACKUP_ZIP" "$VERIFY_ROOT"
BACKUP_APP="$VERIFY_ROOT/FinalSub.app"
if [ "$(plutil -extract CFBundleIdentifier raw "$BACKUP_APP/Contents/Info.plist" 2>/dev/null || true)" != "$BUNDLE_ID" ]; then
  echo "Rollback archive contains the wrong application." >&2
  exit 1
fi
codesign --verify --deep --strict "$BACKUP_APP"
OLD_REQUIREMENT="$(codesign -d -r- "$DEST_APP" 2>&1 | sed -n 's/^designated => //p' | head -n 1)"

osascript -e 'tell application id "com.gravitypoet.finalsub" to quit' >/dev/null 2>&1 || true
for _ in 1 2 3 4 5; do
  if ! pgrep -f "$PROCESS_PATTERN" >/dev/null; then
    break
  fi
  sleep 1
done
if pgrep -f "$PROCESS_PATTERN" >/dev/null; then
  pkill -TERM -f "$PROCESS_PATTERN"
  sleep 1
fi
if pgrep -f "$PROCESS_PATTERN" >/dev/null; then
  echo "FinalSub did not stop cleanly." >&2
  exit 1
fi

"$LSREGISTER" -u "$DEST_APP" >/dev/null 2>&1 || true
mv "$DEST_APP" "$DISPLACED_APP"
mv "$STAGE_APP" "$DEST_APP"
bash "$REPO_ROOT/scripts/verify-finalsub-macos-app.sh" "$DEST_APP"
NEW_REQUIREMENT="$(codesign -d -r- "$DEST_APP" 2>&1 | sed -n 's/^designated => //p' | head -n 1)"
if [ "$(plutil -extract CFBundleIdentifier raw "$DEST_APP/Contents/Info.plist")" != "$BUNDLE_ID" ]; then
  echo "Installed app has the wrong bundle identifier." >&2
  exit 1
fi
for arch in arm64 x86_64; do
  lipo "$DEST_APP/Contents/MacOS/$BINARY_NAME" -verify_arch "$arch"
done
"$LSREGISTER" -f "$DEST_APP" >/dev/null 2>&1 || true

open "$DEST_APP"
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
  if pgrep -f "$PROCESS_PATTERN" >/dev/null; then
    break
  fi
  sleep 1
done
if ! pgrep -f "$PROCESS_PATTERN" >/dev/null; then
  echo "FinalSub did not launch from /Applications." >&2
  exit 1
fi

bash "$REPO_ROOT/scripts/cleanup-finalsub-bundle-apps-macos.sh"
rm -rf "$VERIFY_ROOT"

physical_paths="$(
  for root in \
    /Applications \
    "$REPO_ROOT" \
    /private/tmp \
    "${TMPDIR:-/tmp}" \
    "$HOME/Library/Application Support/FinalSub/Backups"
  do
    [ -d "$root" ] || continue
    find "$root" -type d -name '*.app' -prune -print0 2>/dev/null
  done | while IFS= read -r -d '' app; do
    plist="$app/Contents/Info.plist"
    [ -f "$plist" ] || continue
    if [ "$(plutil -extract CFBundleIdentifier raw "$plist" 2>/dev/null || true)" = "$BUNDLE_ID" ]; then
      printf '%s\n' "$app"
    fi
  done | sort -u
)"
if [ "$physical_paths" != "$DEST_APP" ]; then
  echo "FinalSub still has duplicate app bundles on disk:" >&2
  printf '%s\n' "${physical_paths:-<none>}" >&2
  exit 1
fi

launchservices_paths="$(
  "$LSREGISTER" -dump | awk '
    /^path:/ {
      path=$0
      sub(/^path:[[:space:]]*/, "", path)
      sub(/[[:space:]]+\(0x[0-9a-fA-F]+\)$/, "", path)
    }
    /^identifier:[[:space:]]+com\.gravitypoet\.finalsub$/ { print path }
  ' | sort -u
)"
if [ -n "$launchservices_paths" ]; then
  while IFS= read -r registered_path; do
    if [ -n "$registered_path" ] && [ "$registered_path" != "$DEST_APP" ]; then
      "$LSREGISTER" -u "$registered_path" >/dev/null 2>&1 || true
    fi
  done <<EOF
$launchservices_paths
EOF
fi
"$LSREGISTER" -f "$DEST_APP" >/dev/null 2>&1 || true

for _ in 1 2 3 4 5 6 7 8 9 10; do
  spotlight_matches="$(mdfind 'kMDItemCFBundleIdentifier == "com.gravitypoet.finalsub"c' | sort)"
  if [ "$spotlight_matches" = "$DEST_APP" ]; then
    break
  fi
  sleep 1
done
if [ "${spotlight_matches:-}" != "$DEST_APP" ]; then
  echo "FinalSub still has duplicate Spotlight entries:" >&2
  printf '%s\n' "${spotlight_matches:-<none>}" >&2
  exit 1
fi

remaining_launchservices_paths="$(
  FINAL_APP_BUNDLE_ID="$BUNDLE_ID" /usr/bin/swift -e '
    import Foundation
    import CoreServices
    let identifier = ProcessInfo.processInfo.environment["FINAL_APP_BUNDLE_ID"]! as CFString
    let urls = (LSCopyApplicationURLsForBundleIdentifier(identifier, nil)?.takeRetainedValue() as? [URL]) ?? []
    for url in urls.sorted(by: { $0.path < $1.path }) { print(url.path) }
  ' | sort -u
)"
if [ "$remaining_launchservices_paths" != "$DEST_APP" ]; then
  echo "FinalSub still has duplicate LaunchServices records:" >&2
  printf '%s\n' "${remaining_launchservices_paths:-<none>}" >&2
  exit 1
fi

dock_paths="$(dock_paths_for_bundle)"
if [ -n "$dock_paths" ] && [ "$dock_paths" != "$DEST_APP" ]; then
  killall Dock >/dev/null 2>&1 || true
  sleep 2
  dock_paths="$(dock_paths_for_bundle)"
fi
if [ -n "$dock_paths" ] && [ "$dock_paths" != "$DEST_APP" ]; then
  echo "FinalSub Dock entry points to a non-canonical path:" >&2
  printf '%s\n' "$dock_paths" >&2
  exit 1
fi

rm -rf "$DISPLACED_APP"
trap - EXIT INT TERM
if [ "$OLD_REQUIREMENT" = "$NEW_REQUIREMENT" ]; then
  printf 'SIGNING_REQUIREMENT_CHANGED=0\n'
else
  printf 'SIGNING_REQUIREMENT_CHANGED=1\n'
fi
printf 'SIGNING_REQUIREMENT=%s\n' "$NEW_REQUIREMENT"
printf 'INSTALLED_APP=%s\nBACKUP_ZIP=%s\n' "$DEST_APP" "$BACKUP_ZIP"
pgrep -fl "$PROCESS_PATTERN"
