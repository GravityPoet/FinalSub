#!/bin/bash
set -euo pipefail

ARCHIVE_NAME="ffmpeg-n7.1.5-2-g998de74adf-linux64-gpl-7.1.tar.xz"
ARCHIVE_SHA256="7383b376bce89252b00b1196e1d384cbd62c5597e7d42bb6de9a42adcd4fd55b"
ARCHIVE_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-07-14-13-19/${ARCHIVE_NAME}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BIN_DIR="${REPO_ROOT}/src-tauri/binaries"
DESTINATION="${BIN_DIR}/ffmpeg-x86_64-unknown-linux-gnu"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-ffmpeg-linux.XXXXXX")"
BACKUP="${WORK_DIR}/ffmpeg.backup"
INSTALL_STARTED=0

cleanup() {
  status=$?
  trap - EXIT
  if [ "${status}" -ne 0 ] && [ "${INSTALL_STARTED}" -eq 1 ]; then
    if [ -f "${BACKUP}" ]; then
      cp -p "${BACKUP}" "${DESTINATION}"
    else
      rm -f "${DESTINATION}"
    fi
  fi
  rm -rf "${WORK_DIR}"
  exit "${status}"
}
trap cleanup EXIT

for command in curl file grep install sha256sum tar; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "Required command not found: ${command}" >&2
    exit 1
  fi
done

if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
  echo "This script requires x86_64 Linux." >&2
  exit 1
fi

archive="${WORK_DIR}/${ARCHIVE_NAME}"
curl --http1.1 --fail --location --retry 5 --retry-all-errors \
  --connect-timeout 20 --max-time 900 --output "${archive}" "${ARCHIVE_URL}"

actual_sha256="$(sha256sum "${archive}" | awk '{print $1}')"
if [ "${actual_sha256}" != "${ARCHIVE_SHA256}" ]; then
  echo "FFmpeg archive checksum mismatch: ${actual_sha256}" >&2
  exit 1
fi

tar -xJf "${archive}" -C "${WORK_DIR}"
source_binary="$(find "${WORK_DIR}" -type f -path '*/bin/ffmpeg' -print -quit)"
if [ -z "${source_binary}" ]; then
  echo "FFmpeg archive did not contain bin/ffmpeg." >&2
  exit 1
fi

buildconf="$(${source_binary} -hide_banner -buildconf 2>&1)"
if echo "${buildconf}" | grep -q -- '--enable-nonfree'; then
  echo "Refusing to bundle a nonfree FFmpeg build." >&2
  exit 1
fi
filters="$(${source_binary} -hide_banner -filters 2>&1)"
if ! echo "${filters}" | grep -q '[[:space:]]subtitles[[:space:]]'; then
  echo "FFmpeg build lacks the subtitles filter." >&2
  exit 1
fi
encoders="$(${source_binary} -hide_banner -encoders 2>&1)"
if ! echo "${encoders}" | grep -q '[[:space:]]libx264[[:space:]]'; then
  echo "FFmpeg build lacks the libx264 encoder." >&2
  exit 1
fi

mkdir -p "${BIN_DIR}"
if [ -f "${DESTINATION}" ]; then
  cp -p "${DESTINATION}" "${BACKUP}"
fi
INSTALL_STARTED=1
install -m 755 "${source_binary}" "${DESTINATION}.new"
mv -f "${DESTINATION}.new" "${DESTINATION}"

file "${DESTINATION}" | grep -q 'x86-64'
"${DESTINATION}" -version >/dev/null
sha256sum "${DESTINATION}"
