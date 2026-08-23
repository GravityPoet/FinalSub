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
UPDATER_NAME="FinalSub-${VERSION}-macos-universal-self-signed.app.tar.gz"
OUTPUT_DIR="$REPO_ROOT/src-tauri/target/self-signed-release/$TAG"
SOURCE_DIR="$REPO_ROOT/src-tauri/target/universal-apple-darwin/release/bundle/dmg"
SOURCE_DMG="$SOURCE_DIR/FinalSub_${VERSION}_universal.dmg"
SOURCE_UPDATER="$REPO_ROOT/src-tauri/target/universal-apple-darwin/release/bundle/macos/FinalSub.app.tar.gz"
SOURCE_UPDATER_SIGNATURE="$SOURCE_UPDATER.sig"
UPDATER_PUBLIC_KEY_PATH="$REPO_ROOT/src-tauri/signing/finalsub-updater-root-v1.pub"
DEFAULT_UPDATER_PRIVATE_KEY="$HOME/Library/Application Support/GravityPoet/ReleaseKeys/FinalSub/updater/root-v1/finalsub-updater-root-v1.key"
UPDATER_PRIVATE_KEY_PATH="${FINALSUB_UPDATER_PRIVATE_KEY_PATH:-$DEFAULT_UPDATER_PRIVATE_KEY}"

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
REUSE_DRAFT="${FINALSUB_REUSE_DRAFT_RELEASE:-0}"
if git rev-parse --verify --quiet "refs/tags/$TAG" >/dev/null; then
  if [ "$REUSE_DRAFT" != "1" ] || [ "$(git rev-list -n 1 "$TAG")" != "$HEAD_SHA" ]; then
    echo "Release tag already exists locally: $TAG" >&2
    exit 1
  fi
fi
REMOTE_TAG_SHA="$(git ls-remote --tags origin "refs/tags/$TAG^{}" | awk '{print $1}')"
if [ -z "$REMOTE_TAG_SHA" ]; then
  REMOTE_TAG_SHA="$(git ls-remote --tags origin "refs/tags/$TAG" | awk '{print $1}')"
fi
if [ -n "$REMOTE_TAG_SHA" ] && { [ "$REUSE_DRAFT" != "1" ] || [ "$REMOTE_TAG_SHA" != "$HEAD_SHA" ]; }; then
  echo "Release tag already exists on origin: $TAG" >&2
  exit 1
fi
if command -v gh >/dev/null 2>&1 && gh release view "$TAG" >/dev/null 2>&1; then
  if [ "$REUSE_DRAFT" != "1" ] || [ "$(gh release view "$TAG" --json isDraft --jq .isDraft)" != "true" ]; then
    echo "GitHub Release already exists: $TAG" >&2
    exit 1
  fi
fi

if [ ! -s "$UPDATER_PUBLIC_KEY_PATH" ]; then
  echo "Missing tracked updater public key: $UPDATER_PUBLIC_KEY_PATH" >&2
  exit 1
fi
if [ ! -s "$UPDATER_PRIVATE_KEY_PATH" ]; then
  echo "Missing local updater private key: $UPDATER_PRIVATE_KEY_PATH" >&2
  exit 1
fi
if [ "$(stat -f '%Lp' "$UPDATER_PRIVATE_KEY_PATH")" != "600" ]; then
  echo "Updater private key must have mode 600: $UPDATER_PRIVATE_KEY_PATH" >&2
  exit 1
fi

export FINALSUB_UPDATER_PUBLIC_KEY
FINALSUB_UPDATER_PUBLIC_KEY="$(tr -d '\r\n' < "$UPDATER_PUBLIC_KEY_PATH")"
export TAURI_SIGNING_PRIVATE_KEY="$UPDATER_PRIVATE_KEY_PATH"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
unset APPLE_CERTIFICATE
unset APPLE_CERTIFICATE_PASSWORD
unset APPLE_SIGNING_IDENTITY
unset APPLE_ID
unset APPLE_PASSWORD
unset APPLE_TEAM_ID

for generated_artifact in "$SOURCE_DMG" "$SOURCE_UPDATER" "$SOURCE_UPDATER_SIGNATURE"; do
  case "$generated_artifact" in
    "$REPO_ROOT"/src-tauri/target/*) rm -f -- "$generated_artifact" ;;
    *)
      echo "Refusing to clean unexpected build artifact: $generated_artifact" >&2
      exit 1
      ;;
  esac
done
npm run build:universal:updater
if [ "$(git rev-parse HEAD)" != "$HEAD_SHA" ] || [ -n "$(git status --porcelain)" ]; then
  echo "Repository state changed during the updater build." >&2
  git status --short >&2
  exit 1
fi
if [ ! -f "$SOURCE_DMG" ]; then
  echo "Expected Universal DMG was not generated: $SOURCE_DMG" >&2
  exit 1
fi
if [ ! -f "$SOURCE_UPDATER" ] || [ ! -s "$SOURCE_UPDATER_SIGNATURE" ]; then
  echo "Expected signed Universal updater artifacts were not generated." >&2
  printf 'archive=%s\nsignature=%s\n' "$SOURCE_UPDATER" "$SOURCE_UPDATER_SIGNATURE" >&2
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
UPDATER_PATH="$OUTPUT_DIR/$UPDATER_NAME"
UPDATER_SIGNATURE_PATH="$UPDATER_PATH.sig"
INSTALL_PATH="$OUTPUT_DIR/INSTALL-macOS-self-signed.md"
NOTES_PATH="$OUTPUT_DIR/RELEASE_NOTES.md"
MANIFEST_PATH="$OUTPUT_DIR/release-manifest.json"
CHECKSUM_PATH="$ARTIFACT_PATH.sha256"

ditto --norsrc --noextattr "$SOURCE_DMG" "$ARTIFACT_PATH"
ditto --norsrc --noextattr "$SOURCE_UPDATER" "$UPDATER_PATH"
ditto --norsrc --noextattr "$SOURCE_UPDATER_SIGNATURE" "$UPDATER_SIGNATURE_PATH"
ditto --norsrc --noextattr "$REPO_ROOT/docs/macos-self-signed-install.md" "$INSTALL_PATH"
ditto --norsrc --noextattr "$REPO_ROOT/docs/releases/$TAG.md" "$NOTES_PATH"
bash "$REPO_ROOT/scripts/verify-macos-self-signed-package.sh" "$ARTIFACT_PATH"
bash "$REPO_ROOT/scripts/verify-macos-self-signed-updater.sh" "$UPDATER_PATH" "$UPDATER_SIGNATURE_PATH"

(
  cd "$OUTPUT_DIR"
  shasum -a 256 "$ARTIFACT_NAME" > "$ARTIFACT_NAME.sha256"
  shasum -a 256 "$UPDATER_NAME" > "$UPDATER_NAME.sha256"
)
DMG_SHA256="$(awk '{print $1}' "$CHECKSUM_PATH")"
DMG_SIZE="$(stat -f '%z' "$ARTIFACT_PATH")"
UPDATER_SHA256="$(awk '{print $1}' "$UPDATER_PATH.sha256")"
UPDATER_SIZE="$(stat -f '%z' "$UPDATER_PATH")"
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
RELEASE_UPDATER_ARTIFACT="$UPDATER_NAME" \
RELEASE_UPDATER_ARTIFACT_SIZE="$UPDATER_SIZE" \
RELEASE_UPDATER_ARTIFACT_SHA256="$UPDATER_SHA256" \
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
    mode: "tauri-signed-github-release",
    signedInAppInstall: true,
    artifact: {
      name: process.env.RELEASE_UPDATER_ARTIFACT,
      size: Number(process.env.RELEASE_UPDATER_ARTIFACT_SIZE),
      sha256: process.env.RELEASE_UPDATER_ARTIFACT_SHA256,
    },
  },
};

fs.writeFileSync(
  process.env.RELEASE_MANIFEST_PATH,
  `${JSON.stringify(manifest, null, 2)}\n`,
  { mode: 0o644 },
);
NODE

chmod 0644 \
  "$ARTIFACT_PATH" \
  "$CHECKSUM_PATH" \
  "$UPDATER_PATH" \
  "$UPDATER_PATH.sha256" \
  "$UPDATER_SIGNATURE_PATH" \
  "$INSTALL_PATH" \
  "$NOTES_PATH" \
  "$MANIFEST_PATH"

printf 'release_tag=%s\n' "$TAG"
printf 'release_commit=%s\n' "$HEAD_SHA"
printf 'artifact=%s\n' "$ARTIFACT_PATH"
printf 'artifact_size=%s\n' "$DMG_SIZE"
printf 'artifact_sha256=%s\n' "$DMG_SHA256"
printf 'updater_artifact=%s\n' "$UPDATER_PATH"
printf 'updater_artifact_size=%s\n' "$UPDATER_SIZE"
printf 'updater_artifact_sha256=%s\n' "$UPDATER_SHA256"
printf 'updater_signature=%s\n' "$UPDATER_SIGNATURE_PATH"
printf 'install_guide=%s\n' "$INSTALL_PATH"
printf 'release_notes=%s\n' "$NOTES_PATH"
printf 'release_manifest=%s\n' "$MANIFEST_PATH"
printf 'github_distribution=dry-run\n'
