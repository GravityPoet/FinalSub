# FinalSub 迁移矩阵（Electron → Tauri）

生成时间：2026-06-19  
审查更新时间：2026-06-21

品牌图标母版：`/Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/icons/app-icon-source.png`。从 2026-06-19 起，Tauri 版、旧 Electron 兼容包和文档图标均使用该母版生成，不得恢复旧图标。

## 当前裁决

FinalSub Tauri 版已经进入“本机/内测可用”状态，但不能宣称“完全迁移完成”或“公开发布成品”。核心任务流水线、模型扫描、翻译管理、字幕校对基础流程、视频合字幕基础命令已经可运行；正式发布仍缺模型内置下载、GUI E2E、真实 provider 验收、签名 notarization、Intel 实机验证和恢复/异常路径验收。

## 1. 主导航迁移状态

| # | 路由 | 中文名 | 新 Tauri 文件 | 当前状态 |
|---|------|--------|---------------|----------|
| 1 | `/` | 任务 | `src/pages/HomePage.tsx` | 🟡 核心可用。真实 `create_task` 流水线已接入，包含音频提取、ASR、可选翻译、多格式字幕写出；开始按钮不再静默灰掉，会明确提示缺文件或缺模型 |
| 2 | `/tasks` | 任务队列 | `src/pages/TasksPage.tsx` | 🟡 核心可用。支持真实/预览任务生命周期、持久化、暂停/恢复/重试、日志查看和实时日志事件；仍需真实长任务 GUI 点击流与异常恢复验收 |
| 3 | `/models` | 模型管理 | `src/pages/ModelsPage.tsx` | 🟡 部分可用。扫描设置中的模型目录，可删除受管 Whisper 模型，可打开下载页并提示放置路径；仍缺内置下载、导入、checksum 和下载取消 |
| 4 | `/translation` | 翻译管理 | `src/pages/TranslationPage.tsx` | 🟡 部分可用。Provider 配置、Keychain 密钥保存/检测、测试翻译已接入；18 个 provider 当前均标记 `implemented: true` 并进入 dispatch，商业 provider 仍需真实密钥验收 |
| 5 | `/proofread` | 字幕校对 | `src/pages/proofread/ProofreadPage.tsx` | 🟡 基础迁移完成。支持导入视频/字幕、检测同目录字幕、编辑、保存任务历史；仍需复杂编辑、失败恢复和 GUI 点击流验收 |
| 6 | `/subtitle-merge` | 视频合字幕 | `src/pages/SubtitleMergePage.tsx` | 🟡 部分可用。文件选择、样式预设、FFmpeg 烧录命令存在；开始按钮不再静默灰掉；仍缺进度解析、取消、预览和媒体信息读取 |
| 7 | `/settings` | 设置 | `src/pages/SettingsPage.tsx` | 🟡 部分可用。设置读写、校验、重置、JSON 导入导出存在；API Key 明文不回传前端；仍缺加密导出和旧 Electron 配置完整迁移 |

## 2. 任务系统

| 能力 | 新 Tauri | 当前状态 |
|------|----------|----------|
| 任务类型枚举 | `GenerateAndTranslate` / `GenerateOnly` / `TranslateOnly` | 🟢 类型存在 |
| 真实任务创建 | `create_task` | 🟡 核心可用。接入后台 `task_runner` 真实流水线，FFmpeg/ASR/翻译阶段均接入取消信号；商业 provider 仍需真实 API Key 验收 |
| 预览任务 | `create_preview_task` | 🟢 可用于验证队列事件和 UI 流 |
| 队列状态 | `pending/running/paused/cancelled/done/error` | 🟡 核心可用。真实任务与预览任务均支持状态流转；重启时未完成任务会恢复为 paused |
| 任务事件 | `task-updated` | 🟢 已实现 |
| 日志流 | `task-log` + `get_task_logs` | 🟡 已实现文件追加、事件推送和 UI 查看；仍需真实长任务日志流验收 |
| 暂停/恢复/重试 | `pause_task` / `resume_task` / `retry_task` | 🟡 命令与 UI 已接入；恢复/重试的长任务边界仍需 E2E 验收 |
| 持久化 | `tasks/tasks.json` | 🟢 已实现 JSON 读写和临时文件原子替换 |

## 3. ASR 与模型

### 3.1 ASR 引擎

| 引擎 | 新实现 | 当前状态 |
|------|--------|----------|
| Whisper.cpp | `WhisperCppEngine` + `transcribe_audio` | 🟢 已接入任务流水线，支持取消信号，使用 sidecar 自动解析，并读取设置中的模型目录 |
| Parakeet MLX | `ParakeetMlxEngine` + `transcribe_parakeet` | 🟡 已接入。本机流水线支持取消信号，仍依赖本机 `uv`、MLX 与 Hugging Face 缓存，目标机器需单独验收 |
| SenseVoice | catalog only | 🔴 仅模型候选，未接入运行时 |
| Custom Command | catalog only | 🔴 未设计权限方案，不可用 |

### 3.2 模型操作

| 操作 | 新 Tauri command / UI | 当前状态 |
|------|------------------------|----------|
| 列出/扫描模型 | `list_asr_models` / `scan_models` | 🟢 使用设置里的 `models_path` |
| 删除 Whisper 模型 | `delete_model` | 🟢 只删除受管 `ggml-*.bin`，拒绝路径逃逸 |
| 打开下载页 | `ModelsPage` + `openUrl` | 🟡 已提供外部下载入口和模型目录提示 |
| 下载模型 | — | 🔴 未实现 |
| 取消下载 | — | 🔴 未实现 |
| 导入本地模型 | — | 🔴 未实现 |
| checksum 校验 | — | 🔴 未实现 |

## 4. FFmpeg

| 功能 | 新 Tauri | 当前状态 |
|------|----------|----------|
| 版本检测 | `get_ffmpeg_version` | 🟢 sidecar 调用 |
| 音频提取 | `extract_audio` / task runner | 🟡 命令和任务流水线均已使用结构化参数，输出路径默认拒绝覆盖 |
| 字幕烧录 | `burn_subtitle` | 🟡 命令存在，校验视频/字幕/输出路径和 ASS 颜色；无进度解析/取消 |
| 进度解析 | `parse_duration_ms` / `parse_current_time_ms` | 🟡 工具函数有测试，但未接入 UI |
| sidecar 打包 | `bundle.externalBin` | 🟢 |
| GPL 合规 | `src-tauri/licenses/ffmpeg-GPLv2.txt` + `ffmpeg-NOTICE.md`，经 `bundle.resources` 打进 `.app` | 🟢 许可证全文、出处、SHA256 和源码书面要约随二进制分发 |
| 架构覆盖 | arm64 + x86_64 thin sidecar；`npm run build:universal` lipo 成 universal | 🟡 通用包路径存在；Intel/x86_64 实机仍需验收 |

## 5. 翻译

| 能力 | 新 Tauri | 当前状态 |
|------|----------|----------|
| Provider 列表 | `list_translation_providers` | 🟢 18 个 provider 元数据，包含 `implemented` 状态；当前全部为 `true` |
| 已接入 provider | `translate_text` dispatch | 🟡 18 个 provider 均有 dispatch 分支，包含百度、谷歌、阿里云、火山、豆包、小牛、腾讯、讯飞、DeepLX、微软、Ollama、DeepSeek、Azure OpenAI、DeerAPI、Gemini、硅基流动、通义千问、自定义 OpenAI 兼容 |
| 真实 provider 验收 | 商业 API / 本地服务 | 🟡 代码路径已接入，但百度、腾讯、阿里云、火山、讯飞等复杂签名 provider 缺真实服务端 E2E；不得仅凭单元测试宣称发布级可用 |
| DeepLX 本地服务 | `api_url` 默认 `http://localhost:1188/translate` | 🟢 不要求 API Key |
| API Key 安全存储 | Keychain commands + keyring native backend | 🟡 `set_provider_secret` / `has_provider_secret` / `delete_provider_secret` 已实现；macOS/Windows 原生 keyring 后端已启用，Linux 后端仍待产品/构建决策 |
| 字幕批量翻译 | task runner | 🟡 主任务可选翻译已接入；校对页批量 AI 优化仍需真实 provider 验收 |
| AI 优化翻译 | proofread components | 🔴 交互存在但未完成发布级真实验收 |

## 6. 设置

| 能力 | 新 Tauri | 当前状态 |
|------|----------|----------|
| 读取/保存/重置 | `get_settings` / `save_settings_cmd` / `reset_settings` | 🟢 JSON 存储，保存前校验 |
| 字段兼容 | `Settings` serde alias | 🟢 前端 snake_case 与旧 camelCase 导入均可处理 |
| 原子写入 | temp file + rename | 🟢 |
| JSON 导入导出 | `import_config_from_path` / `export_config_to_path` | 🟢 由 Rust 受控读写 |
| 加密导入导出 | — | 🔴 未实现 |
| 旧 Electron 配置迁移 | — | 🔴 未完整实现 |

## 7. 字幕校对

| 功能 | 当前状态 |
|------|----------|
| 导入视频/字幕、自动检测同目录字幕 | 🟡 已迁移基础能力，依赖 `tauri-plugin-fs` 受控授权 |
| 视频预览 + 字幕列表 + 当前字幕联动 | 🟡 组件存在，仍需 GUI 点击流验收 |
| 编辑、保存、重新打开一致性 | 🟡 基础可用，任务历史保存到 app config；复杂失败恢复待补 |
| 合并/拆分、时间偏移、搜索替换、撤销重做 | 🟡 组件能力存在，仍需端到端验收 |
| AI 优化翻译、历史任务 | 🟡 入口存在，真实 provider 和异常路径验收不足 |

## 8. 安全与权限

| 安全项 | 当前状态 |
|--------|----------|
| Tauri capability | 🟡 前端有 core/opener/dialog 和必要的 `plugin-fs` 文本读写/读目录/exists 权限；权限面需要随字幕校对继续收敛 |
| 前端文件系统权限 | 🟡 字幕校对使用 `@tauri-apps/plugin-fs`；通过 dialog 选择、运行时 `authorize_subtitle_directory` 非递归授权和敏感目录黑名单约束 |
| 配置文件导入导出 | 🟢 走 Rust command 受控读写，不需要前端任意文件写权限 |
| FFmpeg 命令注入 | 🟢 Rust 结构化参数，不接收前端可执行路径 |
| 输出覆盖保护 | 🟢 音频提取、字幕输出、视频烧录和配置导出默认拒绝覆盖已有文件 |
| 模型删除路径逃逸 | 🟢 模型 ID 限定字符集，删除目标限定 `ggml-*.bin` |
| API Key 防泄漏 | 🟡 Keychain 存储与原生后端已接入；仍需真实 provider 错误日志脱敏验收 |
| 正式签名/Gatekeeper | 🔴 仍未做 Developer ID、hardened runtime、notarization、stapling |

## 9. 不可宣称完成的缺口

- 模型内置下载、导入、checksum、下载取消未实现。
- SenseVoice、自定义 ASR 命令未接入运行时。
- 多个商业 provider 已有 dispatch，但仍缺真实密钥/E2E 验收；Linux keyring 后端是否启用仍待决策。
- 任务队列持久化、暂停/恢复/重试、日志流已有实现，仍缺真实长任务 GUI E2E 和异常恢复验收。
- 字幕校对仍有原生 `alert()`、复杂编辑和失败恢复体验债，需要 GUI E2E 覆盖。
- 视频合字幕缺进度、取消、预览和媒体信息读取。
- 正式签名、notarization、stapling、Intel/x86_64 实机验收未完成。
