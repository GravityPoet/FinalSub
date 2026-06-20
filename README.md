# FinalSub

独立的跨平台字幕生成与翻译桌面应用，基于 Tauri 2 + React + Rust。

## 品牌图标

从 2026-06-19 起，FinalSub 所有新版打包图标统一使用 `src-tauri/icons/app-icon-source.png` 作为母版。

生成 Tauri 全平台图标时执行：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npx tauri icon src-tauri/icons/app-icon-source.png
```

## 当前可用

- 中文界面和主导航入口：任务、任务队列、模型管理、翻译管理、字幕校对、视频合字幕、设置
- FFmpeg sidecar 版本检测
- 真实任务流水线（核心可用）：音频提取、ASR、翻译、字幕格式化写出（支持 srt, vtt, txt, lrc, ass）、取消强杀外部进程、任务状态更新
- 预览任务创建、进度事件、取消和任务列表刷新
- ASR 模型目录扫描和受管 Whisper 模型删除
- 设置读写、重置、JSON 导入导出
- 视频合字幕基础烧录命令
- SRT/VTT/LRC/TXT/ASS 解析与格式化写出核心测试

## 待迁移

- SenseVoice 运行时验证
- 商业翻译 provider 的真实 API Key/模型配置验收、字幕批量翻译
- GUI 点击流端到端人工验收
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

## FFmpeg 与 ASR Sidecar 说明

本项目内置了已完成签名的、可直接分发的静态多架构 (Universal) `ffmpeg` 与 `whisper-cli` Sidecar 二进制文件（支持 x86_64 与 arm64），无外部 Homebrew 或系统运行时依赖，符合全自包含打包与沙箱安全合规要求。

## 致敬与开源授权

FinalSub 是一个独立的字幕生成与翻译应用。本项目在研发与设计过程中，其早期的基础架构及部分功能设计灵感来自优秀的开源项目 **SmartSub (妙幕)** (`https://github.com/buxuku/SmartSub`，基于 MIT 许可证开源，Copyright (c) 2024 Lin Xiaodong)。我们对此表示诚挚的谢意。

关于第三方开源依赖及上游基座的完整许可协议与版权声明，请参阅 [THIRD_PARTY_NOTICES.md](file:///Users/moonlitpoet/Tools/AI-tools/FinalSub/THIRD_PARTY_NOTICES.md)。
