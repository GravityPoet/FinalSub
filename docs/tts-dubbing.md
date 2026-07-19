# TTS 与配音工作台实现说明

本文对应 FinalSub `13124399b0a28757b49704a20811022394eefce4`。它记录当前已经交付的边界，不把后续声音克隆管理能力算作完成。

## 模型与服务分类

模型管理页先按运行位置分成两类，再按任务类型分组：

- **本地模型**：文件位于本机，ASR 与 TTS 都可离线运行。
- **云端服务**：只保存 API endpoint、服务参数和显式上传授权，不下载模型。
- **本地 ASR**：Whisper、Parakeet、SenseVoice、Paraformer、Qwen3-ASR、FireRedASR2。
- **本地 TTS**：Kokoro、VITS、ZipVoice。
- **云端 ASR / TTS**：分别维护独立实例和授权，不把服务商模型伪装成本地文件。

本地 TTS 默认受管根目录为 `~/Tools/Local-LLM/tts-models`。列表命令也会有限深度扫描配置目录及 `~/Tools/Local-LLM` 的常见子目录；发现完整模型时直接登记其规范化绝对路径，不复制文件。取消外部登记只删除路径引用，绝不删除源目录。

| 模型 | 必需文件摘要 | 特性 |
|---|---|---|
| Kokoro multi-lang v1.1 | `model.int8.onnx`、`voices.bin`、词典、tokens、espeak 数据 | 中英、103 个音色、24 kHz |
| VITS AIShell3 | `model.onnx`、`tokens.txt`、`lexicon.txt` | 中文、174 个说话人、8 kHz |
| ZipVoice distill zh/en | encoder、decoder、tokens、词典、espeak 数据、`vocos_24khz.onnx` | 中英零样本克隆、24 kHz |

ZipVoice 的 `vocos_24khz.onnx` 是官方独立声码器工件，不在主模型压缩包内。界面因此分别提供“获取模型包”和“获取声码器”，并要求将两者放入同一模型目录。

## 本地与云端合成

本地 TTS 由 Rust 后端直接创建 sherpa-onnx `OfflineTts`，最多缓存两个模型实例。合成请求包含文本、音色、语速与绝对 WAV 输出路径；产物先写同目录临时文件，再原子替换目标。取消信号会传入生成回调。

ZipVoice 额外要求：

- 参考音频必须是存在的绝对 WAV 路径；
- 文件不超过 64 MB，时长不超过 30 秒；
- 参考文本必须逐字对应，不超过 4,000 字节；
- 当前工作台提供 4 步“标准”和 8 步“高质量”两个档位。

云端 TTS 当前支持 OpenAI-compatible、Azure Speech 与 ElevenLabs：

- endpoint 只允许 HTTP(S)，禁用 redirects，拒绝 URL 内嵌账号密码；
- API Key 使用 `provider id + endpoint + field` 绑定的系统 Keychain 身份；
- 未勾选文本上传授权时，后端拒绝合成；
- 音频响应最多 64 MB，错误体最多 16 KB 并移除控制字符；
- 返回音频统一转换为 24 kHz 单声道 PCM WAV，再进入时间轴层。

## 可恢复配音会话

会话文件位于应用配置目录的 `tts/dubbing-sessions/<uuid>/session.json`，每行产物位于同会话的 `cues/`。字幕最多 20 MB、2,000 行；支持 SRT、VTT、ASS/SSA 与 LRC。

每行完成后立即原子保存。重启恢复时：

- 上次停在 `synthesizing` 的行回到 `pending`；
- 只有 WAV 丢失的行会回到 `pending`，其它已完成行保留；
- 字幕内容改变或源文件消失会标记 `source_changed`；
- 用户界面只在 localStorage 保存最近会话 ID，完整数据仍由 Rust 后端持久化。

时间轴决策规则：

1. 非重叠字幕可借用到下一句开始前的静音间隙。
2. 原字幕存在重叠时使用各自原始时间窗，并在最终混音中保留重叠。
3. 合成音频略长时使用 FFmpeg `atempo` 收进时间槽。
4. 需要超过 1.5× 才能放入的行进入 `overlong`，必须由用户显式接受。
5. 导出时每行按原始 `start_ms` 延迟，多路通过 `amix` 混合并加 limiter，支持 WAV 与 MP3。

应用更新安装会检查 TTS 控制句柄；生成、对齐或导出进行中时不会重启替换应用。

## 当前未覆盖

- TTS 模型仍是“官方工件链接 + 选择已有目录”，尚未接入带断点续传、摘要校验和安全解包的应用内受管下载器。
- ZipVoice 目前可直接使用参考 WAV 与文本合成，但尚无克隆音色实体、录音、自动转写、选段波形、质量评分、降噪、重命名/删除、导入导出与 A/B 试听管理。
- 尚未接入 Edge TTS、火山豆包 TTS、火山声音复刻与 ElevenLabs IVC 管理。
- 工作台尚无视频播放联动、逐行文本编辑、逐行音色覆盖和同步字幕重写。
- 本地 TTS 仍在主 Rust 进程内运行，尚无独立 worker 崩溃隔离和多进程并行。
- 配音会话尚未纳入 ASR → 翻译 → 审核 → 配音 → compose 的统一阶段编排。

## 最小验收

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo fmt --check && cargo test --lib core::tts && cargo clippy --all-targets --all-features -- -D warnings
```

真实付费云服务与本地 TTS 模型仍需在具备相应账号/模型的机器上做最终音质 E2E；单元测试与 FFmpeg 混音冒烟不能替代该验收。
