# FinalSub 品牌资源

## App 图标

唯一母版：`$HOME/Tools/AI-tools/FinalSubTauri/src-tauri/icons/app-icon-source.png`

用户指定源图：`$HOME/Downloads/ChatGPT Image 2026年6月19日 04_36_58.png`

当前规则：

- Tauri 新版 App 图标从 `src-tauri/icons/app-icon-source.png` 生成。
- 旧 Electron 仓库图标从同一母版同步到 `$HOME/Tools/AI-tools/FinalSub/resources/icon-source.png`。
- 不要只替换单个生成文件；需要更新时先换母版，再重新生成全部平台图标。

生成命令：

```bash
cd $HOME/Tools/AI-tools/FinalSubTauri && npx tauri icon src-tauri/icons/app-icon-source.png
```

旧 Electron 兼容产物：

- `$HOME/Tools/AI-tools/FinalSub/resources/icon.png`
- `$HOME/Tools/AI-tools/FinalSub/resources/icon.icns`
- `$HOME/Tools/AI-tools/FinalSub/resources/icon.ico`
- `$HOME/Tools/AI-tools/FinalSub/docs/static/img/icon.png`
- `$HOME/Tools/AI-tools/FinalSub/docs/static/img/favicon.ico`
