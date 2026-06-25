<!-- Note: This document is for development reference only. For the user guide, please refer to README.md -->

# FinalSub 开发者备忘与历史验收指南

这是从旧 `README.md` 迁移过来的技术开发与验收备忘录。

## 品牌图标

从 2026-06-19 起，FinalSub 所有新版打包图标统一使用 `src-tauri/icons/app-icon-source.png` 作为母版。

生成 Tauri 全平台图标时执行：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npx tauri icon src-tauri/icons/app-icon-source.png
```

## 发布前缺口 (截至 2026-06-21)

- 内置模型下载、导入、checksum 校验和下载取消尚未实现；当前需要用户手动下载 Whisper `ggml-*.bin` 到模型目录
- SenseVoice 运行时、自定义 ASR 命令尚未接入
- 百度、谷歌、阿里云、火山、小牛、腾讯、讯飞、微软、Azure OpenAI 等 provider 仍为未接入状态（已在 Tauri command dispatch 中预留，但需真实 E2E 验证）
- 商业翻译 provider 仍需真实 API Key/模型配置验收，字幕批量翻译和 AI 优化翻译未完整接入主流水线
- 任务队列尚未持久化，暂停/恢复/重试和任务日志流未实现
- 字幕校对仍需 GUI 点击流、复杂编辑能力和失败恢复验收
- 视频合字幕进度解析、预览、取消尚未接入 UI
- GUI 点击流端到端人工验收、Intel/x86_64 实机验收仍未完成
- 正式发布签名与 notarization

## 验收命令

```bash
npm run build
cd src-tauri && cargo test && cargo clippy -- -D warnings
cd .. && npm run build:local
```

`npm run build:local` 会执行 Tauri 打包、本地 ad-hoc 签名和 `codesign --verify --deep --strict` 校验。

正式发布、覆盖安装包、平台产物验证和踩坑记录统一维护在 [Release SOP](release-sop.md)。

## FFmpeg 与 ASR Sidecar 说明

本项目内置了已完成签名的、可直接分发的静态多架构 (Universal) `ffmpeg` 与 `whisper-cli` Sidecar 二进制文件（支持 x86_64 与 arm64），无外部 Homebrew 或系统运行时依赖，符合全自包含打包与沙箱安全合规要求。

## 致敬与开源授权

FinalSub 是一个独立的字幕生成与翻译应用。本项目在研发与设计过程中，其早期的基础架构及部分功能设计灵感来自优秀的开源项目 **SmartSub (妙幕)** (`https://github.com/buxuku/SmartSub`，基于 MIT 许可证开源，Copyright (c) 2024 Lin Xiaodong)。我们对此表示诚挚的谢意。

关于第三方开源依赖及上游基座的完整许可协议与版权声明，请参阅 [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)。
