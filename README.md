# FinalSub Tauri Preview

FinalSub 的 Tauri 2 重写预览工程。当前目标是验证 Tauri + React UI、Rust 核心命令、任务状态事件、FFmpeg sidecar 和本地打包链路。

## 品牌图标

从 2026-06-19 起，FinalSub 所有新版打包图标统一使用 `src-tauri/icons/app-icon-source.png` 作为母版。

生成 Tauri 全平台图标时执行：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSubTauri && npx tauri icon src-tauri/icons/app-icon-source.png
```

旧 Electron 兼容仓库使用同一母版同步到 `/Users/moonlitpoet/Tools/AI-tools/FinalSub/resources/icon-source.png` 和 `resources/icon.*`，迁移过程中不得恢复旧图标。

## 当前可用

- 中文界面和主导航入口：任务、任务队列、模型管理、翻译管理、字幕校对、视频合字幕、设置
- FFmpeg sidecar 版本检测
- 预览任务创建、进度事件、取消和任务列表刷新
- ASR 模型目录扫描和受管 Whisper 模型删除
- 设置读写、重置、JSON 导入导出
- 视频合字幕基础烧录命令
- SRT 解析/序列化核心测试

## 待迁移

- 真实任务流水线：音频提取、ASR、翻译、字幕写出、错误/重试/日志
- Whisper.cpp 和 Parakeet MLX 转录命令接入任务队列
- SenseVoice 运行时验证
- 翻译服务完整配置、API Key 安全存储、字幕批量翻译
- 字幕校对编辑器
- 视频合字幕进度解析、预览、取消
- 正式发布签名与 notarization

## 验收命令

```bash
npm run build
cd src-tauri && cargo test && cargo clippy -- -D warnings
cd .. && npm run build:local
```

`npm run build:local` 会执行 Tauri 打包、本地 ad-hoc 签名和 `codesign --verify --deep --strict` 校验。

## FFmpeg sidecar 说明

当前 `src-tauri/binaries/ffmpeg-aarch64-apple-darwin` 来自本机 Homebrew `ffmpeg 8.1.1`，用于 Apple Silicon 本机预览验证。它仍依赖 Homebrew 动态库，不是正式可再分发的完整 FFmpeg 包。

正式发布前需要替换为可再分发的 FFmpeg 方案，并完成依赖库、许可证、签名和 notarization 验证。
