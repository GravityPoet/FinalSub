#!/bin/bash
set -euo pipefail

UPSTREAM_COMMIT="f049fff95a089aa9969deb009cdd4892b3e74916"
UPSTREAM_ARCHIVE_SHA256="279af4ce60dbf397362868f3bacc75b56a4332ac2541cae155070093f6aaf0e3"
DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
ARCHIVE_URL="https://codeload.github.com/ggml-org/whisper.cpp/tar.gz/${UPSTREAM_COMMIT}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BIN_DIR="${REPO_ROOT}/src-tauri/binaries"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-whisper.XXXXXX")"
SOURCE_DIR="${WORK_DIR}/source"
STAGING_DIR="${WORK_DIR}/staging"
BACKUP_DIR="${WORK_DIR}/backup"
JOBS="${JOBS:-$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)}"
INSTALL_STARTED=0

cleanup() {
  status=$?
  trap - EXIT

  if [ "${status}" -ne 0 ] && [ "${INSTALL_STARTED}" -eq 1 ]; then
    set +e
    echo "Build or installation failed; restoring previous sidecars." >&2
    for name in \
      whisper-cli-aarch64-apple-darwin \
      whisper-cli-x86_64-apple-darwin \
      whisper-cli-universal-apple-darwin; do
      if [ -f "${BACKUP_DIR}/${name}" ]; then
        cp -p "${BACKUP_DIR}/${name}" "${BIN_DIR}/${name}"
      else
        rm -f "${BIN_DIR}/${name}"
      fi
    done
  fi

  rm -rf "${WORK_DIR}"
  exit "${status}"
}
trap cleanup EXIT

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command not found: $1" >&2
    exit 1
  fi
}

for command in arch cmake codesign curl file lipo make otool shasum tar vtool xcrun; do
  require_command "${command}"
done

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This script must run on macOS." >&2
  exit 1
fi

mkdir -p "${SOURCE_DIR}" "${STAGING_DIR}" "${BACKUP_DIR}"

ARCHIVE_PATH="${WHISPER_ARCHIVE:-${WORK_DIR}/whisper.cpp.tar.gz}"
if [ -z "${WHISPER_ARCHIVE:-}" ]; then
  echo "Downloading whisper.cpp ${UPSTREAM_COMMIT}..."
  curl --http1.1 --fail --location --retry 3 --retry-all-errors \
    --connect-timeout 15 --max-time 300 \
    --output "${ARCHIVE_PATH}" "${ARCHIVE_URL}"
fi

actual_archive_sha256="$(shasum -a 256 "${ARCHIVE_PATH}" | awk '{print $1}')"
if [ "${actual_archive_sha256}" != "${UPSTREAM_ARCHIVE_SHA256}" ]; then
  echo "whisper.cpp archive checksum mismatch." >&2
  echo "Expected: ${UPSTREAM_ARCHIVE_SHA256}" >&2
  echo "Actual:   ${actual_archive_sha256}" >&2
  exit 1
fi

tar -xzf "${ARCHIVE_PATH}" --strip-components=1 -C "${SOURCE_DIR}"
SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"

build_arch() {
  arch_name="$1"
  build_dir="${WORK_DIR}/build-${arch_name}"
  output_name="whisper-cli-${arch_name}-apple-darwin"

  echo "Building ${output_name} for macOS ${DEPLOYMENT_TARGET}+..."
  env MACOSX_DEPLOYMENT_TARGET="${DEPLOYMENT_TARGET}" \
    cmake -S "${SOURCE_DIR}" -B "${build_dir}" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_OSX_ARCHITECTURES="${arch_name}" \
      -DCMAKE_OSX_DEPLOYMENT_TARGET="${DEPLOYMENT_TARGET}" \
      -DCMAKE_OSX_SYSROOT="${SDK_PATH}" \
      -DCMAKE_IGNORE_PREFIX_PATH="/opt/homebrew;/usr/local" \
      -DBUILD_SHARED_LIBS=OFF \
      -DGGML_STATIC=ON \
      -DGGML_NATIVE=OFF \
      -DGGML_OPENMP=OFF \
      -DGGML_BLAS=OFF \
      -DGGML_ACCELERATE=OFF \
      -DGGML_METAL=ON \
      -DGGML_METAL_EMBED_LIBRARY=ON \
      -DGGML_METAL_MACOSX_VERSION_MIN="${DEPLOYMENT_TARGET}" \
      -DWHISPER_COREML=OFF \
      -DWHISPER_CURL=OFF \
      -DWHISPER_COMMON_FFMPEG=OFF \
      -DWHISPER_BUILD_TESTS=OFF \
      -DWHISPER_BUILD_SERVER=OFF \
      -DWHISPER_BUILD_EXAMPLES=ON

  cmake --build "${build_dir}" --config Release --parallel "${JOBS}" --target whisper-cli
  install -m 755 "${build_dir}/bin/whisper-cli" "${STAGING_DIR}/${output_name}"
  codesign --force --sign - --timestamp=none "${STAGING_DIR}/${output_name}"
}

validate_thin_binary() {
  binary="$1"
  expected_arch="$2"
  actual_arch="$(lipo -archs "${binary}")"
  actual_minos="$(vtool -show-build "${binary}" | awk '/minos/{print $2; exit}')"
  unexpected_dependencies="$(otool -L "${binary}" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/usr/lib/|/System/Library/Frameworks/)' || true)"

  if [ "${actual_arch}" != "${expected_arch}" ]; then
    echo "Unexpected architecture for ${binary}: ${actual_arch}" >&2
    exit 1
  fi
  if [ "${actual_minos}" != "${DEPLOYMENT_TARGET}" ]; then
    echo "Unexpected deployment target for ${binary}: ${actual_minos}" >&2
    exit 1
  fi
  if [ -n "${unexpected_dependencies}" ]; then
    echo "Non-system runtime dependencies found in ${binary}:" >&2
    echo "${unexpected_dependencies}" >&2
    exit 1
  fi

  codesign --verify --strict "${binary}"
}

smoke_test() {
  binary="$1"
  binary_arch="$2"
  help_output="${WORK_DIR}/help-${binary_arch}.txt"

  arch -"${binary_arch}" "${binary}" --help >"${help_output}" 2>&1
  if ! grep -q '^usage:' "${help_output}"; then
    echo "whisper-cli --help smoke test failed for ${binary_arch}." >&2
    exit 1
  fi
}

build_arch arm64
build_arch x86_64

lipo -create \
  "${STAGING_DIR}/whisper-cli-arm64-apple-darwin" \
  "${STAGING_DIR}/whisper-cli-x86_64-apple-darwin" \
  -output "${STAGING_DIR}/whisper-cli-universal-apple-darwin"
chmod 755 "${STAGING_DIR}/whisper-cli-universal-apple-darwin"
codesign --force --sign - --timestamp=none "${STAGING_DIR}/whisper-cli-universal-apple-darwin"

validate_thin_binary "${STAGING_DIR}/whisper-cli-arm64-apple-darwin" arm64
validate_thin_binary "${STAGING_DIR}/whisper-cli-x86_64-apple-darwin" x86_64

universal_archs="$(lipo -archs "${STAGING_DIR}/whisper-cli-universal-apple-darwin")"
if [ "${universal_archs}" != "x86_64 arm64" ] && [ "${universal_archs}" != "arm64 x86_64" ]; then
  echo "Universal binary does not contain both architectures: ${universal_archs}" >&2
  exit 1
fi
for arch_name in arm64 x86_64; do
  universal_minos="$(vtool -arch "${arch_name}" -show-build "${STAGING_DIR}/whisper-cli-universal-apple-darwin" | awk '/minos/{print $2; exit}')"
  if [ "${universal_minos}" != "${DEPLOYMENT_TARGET}" ]; then
    echo "Universal ${arch_name} deployment target is ${universal_minos}, expected ${DEPLOYMENT_TARGET}." >&2
    exit 1
  fi
done
codesign --verify --strict "${STAGING_DIR}/whisper-cli-universal-apple-darwin"

smoke_test "${STAGING_DIR}/whisper-cli-arm64-apple-darwin" arm64
smoke_test "${STAGING_DIR}/whisper-cli-x86_64-apple-darwin" x86_64

for name in \
  whisper-cli-aarch64-apple-darwin \
  whisper-cli-x86_64-apple-darwin \
  whisper-cli-universal-apple-darwin; do
  if [ -f "${BIN_DIR}/${name}" ]; then
    cp -p "${BIN_DIR}/${name}" "${BACKUP_DIR}/${name}"
  fi
done

INSTALL_STARTED=1
install -m 755 "${STAGING_DIR}/whisper-cli-arm64-apple-darwin" "${BIN_DIR}/.whisper-cli-aarch64-apple-darwin.new"
install -m 755 "${STAGING_DIR}/whisper-cli-x86_64-apple-darwin" "${BIN_DIR}/.whisper-cli-x86_64-apple-darwin.new"
install -m 755 "${STAGING_DIR}/whisper-cli-universal-apple-darwin" "${BIN_DIR}/.whisper-cli-universal-apple-darwin.new"
mv -f "${BIN_DIR}/.whisper-cli-aarch64-apple-darwin.new" "${BIN_DIR}/whisper-cli-aarch64-apple-darwin"
mv -f "${BIN_DIR}/.whisper-cli-x86_64-apple-darwin.new" "${BIN_DIR}/whisper-cli-x86_64-apple-darwin"
mv -f "${BIN_DIR}/.whisper-cli-universal-apple-darwin.new" "${BIN_DIR}/whisper-cli-universal-apple-darwin"

validate_thin_binary "${BIN_DIR}/whisper-cli-aarch64-apple-darwin" arm64
validate_thin_binary "${BIN_DIR}/whisper-cli-x86_64-apple-darwin" x86_64
smoke_test "${BIN_DIR}/whisper-cli-aarch64-apple-darwin" arm64
smoke_test "${BIN_DIR}/whisper-cli-x86_64-apple-darwin" x86_64

echo "Installed and verified whisper-cli sidecars:"
file "${BIN_DIR}"/whisper-cli-*-apple-darwin
shasum -a 256 "${BIN_DIR}"/whisper-cli-*-apple-darwin
