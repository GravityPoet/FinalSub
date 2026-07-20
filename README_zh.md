<p align="center">
  <a href="./README.md">
    <img src="https://img.shields.io/badge/%F0%9F%87%BA%F0%9F%87%B8%20English%20Version-English%20Version-blue?style=for-the-badge" alt="English Version">
  </a>
</p>

<p align="center">
  <img src="./src-tauri/icons/app-icon-source.png" alt="FinalSub Logo" width="120" height="120">
</p>

<h1 align="center">FinalSub (简体中文)</h1>

<p align="center">
  <strong>彻底告别高昂的云端字幕按分计费！生成完美双语字幕与 AI 语音配音——100% 离线运行，零隐私泄露，完全免费。</strong>
</p>

<p align="center">
  <a href="https://github.com/GravityPoet/FinalSub/releases"><img src="https://img.shields.io/github/v/release/GravityPoet/FinalSub?color=7C3AED&style=flat-square" alt="Version"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2.0-blue?style=flat-square&color=FFC107" alt="Tauri"></a>
  <a href="https://react.dev/"><img src="https://img.shields.io/badge/React-19-blue?style=flat-square&color=0088CC" alt="React 19"></a>
  <a href="https://rust-lang.org"><img src="https://img.shields.io/badge/Rust-Inside-orange?style=flat-square&color=DE3423" alt="Rust"></a>
  <a href="https://github.com/GravityPoet/FinalSub/blob/main/LICENSE"><img src="https://img.shields.io/github/license/GravityPoet/FinalSub?style=flat-square&color=10B981" alt="License"></a>
</p>

<p align="center">
  <a href="./README.md">🇺🇸 English</a> | 🇨🇳 <strong>简体中文</strong>
</p>

---

### 💡 痛点直击：为什么你需要 FinalSub？

如果你平时需要给视频制作字幕或配音，一定经历过以下折磨：
1. **在线 SaaS 平台套路深**：按分钟收费，免费额度瞬间用光；为了转个几 G 的视频得等半天，甚至还要承担商业机密或未公开视频被泄露的风险。
2. **传统开源脚本门槛高**：需要装 Python、配 Conda 虚拟环境、编译 C++ 库、折腾显卡驱动。普通人配环境折腾一天，最后还是报错。
3. **全链路工具支离破碎**：用工具 A 提取转写，去浏览器翻译，用脚本 B 生成配音，最后还得拖进 Premiere 压制。

**FinalSub 彻底打破了这一切。** 它是一个专门为了终极生产力而生的本地客户端。免去一切环境配置，把完整的 AI 视频工坊直接放进你的电脑中。**不要订阅，不上云端，解压即用。**

---

### ⚡ 痛点对比：使用 FinalSub 前后的巨大差距

| 😭 使用前（传统方案的惨状） | 😎 使用后（FinalSub 的爽快感） |
| :--- | :--- |
| **SaaS 账单无底洞：** 按分钟转写、翻译计费。重度创作者每个月都要支付数十至数百美金的订阅费。 | **永久免费，零成本：** 充分调用你 Mac 的本地 GPU/CPU。转写和翻译无限时视频，花费依然是 0 元。 |
| **隐私数据裸奔风险：** 商业机密、未公开短片、私人 Vlog 必须上传云端，数据安全完全无法掌握。 | **银行级隐私防护：** 100% 离线本地处理。所有数据和文件自始至终绝不离开你的电脑。 |
| **配置环境的灾难：** 没日没夜地在终端敲代码、装 Python/Homebrew/FFmpeg，只为跑通一个 Whisper 脚本。 | **开箱即用，拒绝折腾：** 内置预编译好的 Universal FFmpeg 和 Whisper 侧载程序，双击即可运行。 |
| **AI 翻译导致字幕错位：** 大语言模型经常胡乱更改行号、丢时间轴，导致双语字幕和音频完全对不上。 | **严格 JSON 架构锁：** 字幕行号物理锁定，结合术语词典自动替换，确保翻译行完美咬合时间轴。 |
| **在数个软件间疲于奔命：** 转写软件、翻译网页、配音脚本、视频剪辑器……来回复制粘贴效率低下。 | **一站式 AI 字幕工作站：** 视频拖入 ➔ 语音转写 ➔ AI 翻译 ➔ 可视化校对 ➔ 自动避让配音 ➔ 视频压制，一气呵成。 |

---

### 🔥 三大杀手级特性（你的本地 AI 字幕超能力）

#### 1. 100% 离线本地 AI 矩阵 (Mac GPU 硬件加速)
原生支持 Whisper.cpp，完美适配 **macOS Metal GPU 硬件加速**，本地转写速度犹如飞梭。深度集成本地 **Ollama**，你可以一键调用 DeepSeek-R1 或 Qwen 等本地模型进行**零成本大模型翻译**，断网也能输出最信雅达的翻译。内置 Kokoro / VITS / ZipVoice 离线配音引擎，摆脱一切云端束缚。

#### 2. 防断流、防错位的 AI 双语翻译与术语库系统
彻底解决传统 LLM 翻译时“幻觉”导致的丢行、丢时间轴问题。FinalSub 通过严格的 JSON Schema 锁定字幕行，结合确定性冲突解决的多术语词典，自动把上下文术语提示注入 prompt。一旦发现翻译偏离，软件会自动对该批次进行重试，只修复受影响的字幕块。

#### 3. 演播室级 FFmpeg 视频合成工作台
从生肉到熟肉，仅需一步。FinalSub 拥有高度整合的音视频联动校对面板，播放与字幕行实时高亮。支持为每行字幕指定不同的 AI 配音演员，生成配音时支持**自动压低原声音量（Duck 混音）**。最后通过内置 FFmpeg 预设，一键无损封软字幕（MKV）或渲染高画质硬字幕（MP4）。

---

### 🚀 60 秒极速上手

没有任何环境门槛，简单到不可思议：

1. **下载安装：** 从 [Releases 页面](https://github.com/GravityPoet/FinalSub/releases) 下载 macOS Universal 安装包，拖入应用程序。
2. **导入视频：** 将需要处理的音视频文件直接拖入软件。
3. **一键开启：** 选择识别语言，开启翻译，点击 **“开始任务”**。

剩下的就交给我们。你可以在队列里实时看到转写和翻译的极速渲染。

---

### 🎯 谁最需要这个工具？

- 🎬 **视频博主与内容创作者：** 需要批量翻译出海视频、制作高质量中英双语字幕，希望节省昂贵 SaaS 平台订阅费的创作者。
- 🧑‍💻 **开发者与效率极客：** 希望用最简单的配置实现本地 Whisper 语音转写，并能保存自定义任务配方的工程师。
- 🔐 **对隐私要求极高的专业人士：** 需要处理涉密会议记录、商业采访、法律诉讼等敏感音视频的团队或个人。
- 🎓 **教育工作者与研究员：** 需要将大量英文学术视频、网课翻译成中英双语字幕并配音的学者。

---

### 🛠️ 极其精悍的技术栈

- **桌面框架：** [Tauri 2.0](https://tauri.app/)（基于 Rust 构建，极致省内存，绝不使用臃肿的 Electron）
- **前端界面：** [React 19](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/) + [TailwindCSS 4.0](https://tailwindcss.com/)
- **核心引擎：** [Whisper.cpp](https://github.com/ggerganov/whisper.cpp)（Metal 硬件加速）+ [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) + [FFmpeg 7.x](https://ffmpeg.org/)（已签名多架构静态 sidecar）
- **密钥安全：** 借助 Rust `keyring` 直接对接 OS 级 Keychain/凭据管理器，安全存储 API Key。

---

### 🔒 隐私承诺

- FinalSub 默认在本地运行。你的音视频、字幕和缓存数据自始至终在本地，绝不上报。
- 只有当你主动勾选并使用云端 API（如 Cloud ASR/TTS/AI 翻译）时，数据才会以经过本地 VAD 切片的安全形式发送到指定的 API 地址。
- 密钥存在系统 Keychain 中，前端 IPC 绝不回传，保证密钥绝不泄漏。

---

### 🤝 支持与赞助

为了让大家享受“开箱即用”的体验，我们花费了大量心血为各平台编译、签名 Universal 架构的 FFmpeg 和 Whisper 侧载程序，并进行了无数次实机适配测试。如果你觉得 FinalSub 为你节省了时间与资金：

- 🌟 给我们在 GitHub 点个 **Star**（这对我们非常重要！）。
- ☕ **请作者喝杯咖啡**，支持我们持续投入时间精力进行维护和测试！

| PayPal 收款码 | 微信赞助码 |
| :---: | :---: |
| <img src="./docs/sponsors/paypal.jpg" width="220" alt="PayPal" /> | <img src="./docs/sponsors/wechat_pay.jpg" width="220" alt="WeChat Sponsor" /> |

---

### ⚖️ 开源协议与鸣谢

- 本项目在早期架构与设计上，从优秀开源项目 [SmartSub](https://github.com/smartsub)（基于 MIT 协议，版权所有 (c) 2024 Lin Xiaodong）中汲取了许多宝贵灵感，特此表达诚挚的谢意！
- 第三方开源依赖及版权声明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
- 本项目基于 **MIT 协议** 开放源代码。
