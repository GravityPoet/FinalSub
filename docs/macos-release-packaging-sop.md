# macOS 发布打包 SOP

本 SOP 用于 FinalSub macOS 发布打包、覆盖旧版安装、签名验证和问题复盘。后续打包遇到新的障碍，直接追加到本文「踩坑记录」小节，避免重复踩坑。

## 目标

- 产出可覆盖旧版 `FinalSub.app` 的 macOS 安装包。
- 验证 `.app`、`.dmg`、`.pkg` 内部产物一致且可运行。
- 明确区分本地/内部测试包与正式外发包。

## 当前项目事实

- App 名称：`FinalSub`
- Bundle ID：`com.gravitypoet.finalsub`
- 版本来源：`package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`
- 当前默认构建脚本：`npm run build:local`
- `npm run build:local` 会执行：
  - `tauri build`
  - 对 `src-tauri/target/release/bundle/macos/FinalSub.app` 做 ad-hoc 签名
  - `codesign --verify --deep --strict` 校验 `.app`

## 发布前检查

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

## 标准打包流程

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run build:local
```

基础产物：

- `src-tauri/target/release/bundle/macos/FinalSub.app`
- Tauri 默认生成的 DMG：`src-tauri/target/release/bundle/dmg/FinalSub_<version>_aarch64.dmg`

基础验收：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && codesign --verify --deep --strict --verbose=4 "src-tauri/target/release/bundle/macos/FinalSub.app"
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && file "src-tauri/target/release/bundle/macos/FinalSub.app/Contents/MacOS/finalsubtauri"
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && file "src-tauri/target/release/bundle/macos/FinalSub.app/Contents/MacOS/ffmpeg"
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && file "src-tauri/target/release/bundle/macos/FinalSub.app/Contents/MacOS/whisper-cli"
```

## 重新制作可验证 DMG

经验教训：当前脚本是在 Tauri 生成 DMG 后，再对磁盘上的 `.app` 做 ad-hoc 签名。因此 Tauri 默认 DMG 里的 `.app` 不一定等于最终已校验的 `.app`。必须挂载 DMG 检查内部 `.app`，不能只看 `hdiutil verify`。

重新制作 DMG：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && /bin/bash <<'EOF'
set -euo pipefail

APP="src-tauri/target/release/bundle/macos/FinalSub.app"
VERSION="$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist")"
OUT="src-tauri/target/release/bundle/dmg/FinalSub_${VERSION}_aarch64_signed.dmg"
STAGE="$(mktemp -d /tmp/finalsub-dmg-stage.XXXXXX)"
MOUNT="$(mktemp -d /tmp/finalsub-dmg-mount.XXXXXX)"

cleanup() {
  hdiutil detach "$MOUNT" >/dev/null 2>&1 || true
  rm -rf "$STAGE" "$MOUNT"
}
trap cleanup EXIT

codesign --verify --deep --strict --verbose=4 "$APP"
ditto "$APP" "$STAGE/FinalSub.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "FinalSub" -srcfolder "$STAGE" -ov -format UDZO "$OUT"
hdiutil verify "$OUT"
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT" "$OUT" >/dev/null
codesign --verify --deep --strict --verbose=4 "$MOUNT/FinalSub.app"
EOF
```

交付 DMG 使用：

```text
src-tauri/target/release/bundle/dmg/FinalSub_<version>_aarch64_signed.dmg
```

## 制作覆盖旧版的 PKG

`.pkg` 适合“安装器覆盖旧软件”的场景。安装路径固定为 `/Applications/FinalSub.app`，并通过 `upgrade-bundle` 匹配 `com.gravitypoet.finalsub`。

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && /bin/bash <<'EOF'
set -euo pipefail

APP="src-tauri/target/release/bundle/macos/FinalSub.app"
VERSION="$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist")"
PKG_DIR="src-tauri/target/release/bundle/pkg"
PKG="$PKG_DIR/FinalSub_${VERSION}_aarch64.pkg"
ROOT="$(mktemp -d /tmp/finalsub-pkg-root.XXXXXX)"
COMPONENTS="$(mktemp /tmp/finalsub-components.XXXXXX.plist)"

cleanup() {
  rm -rf "$ROOT" "$COMPONENTS"
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

PKG="$(ls -1 src-tauri/target/release/bundle/pkg/FinalSub_*_aarch64.pkg | tail -1)"
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

## 覆盖安装验证

只在明确需要安装到本机时执行：

```bash
sudo installer -pkg "/Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/target/release/bundle/pkg/FinalSub_<version>_aarch64.pkg" -target /
codesign --verify --deep --strict --verbose=4 "/Applications/FinalSub.app"
plutil -extract CFBundleIdentifier raw "/Applications/FinalSub.app/Contents/Info.plist"
plutil -extract CFBundleShortVersionString raw "/Applications/FinalSub.app/Contents/Info.plist"
```

熔断条件：

- 安装后 `/Applications/FinalSub.app` 不存在。
- Bundle ID 不是 `com.gravitypoet.finalsub`。
- 版本号不是本次发布版本。
- `codesign --verify --deep --strict` 失败。

回滚方式：

- 重新安装上一版 `.pkg` 或 DMG 中的上一版 `.app`。
- 若只是本机测试，可先备份旧版：

```bash
cp -a "/Applications/FinalSub.app" "/Applications/FinalSub.app.backup.$(date +%Y%m%d%H%M%S)"
```

## 正式外发要求

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

## 踩坑记录

### 2026-06-22：Tauri 默认 DMG 内部 App 签名校验失败

现象：

```text
FinalSub.app: code has no resources but signature indicates they must be present
```

触发条件：

- `npm run build:local` 先执行 `tauri build` 生成 DMG。
- 随后脚本才对 `src-tauri/target/release/bundle/macos/FinalSub.app` 重新做 ad-hoc 签名。
- 结果是磁盘上的 `.app` 校验通过，但 Tauri 默认 DMG 内部的 `.app` 不是最终签名状态。

处理：

- 不直接交付 Tauri 默认 DMG。
- 用最终校验通过的 `FinalSub.app` 重新制作 `FinalSub_<version>_aarch64_signed.dmg`。
- 挂载新 DMG 后，对镜像内部的 `FinalSub.app` 再跑一次 `codesign --verify --deep --strict`。

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
