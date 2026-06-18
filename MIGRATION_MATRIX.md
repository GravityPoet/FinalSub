# FinalSub 迁移矩阵

生成时间：2026-06-19  
来源：旧 Electron 仓库 `/Users/moonlitpoet/Tools/AI-tools/FinalSub` + 新 Tauri 仓库 `/Users/moonlitpoet/Tools/AI-tools/FinalSubTauri`  
计划书：`/Users/moonlitpoet/Desktop/交接书/handoff-20260619-0238-finalsub-tauri-full-migration-plan.md`

---

## 1. 主导航迁移状态

| # | 路由 | 中文名 | 旧 Electron 文件 | 新 Tauri 文件 | 状态 |
|---|------|--------|------------------|--------------|------|
| 1 | `/` | 任务 | `pages/[locale]/home.tsx` | `pages/HomePage.tsx` | 🟡 部分（预览任务，非真实） |
| 2 | `/tasks` | 任务队列 | `components/TaskList.tsx` + `TaskControls.tsx` | `pages/TasksPage.tsx` | 🟡 部分（内存队列，无持久化） |
| 3 | `/models` | 模型管理 | `pages/[locale]/modelsControl.tsx` | `pages/ModelsPage.tsx` | 🟡 部分（只读展示，无下载/删除） |
| 4 | `/translation` | 翻译管理 | `pages/[locale]/translateControl.tsx` | `pages/PlaceholderPage.tsx` | 🔴 占位 |
| 5 | `/proofread` | 字幕校对 | `pages/[locale]/proofread.tsx` | `pages/PlaceholderPage.tsx` | 🔴 占位 |
| 6 | `/subtitle-merge` | 视频合字幕 | `pages/[locale]/subtitleMerge.tsx` | `pages/PlaceholderPage.tsx` | 🔴 占位 |
| 7 | `/settings` | 设置 | `pages/[locale]/settings.tsx` | `pages/PlaceholderPage.tsx` | 🔴 占位 |

---

## 2. 任务系统迁移矩阵

### 2.1 任务类型

| 旧值 | 中文 | 新 Tauri TaskType | 状态 |
|------|------|-------------------|------|
| `generateAndTranslate` | 生成字幕并翻译 | `GenerateAndTranslate` | 🟡 类型存在，无创建路径 |
| `generateOnly` | 仅生成字幕 | `GenerateOnly` | 🟢 `create_task` 可创建 |
| `translateOnly` | 仅翻译字幕 | `TranslateOnly` | 🟡 类型存在，无创建路径 |

### 2.2 任务状态

| 旧状态 | 新 TaskStatus | 状态 |
|--------|---------------|------|
| `loading` / `pending` | `Pending` | 🟢 |
| `running` | `Running` | 🟢（模拟） |
| `paused` | `Paused` | 🟡 枚举存在，无触发 |
| `cancelled` | `Cancelled` | 🟢 |
| `done` | `Done` | 🟢（模拟） |
| `error` | `Error` | 🟡 结构体有 error 字段 |

### 2.3 任务进度阶段

| 旧阶段 | 新阶段 | 状态 |
|--------|--------|------|
| `extracting-audio` | — | 🔴 未实现 |
| `preparing-model` | — | 🔴 未实现 |
| `transcribing` | — | 🔴 未实现 |
| `translating` | — | 🔴 未实现 |
| `writing-subtitle` | — | 🔴 未实现 |
| `done` | — | 🔴 未实现 |

### 2.4 任务事件

| 旧 IPC 事件 | 新 Tauri 事件 | 状态 |
|-------------|---------------|------|
| `taskStatusChange` | `task-updated` | 🟢 合并为单一事件 |
| `taskProgressChange` | `task-updated` | 🟢 合并 |
| `taskErrorChange` | `task-updated` | 🟢 合并 |
| `taskFileChange` | — | 🔴 未实现 |
| `taskComplete` | `task-updated` (status=done) | 🟢 合并 |
| `file-selected` | — | 🔴 未实现（Tauri 用 dialog plugin） |

---

## 3. ASR 引擎迁移矩阵

### 3.1 引擎

| 引擎 ID | 旧实现 | 新 trait impl | 状态 |
|---------|--------|--------------|------|
| `builtin-whisper` | `subtitleGenerator.ts` native addon | `AsrEngine` trait（零实现） | 🔴 |
| `local-whisper` | `subtitleGenerator.ts` CLI exec | — | 🔴 |
| `parakeet-v2` | `parakeetTranscriber.ts` Python/uv | `AsrEngine` trait（零实现） | 🔴 |
| `sensevoice` | — | `AsrEngine` trait（零实现） | 🔴 |
| `custom-command` | — | `AsrEngine` trait（零实现） | 🔴 |

### 3.2 模型

| 模型 ID | 旧仓库状态 | 新 Tauri catalog | 状态 |
|---------|-----------|------------------|------|
| `whisper-large-v3-turbo` | ✅ 可下载/使用 | 🟡 元数据存在，NotReady | 🟡 |
| `whisper-large-v3` | ✅ | 🟡 NotReady | 🟡 |
| `whisper-medium` | ✅ | 🟡 NotReady | 🟡 |
| `whisper-small` | ✅ | 🟡 NotReady | 🟡 |
| `parakeet-tdt-0.6b-v2` | ✅ 自动缓存 | 🟡 NotReady | 🟡 |
| `sensevoice-small` | 🔴 待验证 | 🟡 NotReady | 🟡 |
| `custom-command` | — | 🟡 NotReady | 🟡 |

### 3.3 模型管理操作

| 操作 | 旧 IPC | 新 Tauri command | 状态 |
|------|--------|------------------|------|
| 列出已安装 | `getSystemInfo` → `modelsInstalled` | `list_asr_models` | 🟢 只读 |
| 下载模型 | `downloadModel` | — | 🔴 |
| 取消下载 | `cancelModelDownload` | — | 🔴 |
| 删除模型 | `deleteModel` | — | 🔴 |
| 导入本地模型 | `importModel` | — | 🔴 |
| 模型路径配置 | `settings.modelsPath` | — | 🔴 |

---

## 4. 翻译系统迁移矩阵

### 4.1 Provider 清单（旧 17 个 + 自定义）

| Provider ID | 中文名 | 类型 | 新 Tauri | 状态 |
|-------------|--------|------|----------|------|
| `baidu` | 百度翻译 | API | — | 🔴 |
| `google` | 谷歌翻译 | API | — | 🔴 |
| `aliyun` | 阿里云翻译 | API | — | 🔴 |
| `volc` | 火山翻译 | API | — | 🔴 |
| `doubao` | 豆包翻译 | API | — | 🔴 |
| `niutrans` | 小牛翻译 | API | — | 🔴 |
| `tencent` | 腾讯翻译 | API | — | 🔴 |
| `xunfei` | 讯飞翻译 | API | — | 🔴 |
| `deeplx` | DeepLX | API | — | 🔴 |
| `azure` | 微软翻译 | API | — | 🔴 |
| `ollama` | Ollama | AI | — | 🔴 |
| `deepseek` | 深度求索 | AI | — | 🔴 |
| `azureopenai` | Azure OpenAI | AI | — | 🔴 |
| `DeerAPI` | DeerAPI | AI | — | 🔴 |
| `Gemini` | Gemini | AI | — | 🔴 |
| `siliconflow` | 硅基流动 | AI | — | 🔴 |
| `qwen` | 通义千问 | AI | — | 🔴 |
| custom | 自定义 | OpenAI 兼容 | — | 🔴 |

### 4.2 翻译功能

| 功能 | 旧 IPC | 新 Tauri | 状态 |
|------|--------|----------|------|
| 加载 provider 列表 | `getTranslationProviders` | — | 🔴 |
| 保存 provider 配置 | `setTranslationProviders` | — | 🔴 |
| 测试翻译 | `testTranslation` | — | 🔴 |
| 字幕翻译执行 | `handleTask` (translateOnly) | — | 🔴 |
| AI 优化翻译 | `optimizeSubtitle` | — | 🔴 |
| 批量 AI 优化 | `batchOptimizeSubtitles` | — | 🔴 |

---

## 5. 设置迁移矩阵

### 5.1 设置字段

| 字段 | 旧 store key | 类型 | 新 Tauri | 状态 |
|------|-------------|------|----------|------|
| ASR 引擎 | `settings.asrEngine` | enum | — | 🔴 |
| Whisper 命令模板 | `settings.whisperCommand` | string | — | 🔴 |
| 界面语言 | `settings.language` | string | — | 🔴 |
| 本地 Whisper | `settings.useLocalWhisper` | bool | — | 🔴 |
| CUDA 加速 | `settings.useCuda` | bool | — | 🔴 |
| 模型路径 | `settings.modelsPath` | string | — | 🔴 |
| 最大上下文 | `settings.maxContext` | number | — | 🔴 |
| 自定义临时目录 | `settings.useCustomTempDir` | bool | — | 🔴 |
| VAD 启用 | `settings.useVAD` | bool | — | 🔴 |
| VAD 阈值 | `settings.vadThreshold` | number | — | 🔴 |
| VAD 最小语音时长 | `settings.vadMinSpeechDuration` | number | — | 🔴 |
| VAD 最小静音时长 | `settings.vadMinSilenceDuration` | number | — | 🔴 |
| VAD 最大语音时长 | `settings.vadMaxSpeechDuration` | number | — | 🔴 |
| VAD 语音填充 | `settings.vadSpeechPad` | number | — | 🔴 |
| VAD 样本重叠 | `settings.vadSamplesOverlap` | number | — | 🔴 |
| 启动检查更新 | `settings.checkUpdateOnStartup` | bool | — | 🔴 |
| 并发任务数 | `maxConcurrentTasks` | number | — | 🔴 |
| 字幕输出格式 | `subtitleOutputFormat` | enum | — | 🔴 |
| 翻译 provider | `translateProvider` | string | — | 🔴 |
| 目标语言 | `targetLanguage` | string | — | 🔴 |
| 翻译重试次数 | `translateRetryTimes` | number | — | 🔴 |

### 5.2 设置操作

| 操作 | 旧 IPC | 新 Tauri | 状态 |
|------|--------|----------|------|
| 读取设置 | `getSettings` | — | 🔴 |
| 保存设置 | `setSettings` | — | 🔴 |
| 恢复默认 | `clearConfig` | — | 🔴 |
| 清除缓存 | `clearCache` | — | 🔴 |
| 导出配置（加密） | `exportConfig` | — | 🔴 |
| 导入配置（解密） | `importConfig` | — | 🔴 |
| 选择目录 | `selectDirectory` | — | 🔴 |

---

## 6. 字幕校对迁移矩阵

| 功能 | 旧 IPC / 组件 | 新 Tauri | 状态 |
|------|--------------|----------|------|
| 导入视频+字幕 | `openDialog` + `file-selected` | — | 🔴 |
| 自动检测字幕 | `detectSubtitles` | — | 🔴 |
| 目录扫描字幕 | `scanDirectorySubtitles` | — | 🔴 |
| 智能扫描 | `smartScanDirectory` | — | 🔴 |
| 语言检测 | `detectLanguage` | — | 🔴 |
| 创建校对任务 | `createProofreadTask` | — | 🔴 |
| 更新校对任务 | `updateProofreadTask` | — | 🔴 |
| 字幕编辑器 | `ProofreadEditor` 组件 | — | 🔴 |
| AI 优化翻译 | `optimizeSubtitle` | — | 🔴 |
| 批量 AI 优化 | `batchOptimizeSubtitles` | — | 🔴 |
| 搜索替换 | 前端实现 | — | 🔴 |
| 时间偏移 | 前端实现 | — | 🔴 |
| 合并字幕 | 前端实现 | — | 🔴 |
| 拆分字幕 | 前端实现 | — | 🔴 |
| 撤销/重做 | 前端实现 | — | 🔴 |
| 历史任务 | `getProofreadHistories` | — | 🔴 |

---

## 7. 视频合字幕迁移矩阵

| 功能 | 旧 IPC / 组件 | 新 Tauri | 状态 |
|------|--------------|----------|------|
| 选择视频 | `selectFile` | — | 🔴 |
| 选择字幕 | `selectFile` | — | 🔴 |
| 获取视频信息 | `subtitleMerge:getVideoInfo` | — | 🔴 |
| 获取字幕信息 | `subtitleMerge:getSubtitleInfo` | — | 🔴 |
| 样式预设（5种） | 前端 `StylePreset` | — | 🔴 |
| 字体设置 | 前端 | — | 🔴 |
| 边框设置 | 前端 | — | 🔴 |
| 位置设置（9宫格） | 前端 | — | 🔴 |
| CSS 预览 | 前端 | — | 🔴 |
| 精确预览 | 前端 | — | 🔴 |
| 开始烧录 | `subtitleMerge:startMerge` → FFmpeg | — | 🔴 |
| 进度展示 | FFmpeg stdout 解析 | — | 🔴 |
| 选择输出路径 | `subtitleMerge:selectOutputPath` | — | 🔴 |
| 打开输出目录 | `subtitleMerge:openOutputFolder` | — | 🔴 |

---

## 8. FFmpeg 迁移矩阵

| 功能 | 旧实现 | 新 Tauri | 状态 |
|------|--------|----------|------|
| 版本检测 | Node spawn | `get_ffmpeg_version` (sidecar) | 🟢 |
| 音频提取 | `subtitleGenerator.ts` | `extract_audio_plan`（只生成计划） | 🟡 |
| 字幕烧录 | `subtitleMerge:startMerge` | `audio/mod.rs` plan（未暴露命令） | 🟡 |
| 进度解析 | FFmpeg stdout | — | 🔴 |
| sidecar 打包 | extraResources | `bundle.externalBin` | 🟢 |
| sidecar 来源 | Homebrew 动态版 | Homebrew 动态版（需替换） | ⚠️ |

---

## 9. IPC 通道完整映射

### 9.1 Invoke 通道（旧 89 个）

| 旧通道 | 新 Tauri command | 状态 |
|--------|------------------|------|
| `getTasks` | `list_tasks` | 🟢 |
| `getSettings` | — | 🔴 |
| `setSettings` | — | 🔴 |
| `getTranslationProviders` | — | 🔴 |
| `testTranslation` | — | 🔴 |
| `getSystemInfo` | `get_app_info` + `list_asr_models` | 🟡 拆分 |
| `deleteModel` | — | 🔴 |
| `downloadModel` | — | 🔴 |
| `cancelModelDownload` | — | 🔴 |
| `importModel` | — | 🔴 |
| `selectDirectory` | dialog plugin | 🟢 |
| `selectFile` | dialog plugin | 🟢 |
| `selectFiles` | dialog plugin | 🟢 |
| `getDroppedFiles` | — | 🔴 |
| `readSubtitleFile` | — | 🔴 |
| `saveSubtitleFile` | — | 🔴 |
| `getSubtitleAsVtt` | — | 🔴 |
| `readRawFileContent` | fs plugin | 🟡 |
| `checkFileExists` | fs plugin | 🟡 |
| `getDirectoryFiles` | fs plugin | 🟡 |
| `clearConfig` | — | 🔴 |
| `getTempDir` | — | 🔴 |
| `clearCache` | — | 🔴 |
| `exportConfig` | — | 🔴 |
| `importConfig` | — | 🔴 |
| `check-for-updates` | — | 🔴 |
| `download-update` | — | 🔴 |
| `install-update` | — | 🔴 |
| `get-cuda-environment` | — | 🔴（macOS 无 CUDA） |
| `get-addon-summary` | — | 🔴 |
| `getTaskStatus` | `list_tasks` | 🟢 |
| `createProofreadTask` | — | 🔴 |
| `updateProofreadTask` | — | 🔴 |
| `detectSubtitles` | — | 🔴 |
| `optimizeSubtitle` | — | 🔴 |
| `batchOptimizeSubtitles` | — | 🔴 |
| `subtitleMerge:getVideoInfo` | — | 🔴 |
| `subtitleMerge:startMerge` | — | 🔴 |
| 其余 ~50 通道 | — | 🔴 |

### 9.2 Send 通道（旧 11 个）

| 旧通道 | 新 Tauri | 状态 |
|--------|----------|------|
| `setTasks` | `create_task` / `cancel_task` | 🟡 命令式替代 |
| `setTranslationProviders` | — | 🔴 |
| `handleTask` | `create_task` | 🟡 |
| `pauseTask` | — | 🔴 |
| `resumeTask` | — | 🔴 |
| `cancelTask` | `cancel_task` | 🟢 |
| `openDialog` | dialog plugin | 🟢 |
| `openUrl` | opener plugin | 🟢 |

### 9.3 On 通道（旧 13 个）

| 旧通道 | 新 Tauri event | 状态 |
|--------|----------------|------|
| `taskStatusChange` | `task-updated` | 🟢 |
| `taskProgressChange` | `task-updated` | 🟢 |
| `taskErrorChange` | `task-updated` | 🟢 |
| `taskFileChange` | — | 🔴 |
| `taskComplete` | `task-updated` | 🟢 |
| `file-selected` | dialog plugin 回调 | 🟢 |
| `message` | — | 🔴 |
| `update-status` | — | 🔴 |
| `newLog` | — | 🔴 |
| `downloadProgress` | — | 🔴 |
| `modelDownloadDetail` | — | 🔴 |
| `addon-download-progress` | — | 🔴 |
| `batchOptimizeProgress` | — | 🔴 |

---

## 10. 安全迁移清单

| 安全项 | 旧实现 | 新 Tauri | 状态 |
|--------|--------|----------|------|
| IPC 白名单 | preload.ts 89+11+13 通道 | capabilities JSON | 🟢 |
| URL scheme 白名单 | `openExternal` scheme 检查 | opener plugin | 🟢 |
| 路径遍历防护 | `validate_media_path` | `validate_media_path` | 🟢 |
| API Key 存储 | electron-store（本地） | — | 🔴 |
| 配置导入加密 | AES password | — | 🔴 |
| CSP | webSecurity true | tauri.conf.json CSP | 🟢 |
| XSS 防护 | DOMPurify sanitize | — | 🔴（暂无 HTML 渲染） |
| shell 命令注入 | execFile + validateShellVar | sidecar + args 白名单 | 🟢 |
| FFmpeg 参数注入 | 结构化生成 | 结构化 plan | 🟢 |

---

## 11. 统计摘要

| 类别 | 总数 | 已迁移 | 部分迁移 | 未迁移 |
|------|------|--------|----------|--------|
| 主导航 | 7 | 0 | 3 | 4 |
| 任务类型 | 3 | 1 | 2 | 0 |
| ASR 引擎 | 5 | 0 | 0 | 5 |
| 模型操作 | 6 | 1 | 0 | 5 |
| 翻译 Provider | 18 | 0 | 0 | 18 |
| 设置字段 | 21 | 0 | 0 | 21 |
| 设置操作 | 7 | 0 | 0 | 7 |
| 字幕校对功能 | 16 | 0 | 0 | 16 |
| 视频合字幕功能 | 14 | 0 | 0 | 14 |
| FFmpeg 功能 | 6 | 2 | 2 | 2 |
| Invoke 通道 | ~89 | ~8 | ~5 | ~76 |
| Send 通道 | 11 | ~4 | ~2 | ~5 |
| On 通道 | 13 | ~5 | 0 | ~8 |
| 安全项 | 9 | 5 | 0 | 4 |

**总体迁移进度：约 12%**
