# Release SOP

本 SOP 是 FinalSub 的项目级发版入口，覆盖 macOS、Windows、Linux 的安装包构建、验收、分发和问题复盘。后续发版遇到新的障碍，直接追加到本文「踩坑记录」小节，避免重复踩坑。

## 目标

- 产出可安装/可覆盖旧版的桌面安装包。
- 验证各平台安装包内部产物一致且可运行。
- 明确区分本地/内部测试包、正式外发包和 GitHub Release 分发。

## 当前项目事实

- App 名称：`FinalSub`
- Bundle ID：`com.gravitypoet.finalsub`
- 版本来源：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`
- 包管理器：`npm`
- 当前本机完整验收平台：macOS 12+ Universal（arm64 + x86_64）
- 当前默认 macOS 构建脚本：`npm run build:universal`
- `src-tauri/tauri.conf.json` 本地使用稳定自签名身份 `ChordVox Local Code Signing`；仓库只钉扎公开证书，私钥留在本机钥匙串。该身份用于本机覆盖安装与权限连续性验证，不等同于 Apple Developer ID、Gatekeeper 信任或 notarization；GitHub Actions 中的正式 `APPLE_SIGNING_IDENTITY` 会覆盖本地身份。
- Windows/Linux 已有固定来源与摘要的 sidecar 脚本及 GitHub Actions 构建矩阵；`b96134f` 已在 GitHub-hosted Windows/Linux runner 完成原生凭据、构建、安装、启动和卸载验证。

## 平台产物规划

| 平台 | 当前状态 | 目标产物 | 备注 |
| --- | --- | --- | --- |
| macOS | Universal `.app` / `.dmg` 已验证 | `.dmg` | 正式外发需 Developer ID 签名和 notarization |
| Windows | NSIS 构建/安装/启动/卸载已验证 | NSIS `.exe` | 正式公开下载仍需代码签名与 SmartScreen 验收 |
| Linux | AppImage/DEB 构建、启动、安装/卸载及 Secret Service 已验证 | `.AppImage` / `.deb` | 扩大兼容性声明前仍建议目标发行版桌面抽检 |

## GitHub Release 规则

- Tag 格式：`v<package.json version>`，例如 `v1.0.10`。
- Release assets 必须同时上传安装包和对应 `.sha256`。
- 公开创建 tag、推送 tag、创建 GitHub Release、上传资产属于 `[P1]`，执行前必须有回滚路径和熔断条件。
- 本地打包、校验和生成、草稿说明属于低风险本地写入，不推送、不公开分发。
- Release notes 来源：`CHANGELOG`、上一个 tag 以来的 commits，或本文件记录的验收摘要；禁止声称未执行过的测试通过。

## 签名应用内更新

正式 release 使用 Tauri 官方 updater 产物；本地普通构建不生成 updater 包，也不要求持有生产私钥：

- `src-tauri/tauri.release.conf.json` 是不含密钥的 release 模板，启用 `bundle.createUpdaterArtifacts`；workflow 调用 `npm run prepare:release-updater-config`，从 Secret 原子生成 git-ignored 的 `src-tauri/target/tauri.release.generated.conf.json` 后再叠加构建。
- `FINALSUB_UPDATER_PUBLIC_KEY` 在编译时写入正式分发二进制；没有该值的构建只提供 Releases 页面手动下载。
- `TAURI_SIGNING_PRIVATE_KEY` 只存在于 GitHub Actions secret，用于签名 updater 包；如私钥带密码，再配置 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
- release job 必须生成并合并 `latest.json`，同时包含 `darwin-aarch64-app`、`darwin-x86_64-app`、`linux-x86_64-appimage`、`linux-x86_64-deb` 与 `windows-x86_64-nsis`。任一 URL 或签名缺失，publish job 必须停止，draft 不得公开。
- 应用只从固定 HTTPS 地址读取 `latest.json`，并只接受 `api.github.com/repos/GravityPoet/FinalSub/releases/assets/<数字 ID>` 下载地址；正式构建存在公钥时，来源或签名检查失败都不会降级到未签名安装。
- 临时生成的 release 配置权限为 `0600`，位于已忽略的 `src-tauri/target`；不得上传为 artifact 或打印其内容。
- 安装前后端会阻止运行中/排队中的字幕任务、模型下载/安装和视频合成；下载期间若出现新任务，安装前会再次检查并停止替换。

生产 updater 根密钥属于 `[P0]` 信任根：一旦旧版本内置公钥，丢失对应私钥会让这些安装无法接受后续更新。生成或更换生产根密钥前，必须先确认离线加密备份、恢复演练、双人保管边界和旧版本迁移方案；不得把测试私钥、空公钥或私钥内容写入仓库。

workflow 所需 updater secrets：

```text
FINALSUB_UPDATER_PUBLIC_KEY
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD  # 仅私钥设置密码时需要
```

## 通用发布前检查

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && git status --short --branch
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && git remote -v
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && git tag --sort=-version:refname | head -20
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && node -v && npm -v && cargo --version && rustc --version
```

确认点：

- 工作树里没有会被打包误带入或误覆盖的无关改动。
- 三处版本号一致：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`。
- Tag 与版本号一致，除非明确发布 prerelease。
- 正式外发包必须有对应平台签名、notarization 或发行渠道要求的验收证据。

## 通用命令

### Install

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm ci
```

### Verify

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo fmt --check && cargo test --lib && cargo clippy --all-targets --all-features -- -D warnings
```

### Checksums

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && shasum -a 256 <artifact> > <artifact>.sha256
```

## Windows / Linux 非公开安装包验证

`.github/workflows/platform-validation.yml` 是只允许手动触发的目标机验证入口。它不读取生产签名密钥，不创建 Tag 或 Release，也不把产物公开分发；生成的未签名安装包只作为 7 天临时 Actions Artifact 保存。

触发前必须满足：

- `main` 对应的 `Quality` 在精确 commit SHA 上完成且成功。
- 工作树与 `origin/main` 同步，准备验证的 commit 没有并发替换。
- Windows/Linux sidecar 来源、固定 commit 与 SHA-256 检查仍由仓库脚本执行，禁止占位二进制绕过 Tauri `externalBin` 检查。

触发和观察：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && gh workflow run platform-validation.yml --ref main
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && gh run list --workflow "Platform Package Validation" --limit 5
```

验收范围：

- Linux：真实构建 AppImage 与 DEB；在临时 D-Bus、GNOME Keyring 和 Xvfb 会话中运行原生凭据存储回环；AppImage 与安装后的 DEB 主程序都必须连续存活 15 秒；随后真实移除 DEB并生成 SHA-256。
- Windows：真实构建 NSIS；运行 Windows Credential Manager 回环；静默安装到 Runner 临时目录，主程序连续存活 10 秒，再静默卸载并生成 SHA-256。
- 两个平台：安装包与 `.sha256` 由 `actions/upload-artifact` 保存 7 天；任一文件缺失即失败。

边界：

- 此工作流证明对应 GitHub-hosted Runner 上的构建、原生凭据存取、安装、启动和卸载，不证明任意客户机器或驱动组合均兼容。
- Windows 未签名包的 Authenticode 状态必须写入 Step Summary，但当前验证阶段不因 `NotSigned` 失败；正式公开下载仍需代码签名并评估 SmartScreen。
- Linux Xvfb + GNOME Keyring 是真实桌面服务 E2E，但仍需至少一台目标发行版实体/虚拟桌面做人工 UI 与桌面集成验收后才可扩大兼容性声明。

最近通过的证据：GitHub Actions `Platform Package Validation` run `29812150992`，commit `b96134f96a18d6bee824a501045216da31df1ded`。Windows 与 Linux job 均成功；未签名验证产物及 SHA-256 作为 7 天临时 Artifact 保存至 2026-07-28。

## macOS 打包

### 发布前检查

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && git status --short --branch
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && node -v && npm -v && cargo --version && rustc --version
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && plutil -p src-tauri/tauri.conf.json | sed -n '1,120p'
```

确认点：

- `productName` 仍为 `FinalSub`。
- `identifier` 仍为 `com.gravitypoet.finalsub`。
- 三处版本号一致：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`。
- 若要覆盖旧版，不能随意修改 `Bundle ID`。
- 若要给 Intel 用户发布，不能只打 `aarch64`，需要走 Universal 构建。

### 标准打包流程

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build:universal
```

基础产物：

- `src-tauri/target/universal-apple-darwin/release/bundle/dmg/FinalSub_<version>_universal.dmg`

Tauri 生成的 `.app` 是临时打包输入：脚本在构建前先清旧残留，验签后再在成功、失败或可捕获中断时物理删除。构建期间的 `target/.metadata_never_index` 仅是防止 Spotlight 收录临时 App 的第二道防线，不代替文件清理；若上次进程被强制终止，下一次构建会先物理清场。

基础验收（完整 App 内容从 DMG 挂载后检查）：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && hdiutil verify "src-tauri/target/universal-apple-darwin/release/bundle/dmg/FinalSub_<version>_universal.dmg"
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && test -z "$(find src-tauri/target -type d -path '*/bundle/macos/FinalSub.app' -prune -print)"
```

### 验证 DMG 内部 App

Tauri 现在会在制作 DMG 前签名 `.app`，不再需要二次制作镜像。仍必须挂载 DMG 验证内部 `.app`，因为只运行 `hdiutil verify` 不能证明应用签名、架构和最低系统版本正确。

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && /bin/bash <<'EOF'
set -euo pipefail

DMG="$(ls -1 src-tauri/target/universal-apple-darwin/release/bundle/dmg/FinalSub_*_universal.dmg | tail -1)"
MOUNT="$(mktemp -d /tmp/finalsub-dmg-mount.XXXXXX)"
DEVICE=""
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"

cleanup() {
  if [ -d "$MOUNT/FinalSub.app" ]; then
    "$LSREGISTER" -u "$MOUNT/FinalSub.app" >/dev/null 2>&1 || true
  fi
  if [ -n "$DEVICE" ]; then
    diskutil eject "$DEVICE" >/dev/null 2>&1 || hdiutil detach "$DEVICE" >/dev/null 2>&1 || true
  fi
  rmdir "$MOUNT" >/dev/null 2>&1 || true
}
trap cleanup EXIT

hdiutil verify "$DMG"
ATTACH_OUTPUT="$(diskutil image attach --readOnly --nobrowse --mountPoint "$MOUNT" "$DMG")"
DEVICE="$(printf '%s\n' "$ATTACH_OUTPUT" | awk 'NR==1{print $1}')"
codesign --verify --deep --strict --verbose=4 "$MOUNT/FinalSub.app"
test "$(lipo -archs "$MOUNT/FinalSub.app/Contents/MacOS/finalsubtauri")" = "x86_64 arm64"
test "$(lipo -archs "$MOUNT/FinalSub.app/Contents/MacOS/ffmpeg")" = "x86_64 arm64"
test "$(lipo -archs "$MOUNT/FinalSub.app/Contents/MacOS/whisper-cli")" = "x86_64 arm64"
test "$(xcrun vtool -show-build "$MOUNT/FinalSub.app/Contents/MacOS/finalsubtauri" | awk '/minos/{print $2}' | sort -u)" = "12.0"
test "$(xcrun vtool -show-build "$MOUNT/FinalSub.app/Contents/MacOS/whisper-cli" | awk '/minos/{print $2}' | sort -u)" = "12.0"
test "$(plutil -extract LSMinimumSystemVersion raw "$MOUNT/FinalSub.app/Contents/Info.plist")" = "12.0"
FILTERS="$("$MOUNT/FinalSub.app/Contents/MacOS/ffmpeg" -hide_banner -filters 2>&1)"
ENCODERS="$("$MOUNT/FinalSub.app/Contents/MacOS/ffmpeg" -hide_banner -encoders 2>&1)"
rg -q ' subtitles ' <<<"$FILTERS"
rg -q 'libx264' <<<"$ENCODERS"
test -f "$MOUNT/FinalSub.app/Contents/Resources/licenses/ffmpeg-GPLv3.txt"
test -f "$MOUNT/FinalSub.app/Contents/Resources/licenses/whisper.cpp-LICENSE.txt"
shasum -a 256 "$DMG" > "$DMG.sha256"
EOF
```

### 制作覆盖旧版的 PKG

`.pkg` 适合“安装器覆盖旧软件”的场景。安装路径固定为 `/Applications/FinalSub.app`，并通过 `upgrade-bundle` 匹配 `com.gravitypoet.finalsub`。

PKG 需要临时 `.app`，因此先运行 `npm run build:universal:bundle`；下方脚本的退出处理会在成功或失败时物理删除该 App：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build:universal:bundle
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && /bin/bash <<'EOF'
set -euo pipefail

APP="src-tauri/target/universal-apple-darwin/release/bundle/macos/FinalSub.app"
VERSION="$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist")"
PKG_DIR="src-tauri/target/universal-apple-darwin/release/bundle/pkg"
PKG="$PKG_DIR/FinalSub_${VERSION}_universal.pkg"
ROOT="$(mktemp -d /tmp/finalsub-pkg-root.XXXXXX)"
COMPONENTS="$(mktemp /tmp/finalsub-components.XXXXXX.plist)"

cleanup() {
  rm -rf "$ROOT" "$COMPONENTS"
  bash scripts/cleanup-finalsub-bundle-apps-macos.sh
}
trap cleanup EXIT

mkdir -p "$PKG_DIR" "$ROOT/Applications"
codesign --verify --deep --strict --verbose=4 "$APP"
ditto "$APP" "$ROOT/Applications/FinalSub.app"
pkgbuild --analyze --root "$ROOT" "$COMPONENTS" >/dev/null
plutil -replace 0.BundleIsRelocatable -bool false "$COMPONENTS"
plutil -replace 0.BundleOverwriteAction -string upgrade "$COMPONENTS"
pkgbuild \
  --root "$ROOT" \
  --install-location "/" \
  --identifier "com.gravitypoet.finalsub.pkg" \
  --version "$VERSION" \
  --component-plist "$COMPONENTS" \
  --ownership recommended \
  "$PKG"
pkgutil --payload-files "$PKG" | rg '^\./Applications/FinalSub\.app/Contents/(Info\.plist|MacOS/finalsubtauri|MacOS/ffmpeg|MacOS/whisper-cli)$'
EOF
```

验收 `.pkg`：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && /bin/bash <<'EOF'
set -euo pipefail

PKG="$(ls -1 src-tauri/target/universal-apple-darwin/release/bundle/pkg/FinalSub_*_universal.pkg | tail -1)"
TMP="$(mktemp -d /tmp/finalsub-pkg-expand.XXXXXX)"

cleanup() {
  rm -rf "$TMP"
}
trap cleanup EXIT

pkgutil --expand-full "$PKG" "$TMP/expanded"
codesign --verify --deep --strict --verbose=4 "$TMP/expanded/Payload/Applications/FinalSub.app"
plutil -extract CFBundleIdentifier raw "$TMP/expanded/Payload/Applications/FinalSub.app/Contents/Info.plist"
plutil -extract CFBundleShortVersionString raw "$TMP/expanded/Payload/Applications/FinalSub.app/Contents/Info.plist"
sed -n '1,220p' "$TMP/expanded/PackageInfo"
EOF
```

关键验收点：

- `PackageInfo` 里 `relocatable="false"`。
- `PackageInfo` 里有 `upgrade-bundle`。
- `bundle id` 是 `com.gravitypoet.finalsub`。
- 展开后的 `.app` 通过 `codesign --verify --deep --strict`。

### 覆盖安装验证

只在明确需要安装到本机时执行：

```bash
sudo installer -pkg "/Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/target/universal-apple-darwin/release/bundle/pkg/FinalSub_<version>_universal.pkg" -target /
codesign --verify --deep --strict --verbose=4 "/Applications/FinalSub.app"
plutil -extract CFBundleIdentifier raw "/Applications/FinalSub.app/Contents/Info.plist"
plutil -extract CFBundleShortVersionString raw "/Applications/FinalSub.app/Contents/Info.plist"
```

若当前会话不能无交互使用 `sudo`，且 `/Applications/FinalSub.app` 归当前用户所有，可使用本机覆盖 fallback。此路径只适合本机测试，不等同于 `.pkg` 安装器验收：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run install:local:universal
```

该脚本会先验签并制作 ZIP 回滚包，再原子替换 `/Applications/FinalSub.app`；成功启动后注销并删除 `src-tauri/target` 中的构建 `.app`，定向清除旧 DMG/构建路径的 LaunchServices 记录，最后要求 LaunchServices 与 Spotlight 对 `com.gravitypoet.finalsub` 都只返回 `/Applications/FinalSub.app`。禁止把第二个可索引 `.app` 或 `.app.backup.*` 留在 `/Applications`、仓库构建目录或 staging 目录。

本机启动验收使用等待循环，避免应用启动较慢导致误判：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && /bin/bash <<'EOF'
set -euo pipefail

open -na "/Applications/FinalSub.app"
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
  if pgrep -f '^/Applications/FinalSub\.app/Contents/MacOS/finalsubtauri( |$)' >/dev/null; then
    echo "LAUNCH_PROCESS_OK"
    PIDS="$(pgrep -f '^/Applications/FinalSub\.app/Contents/MacOS/finalsubtauri( |$)' || true)"
    if [ -n "$PIDS" ]; then
      kill $PIDS
    fi
    echo "QUIT_REQUESTED"
    exit 0
  fi
  sleep 1
done

echo "FinalSub did not appear as a running process within 15s" >&2
exit 1
EOF
```

熔断条件：

- 安装后 `/Applications/FinalSub.app` 不存在。
- Bundle ID 不是 `com.gravitypoet.finalsub`。
- 版本号不是本次发布版本。
- `codesign --verify --deep --strict` 失败。

回滚方式：

- 重新安装上一版 `.pkg` 或 DMG 中的上一版 `.app`。
- 本机脚本的 ZIP 回滚包位于 `~/Library/Application Support/FinalSub/Backups/<timestamp>/FinalSub.app.zip`，不会被 LaunchServices 当成第二个应用。

本机安装验收必须包含：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && test "$(mdfind 'kMDItemCFBundleIdentifier == \"com.gravitypoet.finalsub\"c' | sort)" = "/Applications/FinalSub.app"
```

### 正式外发要求

本地 ad-hoc 签名只适合开发和内部测试，不等同于正式分发签名。

正式外发前必须具备：

- `Developer ID Application`：签 `.app`。
- `Developer ID Installer`：签 `.pkg`。
- Apple notarization：提交并 staple。

检查证书：

```bash
security find-identity -v -p codesigning
security find-identity -v
```

若 `pkgutil --check-signature` 显示 `Status: no signature`，或 `spctl -a -vv -t install` 显示 `source=no usable signature`，说明 `.pkg` 外壳未签名，不适合正式外发。

## Windows 打包

Windows x86_64 release job 会先安装固定 SHA-256 的 GPL FFmpeg、从固定 whisper.cpp commit 构建 sidecar，再让 Tauri 生成 NSIS 安装包：

```powershell
./scripts/install-ffmpeg-sidecar-windows.ps1
./scripts/build-whisper-sidecar-windows.ps1
npm ci
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis
```

GitHub Actions 入口：`.github/workflows/release.yml`。`b96134f` 已在 Windows runner 完成 Credential Manager 回环、NSIS 静默安装、主程序启动与静默卸载。当前仓库尚未配置 Windows 代码签名证书；正式公开下载前仍需完成 Authenticode 与 SmartScreen 验收。

## Linux 打包

Linux x86_64 release job 会安装 WebKit/Secret Service/Keyutils 构建依赖，安装固定 SHA-256 的 GPL FFmpeg、从固定 whisper.cpp commit 构建 sidecar，再生成 AppImage 与 DEB：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && bash scripts/install-ffmpeg-sidecar-linux.sh
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && bash scripts/build-whisper-sidecar-linux.sh
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm ci
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run tauri -- build --target x86_64-unknown-linux-gnu --bundles appimage,deb
```

GitHub Actions 入口：`.github/workflows/release.yml`。`b96134f` 已在 Ubuntu 22.04 runner 的临时 D-Bus、GNOME Keyring 与 Xvfb 会话完成 Secret Service + Keyutils 回环、AppImage/DEB 启动、DEB 安装与卸载；该证据不替代所有目标发行版的桌面兼容性抽检。

## 踩坑记录

### 2026-07-15：无 updater 配置的本机构建启动即崩溃

现象：

```text
PluginInitialization("updater", "Error deserializing 'plugins.updater' ... invalid type: null")
```

原因：

- 本机构建刻意不携带生产 updater 公钥与 endpoint，但仍无条件注册了 updater plugin。
- 构建、签名和 DMG 挂载均能通过，只有从真实安装路径启动时才会暴露初始化失败。

处理与防复发：

- 仅当编译时存在非空 `FINALSUB_UPDATER_PUBLIC_KEY` 时注册 updater plugin；无公钥构建保留固定官方 Releases 手动下载流程。
- 正式签名构建必须同时使用原子生成的 release 配置提供固定 endpoint 与公钥，不允许签名检查失败后降级到手动判断并继续安装。
- 发版验收必须分别运行“无公钥本机构建”和“一次性测试公钥构建”至少 10 秒，并检查进程仍存活且日志不含 `PluginInitialization`、`panic` 或配置反序列化错误；不能以 build、codesign 或进程瞬时出现替代真实运行边界。

### 2026-06-22：Tauri 默认 DMG 内部 App 签名校验失败（2026-07-15 已修复）

现象：

```text
FinalSub.app: code has no resources but signature indicates they must be present
```

触发条件：

- `npm run build:local` 先执行 `tauri build` 生成 DMG。
- 随后脚本才对 `src-tauri/target/release/bundle/macos/FinalSub.app` 重新做 ad-hoc 签名。
- 结果是磁盘上的 `.app` 校验通过，但 Tauri 默认 DMG 内部的 `.app` 不是最终签名状态。

处理：

- 2026-06-22 当时先用 `signingIdentity: "-"` 解决 Tauri 制作 DMG 前的签名时序；2026-07-21 已升级为稳定本地自签名身份 `ChordVox Local Code Signing`，继续由 Tauri 在制作 DMG 前签名，CI 的正式 `APPLE_SIGNING_IDENTITY` 仍可覆盖。
- `build:local` / `build:universal` 不再在 DMG 生成后重签 `.app`。
- 每次仍需挂载 Tauri 生成的 DMG，对镜像内部 `FinalSub.app` 执行深度签名、双架构与最低 macOS 版本验证。

### 2026-06-22：`pkgbuild` 出现 `write: Permission denied` 但包可展开验证

现象：

```text
write: Permission denied
```

处理：

- 不能只凭这几行判断失败，先看 `pkgbuild` exit code。
- 必须执行 `pkgutil --payload-files` 确认主程序和 sidecar 已进入 payload。
- 必须执行 `pkgutil --expand-full` 展开 `.pkg`，再对展开后的 `.app` 跑 `codesign --verify --deep --strict`。

### 2026-06-22：本机没有正式 Installer 签名身份

现象：

```text
Package "FinalSub_1.0.10_aarch64.pkg":
   Status: no signature

FinalSub_1.0.10_aarch64.pkg: rejected
source=no usable signature
```

原因：

- 本机只有本地代码签名身份，没有 `Developer ID Installer` 证书。

处理：

- 内部测试可继续使用未签名 `.pkg`。
- 正式外发必须用 Apple Developer 证书签名 `.pkg`，并完成 notarization。
- 2026-06-24 复现：`pkgutil --check-signature "src-tauri/target/release/bundle/pkg/FinalSub_1.0.10_aarch64.pkg"` 返回 `Status: no signature` 且 exit code 为 1；`spctl -a -vv -t install` 返回 `rejected` / `source=no usable signature` 且 exit code 为 3。内部包验收脚本不能把这两个命令放在 `set -e` 的硬失败链路里，应显式记录退出码；正式外发仍必须签名和 notarize。

### 2026-06-26：本机覆盖安装不能假设有 passwordless sudo

现象：

```text
Command: sudo -n true
sudo: a password is required
```

原因：

- 本机 `.pkg` 的 `PackageInfo` 为 `auth="root"`；`installer -pkg ... -target /` 需要 root 授权。
- 当前 Codex 会话不能交互输入 sudo 密码。
- `/Applications/FinalSub.app` 实际归当前用户 `moonlitpoet:staff` 所有，可用本机 fallback 覆盖 `.app`。

处理：

- 先确认 FinalSub 没有运行。
- 对当前 `/Applications/FinalSub.app` 制作带时间戳的 ZIP 回滚包，禁止在可索引目录留下第二个 `.app`。
- 用 `ditto` 将已签名校验的 `src-tauri/target/universal-apple-darwin/release/bundle/macos/FinalSub.app` 覆盖到 `/Applications/FinalSub.app`。
- 覆盖后重新执行 `codesign --verify --deep --strict`、Bundle ID、版本号和 sidecar 架构校验。

防复发：

- 覆盖本机旧版前先跑 `sudo -n true`、`stat -f '%Su %Sg %Sp %N' /Applications/FinalSub.app` 和精确进程检查。
- 若无 passwordless sudo 但目标 `.app` 归当前用户所有，走本机 fallback；若目标归 root 或权限不明，只给 dry run 和需要用户授权的命令。

### 2026-06-26：启动验收 4 秒等待会误判失败

现象：

```text
Command: open -na "/Applications/FinalSub.app"; sleep 4; ps ax -o args= | rg '^/Applications/FinalSub\.app/Contents/MacOS/finalsubtauri( |$)'
FinalSub did not appear as a running process after launch
```

原因：

- 后续复核 `ps ax -o pid=,comm=,args= | rg -i 'FinalSub|finalsubtauri|finalsub'` 发现 `/Applications/FinalSub.app/Contents/MacOS/finalsubtauri` 已经启动。
- 固定 `sleep 4` 对 Tauri GUI 启动不够稳，容易在应用尚未完成拉起时误判。

处理：

- 改为最多 15 秒的 `pgrep -f '^/Applications/FinalSub\.app/Contents/MacOS/finalsubtauri( |$)'` 等待循环。

防复发：

- 本机启动验收不得只用一次短 sleep；必须使用等待循环和精确可执行路径匹配。

### 2026-06-26：AppleScript quit 不一定退出 Tauri 进程

现象：

```text
Command: osascript -e 'tell application "FinalSub" to quit'
Result: /Applications/FinalSub.app/Contents/MacOS/finalsubtauri remained running
```

原因：

- Tauri 应用不一定响应 AppleScript 的 `quit` 事件。

处理：

- 启动验收确认进程存在后，用精确匹配到的 `finalsubtauri` PID 执行 `kill`；若短时间内仍未退出，再 `kill -9`。

防复发：

- 验收脚本要同时校验启动和退出；退出不要只依赖 `osascript`。

### 2026-06-26：Rust 1.94 下 `cargo clippy -- -D warnings` 失败

现象：

```text
Command: cargo clippy -- -D warnings
error: field assignment outside of initializer for an instance created with Default::default()
error: casting to the same type is unnecessary (`i32` -> `i32`)
error: useless conversion to the same type: `std::string::String`
error: this method chain can be written more clearly with `if .. else ..`
error: found call to `str::trim` before `str::split_whitespace`
error: this `impl` can be derived
```

原因：

- 本机 Rust/Clippy 版本为 `rustc 1.94.0`、`cargo 1.94.0`，`-D warnings` 会把这些风格 lint 升级为构建失败。

处理：

- 对 `src-tauri/src/core/asr/sensevoice.rs`、`src-tauri/src/core/asr/custom.rs`、`src-tauri/src/core/settings/mod.rs`、`src-tauri/src/core/subtitle/mod.rs`、`src-tauri/src/core/task_queue/mod.rs` 做行为保持的机械修复。
- 重新执行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --lib`，再运行 `npm run build:install:universal` 覆盖 `/Applications/FinalSub.app`。

防复发：

- 本机覆盖安装前先完成 `cargo clippy -- -D warnings`；若 clippy 失败，不要把旧构建当成最终安装结果，必须修复后重建再覆盖。

### 2026-07-21：Quality 的 Linux 测试在业务测试开始前缺少 sidecar

现象：

```text
resource path `binaries/ffmpeg-x86_64-unknown-linux-gnu` doesn't exist
```

原因：

- `src-tauri/tauri.conf.json` 声明 FFmpeg 与 Whisper 为 `externalBin`，Tauri build script 在编译测试目标时也会验证目标平台文件。
- Release job 会先运行固定摘要的 Linux sidecar 脚本，但旧 Quality 的 Ubuntu job 直接执行 `cargo test`，两个工作流的前置条件不一致。

处理：

- Quality 的 Ubuntu job 在 Rust 检查前复用 `scripts/install-ffmpeg-sidecar-linux.sh` 与 `scripts/build-whisper-sidecar-linux.sh`。
- 禁止创建空占位文件或在测试配置中移除 `externalBin`，否则 CI 绿色不能证明发布构建的真实前置条件。

防复发：

- 新增或调整平台 sidecar 后，Quality、非公开平台验证和 Release 三条工作流必须复用同一组固定来源构建脚本。

### 2026-07-21：pre-push 把运行时 UUID 误判为 secret assignment

现象：

```text
content:secret-assignment at the runtime-generated session_id test field
```

原因：

- 全局隐私门禁把测试中的 `session_id` 字段名视为疑似密钥赋值，但右值是每次运行动态生成的 UUID，不是硬编码凭据、账号、用户数据或可复用秘密。

处理：

- 精确审计命中提交与行内容后，只对该次推送使用门禁文档提供的 `git push --no-verify` reviewed exception；没有修改或关闭全局 hook。

防复发：

- 门禁命中时必须先审计原提交、字段来源与右值；只有确认是公开代码或运行时表达式时才能使用单次例外，真实常量或用户数据必须正常移除。

### 2026-07-21：macOS 专用参数在 Linux Clippy 中未使用

现象：

```text
error: unused variable: `app_config_dir`
--> src/core/secrets.rs:396:32
```

原因：

- macOS 凭据后端会用 `app_config_dir` 初始化本地加密仓库；Windows/Linux 通过条件编译改用系统凭据后端，同一参数在非 macOS 目标不参与表达式。
- 既有本机 Clippy 只覆盖 macOS，因此直到 Ubuntu Quality 真正越过 sidecar 编译边界后才暴露。

处理：

- 参数改为 `_app_config_dir`，macOS 分支继续使用同一值；没有增加 `allow`、没有跳过 Linux Clippy，也没有改变任何凭据读写路径。
- 本机重新通过凭据专项 6/6、全目标 Clippy 与格式检查；最终结论仍以精确 commit 的 Ubuntu Quality 为准。

防复发：

- 修改 `cfg(target_os)` 分支后，至少要求 macOS 与 Linux 的 `cargo clippy --all-targets --all-features -- -D warnings` 都在远端执行；涉及 Windows 专属代码时再由平台验证工作流覆盖 Windows 编译和原生凭据回环。

### 2026-07-21：Linux DEB 冒烟误把 FFmpeg 当成主程序

现象：

```text
deb exited before the 15 second smoke window (status 1)
ffmpeg version ...
```

原因：

- DEB 同时把 `finalsubtauri`、FFmpeg 与 Whisper sidecar 安装到 `/usr/bin`；旧验证脚本取 `dpkg -L` 返回的第一个可执行文件，实际选中了 FFmpeg。
- AppImage 主程序已连续存活 15 秒，DEB 自身也完成安装；失败发生在测试目标解析，不是应用启动崩溃。

处理：

- DEB 验收改为精确要求包清单包含且文件系统存在 `/usr/bin/finalsubtauri`，不再依赖包内文件顺序。
- 保留 FFmpeg/Whisper sidecar 在安装包内，禁止为了让冒烟通过而删除产品运行依赖。

防复发：

- 安装包包含多个可执行文件时，主程序验证必须绑定仓库声明的真实 binary name，不能用 `head -n 1` 或模糊 basename 推断。

### 追加模板

后续遇到新问题，按这个格式追加：

````markdown
### YYYY-MM-DD：<问题标题>

现象：

```text
<原始错误或关键日志>
```

原因：

- <证据路径或命令>

处理：

- <已验证可行的修复步骤>

防复发：

- <以后发布前必须增加的检查>
````
