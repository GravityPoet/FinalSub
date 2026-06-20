# QA 验收指南

## 构建验证命令

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub

# 前端构建
npm run build

# Rust 测试
cd src-tauri && cargo test

# Clippy lint
cd src-tauri && cargo clippy -- -D warnings

# 完整打包（含签名验证）
npm run build:local
```

## UI 截图验收

### 启动开发服务器

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run dev -- --host 127.0.0.1 --port 5173
```

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

## 签名验证

```bash
# 签名有效性
codesign --verify --deep --strict --verbose=4 \
  "src-tauri/target/release/bundle/macos/FinalSub.app"

# Gatekeeper（预期 rejected，未做 notarization）
spctl -a -vvv -t exec \
  "src-tauri/target/release/bundle/macos/FinalSub.app"
```

## 产物检查

```bash
# App 大小
du -sh "src-tauri/target/release/bundle/macos/FinalSub.app"

# DMG 大小
du -sh "src-tauri/target/release/bundle/dmg/FinalSub_2.17.0_aarch64.dmg"

# FFmpeg sidecar 内嵌
ls -la "src-tauri/target/release/bundle/macos/FinalSub.app/Contents/MacOS/ffmpeg"
```
