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

## Bundled Binaries / 随附二进制

The app ships per-architecture sidecars. A universal build (`npm run build:universal`)
lipo-combines them into `ffmpeg-universal-apple-darwin`, bundled as `Contents/MacOS/ffmpeg`.

| 项 | arm64 (Apple Silicon) | x86_64 (Intel) |
|---|---|---|
| Component | FFmpeg | FFmpeg |
| Version | **7.1.1** | **8.0** |
| Source sidecar | `src-tauri/binaries/ffmpeg-aarch64-apple-darwin` | `src-tauri/binaries/ffmpeg-x86_64-apple-darwin` |
| SHA-256 | `2ff8f1e467477684ebc5281b2d566bd881f911cb8dfcf3daee8f59ed1049ae07` | `df3f1e3facdc1ae0ad0bd898cdfb072fbc9641bf47b11f172844525a05db8d11` |
| Provenance | osxexperts.net `ffmpeg711arm.zip` | osxexperts.net `ffmpeg80intel.zip` |
| Linking | static (no Homebrew/`/opt/homebrew` deps; system libs + frameworks only) | static (same) |

- Both slices are **GPL-2.0-or-later, `--enable-gpl` WITHOUT `--enable-nonfree`** → redistributable.
- The arm64 and x86_64 slices are different FFmpeg point releases (7.1.1 / 8.0); both expose the
  same codecs used by this app (x264/x265/libass) and are functionally equivalent for audio
  extraction and subtitle burn-in.
- Code signature: ad-hoc (`codesign --sign -`); Developer ID / notarization is a later release step.
- SHA-256 values are of the **executable** (matching the values published by osxexperts.net), not the zip.

## Build Configuration / 构建配置

Exact `configuration:` string reported by `ffmpeg -buildconf` for the **arm64** binary
(the x86_64 8.0 build uses an equivalent `--enable-gpl` config, also without `--enable-nonfree`):

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

- Pre-built static binaries obtained from **osxexperts.net**, an upstream-unaffiliated macOS
  static-build distributor:
  - arm64: `https://www.osxexperts.net/ffmpeg711arm.zip`
  - x86_64: `https://www.osxexperts.net/ffmpeg80intel.zip`
- 本仓库直接提交两份 thin 二进制（非构建期下载），以保证可复现、不依赖第三方个人站点的可用性。
  Integrity is pinned by the per-arch SHA-256 above.
- The `ffmpeg-universal-apple-darwin` fat binary is **not committed**; it is produced at build time
  by `npm run binaries:universal` (`lipo -create` of the two thin binaries) and git-ignored.

## Corresponding Source / 对应源码（GPL §3 书面要约）

The complete corresponding source for the bundled FFmpeg and its GPL components is available from:

- FFmpeg 7.1.1 (arm64): `https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz` (git tag `n7.1.1`)
- FFmpeg 8.0 (x86_64): `https://ffmpeg.org/releases/ffmpeg-8.0.tar.xz` (git tag `n8.0`)
  at `https://git.ffmpeg.org/ffmpeg.git`
- GPL dependencies' sources (x264, x265, etc.): see the FFmpeg `LICENSE.md` and each library's
  upstream project listed in the build configuration above.

**Written offer**: For a period of three (3) years, the distributor will, on request, provide a copy
of the complete corresponding source code for the bundled GPL FFmpeg binaries, matching the versions
and build configurations recorded above.

## Intel / Universal builds / 关于 Intel·通用包

The original Electron `finalsub` (MIT) shipped FFmpeg via `ffmpeg-static`, supporting both arm64 and
x86_64. This Tauri port matches that: **both architectures are supported**. Build a universal app/dmg
(runs on Apple Silicon and Intel) with `npm run build:universal`, which requires the
`x86_64-apple-darwin` Rust target (`rustup target add x86_64-apple-darwin`). The arm64-only
`npm run build:local` remains available for a smaller Apple-Silicon-only artifact.
