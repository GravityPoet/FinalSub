#!/bin/bash
set -euo pipefail

UPSTREAM_COMMIT="f049fff95a089aa9969deb009cdd4892b3e74916"
UPSTREAM_ARCHIVE_SHA256="279af4ce60dbf397362868f3bacc75b56a4332ac2541cae155070093f6aaf0e3"
ARCHIVE_URL="https://codeload.github.com/ggml-org/whisper.cpp/tar.gz/${UPSTREAM_COMMIT}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BIN_DIR="${REPO_ROOT}/src-tauri/binaries"
DESTINATION="${BIN_DIR}/whisper-cli-x86_64-unknown-linux-gnu"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-whisper-linux.XXXXXX")"
SOURCE_DIR="${WORK_DIR}/source"
BUILD_DIR="${WORK_DIR}/build"
BACKUP="${WORK_DIR}/whisper.backup"
INSTALL_STARTED=0
JOBS="${JOBS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)}"

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

for command in cmake curl file grep install sha256sum tar; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "Required command not found: ${command}" >&2
    exit 1
  fi
done

if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
  echo "This script requires x86_64 Linux." >&2
  exit 1
fi

mkdir -p "${SOURCE_DIR}" "${BIN_DIR}"
archive="${WORK_DIR}/whisper.cpp.tar.gz"
curl --http1.1 --fail --location --retry 5 --retry-all-errors \
  --connect-timeout 20 --max-time 600 --output "${archive}" "${ARCHIVE_URL}"
actual_sha256="$(sha256sum "${archive}" | awk '{print $1}')"
if [ "${actual_sha256}" != "${UPSTREAM_ARCHIVE_SHA256}" ]; then
  echo "whisper.cpp archive checksum mismatch: ${actual_sha256}" >&2
  exit 1
fi
tar -xzf "${archive}" --strip-components=1 -C "${SOURCE_DIR}"

cmake -S "${SOURCE_DIR}" -B "${BUILD_DIR}" \
  -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=OFF \
  -DGGML_STATIC=ON \
  -DGGML_NATIVE=OFF \
  -DGGML_OPENMP=OFF \
  -DGGML_BLAS=OFF \
  -DGGML_METAL=OFF \
  -DGGML_CUDA=OFF \
  -DGGML_VULKAN=OFF \
  -DWHISPER_COREML=OFF \
  -DWHISPER_CURL=OFF \
  -DWHISPER_COMMON_FFMPEG=OFF \
  -DWHISPER_BUILD_TESTS=OFF \
  -DWHISPER_BUILD_SERVER=OFF \
  -DWHISPER_BUILD_EXAMPLES=ON
cmake --build "${BUILD_DIR}" --config Release --parallel "${JOBS}" --target whisper-cli

source_binary="${BUILD_DIR}/bin/whisper-cli"
if [ ! -x "${source_binary}" ]; then
  echo "whisper-cli build output is missing." >&2
  exit 1
fi
if [ -f "${DESTINATION}" ]; then
  cp -p "${DESTINATION}" "${BACKUP}"
fi
INSTALL_STARTED=1
install -m 755 "${source_binary}" "${DESTINATION}.new"
mv -f "${DESTINATION}.new" "${DESTINATION}"

file "${DESTINATION}" | grep -q 'x86-64'
help_output="$(${DESTINATION} --help 2>&1)"
echo "${help_output}" | grep -q '^usage:'
sha256sum "${DESTINATION}"
