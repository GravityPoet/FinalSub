#!/bin/bash
set -euo pipefail

IDENTITY="ChordVox Local Code Signing"
BUNDLE_ID="com.gravitypoet.finalsub"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PINNED_CERTIFICATE="$ROOT_DIR/src-tauri/signing/gravitypoet-local-signing.crt"
KEYCHAIN="${FINALSUB_SIGNING_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"

if [[ -n "${1:-}" ]]; then
  echo "Usage: $0" >&2
  exit 2
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Error: FinalSub macOS signing verification requires macOS." >&2
  exit 2
fi

if [[ -x /opt/homebrew/bin/openssl ]]; then
  OPENSSL=/opt/homebrew/bin/openssl
elif [[ -x /usr/local/bin/openssl ]]; then
  OPENSSL=/usr/local/bin/openssl
elif [[ -x /usr/bin/openssl ]]; then
  OPENSSL=/usr/bin/openssl
else
  echo "Error: OpenSSL is required to inspect the FinalSub signing certificate." >&2
  exit 2
fi

if [[ ! -f "$PINNED_CERTIFICATE" ]]; then
  echo "Error: pinned GravityPoet public signing certificate is missing." >&2
  exit 1
fi

if ! /usr/bin/security find-identity -v -p codesigning "$KEYCHAIN" 2>/dev/null \
    | /usr/bin/grep -F "\"$IDENTITY\"" >/dev/null; then
  echo "Error: the pinned GravityPoet local signing identity is unavailable." >&2
  echo "Restore the existing encrypted signing backup; do not fall back to ad-hoc signing." >&2
  exit 1
fi

certificate_fingerprint_sha256() {
  "$OPENSSL" x509 -in "$1" -noout -fingerprint -sha256 \
    | /usr/bin/sed 's/^[^=]*=//; s/://g' \
    | /usr/bin/tr '[:lower:]' '[:upper:]'
}

pinned_fingerprint="$(certificate_fingerprint_sha256 "$PINNED_CERTIFICATE")"
keychain_fingerprint="$(
  /usr/bin/security find-certificate -c "$IDENTITY" -p "$KEYCHAIN" \
    | "$OPENSSL" x509 -noout -fingerprint -sha256 \
    | /usr/bin/sed 's/^[^=]*=//; s/://g' \
    | /usr/bin/tr '[:lower:]' '[:upper:]'
)"
if [[ -z "$pinned_fingerprint" || "$keychain_fingerprint" != "$pinned_fingerprint" ]]; then
  echo "Error: the keychain identity does not match FinalSub's pinned certificate." >&2
  exit 1
fi

"$OPENSSL" x509 -in "$PINNED_CERTIFICATE" -checkend 2592000 -noout >/dev/null || {
  echo "Error: the pinned GravityPoet signing certificate expires within 30 days." >&2
  exit 1
}

WORK_DIR="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/finalsub-codesign.XXXXXX")"
cleanup() {
  local status=$?
  trap - EXIT INT TERM
  case "$WORK_DIR" in
    "${TMPDIR:-/tmp}"/finalsub-codesign.*)
      /usr/bin/find "$WORK_DIR" -depth -delete 2>/dev/null || true
      ;;
    *)
      echo "Refusing to clean unexpected temporary path: $WORK_DIR" >&2
      ;;
  esac
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

first_binary="$WORK_DIR/FinalSubSigningSmokeA"
second_binary="$WORK_DIR/FinalSubSigningSmokeB"
/bin/cp /bin/echo "$first_binary"
/bin/cp /bin/date "$second_binary"
/usr/bin/codesign \
  --force \
  --sign "$IDENTITY" \
  --identifier "$BUNDLE_ID" \
  --timestamp=none \
  "$first_binary" \
  >/dev/null
/usr/bin/codesign \
  --force \
  --sign "$IDENTITY" \
  --identifier "$BUNDLE_ID" \
  --timestamp=none \
  "$second_binary" \
  >/dev/null
/usr/bin/codesign --verify --strict "$first_binary"
/usr/bin/codesign --verify --strict "$second_binary"

first_requirement="$(/usr/bin/codesign -d -r- "$first_binary" 2>&1 \
  | /usr/bin/sed -n 's/^designated => //p' \
  | /usr/bin/head -n 1)"
second_requirement="$(/usr/bin/codesign -d -r- "$second_binary" 2>&1 \
  | /usr/bin/sed -n 's/^designated => //p' \
  | /usr/bin/head -n 1)"
if [[ -z "$first_requirement" || "$first_requirement" != "$second_requirement" ]]; then
  echo "Error: FinalSub signing requirement changed across different binary contents." >&2
  exit 1
fi
if [[ "$first_requirement" != *'certificate leaf = H"'* && \
    "$first_requirement" != *'certificate root = H"'* && \
    "$first_requirement" != *'anchor root = H"'* ]]; then
  echo "Error: FinalSub identity did not produce a certificate-bound requirement." >&2
  printf 'actual_requirement=%s\n' "$first_requirement" >&2
  exit 1
fi

printf 'requirement=%s\n' "$first_requirement"
printf 'identity=%s\n' "$IDENTITY"
printf 'certificate_sha256=%s\n' "$keychain_fingerprint"
