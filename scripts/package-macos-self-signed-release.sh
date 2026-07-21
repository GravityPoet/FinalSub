#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This packager is only supported on macOS." >&2
  exit 2
fi

if [ "$#" -ne 0 ]; then
  echo "Usage: FINALSUB_SELF_SIGNED_REVISION=<positive integer> $0" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REVISION="${FINALSUB_SELF_SIGNED_REVISION:-1}"
VERSION="$(node -p 'require(process.argv[1]).version' "$REPO_ROOT/package.json")"
TAG="v${VERSION}-self-signed.${REVISION}"
ARTIFACT_NAME="FinalSub-${VERSION}-macos-universal-self-signed.dmg"
OUTPUT_DIR="$REPO_ROOT/src-tauri/target/self-signed-release/$TAG"
SOURCE_DIR="$REPO_ROOT/src-tauri/target/universal-apple-darwin/release/bundle/dmg"
SOURCE_DMG="$SOURCE_DIR/FinalSub_${VERSION}_universal.dmg"

case "$REVISION" in
  ''|*[!0-9]*|0)
    echo "FINALSUB_SELF_SIGNED_REVISION must be a positive integer." >&2
    exit 2
    ;;
esac

cd "$REPO_ROOT"
if [ "$(git branch --show-current)" != "main" ]; then
  echo "Self-signed customer packages must be built from main." >&2
  exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "Self-signed customer packages require a clean working tree." >&2
  git status --short >&2
  exit 1
fi
if ! git rev-parse --verify '@{upstream}' >/dev/null 2>&1; then
  echo "main has no upstream; refusing to package an untracked release commit." >&2
  exit 1
fi
HEAD_SHA="$(git rev-parse HEAD)"
UPSTREAM_SHA="$(git rev-parse '@{upstream}')"
if [ "$HEAD_SHA" != "$UPSTREAM_SHA" ]; then
  echo "main is not synchronized with its upstream." >&2
  printf 'head=%s\nupstream=%s\n' "$HEAD_SHA" "$UPSTREAM_SHA" >&2
  exit 1
fi
if git rev-parse --verify --quiet "refs/tags/$TAG" >/dev/null; then
  echo "Release tag already exists locally: $TAG" >&2
  exit 1
fi
if [ -n "$(git ls-remote --tags origin "refs/tags/$TAG")" ]; then
  echo "Release tag already exists on origin: $TAG" >&2
  exit 1
fi
if command -v gh >/dev/null 2>&1 && gh release view "$TAG" >/dev/null 2>&1; then
  echo "GitHub Release already exists: $TAG" >&2
  exit 1
fi

unset FINALSUB_UPDATER_PUBLIC_KEY
unset TAURI_SIGNING_PRIVATE_KEY
unset TAURI_SIGNING_PRIVATE_KEY_PASSWORD
unset APPLE_CERTIFICATE
unset APPLE_CERTIFICATE_PASSWORD
unset APPLE_SIGNING_IDENTITY
unset APPLE_ID
unset APPLE_PASSWORD
unset APPLE_TEAM_ID

npm run build:universal
if [ ! -f "$SOURCE_DMG" ]; then
  echo "Expected Universal DMG was not generated: $SOURCE_DMG" >&2
  exit 1
fi
if [ ! -f "$REPO_ROOT/docs/releases/$TAG.md" ]; then
  echo "Missing release notes for $TAG: docs/releases/$TAG.md" >&2
  exit 1
fi

case "$OUTPUT_DIR" in
  "$REPO_ROOT"/src-tauri/target/self-signed-release/v*-self-signed.*) ;;
  *)
    echo "Refusing to prepare an unexpected release output path: $OUTPUT_DIR" >&2
    exit 1
    ;;
esac
if [ -e "$OUTPUT_DIR" ]; then
  find "$OUTPUT_DIR" -depth -delete
fi
mkdir -p "$OUTPUT_DIR"

ARTIFACT_PATH="$OUTPUT_DIR/$ARTIFACT_NAME"
INSTALL_PATH="$OUTPUT_DIR/INSTALL-macOS-self-signed.md"
NOTES_PATH="$OUTPUT_DIR/RELEASE_NOTES.md"
MANIFEST_PATH="$OUTPUT_DIR/release-manifest.json"
CHECKSUM_PATH="$ARTIFACT_PATH.sha256"

ditto --norsrc --noextattr "$SOURCE_DMG" "$ARTIFACT_PATH"
ditto --norsrc --noextattr "$REPO_ROOT/docs/macos-self-signed-install.md" "$INSTALL_PATH"
ditto --norsrc --noextattr "$REPO_ROOT/docs/releases/$TAG.md" "$NOTES_PATH"
bash "$REPO_ROOT/scripts/verify-macos-self-signed-package.sh" "$ARTIFACT_PATH"

(
  cd "$OUTPUT_DIR"
  shasum -a 256 "$ARTIFACT_NAME" > "$ARTIFACT_NAME.sha256"
)
DMG_SHA256="$(awk '{print $1}' "$CHECKSUM_PATH")"
DMG_SIZE="$(stat -f '%z' "$ARTIFACT_PATH")"
signing_state="$(bash "$REPO_ROOT/scripts/ensure-macos-signing-identity.sh")"
CERTIFICATE_SHA256="$(printf '%s\n' "$signing_state" | sed -n 's/^certificate_sha256=//p' | head -n 1)"
DESIGNATED_REQUIREMENT="$(printf '%s\n' "$signing_state" | sed -n 's/^requirement=//p' | head -n 1)"

RELEASE_MANIFEST_PATH="$MANIFEST_PATH" \
RELEASE_VERSION="$VERSION" \
RELEASE_TAG="$TAG" \
RELEASE_COMMIT="$HEAD_SHA" \
RELEASE_ARTIFACT="$ARTIFACT_NAME" \
RELEASE_ARTIFACT_SIZE="$DMG_SIZE" \
RELEASE_ARTIFACT_SHA256="$DMG_SHA256" \
RELEASE_CERTIFICATE_SHA256="$CERTIFICATE_SHA256" \
RELEASE_DESIGNATED_REQUIREMENT="$DESIGNATED_REQUIREMENT" \
node <<'NODE'
const fs = require("node:fs");

const manifest = {
  schemaVersion: 1,
  product: "FinalSub",
  version: process.env.RELEASE_VERSION,
  tag: process.env.RELEASE_TAG,
  commit: process.env.RELEASE_COMMIT,
  platform: "macos-universal",
  minimumSystemVersion: "12.0",
  artifact: {
    name: process.env.RELEASE_ARTIFACT,
    size: Number(process.env.RELEASE_ARTIFACT_SIZE),
    sha256: process.env.RELEASE_ARTIFACT_SHA256,
  },
  signing: {
    strategy: "pinned-self-signed",
    identity: "ChordVox Local Code Signing",
    certificateSha256: process.env.RELEASE_CERTIFICATE_SHA256,
    designatedRequirement: process.env.RELEASE_DESIGNATED_REQUIREMENT,
    notarized: false,
  },
  updater: {
    mode: "manual-release-page",
    signedInAppInstall: false,
  },
};

fs.writeFileSync(
  process.env.RELEASE_MANIFEST_PATH,
  `${JSON.stringify(manifest, null, 2)}\n`,
  { mode: 0o644 },
);
NODE

chmod 0644 "$ARTIFACT_PATH" "$CHECKSUM_PATH" "$INSTALL_PATH" "$NOTES_PATH" "$MANIFEST_PATH"

printf 'release_tag=%s\n' "$TAG"
printf 'release_commit=%s\n' "$HEAD_SHA"
printf 'artifact=%s\n' "$ARTIFACT_PATH"
printf 'artifact_size=%s\n' "$DMG_SIZE"
printf 'artifact_sha256=%s\n' "$DMG_SHA256"
printf 'install_guide=%s\n' "$INSTALL_PATH"
printf 'release_notes=%s\n' "$NOTES_PATH"
printf 'release_manifest=%s\n' "$MANIFEST_PATH"
printf 'github_distribution=dry-run\n'
