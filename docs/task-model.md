# 任务事件/状态/错误模型

## Task 结构体

```rust
pub struct Task {
    pub id: String,           // UUID v4
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub media_path: String,   // 绝对路径
    pub media_name: String,   // 显示名
    pub engine_id: String,    // e.g. "whisper-cpp", "parakeet-mlx"
    pub model_id: String,     // e.g. "large-v3-turbo"
    pub source_language: Option<String>,
    pub target_language: Option<String>,
    pub translation_content_mode: TranslationContentMode,
    pub output_format: String,
    pub output_name: Option<String>,
    pub strip_chinese_punctuation: bool,
    pub review_required: bool, // 旧任务的单一完成前审核开关
    pub max_subtitle_chars: i32,
    pub reviewed_at: Option<String>,
    pub pipeline: Option<PipelineConfig>, // 新任务的目标、阶段与产物快照
    pub progress: f32,        // 0.0 ~ 1.0
    pub status_message: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,   // ISO 8601
    pub updated_at: String,   // ISO 8601
}
```

## TaskType 枚举

| 值 | 序列化 | 说明 |
|---|---|---|
| `GenerateAndTranslate` | `generate-and-translate` | ASR + 翻译 |
| `GenerateOnly` | `generate-only` | 仅 ASR |
| `TranslateOnly` | `translate-only` | 仅翻译 |

## TaskStatus 枚举

| 值 | 序列化 | 说明 |
|---|---|---|
| `Pending` | `pending` | 已创建，等待执行 |
| `Running` | `running` | 正在执行 |
| `Paused` | `paused` | 已暂停，可从 checkpoint 继续 |
| `Cancelled` | `cancelled` | 已取消 |
| `Review` | `review` | 当前流水线节点等待字幕校对或配音确认 |
| `Done` | `done` | 完成 |
| `Error` | `error` | 失败 |

状态转换图：

```
Pending → Running → Done      // 无人工闸门
Pending → Running → Review → Pending → Running → Done // 审核通过后自动续跑下游
Pending → Running → Review → Pending → Running → Review → Pending → Running → Done
Pending → Running → Error     // 真实任务发生错误（ASR/翻译/外部进程出错）
Pending → Running → Cancelled // 预览任务与真实任务被中途取消
Pending → Cancelled
Running → Paused → Running    // 保留 checkpoint 后继续
Running → Cancelled
```

`approve_task` 不再把所有审核任务直接标为 `Done`。若 `PipelineConfig` 仍有下游节点，它会把当前 review 节点标为 done、推进 `current_stage`，将任务置回 `Pending` 并启动 worker；只有进入最终 `done` 节点时才把任务标为 `Done`。旧任务没有 `pipeline` 时继续沿用原有单审核语义。

## 持久流水线

```rust
pub struct PipelineConfig {
    pub enable_dubbing: bool,
    pub enable_compose: bool,
    pub subtitle_review: bool,
    pub dubbing_review: bool,
    pub dubbing: Option<PipelineDubbingConfig>,
    pub compose: Option<PipelineComposeConfig>,
    pub stages: Vec<PipelineStage>,
    pub current_stage: Option<PipelineStageKind>,
    pub subtitle_output_path: Option<String>,
    pub dubbing_session_id: Option<String>,
    pub dubbed_audio_path: Option<String>,
    pub final_video_path: Option<String>,
}
```

节点按交付目标生成，序列化值依次为 `transcribe`、`translate`、`subtitle-review`、`dub`、`dubbing-review`、`compose`、`done`。每个 `PipelineStage` 持久保存 `pending | running | review | done | skipped | error`、节点进度、消息、开始/完成时间和错误。

约束与恢复语义：

- `translate-only` 只接受带时间轴的 SRT/VTT/ASS/LRC 做下游配音或成片，不安排转录节点；TXT 不进入媒体流水线。
- 成片目标要求视频输入；替换、混音或新增配音轨时必须同时开启配音目标。
- TTS 本地模型 ID、云 provider UUID、参考 WAV、语速、采样步数、compose 模式与编码模式均在 Rust IPC 边界重新校验。
- 字幕写出、配音会话、WAV 和最终视频路径随任务快照保存。审核后从现有字幕继续，不重新读取或上传原媒体，也不覆盖已校对字幕。
- 应用重启、暂停或阶段错误会把当前 `running/error` 节点恢复为可重试 `pending`；已经完成的节点及中间产物保持不变。

## 进度阶段（已实现）

| 阶段 | progress 范围 | 说明 |
|------|--------------|------|
| `queued` | 0.0 | 已加入队列 |
| `extracting-audio` | 0.0 ~ 0.15 | FFmpeg 提取音频 |
| `preparing-model` | 0.15 ~ 0.25 | 模型加载/下载 |
| `transcribing` | 0.25 ~ 0.80 | ASR 转录 |
| `translating` | 0.80 ~ 0.95 | 翻译（如有） |
| `writing-subtitle` | 0.95 ~ 1.0 | 写出字幕文件 |
| `done` | 1.0 | 完成 |

以上 `Task.progress` 是跨版本兼容的总进度。目标流水线的精确状态以 `pipeline.stages` 为准：配音和成片节点分别保留自己的进度、状态与产物路径，任务页不再把全部下游工作压缩成一个模糊的 95%–100%。

## 事件

### `task-updated`（已实现）

- 方向：Rust → 前端
- 载荷：完整 `Task` 结构体
- 触发时机：预览任务创建、状态变化、阶段变化、产物落盘、审核、完成、取消

注意：真实任务流水线已全部接入（通过 `create_task` 进入后台 `task_runner`），支持 ASR、翻译、审核、TTS 和 FFmpeg compose；`create_preview_task` 依然保留用于快速的事件与 UI 模拟。

### `task-log`（已实现）

- 方向：Rust → 前端
- 载荷：`{ task_id: String, level: String, message: String, timestamp: String }`
- 用途：高频日志流式输出；日志同时持久化，可在任务队列中查看和复制

## 错误模型

- `Task.error: Option<String>` — 最后一个错误消息
- `TaskStatus::Error` — 终态，需用户手动重试或取消
- 错误消息使用中文，面向用户
- 内部错误链用 `thiserror` 在 Rust 层处理，只暴露用户友好消息
