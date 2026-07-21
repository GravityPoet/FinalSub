## 🌐 [点击这里切换到：中文版 (Chinese Version)](./README_zh.md)

<p align="center">
  <img src="./src-tauri/icons/app-icon-source.png" alt="FinalSub Logo" width="120" height="120">
</p>

<h1 align="center">FinalSub</h1>

<p align="center">
  <strong>Stop paying for subtitle SaaS. The ultimate 100% offline, privacy-first AI workstation for bilingual subtitles and TTS dubbing.</strong>
</p>

<p align="center">
  <a href="https://github.com/GravityPoet/FinalSub/releases"><img src="https://img.shields.io/github/v/release/GravityPoet/FinalSub?color=7C3AED&style=flat-square" alt="Version"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Tauri-2.0-blue?style=flat-square&color=FFC107" alt="Tauri"></a>
  <a href="https://react.dev/"><img src="https://img.shields.io/badge/React-19-blue?style=flat-square&color=0088CC" alt="React 19"></a>
  <a href="https://rust-lang.org"><img src="https://img.shields.io/badge/Rust-Inside-orange?style=flat-square&color=DE3423" alt="Rust"></a>
  <a href="https://github.com/GravityPoet/FinalSub/blob/main/LICENSE"><img src="https://img.shields.io/github/license/GravityPoet/FinalSub?style=flat-square&color=10B981" alt="License"></a>
</p>

<p align="center">
  🇺🇸 <strong>English</strong> | <a href="./README_zh.md">🇨🇳 中文版</a>
</p>

---

### 💡 The Pain: Why Does FinalSub Exist?

If you've ever tried to subtitle or dub a video, you know the drill:
1. **Online SaaS tools** charge you per minute. You run out of free credits in a blink, and uploading GBs of video to external servers exposes your unreleased or private footage.
2. **Traditional open-source scripts** require you to be a command-line magician. Installing Python, managing Conda virtual environments, compiling C++ libraries, and handling FFmpeg configurations is a recipe for errors.
3. **The Workflow is Broken.** You transcribe in one app, translate in a browser, generate voice in another script, and use Premiere or hand-coded FFmpeg to merge them. 

**FinalSub changes everything.** It is a native, pre-compiled desktop workstation that puts a complete AI production studio on your local machine. No subscription. No cloud uploads. No command-line setup.

---

### ⚡ Before vs. After: The Upgrade

| 😭 Without FinalSub | 😎 With FinalSub |
| :--- | :--- |
| **SaaS Bills Drain Your Wallet:** Paying per-minute transcription & translation fees. Heavy creators waste hundreds of dollars a month. | **100% Free Forever:** Runs locally using your Mac's Metal GPU/CPU. Process unlimited videos for $0.00. |
| **Leaking Intellectual Property:** Uploading corporate videos, confidential interviews, or private vlogs to remote servers. | **Vault-Grade Privacy:** 100% offline local processing. What happens on your Mac, stays on your Mac. |
| **Terminal & Dependency Hell:** Installing Python, Homebrew, CUDA, CMake, and Hugging Face dependencies just to run a model. | **Just Unzip & Run:** Pre-packaged universal binaries and FFmpeg sidecars. No terminal setups. No CLI errors. |
| **Hallucinating Subtitles:** AI translations shift line numbers, drop timestamps, and break sync. | **JSON Schema Locked:** Translation lines are strictly mapped and aligned with glossaries automatically. |
| **Cluttered App Switching:** Bouncing between transcribers, translator tabs, TTS scripts, and video editors. | **Unified AI Workstation:** Extraction ➔ Transcription ➔ AI Translation ➔ Proofread ➔ AI Dubbing ➔ Composition in one interface. |

---

### 🔥 3 Killer Features (Your New AI Superpowers)

#### 1. 100% Offline Local AI Power (Mac GPU Optimized)
Run state-of-the-art ASR (Whisper.cpp, SenseVoice, Paraformer) and TTS (Kokoro, VITS, ZipVoice) locally. Whisper.cpp is fully optimized for **macOS Metal GPU acceleration** for blazing-fast transcription. Deep integration with local **Ollama** lets you translate subtitles for free using models like DeepSeek-R1 or Qwen—completely offline, zero API keys required.

#### 2. Bulletproof AI Translation & Glossary Alignment
Say goodbye to broken subtitle files caused by LLM hallucinations. FinalSub locks your subtitle structure using strict JSON Schemas. It automatically scans your text against custom Terminology Glossaries, injects context-aware term hints, and matches source-echo similarities. If a translation drifts, it retries and repairs only the affected cues.

#### 3. Complete Video Production Suite (Powered by FFmpeg)
Go from raw footage to a finished master in minutes. FinalSub features a timeline-linked visual subtitle proofreader. Once proofread, generate timing-aware TTS voiceovers (supporting auto-ducking to lower source volume under dubs). Finally, burn hard subtitles (fully customizable styling) or package soft subtitles into MKV files using built-in high-performance FFmpeg sidecars.

---

### 🚀 Get Started in 60 Seconds

No Python, no setup. It just works.

1. **Download:** Grab the macOS Universal DMG/App from our [Releases Page](https://github.com/GravityPoet/FinalSub/releases).
2. **Import:** Drag and drop your video or audio file.
3. **Run:** Select your model (Local Whisper or Cloud API) and click **"Start Task"**.

That's it. Watch your high-quality, translated subtitles render in real-time.

The current macOS download uses FinalSub's pinned self-signed certificate while Apple Developer ID distribution is pending. macOS therefore requires manual approval when a newly downloaded build is first opened. Follow the [self-signed macOS install guide](./docs/macos-self-signed-install.md); never install a root certificate or disable Gatekeeper.

---

### 🎯 Who Needs This?

- 🎬 **Content Creators & YouTubers:** Localize your videos into multiple languages without paying hundreds of dollars to online SaaS platforms.
- 🧑‍💻 **Developers & Tech Teams:** Protect your source files and build automated subtitle pipelines locally using reusable task recipes.
- 🔐 **Privacy-First Professionals:** Translate and transcribe confidential interviews, corporate presentations, and legal depositions in a completely air-gapped environment.
- 🎓 **Educators & Researchers:** Turn lectures and course materials into bilingual videos with zero setup hassle.

---

### 🛡️ Built on a Rock-Solid Tech Stack

- **Desktop Framework:** [Tauri 2.0](https://tauri.app/) (Rust-powered backend, blazing-fast, avoiding Electron's bloat)
- **Frontend UI:** [React 19](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/) + [TailwindCSS 4.0](https://tailwindcss.com/)
- **Engines:** [Whisper.cpp](https://github.com/ggerganov/whisper.cpp) (Metal GPU accelerated) + [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) + [FFmpeg 7.x](https://ffmpeg.org/) (Static universal binary sidecar)
- **Security:** prompt-free encrypted credential vault on macOS; native OS credential stores on Windows and Linux.

---

### 🔒 The Privacy Promise

- FinalSub is a local client. Subtitles, audio chunks, and video files stay on your machine.
- Local models process everything in memory and on disk offline.
- Cloud APIs (ASR, Translation, TTS) are **strictly opt-in**. Audio is only uploaded in VAD-segmented chunks after you save endpoint credentials and grant explicit consent.
- On macOS, secrets are stored in an app-private XChaCha20-Poly1305 encrypted vault with owner-only permissions, avoiding recurring system password prompts. Windows and Linux use their native credential stores. Plaintext secrets are never returned to the front end or written to logs.

---

### 🤝 Support & Sponsorship

Developing and maintaining high-quality universal sidecars, optimizing local engines, and providing a seamless "unzip and run" experience takes thousands of hours. If FinalSub saves you money and time:

- 🌟 Give us a **Star** on GitHub.
- ☕ **Buy us a coffee** to fuel our development!

| PayPal | WeChat Sponsor |
| :---: | :---: |
| <img src="./docs/sponsors/paypal.jpg" width="220" alt="PayPal" /> | <img src="./docs/sponsors/wechat_pay.jpg" width="220" alt="WeChat Sponsor" /> |

---

### ⚖️ Credits & Licenses

- Early architectural concepts were inspired by the open-source project [SmartSub](https://github.com/buxuku/SmartSub) (MIT licensed, Copyright (c) 2024 Lin Xiaodong). Our deepest gratitude!
- Third-party open-source dependency credits can be found in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
- FinalSub is licensed under the **MIT License**.
