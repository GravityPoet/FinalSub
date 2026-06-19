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
    pub language: Option<String>,
    pub progress: f32,        // 0.0 ~ 1.0
    pub status_message: String,
    pub output_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,   // ISO 8601
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
| `Paused` | `paused` | 已暂停（当前未触发） |
| `Cancelled` | `cancelled` | 已取消 |
| `Done` | `done` | 完成 |
| `Error` | `error` | 失败 |

状态转换图：

```
Pending → Running → Done      // 仅预览任务
Pending → Running → Error     // 真实任务流水线未接入
Pending → Running → Cancelled // 预览任务
Pending → Cancelled
Running → Paused → Running    // 未实现
Running → Cancelled
```

## 进度阶段（规划，未实现）

| 阶段 | progress 范围 | 说明 |
|------|--------------|------|
| `queued` | 0.0 | 已加入队列 |
| `extracting-audio` | 0.0 ~ 0.15 | FFmpeg 提取音频 |
| `preparing-model` | 0.15 ~ 0.25 | 模型加载/下载 |
| `transcribing` | 0.25 ~ 0.80 | ASR 转录 |
| `translating` | 0.80 ~ 0.95 | 翻译（如有） |
| `writing-subtitle` | 0.95 ~ 1.0 | 写出字幕文件 |
| `done` | 1.0 | 完成 |

## 事件

### `task-updated`（已实现）

- 方向：Rust → 前端
- 载荷：完整 `Task` 结构体
- 触发时机：预览任务创建、状态变化、进度更新、完成、取消

注意：审查修复后，`create_task` 不再把真实 ASR/翻译任务伪装成完成；完整任务流水线接入前只允许 `create_preview_task` 产生模拟进度。

### `task-log`（规划，未实现）

- 方向：Rust → 前端
- 载荷：`{ task_id: String, level: String, message: String, timestamp: String }`
- 用途：高频日志流式输出

## 错误模型

- `Task.error: Option<String>` — 最后一个错误消息
- `TaskStatus::Error` — 终态，需用户手动重试或取消
- 错误消息使用中文，面向用户
- 内部错误链用 `thiserror` 在 Rust 层处理，只暴露用户友好消息
