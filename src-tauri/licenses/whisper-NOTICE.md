# whisper-cli Sidecar Notice

This project packages `whisper-cli` as a sidecar for ASR (Speech-to-Text) functionality.

## Source & Version
- **Upstream Repository**: [github.com/ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp)
- **Version / Tag**: `v1.9.1` (Commit `f049fff95a089aa9969deb009cdd4892b3e74916`)
- **License**: MIT (See [whisper.cpp-LICENSE.txt](./whisper.cpp-LICENSE.txt))

## Binary Metadata & Compilation Details
The sidecars are reproducibly built by [`scripts/build-whisper-sidecars-macos.sh`](../../scripts/build-whisper-sidecars-macos.sh) from the pinned upstream archive.

- **Upstream archive SHA-256**: `279af4ce60dbf397362868f3bacc75b56a4332ac2541cae155070093f6aaf0e3`
- **Minimum macOS version**: `12.0` for both thin slices and the universal binary
- **Runtime linkage**: system libraries/frameworks only
- **Accelerate BLAS**: disabled because the current SDK exposes BLAS entry points with macOS 13.3 availability; CPU and Metal backends remain enabled
- **Code signing**: ad-hoc signed after each thin build and after universal merge

### Build Command
```bash
bash scripts/build-whisper-sidecars-macos.sh
```

The script downloads and verifies the pinned source archive, builds `arm64` and `x86_64` with `CMAKE_OSX_DEPLOYMENT_TARGET=12.0`, checks architectures and load commands with `lipo`/`vtool`, rejects non-system dynamic dependencies with `otool`, runs `--help` under both architectures, creates the universal binary, signs all outputs, and restores the previous sidecars if installation fails.

### File Integrity (SHA-256)
- **whisper-cli-aarch64-apple-darwin**: `d97d9e9506494ba7b6196e4aac6b2820b88c365ea7ce4ad9fe9e61ca60ae5a20`
- **whisper-cli-x86_64-apple-darwin**: `f5bd0e0a9ab0a823177e3168178278bdcd555a165cc01860476a5f9adafa0794`
- **whisper-cli-universal-apple-darwin**: `db7d5654f8f168b011fff51ff5db2ebc2fa4163cc5d07a800173c98c3f9fe1f1`
