<p align="center">
  <img src="./src-tauri/icons/app-icon-source.png" alt="FinalSub Logo" width="120" height="120">
</p>

<h1 align="center">FinalSub (简体中文)</h1>

<p align="center">
  <strong>极致极速 · 100% 离线隐私安全的 AI 双语字幕一站式制作终端</strong>
</p>

<p align="center">
  <a href="https://github.com/GravityPoet/FinalSub/releases"><img src="https://img.shields.io/github/v/release/GravityPoet/FinalSub?color=7C3AED&style=flat-square" alt="Version"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2.0-blue?style=flat-square&color=FFC107" alt="Tauri"></a>
  <a href="https://react.dev/"><img src="https://img.shields.io/badge/React-19-blue?style=flat-square&color=0088CC" alt="React 19"></a>
  <a href="https://rust-lang.org"><img src="https://img.shields.io/badge/Rust-Inside-orange?style=flat-square&color=DE3423" alt="Rust"></a>
  <a href="https://github.com/GravityPoet/FinalSub/blob/main/LICENSE"><img src="https://img.shields.io/github/license/GravityPoet/FinalSub?style=flat-square&color=10B981" alt="License"></a>
</p>

<p align="center">
  🌐 <a href="./README.md">English Version</a>
</p>

<p align="center">
  💡 <strong>FinalSub</strong> 是一款基于 Tauri 2.0 + Rust + React 的桌面字幕工作站，将<strong>本地优先、可选云端的语音识别（ASR）</strong>、<strong>18 个翻译引擎</strong>、<strong>可视化字幕校对</strong>与 <strong>FFmpeg 高质量字幕烧录</strong>融为一体。
</p>

---

## 💡 为什么选择 FinalSub？

市面上的字幕软件层出不穷，但为什么你应该拥有一台 **FinalSub**？

| 痛点维度 | 传统在线字幕服务 (Web AI / 平台) | 传统开源字幕工具 (基于 Python / 命令行) | 🌟 FinalSub (本工具) |
| :--- | :--- | :--- | :--- |
| **隐私安全** | ❌ 默认上传完整媒体 | 🟢 本地运行 | **🟢 默认本地；云 ASR 仅在显式授权后上传本机 VAD 切出的语音片段** |
| **环境门槛** | 🟢 无需配置环境 | ❌ 常需 Python、Conda、Homebrew 与环境变量 | **🟢 本地引擎不依赖 Python 或 uv；FFmpeg 与 Whisper sidecar 随应用提供** |
| **使用成本** | ❌ 按分钟或按月收费，额度受限，长期使用费用高昂 | 🟢 开源免费，但学习门槛极高 | **🟢 永久免费开源，支持免 API Key 的本地 Ollama 翻译，零成本产出** |
| **运行性能** | 🟢 占用云端算力，本地省电 | 🟡 纯 CPU 跑效率较低，GPU 配置繁琐 | **🟢 Whisper.cpp 支持 macOS Metal；原生 sherpa-onnx 引擎可完全离线运行** |
| **全链路闭环** | 🟡 仅转写，导出后需要去其他软件剪辑/烧录 | ❌ 链路断散，需要多个脚本配合运行 | **🟢 音频提取 ➔ 本地转写 ➔ AI 翻译 ➔ 可视化校对 ➔ 一键烧录，一条龙搞定** |

---

## ✨ 核心特性矩阵

### 🎙️ 本地优先、可选云端的 ASR（语音转文字）
* **本地引擎**：支持 Whisper.cpp、Parakeet TDT、SenseVoice、Paraformer、Qwen3-ASR 与 FireRedASR2；原生 sherpa-onnx 模型安装后可断网运行，Parakeet 不依赖 Python 或 uv。
* **受管模型**：支持应用内下载、断点续传、速度/ETA、固定大小与 SHA-256 校验、安全解包、原子安装和本地导入。
* **云端协议**：支持 OpenAI 兼容、ElevenLabs、Deepgram、Gladia、火山引擎、腾讯云、阿里云和讯飞；可保存多套配置实例。长音频先由本机 Silero VAD 切片，只有用户明确授权后才上传；同一服务地址跨任务共享可配置的全局并发与启动间隔闸门。

### 🤖 18 大 AI 翻译引擎，畅享智能双语
一键连接你最喜爱的 AI，将转录字幕翻译为优雅、信雅达的双语/多语字幕。
* **主流商业大模型**：已完美接驳 **DeepSeek (V3/R1)**、**豆包 (火山引擎)**、**Gemini**、**通义千问 (Qwen)**、**硅基流动 (SiliconFlow)**、**Azure OpenAI** 与自定义 OpenAI 兼容接口。
* **零成本本地大模型**：深度集成 **Ollama**！如果你本地运行了 Ollama，无需任何 API Key，直接调用本地大模型进行高质量免费翻译。
* **专业翻译通道**：集成 **DeepLX (内置零配置免 Key 通道)**、微软翻译、谷歌翻译、百度、腾讯、火山、小牛、讯飞等多家翻译服务。
* **可靠的批量对齐**：动态 JSON Schema 锁定每条字幕 ID，原文回声相似度可识别串行与合并；异常时先整批重试，再带相邻上下文只补翻问题行。
* **多术语表与优先级**：支持多术语表启停、确定性冲突处理、CSV/TXT 导入与 CSV 导出；每批只把实际命中的术语加入 AI 提示词。
* **安全密钥存储**：密钥存入系统 Keychain/凭据管理器，并与 provider、endpoint 和字段绑定；更换 endpoint 不会自动复用旧密钥，密钥也不会通过 IPC 回传前端。

### 🧩 可复用任务配方与人工审核
* 可直接套用离线转录、双语精校、字幕翻译等内置配方，也可保存、重命名、删除并再次套用自己的任务配置。
* 配方由 Rust 后端持久化；如果引用的本地模型已被移除，FinalSub 会安全切换到当前已安装模型，不留下无法启动的配置。
* 开启 **“完成后等待人工审核”** 后，字幕会先写出并停在 **“待审核”**；可先打开产物检查，再单个通过或原子批量通过所选任务。

### ✏️ 可视化智能字幕校对器
* 摆退难用的文本编辑器！内置专为字幕工作流设计的精细校对界面。
* **音视频联动**：导入视频与字幕后，播放进度与字幕行实时同步高亮。
* **极速编辑**：支持字幕行快捷拆分、合并、批量搜索替换。
* **时间偏移**：支持整轨或选定区域时间轴精准微调，完美解决音画不同步。

### 🎬 FFmpeg 一站式视频合成
* 内置 Universal 架构静态高版本 `ffmpeg` 侧载程序，无须在系统安装任何音视频依赖。
* **硬字幕**：将 `SRT`/`VTT`/`ASS` 永久合入画面，支持字体、描边、阴影、背景、九宫格位置、CRF、编码 preset、真实预览、进度和取消。
* **软字幕**：以可开关轨道封装进 MKV，保留字幕语言与名称；视频和原声使用 stream copy，不损失画质。
* **配音组合**：可选择替换原声、配音出现时自动压低原声并混音，或生成原声/配音可切换的双音轨 MKV；只重编码必须处理的流。

### 📁 丰富的格式支持
* 导入导出完全自由，支持 **SRT**、**VTT**、**ASS**、**LRC (歌词)** 以及 **TXT (会议纪要文本)** 等主流格式。

### 🔐 可验证更新
* FinalSub 支持从签名发布清单检查、下载、验签、安装并重启；字幕任务、模型操作或视频合成进行中时会阻止安装。未内置生产公钥的本地构建会安全降级到 Releases 页面。

---

## 🚀 3 步开启高效字幕制作

### 1. 下载与运行
前往 [Releases 页面](https://github.com/GravityPoet/FinalSub/releases) 下载 macOS Universal 安装包。当前仓库尚未提供经过实机验收的 Windows/Linux 安装包。

### 2. 准备 Whisper 模型
1. 进入软件的 **“模型管理”** 页面。
2. 选择所需模型并点击“应用内下载”；下载可取消和断点续传，受管模型会在安装前完成完整性校验。
3. Whisper 模型也可通过“导入本地”安装；导入后软件会自动扫描并更新状态。

### 3. 创建字幕任务
1. 返回 **“任务”** 页面，拖入您需要制作字幕的视频或音频文件。
2. 选择识别语言（如 Auto 自动识别或指定语言）。
3. (可选) 开启“翻译”选项，配置并测试您的 AI 翻译引擎。
4. 可先套用/保存任务配方并开启人工审核，再点击 **“开始任务”**。在 **“任务队列”** 中查看转写与翻译进度、通过已检查的结果，在 **“字幕校对”** 中微调，再进入 **“视频合成”** 选择硬字幕、软字幕与配音音轨结构。

---

## 🛠️ 现代化技术栈

FinalSub 使用了当前最前沿的桌面开发技术栈，保证了极致的性能与小巧的体积：
* **核心框架**：[Tauri 2.0](https://tauri.app/) (基于 Rust 的新一代跨平台框架，拒绝 Electron 的臃肿)
* **前端逻辑**：[React 19](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/)
* **样式设计**：[TailwindCSS 4.0](https://tailwindcss.com/)
* **ASR 引擎**：[Whisper.cpp](https://github.com/ggerganov/whisper.cpp) + [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)
* **媒体引擎**：[FFmpeg 7.x](https://ffmpeg.org/) (已完成签名的静态多架构 Thin Sidecar)
* **系统安全**：Rust [keyring](https://github.com/hwchen/keyring-rs) 库直连 OS Keychain / 凭据管理器

---

## 🔒 隐私声明

**我们极其看重您的隐私。**
* **FinalSub 是一款 100% 运行在您本地的客户端软件。** 
* 默认使用本地 ASR 时，音视频、字幕与任务缓存都保存在本机。
* 只有当您主动配置云 ASR、勾选音频上传授权并启动对应任务时，软件才会把本机 Silero VAD 切出的语音片段发送到当前配置的 endpoint。
* 只有当您主动配置并启用云端翻译 API 时，待翻译字幕文本才会发送到对应 endpoint；AI 翻译只会附带当前批次实际命中的术语条目。
* 自动启动检查更新与匿名崩溃/错误上报均默认关闭。只有您手动检查或开启启动检查时，软件才会访问 GitHub Release 元数据；只有显式开启遥测后才会向 Sentry 发送错误诊断信息。

---

## 🤝 支持与赞助

**为什么我们需要您的支持？**

**FinalSub** 诞生自对“隐私安全”与“效率自由”的纯粹追求。作为一款 **100% 离线、隐私零泄漏且完全免费开源** 的工具，它的持续维护离不开社区的温度：
*   **帮您省下高昂的 SaaS 账单**：相比于市面上动辄按分钟计费、强制包月的在线字幕平台，FinalSub 帮您把所有算力留在了本地，重度视频创作者和出海团队每年可借此省下成百上千元的云端订阅费。
*   **持续维护与测试的时间精力成本**：为了保证“解压即用”的完美体验，我们内置了预编译的 FFmpeg 与 Whisper 侧载程序，并需要花费大量的精力进行多平台依赖的编译集成、适配操作系统的更新，以及进行多架构实机兼容测试。
*   **支持未来的进化**：您的每一笔赞助，都将直接用于优化本地推理算法、支持更多无损翻译接口，并让我们有底气继续保持纯净无广告的开源体验。

如果您觉得 FinalSub 帮您节省了时间、守护了隐私或创造了价值，不妨：
*   🌟 给我们一个 **Star**（这是对我们最大的精神鼓励！）。
*   ☕ **请作者喝一杯咖啡**，支持我们持续投入时间精力进行维护和测试（请备注您的 GitHub 账号）。

| PayPal 收款码 | 微信赞赏码 |
| :---: | :---: |
| <img src="./docs/sponsors/paypal.jpg" width="220" alt="PayPal 收款码" /> | <img src="./docs/sponsors/wechat_pay.jpg" width="220" alt="微信赞赏码" /> |

---

## 🤝 致敬与开源授权

* **FinalSub** 项目在研发与设计过程中，其早期的基础架构及部分功能设计灵感来自优秀的开源项目 SmartSub (妙幕)（基于 MIT 许可证开源，Copyright (c) 2024 Lin Xiaodong）。我们对此表示诚挚的谢意！
* 关于第三方开源依赖及上游基座的完整许可协议与版权声明，请参阅 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
* 本项目采用 **MIT 许可证** 开源。

---

> 💡 **想要了解底层技术架构或本地构建/打包/测试指南？**  
> 请阅读我们的 📖 [开发者指南 (docs/DEVELOPMENT.md)](./docs/DEVELOPMENT.md)。
