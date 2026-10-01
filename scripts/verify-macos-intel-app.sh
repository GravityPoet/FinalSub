#!/bin/bash
set -euo pipefail

translated="$(sysctl -in sysctl.proc_translated 2>/dev/null || echo 0)"
if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "x86_64" ] || [ "${translated}" = "1" ]; then
  echo "This verifier must run natively on an Intel Mac." >&2
  exit 2
fi

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 /absolute/path/to/FinalSub.app" >&2
  exit 2
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
APP_PATH="$1"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/finalsub-intel-runtime.XXXXXX")"
APP_PID=""

cleanup() {
  cleanup_status=$?
  trap - EXIT INT TERM
  if [ -n "${APP_PID}" ] && kill -0 "${APP_PID}" 2>/dev/null; then
    kill -TERM "${APP_PID}" 2>/dev/null || true
    wait "${APP_PID}" 2>/dev/null || true
  fi
  case "${WORK_DIR}" in
    "${TMPDIR:-/tmp}"/finalsub-intel-runtime.*)
      find "${WORK_DIR}" -depth -delete 2>/dev/null || true
      ;;
    *)
      echo "Refusing to clean unexpected Intel smoke path: ${WORK_DIR}" >&2
      cleanup_status=1
      ;;
  esac
  exit "${cleanup_status}"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command in awk bash codesign find grep kill lipo mkdir node plutil sed sleep sysctl xcrun; do
  if ! command -v "${command}" >/dev/null 2>&1; then
    echo "Required command not found: ${command}" >&2
    exit 1
  fi
done

case "${APP_PATH}" in
  /*) ;;
  *)
    echo "FinalSub app path must be absolute: ${APP_PATH}" >&2
    exit 2
    ;;
esac

if [ ! -d "${APP_PATH}" ]; then
  echo "Missing FinalSub app: ${APP_PATH}" >&2
  exit 1
fi

SANDBOX_HOME="${WORK_DIR}/home"
SANDBOX_TMP="${WORK_DIR}/tmp"
mkdir -p \
  "${SANDBOX_HOME}/Library" \
  "${SANDBOX_TMP}" \
  "${WORK_DIR}/config" \
  "${WORK_DIR}/data" \
  "${WORK_DIR}/cache"

EXPECTED_VERSION="$(node -p 'require(process.argv[1]).version' "${REPO_ROOT}/package.json")"
ACTUAL_VERSION="$(plutil -extract CFBundleShortVersionString raw "${APP_PATH}/Contents/Info.plist")"
ACTUAL_BUNDLE_ID="$(plutil -extract CFBundleIdentifier raw "${APP_PATH}/Contents/Info.plist")"
if [ "${ACTUAL_VERSION}" != "${EXPECTED_VERSION}" ] || [ "${ACTUAL_BUNDLE_ID}" != "com.gravitypoet.finalsub" ]; then
  echo "Intel app metadata does not match the repository release target." >&2
  printf 'expected_version=%s actual_version=%s bundle_id=%s\n' \
    "${EXPECTED_VERSION}" "${ACTUAL_VERSION}" "${ACTUAL_BUNDLE_ID}" >&2
  exit 1
fi
if [ "$(plutil -extract LSRequiresNativeExecution raw "${APP_PATH}/Contents/Info.plist")" != "true" ]; then
  echo "Intel validation app must require native execution on Apple silicon." >&2
  exit 1
fi

MAIN_BINARY="${APP_PATH}/Contents/MacOS/finalsubtauri"
FFMPEG_BINARY="${APP_PATH}/Contents/MacOS/ffmpeg"
WHISPER_BINARY="${APP_PATH}/Contents/MacOS/whisper-cli"

codesign --verify --deep --strict --verbose=2 "${APP_PATH}"
for binary_path in "${MAIN_BINARY}" "${FFMPEG_BINARY}" "${WHISPER_BINARY}"; do
  if [ ! -x "${binary_path}" ]; then
    echo "Missing executable in Intel app: ${binary_path}" >&2
    exit 1
  fi
  lipo "${binary_path}" -verify_arch x86_64
done

if [ "$(xcrun vtool -show-build "${MAIN_BINARY}" | awk '/minos/{print $2; exit}')" != "12.0" ]; then
  echo "Intel app minimum deployment target is not macOS 12.0." >&2
  exit 1
fi
if [ "$(plutil -extract LSMinimumSystemVersion raw "${APP_PATH}/Contents/Info.plist")" != "12.0" ]; then
  echo "Intel app Info.plist minimum system version is not 12.0." >&2
  exit 1
fi

"${FFMPEG_BINARY}" -version >/dev/null
"${FFMPEG_BINARY}" -hide_banner -filters 2>&1 | grep -F ' subtitles ' >/dev/null
"${FFMPEG_BINARY}" -hide_banner -encoders 2>&1 | grep -F 'libx264' >/dev/null
"${WHISPER_BINARY}" --help >"${WORK_DIR}/whisper-help.log" 2>&1
grep -q '^usage:' "${WORK_DIR}/whisper-help.log"

FFMPEG_BIN="${FFMPEG_BINARY}" ARTIFACT_DIR="${WORK_DIR}/burn-in" \
  bash "${SCRIPT_DIR}/verify-ffmpeg-burn-in.sh"

(
  export HOME="${SANDBOX_HOME}"
  export CFFIXED_USER_HOME="${SANDBOX_HOME}"
  export TMPDIR="${SANDBOX_TMP}/"
  export XDG_CONFIG_HOME="${WORK_DIR}/config"
  export XDG_DATA_HOME="${WORK_DIR}/data"
  export XDG_CACHE_HOME="${WORK_DIR}/cache"
  export FINALSUB_INTEL_VALIDATION=1
  exec "${MAIN_BINARY}"
) >"${WORK_DIR}/app.log" 2>&1 &
APP_PID=$!
startup_second=0
while [ "${startup_second}" -lt 10 ]; do
  if ! kill -0 "${APP_PID}" 2>/dev/null; then
    wait "${APP_PID}" || app_status=$?
    echo "Intel FinalSub exited during startup smoke (status ${app_status:-0})." >&2
    sed -n '1,160p' "${WORK_DIR}/app.log" >&2
    exit 1
  fi
  sleep 1
  startup_second=$((startup_second + 1))
done

if grep -Eiq 'panicked at|fatal runtime error|PluginInitialization' "${WORK_DIR}/app.log"; then
  echo "Intel FinalSub emitted a fatal startup signature." >&2
  sed -n '1,160p' "${WORK_DIR}/app.log" >&2
  exit 1
fi

kill -TERM "${APP_PID}" 2>/dev/null || true
shutdown_second=0
while kill -0 "${APP_PID}" 2>/dev/null && [ "${shutdown_second}" -lt 5 ]; do
  sleep 1
  shutdown_second=$((shutdown_second + 1))
done
if kill -0 "${APP_PID}" 2>/dev/null; then
  kill -KILL "${APP_PID}" 2>/dev/null || true
  wait "${APP_PID}" 2>/dev/null || true
  echo "Intel FinalSub did not shut down cleanly after SIGTERM." >&2
  exit 1
fi
wait "${APP_PID}" 2>/dev/null || true
APP_PID=""

printf 'intel_runtime=passed\n'
printf 'version=%s\n' "${ACTUAL_VERSION}"
printf 'bundle_id=%s\n' "${ACTUAL_BUNDLE_ID}"
printf 'app_startup_seconds=10\n'
