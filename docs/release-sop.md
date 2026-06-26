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
- 当前已验证平台：macOS Apple Silicon
- 当前默认 macOS 构建脚本：`npm run build:local`
- `npm run build:local` 会执行：
  - `tauri build`
  - 对 `src-tauri/target/release/bundle/macos/FinalSub.app` 做 ad-hoc 签名
  - `codesign --verify --deep --strict` 校验 `.app`
- Windows/Linux 打包命令：仓库当前没有专用脚本或 CI 配置证据，补齐前不得编造命令作为正式外发路径。

## 平台产物规划

| 平台 | 当前状态 | 目标产物 | 备注 |
| --- | --- | --- | --- |
| macOS | 已有本地验证流程 | `.dmg`、`.pkg` | 正式外发需 Developer ID 签名和 notarization |
| Windows | 待补齐 | `.msi` / `.exe` | 需要 Windows runner、签名证书和安装/卸载验收 |
| Linux | 待补齐 | `.AppImage` / `.deb` / `.rpm` | 需要 Linux runner 和目标发行版验收矩阵 |

## GitHub Release 规则

- Tag 格式：`v<package.json version>`，例如 `v1.0.10`。
- Release assets 必须同时上传安装包和对应 `.sha256`。
- 公开创建 tag、推送 tag、创建 GitHub Release、上传资产属于 `[P1]`，执行前必须有回滚路径和熔断条件。
- 本地打包、校验和生成、草稿说明属于低风险本地写入，不推送、不公开分发。
- Release notes 来源：`CHANGELOG`、上一个 tag 以来的 commits，或本文件记录的验收摘要；禁止声称未执行过的测试通过。

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
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo test && cargo clippy -- -D warnings
```

### Checksums

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && shasum -a 256 <artifact> > <artifact>.sha256
```

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

### 重新制作可验证 DMG

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

### 制作覆盖旧版的 PKG

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

### 覆盖安装验证

只在明确需要安装到本机时执行：

```bash
sudo installer -pkg "/Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/target/release/bundle/pkg/FinalSub_<version>_aarch64.pkg" -target /
codesign --verify --deep --strict --verbose=4 "/Applications/FinalSub.app"
plutil -extract CFBundleIdentifier raw "/Applications/FinalSub.app/Contents/Info.plist"
plutil -extract CFBundleShortVersionString raw "/Applications/FinalSub.app/Contents/Info.plist"
```

若当前会话不能无交互使用 `sudo`，且 `/Applications/FinalSub.app` 归当前用户所有，可使用本机覆盖 fallback。此路径只适合本机测试，不等同于 `.pkg` 安装器验收：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && /bin/bash <<'EOF'
set -euo pipefail

SRC="/Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/target/release/bundle/macos/FinalSub.app"
DST="/Applications/FinalSub.app"
BACKUP="/Applications/FinalSub.app.backup.$(date +%Y%m%d%H%M%S)"
RESTORED=0

rollback() {
  status=$?
  if [ "$status" -ne 0 ]; then
    rm -rf "$DST"
    if [ -d "$BACKUP" ]; then
      mv "$BACKUP" "$DST"
      RESTORED=1
    fi
    echo "ROLLBACK_RESTORED=$RESTORED"
  fi
  exit "$status"
}
trap rollback EXIT

if [ ! -d "$SRC" ] || [ ! -d "$DST" ]; then
  echo "missing source or destination app" >&2
  exit 1
fi
if ps ax -o args= | rg -q '^/Applications/FinalSub\.app/Contents/MacOS/finalsubtauri( |$)'; then
  echo "FinalSub is running; stop before overwrite" >&2
  exit 1
fi

codesign --verify --deep --strict --verbose=4 "$SRC"
mv "$DST" "$BACKUP"
ditto "$SRC" "$DST"
codesign --verify --deep --strict --verbose=4 "$DST"
plutil -extract CFBundleIdentifier raw "$DST/Contents/Info.plist"
plutil -extract CFBundleShortVersionString raw "$DST/Contents/Info.plist"
file "$DST/Contents/MacOS/finalsubtauri"
file "$DST/Contents/MacOS/ffmpeg"
file "$DST/Contents/MacOS/whisper-cli"
echo "BACKUP=$BACKUP"
EOF
```

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
- 若只是本机测试，可先备份旧版：

```bash
cp -a "/Applications/FinalSub.app" "/Applications/FinalSub.app.backup.$(date +%Y%m%d%H%M%S)"
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

当前仓库没有 Windows 专用打包脚本、CI workflow 或签名验收记录。补齐前只允许 dry run 规划，不得声称 Windows 安装包可正式外发。

最低需要补齐：

- Windows runner 或本机 Windows 构建环境。
- Tauri Windows 产物类型：`.msi`、`.exe` 或两者。
- 代码签名证书和签名命令。
- 安装、覆盖安装、卸载、SmartScreen/签名状态验收。
- 产物路径、校验和、GitHub Release asset 命名。

## Linux 打包

当前仓库没有 Linux 专用打包脚本、CI workflow 或发行版验收记录。补齐前只允许 dry run 规划，不得声称 Linux 安装包可正式外发。

最低需要补齐：

- Linux runner 或本机 Linux 构建环境。
- 目标产物：`.AppImage`、`.deb`、`.rpm` 或组合。
- 目标发行版矩阵和基础运行验收。
- 安装、覆盖安装、卸载验收。
- 产物路径、校验和、GitHub Release asset 命名。

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
- 对当前 `/Applications/FinalSub.app` 做时间戳备份。
- 用 `ditto` 将已签名校验的 `src-tauri/target/release/bundle/macos/FinalSub.app` 覆盖到 `/Applications/FinalSub.app`。
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
- 重新执行 `cargo fmt`、`cargo clippy -- -D warnings`、`cargo test`、`npm run build:local`，再重制 `.pkg` 和覆盖 `/Applications/FinalSub.app`。

防复发：

- 本机覆盖安装前先完成 `cargo clippy -- -D warnings`；若 clippy 失败，不要把旧构建当成最终安装结果，必须修复后重建再覆盖。

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
