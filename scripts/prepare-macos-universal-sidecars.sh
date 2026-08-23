#!/bin/bash
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This script requires macOS." >&2
  exit 2
fi

if [ "$#" -ne 0 ]; then
  echo "Usage: $0" >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BIN_DIR="${REPO_ROOT}/src-tauri/binaries"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-universal-sidecars.XXXXXX")"
STAGING_DIR="${WORK_DIR}/staging"
BACKUP_DIR="${WORK_DIR}/backup"
SIGNING_IDENTITY="${FINALSUB_SIDECAR_SIGNING_IDENTITY:-}"
SIGNING_KEYCHAIN="${FINALSUB_SIGNING_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}"
INSTALL_STARTED=0

cleanup() {
  cleanup_status=$?
  trap - EXIT INT TERM
  if [ "${cleanup_status}" -ne 0 ] && [ "${INSTALL_STARTED}" -eq 1 ]; then
    set +e
    for binary_name in ffmpeg whisper-cli; do
      destination="${BIN_DIR}/${binary_name}-universal-apple-darwin"
      backup="${BACKUP_DIR}/${binary_name}-universal-apple-darwin"
      rm -f "${destination}.new"
      if [ -f "${backup}" ]; then
        cp -p "${backup}" "${destination}"
      else
        rm -f "${destination}"
      fi
    done
    set -e
  fi
  case "${WORK_DIR}" in
    "${TMPDIR:-/tmp}"/finalsub-universal-sidecars.*)
      find "${WORK_DIR}" -depth -delete 2>/dev/null || true
      ;;
    *)
      echo "Refusing to clean unexpected sidecar staging path: ${WORK_DIR}" >&2
      cleanup_status=1
      ;;
  esac
  exit "${cleanup_status}"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command in codesign find install lipo security; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "Required command not found: ${command}" >&2
    exit 1
  fi
done

if [ -z "${SIGNING_IDENTITY}" ]; then
  signing_state="$(bash "${SCRIPT_DIR}/ensure-macos-signing-identity.sh")"
  SIGNING_IDENTITY="$(printf '%s\n' "${signing_state}" | sed -n 's/^identity=//p' | head -n 1)"
  if [ -z "${SIGNING_IDENTITY}" ]; then
    echo "Could not resolve FinalSub's pinned local signing identity." >&2
    exit 1
  fi
elif [ "${SIGNING_IDENTITY}" = "-" ]; then
  if [ "${CI:-}" != "true" ] && [ "${FINALSUB_ALLOW_ADHOC_SIDECARS:-0}" != "1" ]; then
    echo "Ad-hoc Universal sidecars are limited to CI validation builds." >&2
    exit 1
  fi
else
  expected_identity="$(bash "${SCRIPT_DIR}/ensure-macos-signing-identity.sh" | sed -n 's/^identity=//p' | head -n 1)"
  if [ "${SIGNING_IDENTITY}" != "${expected_identity}" ]; then
    echo "Only FinalSub's pinned local identity or explicit CI ad-hoc signing is allowed." >&2
    exit 1
  fi
fi

build_universal() {
  binary_name="$1"
  arm_source="${BIN_DIR}/${binary_name}-aarch64-apple-darwin"
  intel_source="${BIN_DIR}/${binary_name}-x86_64-apple-darwin"
  staged_output="${STAGING_DIR}/${binary_name}-universal-apple-darwin"

  if [ ! -x "${arm_source}" ] || [ ! -x "${intel_source}" ]; then
    echo "Missing executable thin sidecars for ${binary_name}." >&2
    exit 1
  fi

  lipo -create "${arm_source}" "${intel_source}" -output "${staged_output}"
  chmod 755 "${staged_output}"
  if [ "${SIGNING_IDENTITY}" = "-" ]; then
    codesign --force --sign - --timestamp=none "${staged_output}"
  else
    codesign --force --keychain "${SIGNING_KEYCHAIN}" --sign "${SIGNING_IDENTITY}" --timestamp=none "${staged_output}"
  fi
  lipo "${staged_output}" -verify_arch arm64 x86_64
  codesign --verify --strict "${staged_output}"

}

mkdir -p "${STAGING_DIR}" "${BACKUP_DIR}"
build_universal ffmpeg
build_universal whisper-cli

for binary_name in ffmpeg whisper-cli; do
  destination="${BIN_DIR}/${binary_name}-universal-apple-darwin"
  if [ -f "${destination}" ]; then
    cp -p "${destination}" "${BACKUP_DIR}/${binary_name}-universal-apple-darwin"
  fi
done

INSTALL_STARTED=1
rollback() {
  if [ "${INSTALL_STARTED}" -ne 1 ]; then
    return
  fi
  for binary_name in ffmpeg whisper-cli; do
    destination="${BIN_DIR}/${binary_name}-universal-apple-darwin"
    backup="${BACKUP_DIR}/${binary_name}-universal-apple-darwin"
    rm -f "${destination}.new"
    if [ -f "${backup}" ]; then
      cp -p "${backup}" "${destination}"
    else
      rm -f "${destination}"
    fi
  done
}
trap 'rollback; exit 130' INT
trap 'rollback; exit 143' TERM

for binary_name in ffmpeg whisper-cli; do
  destination="${BIN_DIR}/${binary_name}-universal-apple-darwin"
  install -m 755 "${STAGING_DIR}/${binary_name}-universal-apple-darwin" "${destination}.new"
  mv -f "${destination}.new" "${destination}"
  lipo "${destination}" -verify_arch arm64 x86_64
  codesign --verify --strict "${destination}"
done

INSTALL_STARTED=0

printf 'sidecar_signing_identity=%s\n' "${SIGNING_IDENTITY}"
lipo -archs "${BIN_DIR}/ffmpeg-universal-apple-darwin"
lipo -archs "${BIN_DIR}/whisper-cli-universal-apple-darwin"
