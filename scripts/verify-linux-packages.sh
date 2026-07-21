#!/bin/bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_ROOT="${1:-${REPO_ROOT}/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle}"
WORK_DIR="$(mktemp -d "${RUNNER_TEMP:-/tmp}/finalsub-linux-package-verify.XXXXXX")"
PACKAGE_NAME=""

cleanup() {
  status=$?
  trap - EXIT
  if [ -n "${PACKAGE_NAME}" ]; then
    sudo dpkg --remove "${PACKAGE_NAME}" >/dev/null 2>&1 || true
  fi
  rm -rf "${WORK_DIR}"
  exit "${status}"
}
trap cleanup EXIT

for command in dbus-run-session dpkg dpkg-deb find gnome-keyring-daemon grep sha256sum sudo timeout xvfb-run; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "Required command not found: ${command}" >&2
    exit 1
  fi
done

APPIMAGE="$(find "${TARGET_ROOT}/appimage" -maxdepth 1 -type f -name '*.AppImage' -print -quit)"
DEB="$(find "${TARGET_ROOT}/deb" -maxdepth 1 -type f -name '*.deb' -print -quit)"
if [ -z "${APPIMAGE}" ] || [ -z "${DEB}" ]; then
  echo "Expected one AppImage and one DEB below ${TARGET_ROOT}." >&2
  exit 1
fi
if [ "$(find "${TARGET_ROOT}/appimage" -maxdepth 1 -type f -name '*.AppImage' | wc -l | tr -d ' ')" -ne 1 ]; then
  echo "Expected exactly one AppImage." >&2
  exit 1
fi
if [ "$(find "${TARGET_ROOT}/deb" -maxdepth 1 -type f -name '*.deb' | wc -l | tr -d ' ')" -ne 1 ]; then
  echo "Expected exactly one DEB." >&2
  exit 1
fi

chmod 755 "${APPIMAGE}"
(
  cd "${WORK_DIR}"
  "${APPIMAGE}" --appimage-extract >/dev/null
)
APP_RUN="${WORK_DIR}/squashfs-root/AppRun"
if [ ! -x "${APP_RUN}" ]; then
  echo "Extracted AppImage does not contain an executable AppRun." >&2
  exit 1
fi

run_gui_smoke() {
  executable="$1"
  label="$2"
  log_path="${WORK_DIR}/${label}.log"
  set +e
  dbus-run-session -- bash -c '
    set -euo pipefail
    printf "\n" | gnome-keyring-daemon --unlock --components=secrets >/dev/null
    timeout 15s xvfb-run -a "$1"
  ' bash "${executable}" >"${log_path}" 2>&1
  exit_code=$?
  set -e
  if [ "${exit_code}" -ne 124 ]; then
    echo "${label} exited before the 15 second smoke window (status ${exit_code})." >&2
    sed -n '1,160p' "${log_path}" >&2
    return 1
  fi
  if grep -Eiq 'PluginInitialization|panicked at|fatal runtime error' "${log_path}"; then
    echo "${label} emitted a fatal startup signature." >&2
    sed -n '1,160p' "${log_path}" >&2
    return 1
  fi
}

run_gui_smoke "${APP_RUN}" "appimage"

PACKAGE_NAME="$(dpkg-deb -f "${DEB}" Package)"
if [ -z "${PACKAGE_NAME}" ]; then
  echo "DEB package name is empty." >&2
  exit 1
fi
sudo dpkg --install "${DEB}"
INSTALLED_BINARY="/usr/bin/finalsubtauri"
if [ ! -x "${INSTALLED_BINARY}" ]; then
  echo "Installed DEB does not expose ${INSTALLED_BINARY}." >&2
  exit 1
fi
if ! dpkg -L "${PACKAGE_NAME}" | grep -Fx "${INSTALLED_BINARY}" >/dev/null; then
  echo "Installed DEB does not expose ${INSTALLED_BINARY}." >&2
  exit 1
fi
run_gui_smoke "${INSTALLED_BINARY}" "deb"
sudo dpkg --remove "${PACKAGE_NAME}"
if dpkg-query -W -f='${db:Status-Status}' "${PACKAGE_NAME}" 2>/dev/null | grep -qx installed; then
  echo "DEB package is still installed after removal." >&2
  exit 1
fi
PACKAGE_NAME=""

sha256sum "${APPIMAGE}" >"${APPIMAGE}.sha256"
sha256sum "${DEB}" >"${DEB}.sha256"
echo "Verified Linux AppImage and DEB startup, DEB install/removal, and SHA-256 files."
