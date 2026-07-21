<!-- Note: This document is for development reference only. For the user guide, please refer to README.md -->

# FinalSub 开发者备忘与历史验收指南

这是从旧 `README.md` 迁移过来的技术开发与验收备忘录。

## 品牌图标

从 2026-06-19 起，FinalSub 所有新版打包图标统一使用 `src-tauri/icons/app-icon-source.png` 作为母版。

生成 Tauri 全平台图标时执行：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npx tauri icon src-tauri/icons/app-icon-source.png
```

## 当前发布边界（更新于 2026-07-21）

模型下载、本地原生 ASR、自定义命令、18 个翻译 provider、动态结构化批量翻译、术语表、回声对齐、服务商感知的思考控制、持久任务队列、目标驱动向导、可保存任务配方、字幕/配音双审核闸门、批准后自动续跑、校对、本地/云端 TTS（含豆包语音 V3）、完整云声音克隆与找回、独立本地 TTS worker、可恢复且可与视频联动的配音工作台、字幕安全写回、硬/软字幕、可命名与重排并跨任务复用的字幕样式预设、配音音轨组合、合成进度/预览/取消、硬件编码探测与 CPU 回退、跨任务日志中心和 Universal 构建均已交付。对齐 SmartSub 的核心产品边界已经闭环；剩余交付项是跨平台正式签名、公证、公开更新链和付费云服务真实账号 smoke test。逐项源码与验证证据见 [`MIGRATION_MATRIX.md`](../MIGRATION_MATRIX.md)，任务阶段与恢复语义见 [`task-model.md`](task-model.md)，TTS 数据边界见 [`tts-dubbing.md`](tts-dubbing.md)。

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

本地 TTS 不新增独立 sidecar 工件：Kokoro、VITS 与 ZipVoice 由当前已签名应用可执行文件以 `--finalsub-tts-worker` 模式启动，最多保留三个按需 worker 槽位并隔离原生推理崩溃。worker 内复用 `sherpa-onnx 1.13.3`，配音时间轴变速和最终 WAV/MP3 混音复用内置 FFmpeg。

## 致敬与开源授权

FinalSub 是一个独立的字幕生成与翻译应用。本项目在研发与设计过程中，其早期的基础架构及部分功能设计灵感来自优秀的开源项目 **SmartSub (妙幕)** (`https://github.com/buxuku/SmartSub`，基于 MIT 许可证开源，Copyright (c) 2024 Lin Xiaodong)。我们对此表示诚挚的谢意。

关于第三方开源依赖及上游基座的完整许可协议与版权声明，请参阅 [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md)。
