# FFmpeg Sidecar Spike

## 当前状态

- 二进制：`src-tauri/binaries/ffmpeg-aarch64-apple-darwin`
- 来源：`/opt/homebrew/Cellar/ffmpeg/8.1.1/bin/ffmpeg`（Homebrew 动态依赖版）
- SHA256：`00d01197255300c02122c783dd0126a9e7f47d6c6a19faafae2e6610efd071d3`
- 架构：`arm64`
- 限制：依赖 Homebrew 动态库（libvpx, libx264, libx265 等），不适合正式分发

## otool 依赖分析

```bash
otool -L src-tauri/binaries/ffmpeg-aarch64-apple-darwin | head -20
```

预期输出包含 `/opt/homebrew/Cellar/...` 动态库路径。这些路径在未安装 Homebrew 的机器上不存在。

## 替代方案评估

### 方案 A：evermeet.cx 静态 FFmpeg

- 来源：`https://evermeet.cx/ffmpeg/`
- 优点：静态编译，无外部依赖，可签名
- 缺点：GPL 许可证，需要确认再分发条款
- 下载：`curl -L "https://evermeet.cx/ffmpeg/ffmpeg-7.1.1.zip" -o ffmpeg.zip`

### 方案 B：gyan.dev 静态 FFmpeg（Windows 用）

- 仅适用于 Windows，macOS 需用方案 A 或 C

### 方案 C：从源码编译静态 FFmpeg

- 优点：完全控制依赖和许可证
- 缺点：编译复杂，维护成本高
- 适用场景：正式产品发布

### 方案 D：保留 Homebrew 版本用于开发预览

- 优点：零成本
- 缺点：`build:local` 产物只能在安装了相同 Homebrew 版本的机器上运行
- 适用场景：开发阶段 MVP

## 当前决策

MVP 开发阶段使用方案 D（Homebrew 版本）。正式分发前切换到方案 A 或 C。

## 签名验证

```bash
# 当前产物签名状态
codesign --verify --deep --strict --verbose=4 \
  "src-tauri/target/release/bundle/macos/FinalSub Tauri Preview.app"

# Gatekeeper 状态（预期：rejected，因为未做 notarization）
spctl -a -vvv -t exec \
  "src-tauri/target/release/bundle/macos/FinalSub Tauri Preview.app"
```

## 正式分发前置条件

1. 替换为静态/可再分发 FFmpeg
2. Developer ID 签名
3. Hardened runtime
4. Apple notarization
5. Stapling
6. DMG 安装测试
