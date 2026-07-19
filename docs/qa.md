# QA 验收指南

## 构建验证命令

```bash
# 前端构建
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build

# Rust 测试
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo fmt --check && cargo test --lib

# Clippy lint
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo clippy --all-targets --all-features -- -D warnings

# macOS Universal 完整打包（含签名验证）
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build:universal
```

`build:universal` 会在构建前清掉旧残留，并在成功、失败或可捕获中断后删除 `target` 内的构建 `.app`，只保留 DMG；`target/.metadata_never_index` 是构建期间的第二道防线。需要检查 App 内容时应挂载 DMG，不得把第二个 `FinalSub.app` 长期留在仓库。

## UI 截图验收

### 启动开发服务器

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run dev -- --host 127.0.0.1 --port 5173
```

说明：Vite 浏览器预览启用开发 mock，可用于 UI 排布、路由、表单状态和响应式 QA；它不能替代 Tauri native API 验收，文件权限、系统 dialog、事件、sidecar、签名和系统打开文件必须用 Tauri smoke 或打包产物验证。

### 截取 7 个主导航页面

| 路由 | 截图命令（Playwright） |
|------|----------------------|
| `/` | `page.goto('http://127.0.0.1:5173/')` |
| `/tasks` | `page.goto('http://127.0.0.1:5173/tasks')` |
| `/models` | `page.goto('http://127.0.0.1:5173/models')` |
| `/translation` | `page.goto('http://127.0.0.1:5173/translation')` |
| `/proofread` | `page.goto('http://127.0.0.1:5173/proofread')` |
| `/subtitle-merge` | `page.goto('http://127.0.0.1:5173/subtitle-merge')` |
| `/settings` | `page.goto('http://127.0.0.1:5173/settings')` |

### 响应式验收

| 视口 | 宽度 | 检查项 |
|------|------|--------|
| 桌面 | 1280px | 无文本重叠，侧边栏 224px |
| 移动 | 390px | 无横向滚动，内容不被侧栏挤压 |

### 验收标准

- [ ] 中文界面，无英文骨架文案
- [ ] 7 个主导航入口全部可见
- [ ] 1280px 无横向溢出
- [ ] 390px 无横向滚动
- [ ] 当前路由高亮正确
- [ ] 深色/浅色模式切换正常
- [ ] 快速命令从窗口根节点打开，桌面/移动端命令标题不换行且无侧栏裁切
- [ ] 主题切换仅出现在设置页，不在侧栏重复出现
- [ ] 活动中心位于侧栏品牌区，不与页面右上角操作按钮重叠

### Tauri Smoke 验收

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run tauri dev
```

| 场景 | 操作 | 成功判据 |
|------|------|----------|
| 首页启动 | 打开 Tauri dev app | 无白屏；系统信息显示 FFmpeg 可用；控制台无 Tauri invoke/listen/dialog 错误 |
| 仅翻译 | 选择“仅翻译” | 文件按钮显示“选择字幕文件”；不显示 ASR 引擎 / ASR 模型；说明文案列出 SRT / VTT / ASS / LRC |
| 仅翻译输入格式 | 选择 `.srt`、`.vtt`、`.ass`、`.lrc` | 任务创建前端可通过；后端不拒绝支持格式 |
| 模型页 | 打开模型管理 | 每个模型展示来源；内置下载模型展示“仅大小校验；SHA256 未发布”；运行时/导入模型展示对应说明 |
| 任务队列 | 打开任务队列 | 已有任务可加载；日志弹层可打开；打开文件/文件夹失败时不崩溃 |
| 视频合字幕 | 选择视频、字幕、输出路径 | 系统 dialog 正常；视频 metadata 能加载；预览/烧录错误可见 |
| 设置 | 导入/导出配置、选择模型目录 | 系统 dialog 正常；保存后重启仍可读取配置 |

## 翻译 Provider 真实 API 验收矩阵

测试入口：Tauri app → 翻译管理 → 选择 provider → 填写 endpoint / model / secret → 点击“测试翻译”。测试文本固定为 `Hello, how are you?`，源语言 `en`，目标语言 `zh`。真实 API 结果需要记录 provider、模型、区域、请求时间、HTTP 状态、返回文本摘要和失败错误；禁止用浏览器 mock 或单元测试结果替代真实 API 证据。

| Provider ID | 类型 | 必填密钥字段 | Endpoint / Model 要求 | 通过判据 |
|-------------|------|--------------|------------------------|----------|
| `baidu` | API | `appId`, `secretKey` | 不需要 endpoint/model | 返回中文译文；签名/鉴权错误可明确显示 |
| `google` | API | `apiKey` | 不需要 endpoint/model | 返回中文译文；quota/key 错误可明确显示 |
| `aliyun` | API | `accessKeyId`, `accessKeySecret` | 不需要 endpoint/model | 返回中文译文；鉴权/地域错误可明确显示 |
| `volc` | API | `accessKeyId`, `accessKeySecret` | 不需要 endpoint/model | 返回中文译文；鉴权错误可明确显示 |
| `doubao` | API | `apiKey` | endpoint 默认 `https://ark.cn-beijing.volces.com/api/v3`；必须填写可用模型 | 返回中文译文；模型不存在错误可明确显示 |
| `niutrans` | API | `apiKey` | 不需要 endpoint/model | 返回中文译文；key 错误可明确显示 |
| `tencent` | API | `secretId`, `secretKey`, `region` | 不需要 endpoint/model | 返回中文译文；region/签名错误可明确显示 |
| `xunfei` | API | `appId`, `apiKey`, `apiSecret` | 不需要 endpoint/model | 返回中文译文；鉴权错误可明确显示 |
| `deeplx` | API | 无 | 必须填写可访问 endpoint | 返回中文译文；endpoint 不通错误可明确显示 |
| `azure` | API | `apiKey`, `region` | endpoint 默认 `https://api.cognitive.microsofttranslator.com` | 返回中文译文；region/key 错误可明确显示 |
| `ollama` | AI | 无 | endpoint 默认 `http://localhost:11434/api/generate`；必须填写本机已拉取模型 | 返回中文译文；本机服务未启动/模型不存在错误可明确显示 |
| `deepseek` | AI | `apiKey` | endpoint 默认 `https://api.deepseek.com/v1`；必须填写模型 | 返回中文译文；模型/key 错误可明确显示 |
| `azureopenai` | AI | `apiKey`, `apiVersion` | 必须填写 Azure OpenAI endpoint 和 deployment/model | 返回中文译文；deployment/apiVersion 错误可明确显示 |
| `deerapi` | AI | `apiKey` | endpoint 默认 `https://api.deerapi.com/v1`；必须填写模型 | 返回中文译文；模型/key 错误可明确显示 |
| `gemini` | AI | `apiKey` | endpoint 默认 `https://generativelanguage.googleapis.com`；必须填写模型 | 返回中文译文；模型/key 错误可明确显示 |
| `siliconflow` | AI | `apiKey` | endpoint 默认 `https://api.siliconflow.cn/v1`；必须填写模型 | 返回中文译文；模型/key 错误可明确显示 |
| `qwen` | AI | `apiKey` | endpoint 默认 `https://dashscope.aliyuncs.com/compatible-mode/v1`；必须填写模型 | 返回中文译文；模型/key 错误可明确显示 |
| `custom-openai` | AI | `apiKey` | 必须填写 OpenAI-compatible endpoint 和模型 | 返回中文译文；endpoint/model/key 错误可明确显示 |

## 翻译对齐与术语表验收

| 场景 | 操作 | 通过判据 |
|------|------|----------|
| 动态 Schema | 用 2–3 条字幕启动 AI 翻译，并在测试 endpoint 记录请求体 | `required` 仅含当前批次 ID，顶层与 `{src,tr}` 均禁止额外字段 |
| 结构化输出降级 | endpoint 依次拒绝 `json_schema`、`json_object` | 请求自动降级到普通文本模式，任务仍能解析提示词约定的 JSON |
| 回声错位 | 返回正确 ID，但把第二条 `src` 放到第一条 | 相似度校验标记错位；大面积异常整批重试一次，小面积异常只补翻问题条目 |
| 局部失败 | 定点补翻连续返回无效结构 | 该行写入显式失败标记，校对页把它计入失败项而不是完成项 |
| 术语优先级 | 启用两个包含同一原文的术语表并调整顺序 | 冲突数可见；仅采用优先级更高的译法，任务日志不记录术语正文 |
| 命中最小化 | 术语表包含命中与未命中条目 | 发给 AI 的术语数据只含当前批次命中的条目，最多 100 条 |
| 导入导出 | 导入带引号、逗号和备注的 CSV，再导出 | 条目完整、重复原文更新而不重复追加；TXT 的 Tab、`=>`、`->`、`→`、`=` 分隔可识别 |

## 签名验证

```bash
# 已安装唯一 App 的签名有效性
codesign --verify --deep --strict --verbose=4 \
  "/Applications/FinalSub.app"

# Gatekeeper（预期 rejected，未做 notarization）
spctl -a -vvv -t exec \
  "/Applications/FinalSub.app"
```

正式外发证据不得只包含 ad-hoc 签名。至少补齐：

- [ ] GUI E2E 或 Tauri smoke 截图/日志
- [ ] Apple Silicon 实机验证
- [ ] Intel / x86_64 实机或 Universal 构建验证
- [ ] Developer ID 签名验证
- [ ] notarization 提交与 `spctl` 通过证据
- [ ] 安装包 `.sha256`

## 产物检查

```bash
# DMG 大小
du -sh "src-tauri/target/universal-apple-darwin/release/bundle/dmg/FinalSub_<version>_universal.dmg"

# 构建 App 已真实清理
test -z "$(find src-tauri/target -type d -path '*/bundle/macos/FinalSub.app' -prune -print)"
```

发布打包、覆盖安装、平台产物深度验收和历史踩坑记录见 [Release SOP](release-sop.md)。
