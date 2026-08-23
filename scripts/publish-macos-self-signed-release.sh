#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This publisher is only supported on macOS." >&2
  exit 2
fi
if [ "$#" -ne 0 ]; then
  echo "Usage: FINALSUB_SELF_SIGNED_REVISION=<positive integer> $0" >&2
  exit 2
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(node -p 'require(process.argv[1]).version' "$REPO_ROOT/package.json")"
REVISION="${FINALSUB_SELF_SIGNED_REVISION:-1}"
TAG="v${VERSION}-self-signed.${REVISION}"
OUTPUT_DIR="$REPO_ROOT/src-tauri/target/self-signed-release/$TAG"
UPDATER_NAME="FinalSub-${VERSION}-macos-universal-self-signed.app.tar.gz"
UPDATER_PATH="$OUTPUT_DIR/$UPDATER_NAME"
UPDATER_SIGNATURE="$UPDATER_PATH.sig"
NOTES_PATH="$OUTPUT_DIR/RELEASE_NOTES.md"
LATEST_PATH="$OUTPUT_DIR/latest.json"
RELEASE_CREATED=0
RELEASE_PUBLISHED=0
VERIFY_DIR=""

finish() {
  status=$?
  trap - EXIT INT TERM
  if [ -n "$VERIFY_DIR" ]; then
    case "$VERIFY_DIR" in
      "${TMPDIR:-/tmp}"/finalsub-release-download.*)
        find "$VERIFY_DIR" -depth -delete 2>/dev/null || true
        ;;
    esac
  fi
  if [ "$status" -ne 0 ] && [ "$RELEASE_CREATED" -eq 1 ]; then
    gh release edit "$TAG" --draft=true >/dev/null 2>&1 || true
    echo "Release validation failed; $TAG was left as a draft." >&2
  fi
  exit "$status"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cd "$REPO_ROOT"
case "$REVISION" in
  ''|*[!0-9]*|0)
    echo "FINALSUB_SELF_SIGNED_REVISION must be a positive integer." >&2
    exit 2
    ;;
esac
if [ "$(git branch --show-current)" != "main" ] || [ -n "$(git status --porcelain)" ]; then
  echo "Self-signed releases require a clean main working tree." >&2
  git status --short >&2
  exit 1
fi
HEAD_SHA="$(git rev-parse HEAD)"
UPSTREAM_SHA="$(git rev-parse '@{upstream}')"
if [ "$HEAD_SHA" != "$UPSTREAM_SHA" ]; then
  echo "main must be synchronized with its upstream before release." >&2
  exit 1
fi

FINALSUB_SELF_SIGNED_REVISION="$REVISION" \
FINALSUB_REUSE_DRAFT_RELEASE=1 \
npm run package:release:self-signed:macos
test -s "$UPDATER_PATH"
test -s "$UPDATER_SIGNATURE"
test -s "$NOTES_PATH"

if git rev-parse --verify --quiet "refs/tags/$TAG" >/dev/null; then
  test "$(git rev-list -n 1 "$TAG")" = "$HEAD_SHA"
else
  git tag -a "$TAG" -m "FinalSub $VERSION macOS self-signed"
fi
remote_tag_sha="$(git ls-remote --tags origin "refs/tags/$TAG^{}" | awk '{print $1}')"
if [ -n "$remote_tag_sha" ]; then
  test "$remote_tag_sha" = "$HEAD_SHA"
else
  git push origin "$TAG"
fi

if gh release view "$TAG" >/dev/null 2>&1; then
  test "$(gh release view "$TAG" --json isDraft --jq .isDraft)" = "true"
else
  gh release create "$TAG" \
    --verify-tag \
    --draft \
    --title "FinalSub $VERSION · macOS Universal Self-Signed" \
    --notes-file "$NOTES_PATH"
fi
RELEASE_CREATED=1

gh release upload "$TAG" --clobber "$OUTPUT_DIR"/*
asset_url="$(
  gh release view "$TAG" --json assets | node -e '
    let input = "";
    process.stdin.on("data", chunk => input += chunk);
    process.stdin.on("end", () => {
      const name = process.argv[1];
      const asset = JSON.parse(input).assets.find(item => item.name === name);
      if (!asset?.apiUrl) process.exit(1);
      process.stdout.write(asset.apiUrl);
    });
  ' "$UPDATER_NAME"
)"
node scripts/create-macos-updater-manifest.mjs \
  "$VERSION" \
  "$asset_url" \
  "$UPDATER_SIGNATURE" \
  "$NOTES_PATH" \
  "$LATEST_PATH"
gh release upload "$TAG" --clobber "$LATEST_PATH"

VERIFY_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-release-download.XXXXXX")"
gh release download "$TAG" --dir "$VERIFY_DIR"
cmp "$VERIFY_DIR/latest.json" "$LATEST_PATH"
(
  cd "$VERIFY_DIR"
  shasum -a 256 -c "FinalSub-${VERSION}-macos-universal-self-signed.dmg.sha256"
  shasum -a 256 -c "$UPDATER_NAME.sha256"
)
bash scripts/verify-macos-self-signed-package.sh \
  "$VERIFY_DIR/FinalSub-${VERSION}-macos-universal-self-signed.dmg"
bash scripts/verify-macos-self-signed-updater.sh \
  "$VERIFY_DIR/$UPDATER_NAME" \
  "$VERIFY_DIR/$UPDATER_NAME.sig"
node -e '
  const fs = require("node:fs");
  const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  const expected = process.argv[2];
  const expectedUrl = process.argv[3];
  const expectedSignature = fs.readFileSync(process.argv[4], "utf8").trim();
  if (manifest.version !== expected) process.exit(1);
  for (const platform of [
    "darwin-aarch64-app",
    "darwin-aarch64",
    "darwin-x86_64-app",
    "darwin-x86_64",
  ]) {
    const entry = manifest.platforms?.[platform];
    if (entry?.url !== expectedUrl || entry?.signature !== expectedSignature) process.exit(1);
  }
' "$VERIFY_DIR/latest.json" "$VERSION" "$asset_url" "$VERIFY_DIR/$UPDATER_NAME.sig"

gh release edit "$TAG" --draft=false --latest
RELEASE_PUBLISHED=1
test "$(gh release view "$TAG" --json isDraft --jq .isDraft)" = "false"

PUBLIC_LATEST="$VERIFY_DIR/latest.public.json"
PUBLIC_UPDATER="$VERIFY_DIR/$UPDATER_NAME.public"
PUBLIC_SIGNATURE="$VERIFY_DIR/$UPDATER_NAME.sig.public"
curl --fail --location --retry 6 --retry-all-errors --retry-delay 2 \
  -H 'Cache-Control: no-cache' \
  "https://github.com/GravityPoet/FinalSub/releases/latest/download/latest.json" \
  --output "$PUBLIC_LATEST"
cmp "$PUBLIC_LATEST" "$LATEST_PATH"
curl --fail --location --retry 6 --retry-all-errors --retry-delay 2 \
  -H 'Accept: application/octet-stream' \
  "$asset_url" \
  --output "$PUBLIC_UPDATER"
curl --fail --location --retry 6 --retry-all-errors --retry-delay 2 \
  "https://github.com/GravityPoet/FinalSub/releases/latest/download/$UPDATER_NAME.sig" \
  --output "$PUBLIC_SIGNATURE"
cmp "$PUBLIC_SIGNATURE" "$UPDATER_SIGNATURE"
bash scripts/verify-macos-self-signed-updater.sh "$PUBLIC_UPDATER" "$PUBLIC_SIGNATURE"

find "$VERIFY_DIR" -depth -delete
VERIFY_DIR=""
printf 'release_tag=%s\n' "$TAG"
printf 'release_commit=%s\n' "$HEAD_SHA"
printf 'release_url=%s\n' "$(gh release view "$TAG" --json url --jq .url)"
printf 'updater=tauri-signed-in-app\n'
