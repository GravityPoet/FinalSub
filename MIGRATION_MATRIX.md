# FinalSub 迁移矩阵（Electron → Tauri）

生成时间：2026-06-19  
审查更新时间：2026-06-19
来源：旧 Electron 仓库 `/Users/moonlitpoet/Tools/AI-tools/FinalSub`（已删除）+ 新仓库 `/Users/moonlitpoet/Tools/AI-tools/FinalSub`（原 FinalSubTauri，2026-06-20 改名占用同一路径）  
计划书：`/Users/moonlitpoet/Desktop/交接书/handoff-20260619-0238-finalsub-tauri-full-migration-plan.md`

品牌图标母版：`/Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/icons/app-icon-source.png`。从 2026-06-19 起，Tauri 版、旧 Electron 兼容包和文档图标均使用该母版生成，不得恢复旧图标。

## 1. 主导航迁移状态

| # | 路由 | 中文名 | 新 Tauri 文件 | 当前状态 |
|---|------|--------|---------------|----------|
| 1 | `/` | 任务 | `src/pages/HomePage.tsx` | 🟡 核心可用。真实 `create_task` 流水线已接入，包含音频提取、ASR（Whisper/Parakeet）、翻译及多格式字幕写出；GUI 点击流仍需人工验收 |
| 2 | `/tasks` | 任务队列 | `src/pages/TasksPage.tsx` | 🟡 核心可用。支持真实/预览任务生命周期，提供“打开输出文件”和“打开所在目录”功能；无持久化、暂停/恢复/重试 |
| 3 | `/models` | 模型管理 | `src/pages/ModelsPage.tsx` | 🟡 部分。可扫描设置中的 Whisper 模型目录，可删除受管 Whisper 模型；无下载、导入、checksum |
| 4 | `/translation` | 翻译管理 | `src/pages/TranslationPage.tsx` | 🟡 部分。Provider 列表和测试入口存在；DeepLX/Ollama 本地路径可走，多数商业 provider 仍未实现或缺配置 UI |
| 5 | `/proofread` | 字幕校对 | `src/pages/PlaceholderPage.tsx` | 🔴 未迁移。仍为占位 |
| 6 | `/subtitle-merge` | 视频合字幕 | `src/pages/SubtitleMergePage.tsx` | 🟡 部分。文件选择、样式预设、FFmpeg 烧录命令存在；无进度解析、取消、预览、视频/字幕信息读取 |
| 7 | `/settings` | 设置 | `src/pages/SettingsPage.tsx` | 🟡 部分。设置读写、校验、重置、JSON 导入导出存在；无加密导出、Keychain/API Key 安全存储、旧配置完整迁移 |

## 2. 任务系统

| 能力 | 新 Tauri | 当前状态 |
|------|----------|----------|
| 任务类型枚举 | `GenerateAndTranslate` / `GenerateOnly` / `TranslateOnly` | 🟡 类型存在 |
| 真实任务创建 | `create_task` | 🟡 核心可用。接入后台 task_runner 真实流水线，FFmpeg/ASR/翻译阶段均接入取消信号；商业翻译 provider 仍需真实 API Key 验证 |
| 预览任务 | `create_preview_task` | 🟢 可用于验证队列事件和 UI 流 |
| 队列状态 | `pending/running/cancelled/done/error` | 🟡 核心可用。真实任务与预览任务均支持全状态流转，前端稳定更新；任务队列尚未持久化 |
| 任务事件 | `task-updated` | 🟢 已实现 |
| 日志流 | `task-log` | 🔴 未实现 |
| 暂停/恢复/重试 | — | 🔴 未实现 |
| 持久化 | — | 🔴 未实现 |

## 3. ASR 与模型

### 3.1 ASR 引擎

| 引擎 | 新实现 | 当前状态 |
|------|--------|----------|
| Whisper.cpp | `WhisperCppEngine` + `transcribe_audio` | 🟢 已接入。接入任务流水线，支持 cancel_rx 中断，使用 sidecar 自动解析，并正确读写指定目录模型 |
| Parakeet MLX | `ParakeetMlxEngine` + `transcribe_parakeet` | 🟡 已接入。本机流水线支持 cancel_rx 中断，仍依赖本机 `uv`、MLX 与 Hugging Face 缓存，目标机器需单独验收 |
| SenseVoice | catalog only | 🔴 仅模型候选，未接入运行时 |
| Custom Command | catalog only | 🔴 未设计权限方案，不可用 |

### 3.2 模型 ID 与文件名

| 模型 ID | 期望文件 | 当前状态 |
|---------|----------|----------|
| `large-v3-turbo` | `ggml-large-v3-turbo.bin` | 🟢 审查修复：不再误找 `ggml-whisper-large-v3-turbo.bin` |
| `large-v3` | `ggml-large-v3.bin` | 🟢 |
| `medium` | `ggml-medium.bin` | 🟢 |
| `small` | `ggml-small.bin` | 🟢 |
| `parakeet-tdt-0.6b-v2` | 自动缓存目录 | 🟡 只检测本机缓存目录 |
| `sensevoice-small` | 待定 | 🔴 未接入 |

### 3.3 模型操作

| 操作 | 新 Tauri command | 当前状态 |
|------|------------------|----------|
| 列出/扫描模型 | `list_asr_models` / `scan_models` | 🟢 使用设置里的 `models_path` |
| 删除 Whisper 模型 | `delete_model` | 🟢 只删除 `ggml-*.bin` 受管文件，拒绝路径逃逸 |
| 下载模型 | — | 🔴 未实现 |
| 取消下载 | — | 🔴 未实现 |
| 导入本地模型 | — | 🔴 未实现 |
| checksum 校验 | — | 🔴 未实现 |

## 4. FFmpeg

| 功能 | 新 Tauri | 当前状态 |
|------|----------|----------|
| 版本检测 | `get_ffmpeg_version` | 🟢 sidecar 调用 |
| 音频提取 | `extract_audio` | 🟡 命令存在，参数结构化，输出路径禁止覆盖；未接入任务流水线 |
| 字幕烧录 | `burn_subtitle` | 🟡 命令存在，校验视频/字幕/输出路径和 ASS 颜色；无进度解析/取消 |
| 进度解析 | `parse_duration_ms` / `parse_current_time_ms` | 🟡 工具函数有测试，但未接入 UI |
| sidecar 打包 | `bundle.externalBin` | 🟢 |
| sidecar 来源 | osxexperts.net 静态 arm64 构建（GPL，ffmpeg 7.1.1） | 🟢 静态构建，无 Homebrew 依赖，可再分发 |
| GPL 合规 | `src-tauri/licenses/ffmpeg-GPLv2.txt` + `ffmpeg-NOTICE.md`，经 `bundle.resources` 打进 `.app` | 🟢 许可证全文+出处+SHA256+源码书面要约随二进制分发 |
| 架构覆盖 | arm64 + x86_64 thin sidecar；`npm run build:universal` lipo 成 universal | 🟢 通用包支持 Apple Silicon + Intel；`build:local` 仍可出 arm64-only 小包 |

## 5. 翻译

| 能力 | 新 Tauri | 当前状态 |
|------|----------|----------|
| Provider 列表 | `list_translation_providers` | 🟢 17 个 provider 元数据 |
| 测试翻译 | `test_translation` | 🟡 Ollama、DeepLX、本机/兼容 OpenAI 路径可走；百度/谷歌等仍是 stub |
| DeepLX 本地服务 | `api_url` 默认 `http://localhost:1188/translate` | 🟢 审查修复：不再错误要求 API Key，也不再把 API Key 当 URL |
| API Key 安全存储 | — | 🔴 未实现 |
| 字幕批量翻译 | — | 🔴 未接入任务流水线 |
| AI 优化翻译 | — | 🔴 未实现 |

## 6. 设置

| 能力 | 新 Tauri | 当前状态 |
|------|----------|----------|
| 读取/保存/重置 | `get_settings` / `save_settings_cmd` / `reset_settings` | 🟢 JSON 存储，保存前校验 |
| 字段兼容 | `Settings` serde alias | 🟢 审查修复：前端 snake_case 与旧 camelCase 导入均可处理 |
| 原子写入 | temp file + rename | 🟢 |
| JSON 导入导出 | `import_config_from_path` / `export_config_to_path` | 🟢 由 Rust 受控读写，前端不需要 fs 插件权限 |
| 加密导入导出 | — | 🔴 未实现 |
| API Key/密钥存储 | — | 🔴 未实现 |
| 旧 Electron 配置迁移 | — | 🔴 未完整实现 |

## 7. 字幕校对

| 功能 | 当前状态 |
|------|----------|
| 导入视频/字幕、自动检测同目录字幕 | 🔴 未实现 |
| 视频预览 + 字幕列表 + 当前字幕联动 | 🔴 未实现 |
| 编辑、保存、重新打开一致性 | 🔴 未实现 |
| 合并/拆分、时间偏移、搜索替换、撤销重做 | 🔴 未实现 |
| AI 优化翻译、历史任务 | 🔴 未实现 |

## 8. 安全与权限

| 安全项 | 当前状态 |
|--------|----------|
| Tauri capability | 🟢 前端仅有 core/opener/dialog open/save；审查修复后移除前端 `shell:allow-execute` |
| 前端文件系统权限 | 🟢 无 `plugin-fs` 直接读写权限；配置文件导入导出走 Rust command |
| FFmpeg 命令注入 | 🟢 Rust 结构化参数，不接收前端可执行路径 |
| 输出覆盖保护 | 🟢 音频提取、字幕输出、视频烧录和配置导出默认拒绝覆盖已有文件 |
| 模型删除路径逃逸 | 🟢 模型 ID 限定字符集，删除目标限定 `ggml-*.bin` |
| API Key 防泄漏 | 🔴 配置 UI/存储策略未完成 |
| 正式签名/Gatekeeper | 🔴 仍未做 Developer ID、hardened runtime、notarization、stapling |

## 9. 当前结论

Agent B 的“阶段 0-11 完成”不能按字面接受。当前 Tauri 版是可运行的预览版，已经有 6 个非占位导航页和若干底层命令，但还不能替代旧 Electron 版。

不可宣称完成的核心缺口：

- 真实任务流水线未接入。
- 字幕校对编辑器未迁移。
- ASR/翻译/FFmpeg 命令没有串成端到端业务任务。
- 模型下载、checksum、导入、本地缓存治理未完成。
- API Key 安全存储和加密配置导入导出未完成。
- 正式签名发布未完成。
