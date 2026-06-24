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
  💡 <strong>FinalSub</strong> 是一款基于 Tauri 2.0 + Rust + React 开发的全新一代桌面端字幕制作神兵利器。它打破了传统字幕工具的“环境配置地狱”，将<strong>本地 GPU 加速语音识别（ASR）</strong>、<strong>18 大 AI 智能翻译引擎</strong>、<strong>可视化字幕校对</strong>与 <strong>FFmpeg 无损视频字幕烧录</strong>融为一体。零门槛，解压即用！
</p>

---

## 💡 为什么选择 FinalSub？

市面上的字幕软件层出不穷，但为什么你应该拥有一台 **FinalSub**？

| 痛点维度 | 传统在线字幕服务 (Web AI / 平台) | 传统开源字幕工具 (基于 Python / 命令行) | 🌟 FinalSub (本工具) |
| :--- | :--- | :--- | :--- |
| **隐私安全** | ❌ 视频/音频上传云端，商业机密与个人隐私有泄漏风险 | 🟢 本地运行，安全 | **🟢 100% 本地转写，隐私零泄漏，断网也能用** |
| **环境门槛** | 🟢 无需配置环境 | ❌ 需要安装 Python, Conda, Homebrew，动辄环境变量报错崩溃 | **🟢 零依赖，内置已签名的 FFmpeg 与 Whisper 引擎，解压即用** |
| **使用成本** | ❌ 按分钟或按月收费，额度受限，长期使用费用高昂 | 🟢 开源免费，但学习门槛极高 | **🟢 永久免费开源，支持免 API Key 的本地 Ollama 翻译，零成本产出** |
| **运行性能** | 🟢 占用云端算力，本地省电 | 🟡 纯 CPU 跑效率极低，GPU 驱动配置繁琐 | **🟢 深度支持 macOS Metal & Accelerate 加速，M芯片设备近乎瞬间转录，发热极低** |
| **全链路闭环** | 🟡 仅转写，导出后需要去其他软件剪辑/烧录 | ❌ 链路断散，需要多个脚本配合运行 | **🟢 音频提取 ➔ 本地转写 ➔ AI 翻译 ➔ 可视化校对 ➔ 一键烧录，一条龙搞定** |

---

## ✨ 核心特性矩阵

### 🎙️ 100% 本地离线 ASR（语音转文字）
* **Metal 硬件加速**：基于 `whisper.cpp` 强力驱动，原生适配 Apple Silicon (M1/M2/M3/M4) 的 **Metal** 与 **Accelerate** 硬件加速。7秒钟音频仅需不到1秒即可完成高精度转录！
* **智能模型扫描**：支持放置多个尺寸 of `ggml-*.bin` 模型（提供极简的模型管理界面和快捷路径引导），自动识别，随时切换。
* **多引擎支持**：支持 Whisper.cpp 原生推理，并提供 Parakeet MLX 扩展支持，满足各种场景的转录精度需求。

### 🤖 18 大 AI 翻译引擎，畅享智能双语
一键连接你最喜爱的 AI，将转录字幕翻译为优雅、信雅达的双语/多语字幕。
* **主流商业大模型**：已完美接驳 **DeepSeek (V3/R1)**、**豆包 (火山引擎)**、**Gemini**、**通义千问 (Qwen)**、**硅基流动 (SiliconFlow)**、**Azure OpenAI** 与自定义 OpenAI 兼容接口。
* **零成本本地大模型**：深度集成 **Ollama**！如果你本地运行了 Ollama，无需任何 API Key，直接调用本地大模型进行高质量免费翻译。
* **专业翻译通道**：集成 **DeepLX (内置零配置免 Key 通道)**、微软翻译、谷歌翻译、百度、腾讯、火山、小牛、讯飞等多家翻译服务。
* **安全密钥存储**：采用 **macOS Keychain / Windows Credential Manager 系统级凭据管理器** 原生加密通道存储所有 API Key，绝不将密钥以明文形式保存在前端或普通配置文件中，安全无懈可击。

### ✏️ 可视化智能字幕校对器
* 摆退难用的文本编辑器！内置专为字幕工作流设计的精细校对界面。
* **音视频联动**：导入视频与字幕后，播放进度与字幕行实时同步高亮。
* **极速编辑**：支持字幕行快捷拆分、合并、批量搜索替换。
* **时间偏移**：支持整轨或选定区域时间轴精准微调，完美解决音画不同步。

### 🎬 FFmpeg 一键字幕无损烧录
* 内置 Universal 架构静态高版本 `ffmpeg` 侧载程序，无须在系统安装任何音视频依赖。
* 支持一键将生成的 `SRT`/`VTT` 烧录（Hardsub）至原视频中。
* 内置多种字幕样式与颜色预设，渲染精美，支持无损快速导出。

### 📁 丰富的格式支持
* 导入导出完全自由，支持 **SRT**、**VTT**、**ASS**、**LRC (歌词)** 以及 **TXT (会议纪要文本)** 等主流格式。

---

## 🚀 3 步开启高效字幕制作

### 1. 下载与运行
前往 [Releases 页面](https://github.com/GravityPoet/FinalSub/releases) 下载适合您操作系统的最新安装包（例如 Mac `.dmg` 或 Windows 对应格式），解压并运行。

### 2. 准备 Whisper 模型
1. 进入软件的 **“模型管理”** 页面。
2. 根据页面中的外部下载引导链接，下载您需要的 Whisper 模型（如 `ggml-base.bin` 或 `ggml-medium.bin`）。
3. 点击“打开模型目录”，将下载好的 `.bin` 文件拖入该目录，点击刷新，软件将自动识别并加载。

### 3. 创建字幕任务
1. 返回 **“任务”** 页面，拖入您需要制作字幕的视频或音频文件。
2. 选择识别语言（如 Auto 自动识别或指定语言）。
3. (可选) 开启“翻译”选项，配置并测试您的 AI 翻译引擎。
4. 点击 **“开始任务”**。在 **“任务队列”** 中即可实时查看转写与翻译进度，生成后即可在 **“字幕校对”** 中微调并一键烧录！

---

## 🛠️ 现代化技术栈

FinalSub 使用了当前最前沿的桌面开发技术栈，保证了极致的性能与小巧的体积：
* **核心框架**：[Tauri 2.0](https://tauri.app/) (基于 Rust 的新一代跨平台框架，拒绝 Electron 的臃肿)
* **前端逻辑**：[React 19](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/)
* **样式设计**：[TailwindCSS 4.0](https://tailwindcss.com/)
* **ASR 引擎**：[Whisper.cpp](https://github.com/ggerganov/whisper.cpp) (GGML C/C++ 移植版)
* **媒体引擎**：[FFmpeg 7.x](https://ffmpeg.org/) (已完成签名的静态多架构 Thin Sidecar)
* **系统安全**：Rust [keyring](https://github.com/hwchen/keyring-rs) 库直连 OS Keychain / 凭据管理器

---

## 🔒 隐私声明

**我们极其看重您的隐私。**
* **FinalSub 是一款 100% 运行在您本地的客户端软件。** 
* 您的音视频文件、本地生成的字幕、转录产生的缓存数据，均完全保存在您本地设备上，**绝不会上传到任何第三方云端服务器**。
* 只有当您主动配置并启用了第三方云端翻译 API（如 DeepSeek, Gemini 等）时，软件才会将需要翻译的字幕文本加密发送至对应的官方 API 端点，除此之外没有任何后台联网上传行为。

---

## 🤝 支持与交流

**为什么我们需要您的支持？**

**FinalSub** 诞生自对“隐私安全”与“效率自由”的纯粹追求。作为一款 **100% 离线、隐私零泄漏且完全免费开源** 的工具，它的持续维护离不开社区的温度：
*   **帮您省下高昂的 SaaS 账单**：相比于市面上动辄按分钟计费、强制包月的在线字幕平台，FinalSub 帮您把所有算力留在了本地，重度视频创作者和出海团队每年可借此省下成百上千元的云端订阅费。
*   **独立开发者的真金白银成本**：为了保证“解压即用”的完美体验，我们内置了预编译的 FFmpeg 与 Whisper 侧载包，并需要为 macOS 签名、Apple 开发者证书公证服务以及多架构实机测试支付持续的硬成本。
*   **支持未来的进化**：您的每一笔赞助，都将直接用于优化本地推理算法、支持更多无损翻译接口，并让我们有底气继续保持纯净无广告的开源体验。

如果您觉得 FinalSub 帮您节省了时间、守护了隐私或创造了价值，不妨：
*   🌟 给我们一个 **Star**（这是对我们最大的精神鼓励！）。
*   ☕ **请作者喝一杯咖啡**，帮助我们分担开发者证书与测试设备的硬性成本（请备注您的 GitHub 账号）。
*   👥 加入**微信交流群**，共同探讨本地 ASR 与大模型翻译技术。

| 微信赞赏码 | 微信交流群 | PayPal 收款码 |
| :---: | :---: | :---: |
| <img src="./docs/sponsors/wechat_pay.jpg" width="220" alt="微信赞赏码" /> | <img src="./docs/sponsors/wechat_group.jpg" width="220" alt="微信交流群" /> | <img src="./docs/sponsors/paypal.jpg" width="220" alt="PayPal 收款码" /> |

---

## 🤝 致敬与开源授权

* **FinalSub** 项目在研发与设计过程中，其早期的基础架构及部分功能设计灵感来自优秀的开源项目 [SmartSub (妙幕)](https://github.com/buxuku/SmartSub)（基于 MIT 许可证开源，Copyright (c) 2024 Lin Xiaodong）。我们对此表示诚挚的谢意！
* 关于第三方开源依赖及上游基座的完整许可协议与版权声明，请参阅 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
* 本项目采用 **MIT 许可证** 开源。

---

> 💡 **想要了解底层技术架构或本地构建/打包/测试指南？**  
> 请阅读我们的 📖 [开发者指南 (docs/DEVELOPMENT.md)](./docs/DEVELOPMENT.md)。
