# TTS 与配音工作台实现说明

本文对应 FinalSub 当前主线的 TTS 配音工作台实现。它记录当前已经交付的边界，不把后续声音克隆资产管理能力算作完成。

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

云端 TTS 当前支持 OpenAI-compatible、Azure Speech、ElevenLabs、火山引擎豆包语音与 Edge TTS 免费试用档：

- endpoint 只允许 HTTP(S)，禁用 redirects，拒绝 URL 内嵌账号密码；
- API Key 使用 `provider id + endpoint + field` 绑定的系统 Keychain 身份；
- 未勾选文本上传授权时，后端拒绝合成；
- 音频响应最多 64 MB，错误体最多 16 KB 并移除控制字符；
- 返回音频统一转换为 24 kHz 单声道 PCM WAV，再进入时间轴层。
- 火山引擎豆包使用固定官方 V3 单向流式 HTTP Endpoint、`X-Api-Key`、`X-Api-Resource-Id` 与请求 ID；`seed-tts-2.0`、`seed-tts-1.0`、`seed-tts-1.0-concurr` 由实例显式选择，chunked JSON 中的 base64 裸 PCM 直接拼接并写 WAV 头，协议归一化阶段不启动 FFmpeg。语速以 `speech_rate` 原生映射到 `[-50, 100]`；`S_` 开头音色自动路由到 `seed-icl-2.0`。
- 豆包单行合成采用 1,000 个 Unicode 字符的保守前置上限；超限会在联网前明确要求拆分，避免把长文本请求耗到默认超时。
- 豆包实例是单例，API Key 仍按固定 Endpoint 绑定系统 Keychain；界面明确区分豆包语音 Key 与火山方舟推理 Key，资源版本/音色错配、鉴权失败和并发限流会给出定向提示。官方接口参考：[单向流式语音合成 HTTP](https://www.volcengine.com/docs/6561/2528925)。
- Edge TTS 不需要 API Key 或模型下载，语言区域可从 voice ID 推断；它使用固定版本的 kothok-edge-tts 访问 Microsoft Edge Read Aloud 非公开 WebSocket，仅作为不承诺稳定性的试用通道。请求同样受超时、取消和 64 MB 音频上限保护，断供错误会引导切换本地模型或 OpenAI 兼容服务。
- Edge 通道明确标记为在线文本上传；公开或商业发布应优先使用本地模型或 Azure 等服务条款清晰的接口，FinalSub 不把 Edge Read Aloud 当作稳定的商用 API。

## 受管下载与生命周期

本地 TTS 的“应用内下载”只针对固定目录中的三个官方模型条目：Kokoro、VITS AIShell3 和 ZipVoice。模型管理页先扫描本机；已经登记且文件完整的外部目录显示“直接复用”，不会因为进入页面或切换任务而重新下载。云端服务页只配置 endpoint、密钥和上传授权，永远不进入本地模型下载流程。

受管下载器具备以下边界：

- 先尝试可用镜像，再回退官方 GitHub Release；主包和 ZipVoice 独立 vocoder 都使用固定文件大小与 SHA-256 校验。
- 下载写入 `.part` 文件，支持中断后续传；取消只保留可续传工件，不会把半成品登记为可用模型。
- 校验通过后在临时 staging 目录安全解包，拒绝路径穿越、链接、设备、FIFO、重复条目、异常条目数量和过大解包体积。
- 所有必需文件检查通过后才原子替换目标目录；安装失败会保留上一份完整模型。
- ZipVoice 的 `vocos_24khz.onnx` 单独下载、校验并与主包一起安装，缺少任一工件都不会显示为 ready。
- “删除模型”只删除应用受管目录；外部目录只能“取消登记”，源文件始终保留。正在下载或合成时，后端拒绝删除以避免竞态。

## 可恢复配音会话

会话文件位于应用配置目录的 `tts/dubbing-sessions/<uuid>/session.json`，每行产物位于同会话的 `cues/`。字幕最多 20 MB、2,000 行；支持 SRT、VTT、ASS/SSA 与 LRC。

每行完成后立即原子保存。重启恢复时：

- 上次停在 `synthesizing` 的行回到 `pending`；
- 只有 WAV 丢失的行会回到 `pending`，其它已完成行保留；
- 字幕内容改变或源文件消失会标记 `source_changed`；
- 用户界面只在 localStorage 保存最近会话 ID，完整数据仍由 Rust 后端持久化。

工作台可直接编辑单行字幕文本，也可为该行指定独立音色或恢复使用全局音色。任一字段变化后，后端只删除该会话固定 `cues/<index>.wav` 产物，清空最终导出引用并将该行回退到 `pending`；其他已完成行保留。会话 ID、行索引、文本长度和音色 ID 均在 Rust 信任边界重新校验，不信任前端状态。

时间轴决策规则：

1. 非重叠字幕可借用到下一句开始前的静音间隙。
2. 原字幕存在重叠时使用各自原始时间窗，并在最终混音中保留重叠。
3. 合成音频略长时使用 FFmpeg `atempo` 收进时间槽。
4. 需要超过 1.5× 才能放入的行进入 `overlong`，必须由用户显式接受。
5. 导出时每行按原始 `start_ms` 延迟，多路通过 `amix` 混合并加 limiter，支持 WAV 与 MP3。

应用更新安装会检查 TTS 控制句柄；生成、对齐或导出进行中时不会重启替换应用。

## 当前未覆盖

- ZipVoice 目前可直接使用参考 WAV 与文本合成，但尚无克隆音色实体、录音、自动转写、选段波形、质量评分、降噪、重命名/删除、导入导出与 A/B 试听管理。
- 火山豆包普通 TTS 已接入；仍未接入火山声音复刻与 ElevenLabs IVC 管理。
- 工作台已支持视频播放联动、行级跳转与字幕副本/安全写回；仍缺完整克隆音色资产工作流。
- 本地 TTS 仍在主 Rust 进程内运行，尚无独立 worker 崩溃隔离和多进程并行。
- 配音会话尚未纳入 ASR → 翻译 → 审核 → 配音 → compose 的统一阶段编排。

## 最小验收

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo fmt --check && cargo test --lib core::tts && cargo clippy --all-targets --all-features -- -D warnings
```

受管下载的官方资产布局验收（不改变默认测试的离线性质）可用真实 Release 工件显式运行：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri
FINALSUB_TTS_VITS_ARCHIVE=/path/to/vits.tar.bz2 \
  cargo test --lib core::tts::download::tests::official_vits_archive_installs_with_real_release_layout -- --ignored
FINALSUB_TTS_ZIPVOICE_ARCHIVE=/path/to/zipvoice.tar.bz2 \
FINALSUB_TTS_VOCODER=/path/to/vocos_24khz.onnx \
  cargo test --lib core::tts::download::tests::official_zipvoice_archive_installs_with_vocoder -- --ignored
```

Edge TTS 的真实在线验收（会访问 Microsoft Edge Read Aloud 试用通道，并需要本机 FFmpeg）：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub
FINALSUB_EDGE_FFMPEG="$(command -v ffmpeg)" \
  cargo test --manifest-path src-tauri/Cargo.toml --lib \
  core::tts::providers::tests::edge_provider_real_synthesis_writes_pcm_wav -- --ignored
```
真实付费云服务与本地 TTS 模型仍需在具备相应账号/模型的机器上做最终音质 E2E；单元测试与 FFmpeg 混音冒烟不能替代该验收。
