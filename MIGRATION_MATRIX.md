# FinalSub 功能与发布矩阵

更新时间：2026-07-15
对照基线：SmartSub `764f421`（2026-07-14）与当前 FinalSub 源码、单元测试、生产构建和浏览器 QA。

状态定义：

- 🟢 已实现，并有本地自动化或真实夹具验证。
- 🟡 已实现，但还依赖外部账号、签名凭据或目标机器做最终验收。
- 🔴 当前仓库尚未交付。

## 当前裁决

FinalSub 已不是早期“本机/内测可用”状态。字幕生成、批处理、翻译、校对、合成、模型管理和配置安全主链路已经完整，SmartSub 的主要 ASR 协议与本地引擎类别也已覆盖；在原生离线引擎、云 ASR 协议广度、密钥 endpoint 隔离和签名更新架构上已经形成明确优势。当前发布能力仍以 macOS 12+ Universal 为主；不能笼统宣称所有平台和真实云账号场景均已胜过 SmartSub，也不能宣称 Windows/Linux 安装包、Apple 正式公证或生产 updater 根密钥已经完成。

架构保持为 React/TypeScript 交互层 + Tauri/Rust 核心层。这是当前产品的目标架构，不计划为了“全 Rust”重写成熟前端。

## 1. 产品主链路

| 能力 | 状态 | 当前实现与证据 |
|---|---:|---|
| 单文件字幕任务 | 🟢 | FFmpeg 提取 → ASR → 可选翻译 → 多格式写出；任务进度、日志、暂停、恢复、取消、重试均接入 |
| 批量任务 | 🟢 | 多文件、文件夹递归扫描、拖放、绝对路径粘贴；Rust 后端原子批量建任务，失败不留下半批状态 |
| 输出命名 | 🟢 | 支持 `{name}`、`{lang}`、`{index}`，原子占位避免覆盖和并发重名 |
| 任务持久化 | 🟢 | `tasks.json` 临时文件 + rename；重启后未完成任务恢复为可继续状态 |
| 字幕翻译 | 🟢 | 18 个 provider dispatch；批量大小、并发、间隔、提示词模板、自定义 headers/body、代理和模型发现 |
| 字幕校对 | 🟢 | 视频联动、导入/检测、编辑、拆分、合并、时间偏移、搜索替换、撤销重做、保存及错误恢复 |
| 视频合字幕 | 🟢 | 进度、取消、10 秒预览、字体/描边/阴影/背景、九宫格位置、CRF、编码 preset |
| 配置导入导出 | 🟢 | 普通 JSON 与 Argon2id + XChaCha20-Poly1305 加密格式；Keychain 密钥不导出 |

## 2. ASR 覆盖

### 2.1 本地引擎

| 引擎 | 状态 | 说明 |
|---|---:|---|
| Whisper.cpp | 🟢 | CPU + Metal；arm64/x86_64/universal sidecar 的最低 macOS 均为 12.0；可复现构建脚本和边界检查已存在 |
| Parakeet TDT 0.6B V2 | 🟢 | Rust 进程内 `sherpa-onnx 1.13.3`；无 Python、uv 或首次运行安装；受管模型下载与校验 |
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
| 外部运行时依赖 | 🟢 | Parakeet/SenseVoice/Paraformer/Qwen/FireRed 均不需要 Python 或 uv |

## 4. 翻译

| 能力 | 状态 | 说明 |
|---|---:|---|
| Provider 覆盖 | 🟢 | 百度、Google、阿里云、火山、豆包、小牛、腾讯、讯飞、DeepLX、微软、Ollama、DeepSeek、Azure OpenAI、DeerAPI、Gemini、SiliconFlow、Qwen、自定义 OpenAI 兼容，共 18 个内置项 |
| Provider 独立配置 | 🟢 | endpoint、模型、system/user prompt、自定义 headers/body 与密钥字段按 provider 保存 |
| 批量翻译 | 🟢 | 行数/字符边界、并发、间隔、checkpoint 恢复与 key 对齐校验 |
| 代理 | 🟢 | HTTP(S) proxy 配置与连通性探测 |
| 商业服务真实 E2E | 🟡 | 本地签名/请求边界测试已覆盖；正式上线前仍需真实账号逐项 smoke test |

## 5. UI 与可访问性

| 能力 | 状态 | 说明 |
|---|---:|---|
| 视觉系统 | 🟢 | 深浅主题液态玻璃、统一组件、F 品牌图标、清晰状态色与减少动态效果适配；主题入口集中到设置页 |
| 响应式 | 🟢 | 桌面侧栏、移动底栏；1200×800 与 390×844 无横向溢出 |
| 导航效率 | 🟢 | `⌘K` 根级命令面板、`⌘1`–`⌘7`、活动中心、首次引导和可折叠侧栏；选中态使用导轨而非通知圆点 |
| 国际化 | 🟢 | 中文、英文、日文 720 个 key 完全对齐 |
| 路由体积 | 🟢 | 页面 lazy loading；生产构建按页面拆包 |
| 浏览器 QA | 🟢 | 深浅主题、云 ASR 多实例与讯飞长表单、桌面/移动端已实测；控制台 0 error |

## 6. 安全与隐私

| 能力 | 状态 | 说明 |
|---|---:|---|
| 密钥存储 | 🟢 macOS / Windows；🟡 Linux | Apple/Windows 原生后端与 Linux Secret Service + Keyutils 持久后端已配置；密钥不进普通配置、不通过 IPC 返回前端 |
| Endpoint 绑定 | 🟢 | 导入配置或切换地址不会向新 endpoint 发送旧密钥 |
| 配置加密 | 🟢 | Argon2id + XChaCha20-Poly1305；错误口令和篡改均拒绝 |
| 文件边界 | 🟢 | 绝对路径、扩展名、存在性、敏感目录和路径逃逸校验 |
| 外部命令 | 🟢 | FFmpeg 与自定义 ASR 使用结构化 argv，不经过 shell |
| 隐私默认值 | 🟢 | 云 ASR 上传与遥测需显式开启；自动启动更新检查默认关闭，手动检查仅在用户点击时访问 GitHub |
| Linux 系统密钥库 | 🟡 | `linux-native-sync-persistent`、Secret Service 与 Keyutils 依赖已接入 CI；仍需 Linux 桌面会话中的真实存取 E2E |

## 7. 发布与平台

| 项目 | 状态 | 说明 |
|---|---:|---|
| macOS 12+ arm64/x86_64/universal | 🟢 | Whisper/FFmpeg sidecar 齐全；Universal `.app` 与 DMG 已真实构建、签名、挂载验证并安装到 `/Applications/FinalSub.app` |
| macOS Developer ID / notarization / stapling | 🟡 | GitHub Actions 流程已配置；需仓库注入证书、签名身份、Apple ID、app password 与 Team ID 后做真实发布验收 |
| Windows x86_64 安装包 | 🟡 | 固定摘要的 GPL FFmpeg 与 whisper.cpp 构建脚本、Tauri NSIS release job 已交付；仍需 GitHub Windows runner 真实产物验收与可选代码签名 |
| Linux x86_64 安装包 | 🟡 | 固定摘要的 GPL FFmpeg 与 whisper.cpp 构建脚本、AppImage/DEB release job、Secret Service 构建依赖已交付；仍需 GitHub Linux runner 真实产物验收 |
| 多平台发布编排 | 🟢 配置 | 单一 draft release 预创建，macOS/Windows/Linux 矩阵完成后生成逐资产 SHA-256 再发布，避免并发建 release 与半成品公开 |
| 签名应用内更新 | 🟡 | Rust updater 固定 HTTPS manifest、限定 FinalSub 官方 GitHub Release asset、签名校验、进度、安装前任务/控制句柄竞态复检与重启已接入；CI 从 Secret 原子生成 git-ignored release 配置并产出 macOS App、Linux AppImage/DEB 与 Windows NSIS 签名包，`latest.json` 缺任一目标即熔断发布；仍需生产根密钥 ceremony 与真实远端升级 E2E |
| 质量 CI | 🟢 | 前端 build、Rust fmt/test/clippy、macOS sidecar 重编与最低版本检查；工作流 YAML 与 Bash 脚本静态校验通过 |

## 8. 新鲜验证（2026-07-15）

- `cargo test --lib`：180 passed、0 failed、5 ignored；4 个官方大模型 E2E 因需外部归档而标记 ignored，但已在本轮单独使用官方归档跑通；另 1 个会写入用户 OS Keychain。
- `cargo clippy --all-targets --all-features -- -D warnings`：通过。
- `npm ci && npm run build`：Vite 8.1.4 production build 通过；`npm audit --audit-level=low` 为 0 vulnerability。
- 中英日 locale：720/720/720，missing 0、extra 0、duplicate 0。
- 云端 ASR 聚焦测试：22 passed，覆盖八种协议、签名固定向量、异步续查，以及服务商级闸门的共享、隔离、并发、启动间隔和取消边界。
- SenseVoice、Paraformer、Qwen3-ASR、FireRedASR2 官方模型真实短 WAV E2E：全部通过。
- Liquid Glass UI：1440×900、1200×800 与 390×844、深浅主题、控制台 0 error；云 ASR 请求控制在桌面为四列/两列、移动端为单列且无横向溢出；命令面板挂载于 `document.body`；主题只在设置页出现；任务动态与页面操作按钮无重叠。
- macOS Universal `.app`：主程序、FFmpeg、Whisper 均为 `x86_64 arm64`，主程序与 Whisper 的两套 slice 均为 `minos 12.0`，深度签名验证通过。
- Universal DMG：挂载后内部 `.app` 深度签名、架构、最低系统版本、FFmpeg `subtitles`/`libx264` 和许可证资源均通过；SHA-256 `879e5cc35e5fb0efa396d6c45b7f7dbaa2732f1321e7972be4e44d9d0aba8abd`。
- 签名 updater：使用一次性测试密钥真实生成 `.app.tar.gz` 与 `.sig`，验证 release 配置、官方签名器及产物链路后已物理清理测试密钥、生成配置和临时更新包；生产根密钥仍保持 P0 门控。
- `/Applications/FinalSub.app`：版本 1.0.10，bundle id `com.gravitypoet.finalsub`，真实启动路径与安装路径一致；无 updater 公钥的本地构建从该路径持续运行且不再触发插件初始化 panic。

## 9. 仍不能宣称完成的事项

1. Windows/Linux 构建脚本与 release job 已交付，但当前 macOS 主机不能替代 GitHub Windows/Linux runner 的真实安装包与启动验收。
2. Apple Developer ID 正式签名、公证、stapling 尚未用仓库 secrets 跑通；本机交付是可验证的 ad-hoc 签名。
3. 付费云 ASR/翻译 provider 尚缺真实账号 smoke test；协议边界测试不能替代服务端验收。
4. Linux Secret Service 后端已配置，但尚缺真实 Linux 桌面会话的密钥存取 E2E。
5. Windows 安装包代码签名证书尚未配置；不影响生成 NSIS，但会影响公开下载时的 SmartScreen 体验。
6. 签名应用内更新代码和发布门禁已交付，但生产 updater 根密钥尚未获批生成/托管，也尚未用两个正式版本完成远端覆盖升级与回滚演练。
