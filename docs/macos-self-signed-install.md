# FinalSub macOS 自签名版安装说明 / Self-Signed macOS Install Guide

## 中文

当前 macOS 安装包使用 FinalSub 固定的自签名证书构建，尚未使用 Apple Developer ID，也没有经过 Apple notarization。每个新下载版本第一次打开时，macOS 都可能要求你手动确认。固定证书能保持应用代码身份连续，但在没有 Apple Developer ID 的情况下，不能保证 Gatekeeper 跨版本免确认。

1. 只从 [FinalSub 官方 GitHub Releases](https://github.com/GravityPoet/FinalSub/releases) 下载 DMG 和同名 `.sha256`。
2. 在两个文件所在目录执行 `shasum -a 256 -c FinalSub-*-self-signed.dmg.sha256`，结果必须显示 `OK`。
3. 打开 DMG，把 `FinalSub.app` 拖到“应用程序”。
4. 第一次启动若被 macOS 拦截，先尝试打开一次，然后进入“系统设置 → 隐私与安全性”，滚动到“安全性”，点击“仍要打开”，再确认“打开”。Apple 的当前说明见[安全地打开 Mac App](https://support.apple.com/zh-cn/102445)。

不要安装任何根证书，不要关闭 Gatekeeper，也不要运行来源不明的“破解限制”命令。FinalSub 自签名发布证书的 SHA-256 指纹固定为：

```text
C21E979BC792E4453F46B65B900702C4A3C9A00967273376193A678742B2944F
```

从 1.0.12 开始，这个渠道支持应用内签名更新：FinalSub 会检查新版本，点击“安装并重启”后自动下载、验签、替换并重新打开应用。1.0.11 及更早版本需要最后一次手动安装 1.0.12；以后无需重复下载 DMG。更新失败时仍可从官方 Release 手动覆盖安装。

## English

The current macOS package is signed with FinalSub's pinned self-signed certificate. It is not signed with an Apple Developer ID and is not notarized by Apple, so macOS may require manual approval the first time each newly downloaded build is opened. The pinned certificate preserves the app's code identity, but it cannot guarantee a cross-version Gatekeeper exception without an Apple Developer ID.

1. Download the DMG and matching `.sha256` only from the [official FinalSub GitHub Releases page](https://github.com/GravityPoet/FinalSub/releases).
2. In the download directory, run `shasum -a 256 -c FinalSub-*-self-signed.dmg.sha256`; the result must say `OK`.
3. Open the DMG and drag `FinalSub.app` to Applications.
4. If macOS blocks the first launch, try to open the app once, then go to System Settings → Privacy & Security → Security, click Open Anyway, and confirm Open. See Apple's current [Safely open apps on your Mac](https://support.apple.com/en-us/102445) guidance.

Do not install a root certificate, disable Gatekeeper, or run an untrusted “bypass” command. The SHA-256 fingerprint of FinalSub's self-signed release certificate is pinned to:

```text
C21E979BC792E4453F46B65B900702C4A3C9A00967273376193A678742B2944F
```

Starting with 1.0.12, this channel supports signed in-app updates. FinalSub checks for a new version and, after you choose “Install & Restart,” downloads, verifies, replaces, and relaunches the app. Versions 1.0.11 and earlier need one final manual installation of 1.0.12. A manual download from the official Release remains available as a fallback.
