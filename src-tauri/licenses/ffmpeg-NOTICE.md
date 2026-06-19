# Bundled FFmpeg — License & Provenance Notice

本应用随附一份 **FFmpeg** 可执行文件（sidecar）。该二进制以独立可执行程序形式分发，
通过子进程（Tauri `shell.sidecar()`）调用，**未与本应用代码静态/动态链接**。
This product bundles an **FFmpeg** executable as a standalone sidecar binary, invoked
via subprocess. It is **not linked** into the application code (mere aggregation).

## License / 许可证

- FFmpeg is licensed under the **GNU General Public License, version 2 or later (GPL-2.0-or-later)**,
  because this build was configured with `--enable-gpl` (and **without** `--enable-nonfree`).
- 完整许可证全文见同目录 `ffmpeg-GPLv2.txt`（GNU GPL v2, 1991-06）。
- The application's own source code remains under the **MIT License** (see project `LICENSE`).
  GPL applies **only** to the FFmpeg executable, distributed unmodified alongside the app.

## Bundled Binary / 随附二进制

| 项 | 值 |
|---|---|
| Component | FFmpeg |
| Version | **7.1.1** (Copyright (c) 2000-2025 the FFmpeg developers) |
| File (sidecar) | `src-tauri/binaries/ffmpeg-aarch64-apple-darwin` → bundled as `Contents/MacOS/ffmpeg` |
| Architecture | **arm64 (Apple Silicon) only** — no x86_64/Intel build is bundled |
| SHA-256 | `2ff8f1e467477684ebc5281b2d566bd881f911cb8dfcf3daee8f59ed1049ae07` |
| Linking | static (no Homebrew/`/opt/homebrew` runtime deps; system libs + macOS frameworks only) |
| Code signature | ad-hoc (`codesign --sign -`); Developer ID / notarization is a later release step |

## Build Configuration / 构建配置

Exact `configuration:` string reported by `ffmpeg -buildconf` for the bundled binary:

```
--prefix=/Volumes/tempdisk/sw --extra-cflags=-fno-stack-check --arch=arm64 --cc=/usr/bin/clang \
--enable-gpl --enable-libvmaf --enable-libopenjpeg --enable-libopus --enable-libmp3lame \
--enable-libx264 --enable-libx265 --enable-libvpx --enable-libwebp --enable-libass \
--enable-libfreetype --enable-fontconfig --enable-libtheora --enable-libvorbis --enable-libsnappy \
--enable-libaom --enable-libvidstab --enable-libzimg --enable-libsvtav1 --enable-libharfbuzz \
--enable-libkvazaar --pkg-config-flags=--static --enable-ffplay --enable-postproc --enable-neon \
--enable-runtime-cpudetect --disable-indev=qtkit --disable-indev=x11grab_xcb
```

## Provenance / 来源

- Pre-built static binary obtained from **osxexperts.net** (`https://www.osxexperts.net/ffmpeg711arm.zip`),
  an upstream-unaffiliated macOS static-build distributor.
- 本仓库直接提交该二进制（非构建期下载），以保证可复现、不依赖第三方个人站点的可用性。
  Integrity is pinned by the SHA-256 above.

## Corresponding Source / 对应源码（GPL §3 书面要约）

The complete corresponding source for the bundled FFmpeg and its GPL components is available from:

- FFmpeg 7.1.1 source: `https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz`
  (git tag `n7.1.1` at `https://git.ffmpeg.org/ffmpeg.git`)
- GPL dependencies' sources (x264, x265, etc.): see the FFmpeg `LICENSE.md` and each library's
  upstream project listed in the build configuration above.

**Written offer**: For a period of three (3) years, the distributor will, on request, provide a copy
of the complete corresponding source code for the bundled GPL FFmpeg binary, matching version 7.1.1
and the build configuration recorded above.

## Note on Intel / Universal builds / 关于 Intel·通用包

The original Electron `finalsub` (MIT) shipped FFmpeg via `ffmpeg-static`, supporting both arm64 and
x86_64. This Tauri port currently bundles **arm64 only**. A universal/Intel build requires an additional
`ffmpeg-x86_64-apple-darwin` sidecar with its own provenance + this same GPL compliance. Tracked as an
open decision (target-platform scope).
