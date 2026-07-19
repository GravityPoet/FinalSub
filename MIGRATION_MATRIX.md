# FinalSub 功能与发布矩阵

更新时间：2026-07-19
对照基线：SmartSub `2ea9327f80bd79ec1b950caeb002f17a8722a5e0`（2026-07-18）与 FinalSub `6e0c133a9d890b77be30443e858b8910c2dc33c4` 的源码、单元测试、Universal 生产构建和真实应用 UI。

状态定义：

- 🟢 已实现，并有本地自动化或真实夹具验证。
- 🟡 已实现，但还依赖外部账号、签名凭据或目标机器做最终验收。
- 🟠 部分覆盖；现有能力可用，但尚未达到本轮 SmartSub 基线的完整工作流。
- 🔴 当前仓库尚未交付。
- 🟣 FinalSub 已形成可验证的差异化优势。

## 当前裁决

FinalSub 的字幕生成、批处理、18 个翻译 provider、术语表、动态结构化输出、AI 回显对齐与定点补翻、校对、硬字幕合成、模型管理和配置安全主链路已可用于真实生产；在原生离线 ASR、云 ASR 协议广度、密钥 endpoint 隔离、加密配置和签名更新架构上具备明确优势。SmartSub 3.4.0 已把产品边界扩到“转写 → 翻译 → 校对 → TTS 配音 → 声音克隆 → 音轨/字幕封装 → 流水线批处理”。FinalSub 已对齐其术语表和 AI 对齐保护，但 TTS/声音克隆、软字幕与音轨封装、硬件编码、配方/人工闸门和日志中心仍未交付，因此目前尚不能笼统宣称功能全面对齐或超越。

架构保持为 React/TypeScript 交互层 + Tauri/Rust 核心层。这是当前产品的目标架构，不计划为了“全 Rust”重写成熟前端。

## 1. 产品主链路

| 能力 | 状态 | 当前实现与证据 |
|---|---:|---|
| 单文件字幕任务 | 🟢 | FFmpeg 提取 → ASR → 可选翻译 → 多格式写出；任务进度、日志、暂停、恢复、取消、重试均接入 |
| 批量任务 | 🟢 | 多文件、文件夹递归扫描、拖放、绝对路径粘贴；Rust 后端原子批量建任务，失败不留下半批状态 |
| 输出命名 | 🟢 | 支持 `{name}`、`{lang}`、`{index}`，原子占位避免覆盖和并发重名 |
| 任务持久化 | 🟢 | `tasks.json` 临时文件 + rename；重启后未完成任务恢复为可继续状态 |
| 字幕翻译 | 🟢 | 18 个 provider dispatch；批量大小、并发、间隔、术语表、动态 Schema、回显对齐、定点补翻、提示词模板、自定义 headers/body、代理和模型发现 |
| 字幕校对 | 🟢 | 视频联动、导入/检测、编辑、拆分、合并、时间偏移、搜索替换、撤销重做、保存及错误恢复 |
| 视频合字幕 | 🟢 | 进度、取消、10 秒预览、字体/描边/阴影/背景、九宫格位置、CRF、编码 preset |
| 软字幕 / MKV 封装 | 🔴 | 当前只交付硬字幕烧录；尚缺 stream-copy 软字幕、双音轨 MKV 与音轨元数据 |
| TTS 配音与声音克隆 | 🔴 | 尚未交付；见第 5 节 |
| 端到端流水线 | 🔴 | 尚缺可保存配方、阶段恢复、人工闸门和批量编排；见第 7 节 |
| 配置导入导出 | 🟢 | 普通 JSON 与 Argon2id + XChaCha20-Poly1305 加密格式；Keychain 密钥不导出 |

## 2. ASR 覆盖

### 2.1 本地引擎

| 引擎 | 状态 | 说明 |
|---|---:|---|
| Whisper.cpp | 🟢 | CPU + Metal；arm64/x86_64/universal sidecar 的最低 macOS 均为 12.0；可复现构建脚本和边界检查已存在 |
| Parakeet TDT 0.6B V2 | 🟣 | Rust 进程内 `sherpa-onnx 1.13.3`；无 Python、uv 或首次运行安装；独立模型根目录可直接复用现有模型，应用下载仍保留断点续传与固定摘要校验 |
| SenseVoice Small | 🟢 | Rust 原生推理、Silero VAD 长音频切分；官方 2025 int8 模型真实短 WAV E2E 已通过 |
| Paraformer | 🟢 | Rust 原生推理、受管下载；官方模型真实短 WAV E2E 已通过 |
| Qwen3-ASR | 🟢 | Rust 原生运行时、受管下载和完整 tokenizer 清单；官方 2026 int8 模型真实短 WAV E2E 已通过 |
| FireRedASR2 CTC | 🟢 | Rust 原生运行时与受管下载；官方 2026 int8 模型真实短 WAV E2E 已通过 |
| 自定义命令 | 🟢 | 参数数组直接执行，不进入 shell；支持输入、输出、模型和语言占位符 |

SmartSub 的 `faster-whisper` 没有作为独立 Python/CTranslate2 运行时复制。FinalSub 由 Whisper.cpp 提供同类通用 Whisper 能力，并避免重新引入 Python/uv 与首次安装依赖；这是明确的架构取舍，不是缺失的用户工作流。

### 2.2 云端 ASR

| 协议 | 状态 | 关键边界 |
|---|---:|---|
| OpenAI compatible | 🟢 | multipart、verbose/普通 JSON 回退、endpoint 归一化 |
| ElevenLabs | 🟢 | `scribe_v2`、`xi-api-key`、词级时间戳 |
| Deepgram | 🟢 | `nova-3`、原始 WAV、词级时间戳 |
| Gladia v2 | 🟢 | upload/init/poll；任务 ID 限制；跨暂停/重启续查，避免重复上传 |
| 火山引擎豆包极速版 | 🟢 | API Key headers、base64 JSON、毫秒时间戳和状态码处理 |
| 腾讯云极速版 | 🟢 | AppID/SecretID/SecretKey、HMAC-SHA1、原始 WAV |
| 阿里云极速版 | 🟢 | POP CreateToken + FlashRecognizer、Token 缓存与刷新 |
| 讯飞录音文件转写 | 🟢 | APPID/APIKey/APISecret、Java URLEncoder 兼容签名、异步订单续查、词级时间戳和档位/语言守卫 |

共性能力：

- 本机 Silero VAD 后再上传，单片最长 300 秒。
- 必须显式开启音频上传授权。
- 支持多套云 ASR 配置实例；旧单配置字段继续作为兼容镜像。
- 同一协议与规范化 endpoint 跨任务共享服务商级请求闸门；并发数支持 1-8，片段启动间隔全局生效，排队可取消，不同协议或 endpoint 相互隔离。
- Keychain account 绑定 `provider + endpoint + field`；协议或 endpoint 改变后不会读取旧密钥。
- redirects 禁用，provider 错误体有长度限制并脱敏。
- Gladia 与讯飞的异步订单在建单后原子落盘；完成或确定失败清理，取消、超时或瞬时网络错误保留用于续查。

付费云协议已有本地协议边界测试，但 🟡 仍需要各服务真实账号、额度与区域配置完成服务端 E2E；仓库不能替用户凭据完成这一步。

## 3. 模型与运行时管理

| 能力 | 状态 | 说明 |
|---|---:|---|
| 应用内下载 | 🟢 | 断点续传、速度、ETA、取消与重试 |
| 完整性校验 | 🟢 | 受管 sherpa 模型固定大小与 SHA-256；Whisper 上游未发布摘要的条目明确标记为仅大小校验 |
| 安全解包 | 🟢 | 拒绝路径穿越、缺失文件和空文件；staging + backup + rename 原子替换 |
| 本地导入 | 🟢 | Whisper 与受管模型导入入口 |
| 外部模型目录复用 | 🟣 | Whisper 与 Parakeet 使用独立根目录；真实 2.9 GB Parakeet 模型无需搬移或重下，应用 UI 已识别为“已下载” |
| 外部运行时依赖 | 🟢 | Parakeet/SenseVoice/Paraformer/Qwen/FireRed 均不需要 Python 或 uv |

## 4. 翻译

| 能力 | 状态 | 说明 |
|---|---:|---|
| Provider 覆盖 | 🟢 | 百度、Google、阿里云、火山、豆包、小牛、腾讯、讯飞、DeepLX、微软、Ollama、DeepSeek、Azure OpenAI、DeerAPI、Gemini、SiliconFlow、Qwen、自定义 OpenAI 兼容，共 18 个内置项 |
| Provider 独立配置 | 🟢 | endpoint、模型、system/user prompt、自定义 headers/body 与密钥字段按 provider 保存 |
| 批量翻译 | 🟢 | 行数/字符边界、并发、间隔、checkpoint 恢复、严格 key 对齐与失败可见性 |
| 术语表管理 | 🟢 | 多术语表、优先级、启停、确定性冲突处理、CSV/TXT 导入与 CSV 导出；每批只发送命中的最多 100 条术语 |
| 动态 JSON Schema | 🟢 | 按当前批次 ID 锁定 required keys，顶层和 `{src,tr}` 均禁止额外字段；OpenAI/Ollama/Gemini 原生格式，自动 `json_schema → json_object → disabled` 降级 |
| 回显锚定与错位检测 | 🟢 | 默认要求 `{src,tr}`；原文规范化后按 0.75 相似度识别漏行、串行、合并与错位，可按 provider 关闭 |
| 分级定点补翻 | 🟢 | 大面积异常整批重试一次；其余问题行带前后各 2 条上下文最多补翻 3 轮；未解决行写入显式失败标记并由校对页识别 |
| 代理 | 🟢 | HTTP(S) proxy 配置与连通性探测 |
| 商业服务真实 E2E | 🟡 | 本地签名/请求边界测试已覆盖；正式上线前仍需真实账号逐项 smoke test |

## 5. TTS 配音与声音克隆

| 能力 | 状态 | SmartSub 当前基线与 FinalSub 缺口 |
|---|---:|---|
| 行级配音工作台 | 🔴 | 字幕 + 可选视频、逐行试听、编辑、重生成、音色切换和会话恢复 |
| 本地 TTS | 🔴 | Kokoro 多语言 103 音色、VITS 中文 174 音色；离线、免费、多进程并行 |
| 云 TTS | 🔴 | Edge、OpenAI-compatible、Azure、火山豆包、ElevenLabs |
| 本地声音克隆 | 🔴 | ZipVoice 零样本克隆、参考音频转写、录音、降噪、质量检查与导入导出 |
| 云声音克隆 | 🔴 | 火山声音复刻 2.0、ElevenLabs IVC、云端音色找回 |
| 时间轴对齐 | 🔴 | 合成前语速预控、实测复检、静音借时、1.5x 上限与问题行审阅 |
| 输出模式 | 🔴 | WAV/MP3、替换原音轨、与原音混合、双音轨 MKV、同步输出对齐字幕 |
| 引擎进程隔离 | 🟠 | FinalSub 的 ASR 任务边界和 sidecar 已隔离部分故障；TTS/克隆尚无独立 worker 生命周期 |

## 6. 视频合成与媒体封装

| 能力 | 状态 | 说明 |
|---|---:|---|
| 硬字幕烧录 | 🟢 | 样式、九宫格、进度、取消、10 秒预览、CRF 与 preset 已交付 |
| 实时样式预览 | 🟢 | 可在合成页预览当前样式；最终 FFmpeg 预览为真实 10 秒媒体片段 |
| 样式预设 | 🟠 | 有完整样式参数，但尚缺命名、保存、重排和复用的个人预设管理 |
| 软字幕封装 | 🔴 | 尚缺 stream-copy 的可开关字幕轨及语言/标题 metadata |
| 音轨替换 / 混合 / 新增 | 🔴 | 尚未交付统一 compose builder 与音轨模式 |
| 硬件编码管理 | 🔴 | 尚缺 NVIDIA/AMD/Intel/macOS 能力探测、可选编码器与失败自动 CPU 回退 |

## 7. 流水线、配方与可观测性

| 能力 | 状态 | 说明 |
|---|---:|---|
| 阶段编排 | 🟠 | 单个 FinalSub 任务可完成 ASR + 翻译并持久恢复；尚不能把校对、配音和合成统一编排 |
| 任务向导 | 🟠 | 新建任务已有清晰参数与批量入口；尚缺阶段选择、依赖校验和输出总结 |
| 配方 | 🔴 | 尚缺命名、保存、更新、删除和一键复用任务配置 |
| 人工闸门 | 🔴 | 尚缺翻译/配音问题清单的“等待审阅 → 批准/修订 → 继续”状态机 |
| 阶段级断点恢复 | 🟠 | ASR/翻译任务与翻译 checkpoint 已持久化；配音/compose 阶段尚不存在 |
| 日志中心 | 🟠 | 每个任务有日志并持久写入；尚缺跨任务按日期/级别/关键词查询和过期清理 UI |

## 8. UI 与可访问性

| 能力 | 状态 | 说明 |
|---|---:|---|
| 视觉系统 | 🟢 | 深浅主题液态玻璃、统一组件、F 品牌图标、清晰状态色与减少动态效果适配；主题入口集中到设置页 |
| 响应式 | 🟢 | 桌面侧栏、移动底栏；1200×800 与 390×844 无横向溢出 |
| 导航效率 | 🟢 | `⌘K` 根级命令面板、`⌘1`–`⌘7`、活动中心、首次引导和可折叠侧栏；选中态使用导轨而非通知圆点 |
| 国际化 | 🟢 | 中文、英文、日文 786 个 key 完全对齐 |
| 路由体积 | 🟢 | 页面 lazy loading；生产构建按页面拆包 |
| 浏览器 QA | 🟢 | 本地模型/云端服务分区、深浅主题、云 ASR 多实例与讯飞长表单、桌面/移动端已实测；控制台 0 error |

## 9. 安全与隐私

| 能力 | 状态 | 说明 |
|---|---:|---|
| 密钥存储 | 🟢 macOS / Windows；🟡 Linux | Apple/Windows 原生后端与 Linux Secret Service + Keyutils 持久后端已配置；密钥不进普通配置、不通过 IPC 返回前端 |
| Endpoint 绑定 | 🟢 | 导入配置或切换地址不会向新 endpoint 发送旧密钥 |
| 配置加密 | 🟢 | Argon2id + XChaCha20-Poly1305；错误口令和篡改均拒绝 |
| 文件边界 | 🟢 | 绝对路径、扩展名、存在性、敏感目录和路径逃逸校验 |
| 外部命令 | 🟢 | FFmpeg 与自定义 ASR 使用结构化 argv，不经过 shell |
| 隐私默认值 | 🟢 | 云 ASR 上传与遥测需显式开启；自动启动更新检查默认关闭，手动检查仅在用户点击时访问 GitHub |
| Linux 系统密钥库 | 🟡 | `linux-native-sync-persistent`、Secret Service 与 Keyutils 依赖已接入 CI；仍需 Linux 桌面会话中的真实存取 E2E |

## 10. 发布与平台

| 项目 | 状态 | 说明 |
|---|---:|---|
| macOS 12+ arm64/x86_64/universal | 🟢 | Whisper/FFmpeg sidecar 齐全；Universal `.app` 与 DMG 已真实构建、签名、挂载验证并安装到 `/Applications/FinalSub.app` |
| macOS Developer ID / notarization / stapling | 🟡 | GitHub Actions 流程已配置；需仓库注入证书、签名身份、Apple ID、app password 与 Team ID 后做真实发布验收 |
| Windows x86_64 安装包 | 🟡 | 固定摘要的 GPL FFmpeg 与 whisper.cpp 构建脚本、Tauri NSIS release job 已交付；仍需 GitHub Windows runner 真实产物验收与可选代码签名 |
| Linux x86_64 安装包 | 🟡 | 固定摘要的 GPL FFmpeg 与 whisper.cpp 构建脚本、AppImage/DEB release job、Secret Service 构建依赖已交付；仍需 GitHub Linux runner 真实产物验收 |
| 多平台发布编排 | 🟢 配置 | 单一 draft release 预创建，macOS/Windows/Linux 矩阵完成后生成逐资产 SHA-256 再发布，避免并发建 release 与半成品公开 |
| 签名应用内更新 | 🟡 | Rust updater 固定 HTTPS manifest、限定 FinalSub 官方 GitHub Release asset、签名校验、进度、安装前任务/控制句柄竞态复检与重启已接入；CI 从 Secret 原子生成 git-ignored release 配置并产出 macOS App、Linux AppImage/DEB 与 Windows NSIS 签名包，`latest.json` 缺任一目标即熔断发布；仍需生产根密钥 ceremony 与真实远端升级 E2E |
| 质量 CI | 🟢 | 前端 build、Rust fmt/test/clippy、macOS sidecar 重编与最低版本检查；工作流 YAML 与 Bash 脚本静态校验通过 |

## 11. 新鲜验证（2026-07-19）

- `cargo test --lib`：195 passed、0 failed、5 ignored；新增术语优先级/命中、配置迁移、三段结构化输出降级、动态 Schema、回显相似度与嵌套额外字段拒绝测试。
- `cargo clippy --all-targets --all-features -- -D warnings`：通过。
- `npm run build`：TypeScript 与 Vite 8.1.4 production build 通过。
- 中英日 locale：786/786/786，duplicate 0；TypeScript 的 `Record<keyof typeof zh, string>` 同时约束缺失与多余键。
- 云端 ASR 聚焦测试：22 passed，覆盖八种协议、签名固定向量、异步续查，以及服务商级闸门的共享、隔离、并发、启动间隔和取消边界。
- SenseVoice、Paraformer、Qwen3-ASR、FireRedASR2 官方模型真实短 WAV E2E：全部通过。
- Liquid Glass UI：1440×900、1200×800 与 390×844、深浅主题、控制台 0 error；模型管理默认进入本地模型、已安装模型置顶，云端服务独立成页并明确“无需下载模型”；翻译页新增对齐保护与独立术语表草稿，桌面/移动端无横向溢出，并验证“保存 provider 不会误存术语草稿”；云 ASR 请求控制在桌面为四列/两列、移动端为单列。
- macOS Universal `.app`：主程序、FFmpeg、Whisper 均为 `x86_64 arm64`，主程序与 Whisper 的两套 slice 均为 `minos 12.0`，深度签名验证通过。
- Universal DMG：新鲜构建通过 bundle 签名检查；SHA-256 `32d72dc0a1afcbbce9ab7cf21f1dd2fdba1efce83862f39879cbe059fb6bb38d`。
- 签名 updater：使用一次性测试密钥真实生成 `.app.tar.gz` 与 `.sig`，验证 release 配置、官方签名器及产物链路后已物理清理测试密钥、生成配置和临时更新包；生产根密钥仍保持 P0 门控。
- `/Applications/FinalSub.app`：版本 1.0.10，bundle id `com.gravitypoet.finalsub`，`x86_64 arm64`、deep strict 签名有效，真实启动路径与安装路径一致；文件系统只保留这一份产品 App。
- 本轮安装前备份：`~/Library/Application Support/FinalSub/Backups/20260719-143937/FinalSub.app.zip`，压缩数据与回滚应用签名均已验证。
- 真实应用 UI：模型管理默认打开“本地模型”，已安装模型置顶，并把 `/Users/moonlitpoet/Tools/Local-LLM/parakeet-models/parakeet-tdt-0.6b-v2` 判定为绿色“已下载”；“云端服务”独立成页并明确标注“无需下载模型”，未触发重复下载。

## 12. 仍不能宣称完成的事项

1. Windows/Linux 构建脚本与 release job 已交付，但当前 macOS 主机不能替代 GitHub Windows/Linux runner 的真实安装包与启动验收。
2. Apple Developer ID 正式签名、公证、stapling 尚未用仓库 secrets 跑通；本机交付是可验证的 ad-hoc 签名。
3. 付费云 ASR/翻译 provider 尚缺真实账号 smoke test；协议边界测试不能替代服务端验收。
4. Linux Secret Service 后端已配置，但尚缺真实 Linux 桌面会话的密钥存取 E2E。
5. Windows 安装包代码签名证书尚未配置；不影响生成 NSIS，但会影响公开下载时的 SmartScreen 体验。
6. 签名应用内更新代码和发布门禁已交付，但生产 updater 根密钥尚未获批生成/托管，也尚未用两个正式版本完成远端覆盖升级与回滚演练。
7. TTS 配音、声音克隆、时间轴对齐、音轨混合/替换/双音轨 MKV 尚未交付。
8. 配方、人工闸门、软字幕封装、硬件编码管理和跨任务日志中心尚未达到 SmartSub `2ea9327` 基线。
