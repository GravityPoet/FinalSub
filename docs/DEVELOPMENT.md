<!-- Note: This document is for development reference only. For the user guide, please refer to README.md -->

# FinalSub 开发者备忘与历史验收指南

这是从旧 `README.md` 迁移过来的技术开发与验收备忘录。

## 品牌图标

从 2026-06-19 起，FinalSub 所有新版打包图标统一使用 `src-tauri/icons/app-icon-source.png` 作为母版。

生成 Tauri 全平台图标时执行：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npx tauri icon src-tauri/icons/app-icon-source.png
```

## 当前发布边界（更新于 2026-07-19）

模型下载、本地原生 ASR、自定义命令、18 个翻译 provider、动态结构化批量翻译、术语表、回声对齐、持久任务队列、可保存任务配方、完成前人工审核、校对、硬/软字幕、配音音轨组合、合成进度/预览/取消和 Universal 构建均已交付。对齐 SmartSub 的剩余产品边界主要是 TTS/声音克隆、统一阶段编排与批准后自动续跑、硬件编码管理和跨任务日志中心；外部环境仍需 Apple Developer ID 与 notarization、Windows/Linux 安装启动、Linux Secret Service 桌面会话及付费云服务真实账号 smoke test。逐项源码与验证证据见 [`MIGRATION_MATRIX.md`](../MIGRATION_MATRIX.md)。

## 验收命令

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo fmt --check && cargo test --lib && cargo clippy --all-targets --all-features -- -D warnings
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build:universal
```

`npm run build:universal` 会在构建前清掉旧残留，生成并验签 arm64 + x86_64 Universal 应用与 DMG，再在成功、失败或可捕获中断后真实删除仓库 `target` 中的构建 `.app`，最终只保留 DMG。构建期间 `target/.metadata_never_index` 仅作为第二道防线，避免临时 App 被 Spotlight 收录；即使上次进程被强制终止，下一次构建也会先物理清场。需要构建后直接覆盖本机唯一应用时使用 `npm run build:install:universal`；它会安装到 `/Applications/FinalSub.app`，再清理全部构建 `.app`。

正式发布、覆盖安装包、平台产物验证和踩坑记录统一维护在 [Release SOP](release-sop.md)。

## FFmpeg 与 ASR Sidecar 说明

本项目内置了已完成签名的、可直接分发的静态多架构 (Universal) `ffmpeg` 与 `whisper-cli` Sidecar 二进制文件（支持 x86_64 与 arm64），无外部 Homebrew 或系统运行时依赖，符合全自包含打包与沙箱安全合规要求。

## 致敬与开源授权

FinalSub 是一个独立的字幕生成与翻译应用。本项目在研发与设计过程中，其早期的基础架构及部分功能设计灵感来自优秀的开源项目 **SmartSub (妙幕)** (`https://github.com/buxuku/SmartSub`，基于 MIT 许可证开源，Copyright (c) 2024 Lin Xiaodong)。我们对此表示诚挚的谢意。

关于第三方开源依赖及上游基座的完整许可协议与版权声明，请参阅 [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)。
