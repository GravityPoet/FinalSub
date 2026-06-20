# FinalSub 品牌资源

## App 图标

唯一母版：`/Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/icons/app-icon-source.png`

用户指定源图：`/Users/moonlitpoet/Downloads/ChatGPT Image 2026年6月19日 04_36_58.png`

当前规则：

- Tauri 新版 App 图标从 `src-tauri/icons/app-icon-source.png` 生成。
- 不要只替换单个生成文件；需要更新时先换母版，再重新生成全部平台图标。

生成命令：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npx tauri icon src-tauri/icons/app-icon-source.png
```

Tauri 平台图标产物（由母版经 `npx tauri icon` 生成于 `src-tauri/icons/`）：

- macOS：`icon.icns`
- Windows：`icon.ico`
- 通用 PNG：`icon.png`、`32x32.png`、`64x64.png`、`128x128.png`、`128x128@2x.png`
- 移动端：`android/`、`ios/`、`Square*Logo.png`、`StoreLogo.png`
