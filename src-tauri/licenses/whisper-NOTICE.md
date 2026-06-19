# whisper-cli Sidecar Notice

This project packages `whisper-cli` as a sidecar for ASR (Speech-to-Text) functionality.

## Source & Version
- **Upstream Repository**: [github.com/ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp)
- **Version / Tag**: `v1.9.1` (Commit `f049fff95a089aa9969deb009cdd4892b3e74916`)
- **License**: MIT (See [whisper.cpp-LICENSE.txt](./whisper.cpp-LICENSE.txt))

## Binary Metadata & Compilation Details
The sidecar binaries are compiled locally on macOS. They are built as fully self-contained static executables to prevent runtime dependency issues.

### Compile Commands
- **aarch64-apple-darwin**:
  ```bash
  cmake -B build-arm64-static -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DWHISPER_COREML=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON \
    -DBUILD_SHARED_LIBS=OFF
  cmake --build build-arm64-static --config Release -j --target whisper-cli
  ```

- **x86_64-apple-darwin**:
  ```bash
  cmake -B build-x64-static -DCMAKE_BUILD_TYPE=Release -DCMAKE_OSX_ARCHITECTURES=x86_64 \
    -DWHISPER_COREML=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON \
    -DBUILD_SHARED_LIBS=OFF -DGGML_NATIVE=OFF
  cmake --build build-x64-static --config Release -j --target whisper-cli
  ```

### File Integrity (SHA-256)
- **whisper-cli-aarch64-apple-darwin**: `c7bf9701e2937f9b9f06b1a0b6c45806c4006990bd85bb89ac32fa6486b8e563`
- **whisper-cli-x86_64-apple-darwin**: `ace8c282fc11cd1570d79f42f63a0c1e54be2c72ded434166c25dfea5713156e`
