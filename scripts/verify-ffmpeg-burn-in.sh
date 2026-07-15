#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

case "$(uname -m)" in
  arm64)
    DEFAULT_FFMPEG="${REPO_ROOT}/src-tauri/binaries/ffmpeg-aarch64-apple-darwin"
    ;;
  x86_64)
    DEFAULT_FFMPEG="${REPO_ROOT}/src-tauri/binaries/ffmpeg-x86_64-apple-darwin"
    ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

FFMPEG_BIN="${FFMPEG_BIN:-${DEFAULT_FFMPEG}}"
WORK_DIR="${ARTIFACT_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/finalsub-burn-in.XXXXXX")}"
CREATED_TEMP_DIR=0
if [ -z "${ARTIFACT_DIR:-}" ]; then
  CREATED_TEMP_DIR=1
fi

cleanup() {
  status=$?
  trap - EXIT
  if [ "${status}" -ne 0 ]; then
    echo "Burn-in verification failed; artifacts kept at ${WORK_DIR}" >&2
  elif [ "${CREATED_TEMP_DIR}" -eq 1 ] && [ "${KEEP_ARTIFACTS:-0}" != "1" ]; then
    rm -rf "${WORK_DIR}"
  fi
  exit "${status}"
}
trap cleanup EXIT

if [ ! -x "${FFMPEG_BIN}" ]; then
  echo "Bundled FFmpeg is not executable: ${FFMPEG_BIN}" >&2
  exit 1
fi

mkdir -p "${WORK_DIR}"
INPUT_VIDEO="${WORK_DIR}/input.mp4"
SUBTITLE_FILE="${WORK_DIR}/sample.srt"
OUTPUT_VIDEO="${WORK_DIR}/burned.mp4"
PROOF_FRAME="${WORK_DIR}/proof-frame.png"
UI_ALIGNMENT="${UI_ALIGNMENT:-8}"

case "${UI_ALIGNMENT}" in
  1|2|3) FFMPEG_ALIGNMENT="${UI_ALIGNMENT}" ;;
  4) FFMPEG_ALIGNMENT=9 ;;
  5) FFMPEG_ALIGNMENT=10 ;;
  6) FFMPEG_ALIGNMENT=11 ;;
  7) FFMPEG_ALIGNMENT=5 ;;
  8) FFMPEG_ALIGNMENT=6 ;;
  9) FFMPEG_ALIGNMENT=7 ;;
  *)
    echo "UI_ALIGNMENT must be an integer from 1 to 9" >&2
    exit 1
    ;;
esac

FFMPEG_ENCODERS="$("${FFMPEG_BIN}" -hide_banner -encoders 2>/dev/null)"
if ! grep -q 'libx264' <<< "${FFMPEG_ENCODERS}"; then
  echo "Bundled FFmpeg does not include libx264" >&2
  exit 1
fi
FFMPEG_FILTERS="$("${FFMPEG_BIN}" -hide_banner -filters 2>/dev/null)"
if ! grep -q ' subtitles ' <<< "${FFMPEG_FILTERS}"; then
  echo "Bundled FFmpeg does not include the libass subtitles filter" >&2
  exit 1
fi

printf '%s\n' \
  '1' \
  '00:00:00,500 --> 00:00:04,800' \
  'FinalSub · Liquid Glass' \
  '高级字幕合成实测' \
  '' \
  '2' \
  '00:00:05,000 --> 00:00:09,500' \
  'Local-first subtitles, verified end to end.' \
  '本地优先 · 十秒真实渲染' \
  > "${SUBTITLE_FILE}"

"${FFMPEG_BIN}" -hide_banner -loglevel error -y \
  -f lavfi -i 'color=c=0x182238:s=1280x720:r=30:d=10' \
  -f lavfi -i 'sine=frequency=440:sample_rate=48000:duration=10' \
  -vf 'drawgrid=width=80:height=80:thickness=1:color=white@0.08' \
  -c:v libx264 -preset veryfast -crf 20 -pix_fmt yuv420p \
  -c:a aac -b:a 128k -shortest \
  "${INPUT_VIDEO}"

SUBTITLE_FILTER="subtitles=${SUBTITLE_FILE}:force_style='FontName=Helvetica Neue,FontSize=34,PrimaryColour=&H00FFFFFF,OutlineColour=&H00000000,Outline=2,Shadow=1,Alignment=${FFMPEG_ALIGNMENT},MarginV=42,BorderStyle=3,BackColour=&H80000000'"

"${FFMPEG_BIN}" -hide_banner -loglevel error -y \
  -i "${INPUT_VIDEO}" \
  -vf "${SUBTITLE_FILTER}" \
  -c:v libx264 -crf 20 -preset medium \
  -c:a copy \
  "${OUTPUT_VIDEO}"

test -s "${OUTPUT_VIDEO}"
"${FFMPEG_BIN}" -hide_banner -loglevel error -i "${OUTPUT_VIDEO}" -f null -
"${FFMPEG_BIN}" -hide_banner -loglevel error -y -ss 00:00:02 -i "${OUTPUT_VIDEO}" -frames:v 1 "${PROOF_FRAME}"
test -s "${PROOF_FRAME}"

echo "FFmpeg burn-in verification passed"
echo "Output video: ${OUTPUT_VIDEO}"
echo "Proof frame: ${PROOF_FRAME}"
