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
- Apple Developer ID 尚未具备期间，允许发布明确标记的 macOS 自签名客户包：只能从持有固定私钥的本机构建，必须使用 `v<version>-self-signed.<revision>` 标签、双语安装说明、DMG SHA-256、公开签名清单和手动更新模式。该渠道不安装根证书、不关闭 Gatekeeper、不声称经过 Apple 审核。
- Windows/Linux 已有固定来源与摘要的 sidecar 脚本及 GitHub Actions 构建矩阵；`3427b3a` 已在 GitHub-hosted Windows/Linux runner 完成原生凭据、构建、安装、启动和卸载验证，Windows 还完整通过临时 PFX 导入、Tauri Authenticode、RFC 3161 时间戳与同证书验签。
- Windows Release job 会临时导入 Base64 PFX、自动派生 SHA-1 thumbprint，以 SHA-256 + RFC 3161 时间戳交给 Tauri 签名，并在发布前要求安装包、主程序、FFmpeg 与 Whisper sidecar 均由同一证书签名且带时间戳；PFX 文件会在导入后立即物理删除，runner 证书在 `always()` 清理步骤移除。
- Release workflow 会在创建 Draft 前验证 Tag、三处版本号和 11 项全平台必需 Secret；macOS 构建后还会挂载最终 DMG，独立验证 Developer ID、Team ID、secure timestamp、Hardened Runtime、stapled notarization ticket、Gatekeeper 与 Universal sidecar。

## 平台产物规划

| 平台 | 当前状态 | 目标产物 | 备注 |
| --- | --- | --- | --- |
| macOS | Universal 自签名 `.dmg` 客户渠道已验证 | `.dmg` | 当前需首次手动放行；未来正式渠道使用 Developer ID 与 notarization |
| Windows | NSIS 构建/安装/启动/卸载已验证 | NSIS `.exe` | 正式公开下载仍需代码签名与 SmartScreen 验收 |
| Linux | AppImage/DEB 构建、启动、安装/卸载及 Secret Service 已验证 | `.AppImage` / `.deb` | 扩大兼容性声明前仍建议目标发行版桌面抽检 |

## GitHub Release 规则

- Tag 格式：`v<package.json version>`，例如 `v1.0.10`。
- 无 Apple 证书的 macOS 临时渠道使用 `v<version>-self-signed.<revision>`，例如 `v1.0.10-self-signed.1`；该标签由正式 `release.yml` 明确排除，避免误触 Developer ID / 公证 / 多平台 updater 流程。自签名 Release 必须在标题、正文和资产名中写明 `self-signed`，不得覆盖同版本正式 Tag。
- 正式 `v<version>` Release 在创建 Draft 前必须通过 `npm run preflight:release -- v<version>`；缺 Secret、Tag/版本不一致、updater 公钥误填私钥、Apple Team ID 或 Windows 时间戳地址无效时，不得创建半成品 Release。自签名 Tag 不是正式 Tag，不能把 `v<version>-self-signed.<revision>` 传给这个严格的正式渠道预检；自签名渠道使用 `npm run test:release-preflight` 的规则测试，加上自签名打包脚本的版本、身份、资产和清单验收。
- Release assets 必须同时上传安装包和对应 `.sha256`。
- 公开创建 tag、推送 tag、创建 GitHub Release、上传资产属于 `[P1]`，执行前必须有回滚路径和熔断条件。
- 本地打包、校验和生成、草稿说明属于低风险本地写入，不推送、不公开分发。
- 自签名渠道的本地包使用 `npm run package:release:self-signed:macos`；它要求 clean `main` 与 upstream 精确同步、远端标签和 Release 无碰撞，主动移除 updater/Apple 生产环境变量，再构建、挂载、验签并生成发布目录。公开推送 Tag 与 Release 仍是 `[P1]`。
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

自签名 macOS 渠道不内置生产 updater 公钥，也不生成 updater artifact 或 `latest.json`。应用仍可读取 GitHub Releases 的公开版本信息，但只能引导客户手动下载新 DMG；不得把手动下载包装成“应用内自动更新”。

生产 updater 根密钥属于 `[P0]` 信任根：一旦旧版本内置公钥，丢失对应私钥会让这些安装无法接受后续更新。生成或更换生产根密钥前，必须先确认离线加密备份、恢复演练、双人保管边界和旧版本迁移方案；不得把测试私钥、空公钥或私钥内容写入仓库。

workflow 所需 updater secrets：

```text
FINALSUB_UPDATER_PUBLIC_KEY
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD  # 仅私钥设置密码时需要
```

Windows 正式签名还需要：

```text
WINDOWS_CERTIFICATE           # Base64 编码的 PFX
WINDOWS_CERTIFICATE_PASSWORD  # PFX 导出密码
WINDOWS_TIMESTAMP_URL         # 证书服务商提供的 RFC 3161 时间戳地址
```

`WINDOWS_CERTIFICATE_THUMBPRINT` 不由人工配置；导入脚本从 PFX 中唯一带私钥的 Code Signing 证书自动派生。证书缺失、用途不符、有效期不足 30 天、时间戳地址无效、签名证书不一致或缺时间戳都会让 Windows job 失败，draft Release 不会公开。

该路径只适用于证书服务商明确提供的可导出 PFX。Tauri 官方说明，2023-06-01 之后签发的 OV 证书及 EV 证书可能要求硬件或云端签名；确定服务商后若拿到的是 Azure Artifact Signing、Key Vault 或硬件令牌，应改走其 `signCommand`，不得为了套用 PFX 流程强行导出私钥。

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
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run test:release-preflight
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run test:windows-signing-config
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri && cargo fmt --check && cargo test --lib && cargo clippy --all-targets --all-features -- -D warnings
```

### Checksums

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && shasum -a 256 <artifact> > <artifact>.sha256
```

## Windows / Linux 非公开安装包验证

`.github/workflows/platform-validation.yml` 是只允许手动触发的目标机验证入口。它不读取生产签名密钥，不创建 Tag 或 Release，也不把产物公开分发；Windows job 使用只在该 runner 临时生成并信任的自签名测试证书验证完整 Authenticode 管线，生成的验证包只作为 7 天临时 Actions Artifact 保存。该证书不具备客户侧公信力。

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
- Windows：运行 Credential Manager 回环；生成一次性 Code Signing 测试证书，经与正式发布相同的导入/config 链构建 NSIS；要求安装包、主程序、FFmpeg 与 Whisper 均为 `Valid`、证书 thumbprint 一致且带时间戳；静默安装后主程序连续存活 10 秒，再静默卸载并生成 SHA-256。
- 两个平台：安装包与 `.sha256` 由 `actions/upload-artifact` 保存 7 天；任一文件缺失即失败。

边界：

- 此工作流证明对应 GitHub-hosted Runner 上的构建、原生凭据存取、安装、启动和卸载，不证明任意客户机器或驱动组合均兼容。
- Windows 临时自签名证书只因被导入该 runner 的 `LocalMachine\Root` 才能显示 `Valid`；它仅证明 PFX 导入、Tauri 配置、二进制签名和验签门禁可执行，不替代正式 CA 证书或 SmartScreen 信誉。
- Linux Xvfb + GNOME Keyring 是真实桌面服务 E2E，但仍需至少一台目标发行版实体/虚拟桌面做人工 UI 与桌面集成验收后才可扩大兼容性声明。

最近通过的证据：GitHub Actions `Platform Package Validation` run `29819811618`，commit `3427b3a8eed7aaf5a5188db82157d2776ab18a52`。Windows 与 Linux job 均成功；Windows 完成临时 PFX 导入、Tauri 对安装器/主程序/FFmpeg/Whisper 的同证书签名、RFC 3161 时间戳、NSIS 安装/10 秒启动/卸载和强制验签，Linux 完成 Secret Service + Keyutils、AppImage/DEB 构建与安装启动链。两套验证产物及 SHA-256 作为 7 天临时 Artifact 保存至 2026-07-28。

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

### 无 Apple Developer ID 的自签名客户包

当前临时客户分发使用固定 `ChordVox Local Code Signing` 身份，不使用 ad-hoc 签名。该身份稳定绑定 Bundle ID 与 designated requirement，可减少同一台 Mac 覆盖安装时的身份漂移，但不能获得 Apple Gatekeeper 公信力；每个新下载版本首次打开时仍可能需要按系统界面手动确认。

前置条件：

- `main` 工作树 clean，且 `HEAD == @{upstream}`。
- 本机钥匙串中已有与仓库公开证书指纹一致的固定私钥；缺失时只允许从加密备份恢复，禁止临时生成替代证书。
- 精确 commit 的 `Quality` 已完成且成功。
- 目标 `v<version>-self-signed.<revision>` 在本地、origin 与 GitHub Releases 均不存在。
- 不配置生产 updater 根密钥，不注入 Apple Developer ID、公证或 Windows 生产签名 Secret。

Tag / Release 碰撞检查必须使用当前 `gh release list` 实际支持的字段；Release URL 只在目标存在时通过 `gh release view` 读取：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && TAG="v<version>-self-signed.<revision>" && \
  git tag --list "$TAG" && \
  git ls-remote --tags origin "refs/tags/$TAG" && \
  gh release list --limit 100 --json tagName,isDraft,isPrerelease | \
  node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const tag=process.argv[1];console.log(JSON.stringify(JSON.parse(s).filter(x=>x.tagName===tag)))})' "$TAG"
```

三段输出必须分别为空、为空、`[]`；任一非空都要停止重复创建并审计既有目标。

构建：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && \
  FINALSUB_SELF_SIGNED_REVISION=1 npm run package:release:self-signed:macos
```

脚本生成：

```text
src-tauri/target/self-signed-release/v<version>-self-signed.<revision>/
├── FinalSub-<version>-macos-universal-self-signed.dmg
├── FinalSub-<version>-macos-universal-self-signed.dmg.sha256
├── INSTALL-macOS-self-signed.md
├── RELEASE_NOTES.md
└── release-manifest.json
```

硬性验收：

- DMG 与内部 App 都由固定自签名证书签名，证书 SHA-256 必须为 `C21E979BC792E4453F46B65B900702C4A3C9A00967273376193A678742B2944F`。
- `codesign --verify --deep --strict`、Bundle ID、版本、Hardened Runtime、主程序/FFmpeg/Whisper Universal 架构、macOS 12 最低版本、FFmpeg 字幕与 x264 能力、许可证及 DMG 完整性全部通过。
- `stapler validate` 必须确认没有 notarization ticket；`spctl` 必须表现为自签名来源的 Gatekeeper 拒绝，防止把渠道标错为正式包。
- `release-manifest.json` 必须记录 commit、资产大小/SHA-256、证书指纹、designated requirement、`notarized: false` 与手动更新模式，不包含私钥或环境变量值。
- 客户说明必须明确：只从官方 Release 下载、先校验 SHA-256、通过“系统设置 → 隐私与安全性 → 仍要打开”完成每个新下载版本可能需要的首次确认；不安装根证书、不关闭 Gatekeeper、不提供 `xattr` 绕过命令。

发布命令只在目标 commit、Tag、版本和资产已经由上述门禁确定后执行：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && \
  git tag -a v<version>-self-signed.<revision> -m "FinalSub <version> macOS self-signed"
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && \
  git push origin v<version>-self-signed.<revision>
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && \
  gh release create v<version>-self-signed.<revision> \
    src-tauri/target/self-signed-release/v<version>-self-signed.<revision>/* \
    --title "FinalSub <version> · macOS Universal Self-Signed" \
    --notes-file src-tauri/target/self-signed-release/v<version>-self-signed.<revision>/RELEASE_NOTES.md
```

发布后必须下载公开 DMG 与 `.sha256` 到 `mktemp -d` 目录，执行 checksum 和 `verify-macos-self-signed-package.sh`，再核对 Release 为非 Draft、资产名/大小完整、正式 Developer ID workflow 没有被该 Tag 触发。当前自签名版本可以作为 GitHub latest 供手动更新检查发现，但不能包含 `latest.json` 或 Tauri updater 签名包。

回滚：公开前删除本地 Tag；推送 Tag 后但 Release 创建前，可删除远端 Tag（可由同 commit 重新创建）；Release 已公开后优先把 Release 改为 Draft 并保留资产/校验取证，不直接覆盖同名资产。若发现证书、SHA 或 commit 不符，立即 Draft 化并停止下载指引。

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

### Apple Developer ID 正式外发要求

稳定自签名渠道是 Apple Developer ID 暂缺时的透明降级方案，不等同于 Apple 正式信任或 notarization。获得 Apple 证书后，默认客户渠道切换到下述正式流程，自签名 Tag 不得冒充或覆盖正式 Tag。

正式外发前必须具备：

- `Developer ID Application`：签 `.app`。
- `Developer ID Installer`：签 `.pkg`。
- Apple notarization：提交并 staple。

GitHub Release 的 macOS job 会对最终 DMG 调用 `scripts/verify-macos-release-package.sh`。任一 Developer ID/Team ID/secure timestamp/Hardened Runtime/双架构检查不符，或 `stapler validate`、`spctl --assess` 未通过，整个 Release 保持 Draft，不会公开。

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

GitHub Actions 入口：`.github/workflows/release.yml`。Release job 已接入 PFX 临时导入、自动 thumbprint、SHA-256 + RFC 3161 时间戳、Tauri 签名配置、安装包/主程序/sidecar 同证书验签及失败后证书清理。`3427b3a` 的 run `29819811618` 已在 Windows runner 完成 Credential Manager 回环、临时 PFX 导入、Tauri 实际签名、四类文件同证书/时间戳验签、NSIS 静默安装、10 秒启动与静默卸载；正式 CA 证书尚未配置，公开下载前仍需完成生产 Authenticode 与 SmartScreen 验收。

## Linux 打包

Linux x86_64 release job 会安装 WebKit/Secret Service/Keyutils 构建依赖，安装固定 SHA-256 的 GPL FFmpeg、从固定 whisper.cpp commit 构建 sidecar，再生成 AppImage 与 DEB：

```bash
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && bash scripts/install-ffmpeg-sidecar-linux.sh
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && bash scripts/build-whisper-sidecar-linux.sh
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm ci
cd /Users/moonlitpoet/Tools/AI-tools/FinalSub && npm run tauri -- build --target x86_64-unknown-linux-gnu --bundles appimage,deb
```

GitHub Actions 入口：`.github/workflows/release.yml`。`3427b3a` 的 run `29819811618` 已在 Ubuntu 22.04 runner 的临时 D-Bus、GNOME Keyring 与 Xvfb 会话完成 Secret Service + Keyutils 回环、AppImage/DEB 启动、DEB 安装与卸载；该证据不替代所有目标发行版的桌面兼容性抽检。

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

### 2026-07-21：Windows 签名脚本的运行时密码变量被误判为 secret assignment

现象：

```text
content:secret-assignment .github/workflows/platform-validation.yml:100
content:secret-assignment scripts/import-windows-signing-certificate.ps1:35
```

原因：

- 第一处把 runner 现场生成的随机 UUID 转为一次性测试 PFX 密码；第二处把 GitHub Secret 注入的 PFX 密码转为 `SecureString`。两处右值均不是仓库常量，没有输出密码、PFX 或私钥。

处理：

- 逐行复核暂存内容与变量来源后，仅对该次提交使用 `git commit --no-verify`；保留全局隐私钩子，生产密码继续只通过单步 Secret 环境变量注入。

防复发：

- Windows 签名改动若命中 `secret-assignment`，必须确认右值是运行时随机值或 Secret 引用，并复核没有日志输出、持久文件或仓库常量；不满足任一条件时禁止例外提交。

### 2026-07-21：无桌面的 Windows runner 卡在用户根证书信任确认

现象：

```text
signing-fixture: trusting public certificate
The action 'Create ephemeral Windows signing fixture' has timed out after 5 minutes.
```

原因：

- `Platform Package Validation` run `29815630855` 在同一步持续等待后被人工熔断；增加阶段标记和 5 分钟硬超时后，run `29817028449` 证明 `Import-Certificate` 卡在 `CurrentUser\Root`，run `29818067101` 证明 `certutil -user` 访问同一受保护根证书库也会等待交互确认。
- 证书生成、PFX 导出和公钥导出均已在日志中完成；问题不在 Tauri、PFX 或业务代码，而是无桌面 runner 无法响应当前用户根证书信任对话框。

处理：

- GitHub-hosted Windows runner 以管理员身份运行且关闭 UAC，因此测试根证书改为通过 `certutil -f -addstore Root` 导入 `LocalMachine\Root`；导入后必须按精确 SHA-1 thumbprint 验证存在。
- 清理脚本同时精确删除 `CurrentUser\My` 的临时私钥证书与 `LocalMachine\Root` 的测试根证书；夹具在根证书写入前就把两个 thumbprint 写入 `GITHUB_ENV`，失败或超时仍可由 `always()` 清理。

防复发：

- 无桌面 CI 不再把临时自签名证书写入 `CurrentUser\Root`；所有临时信任写入必须有硬超时、阶段标记、精确存在性校验和按 thumbprint 清理。

### 2026-07-21：PowerShell EKU 的 ObjectId 被当成 Oid 对象

现象：

```text
The property 'Value' cannot be found on this object.
scripts/import-windows-signing-certificate.ps1:41
```

原因：

- `EnhancedKeyUsageList` 的元素类型是 `EnhancedKeyUsageRepresentation`，其 `ObjectId` 属性已经是字符串；脚本错误地继续读取 `.ObjectId.Value`，导致 PFX 已导入后在 Code Signing EKU 筛选处失败。

处理：

- 按官方类型定义直接收集 `EnhancedKeyUsageList.ObjectId` 字符串，再与 Code Signing OID `1.3.6.1.5.5.7.3.3` 比较；无 EKU 的证书安全得到空列表，不放宽“必须恰好一个带私钥的代码签名证书”门禁。

防复发：

- PowerShell provider 注入的脚本属性必须按其公开类型使用，不能从底层 .NET `Oid` 类型推断属性层级；Windows runner 必须真实跑过 PFX 导入与 EKU 判断。

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

### 2026-07-21：本地安全执行器拒绝用 `rm -f` 清理生成配置

现象：

```text
rm -f .../tauri.windows-signing.generated.conf.json .../tauri.release.generated.conf.json
rejected: rm -f style commands are not permitted. Use a safer approach
```

原因：

- 两个文件都是已知路径下的可重建 `target` 临时配置，但当前终端安全执行器仍按命令形态拒绝 `rm -f`。

处理：

- 改用精确文件级删除，没有删除目录、通配符、仓库文件或用户数据。

防复发：

- 自动验收清理由脚本自身的原子替换/退出清理负责；交互式补清理必须绑定完整已验证路径，若执行器拒绝 `rm -f`，改用精确文件删除，不扩大为递归清理。

### 2026-07-22：无 Apple Developer ID 时没有可公开验收的降级渠道

现象：

```text
GitHub Actions production secret names: <none>
Release workflow tag filter: v*
```

原因：

- 现有 `release.yml` 在任何 `v*` Tag 上都要求 Apple Developer ID、公证、Windows 生产证书和 updater 根密钥；仓库当前没有这些生产 Secret。
- 固定自签名 DMG 已能构建和覆盖安装，但旧 SOP 只把它定义为内部包，没有独立 Tag、客户首次放行说明、公开清单或发布后下载复验；直接推 Tag 只会制造失败的正式 workflow。

处理：

- 新增 `v<version>-self-signed.<revision>` 渠道，并在正式 workflow 的正向 Tag 规则之后用负向模式排除。
- 新增可重复打包/挂载验签脚本、双语安装说明、双语 Release notes、DMG SHA-256 和公开 manifest；自签名构建主动移除生产 updater/Apple 环境变量，只保留 GitHub Release 手动更新。
- 公开说明不要求客户安装根证书或执行 Gatekeeper 绕过命令，只使用 Apple 提供的“隐私与安全性 → 仍要打开”路径。

防复发：

- 正式 Developer ID 与临时自签名渠道必须使用不同 Tag 形态、不同验收器和不同发布说明；任何一方都不得静默回退到另一方。
- 推送自签名 Tag 前先验证正式 workflow 的负向过滤、精确 SHA 的 Quality、资产 checksum 与从公开 Release 重新下载后的签名；缺一项就不公开。

### 2026-07-22：`hdiutil attach/detach` 的旧参数组合已弃用

现象：

```text
hdiutil: WARNING: 'hdiutil attach -readonly -nobrowse -mountpoint ...' is deprecated.
Please use 'diskutil image attach --readOnly --mountOptions nobrowse --mountPoint ...' instead.
hdiutil: WARNING: 'hdiutil detach ...' is deprecated. Please use 'diskutil eject ...' instead.
```

原因：

- 第一版自签名 DMG 验收器沿用了旧 `hdiutil attach` 与 `hdiutil detach` 参数；当前 macOS 仍能挂载/弹出，但已经明确提示弃用。

处理与防复发：

- 验收器改用系统提示的 `diskutil image attach --readOnly --mountOptions nobrowse --mountPoint`，退出清理按解析出的精确 device 执行 `diskutil eject`。
- 新增 macOS DMG 挂载流程不得再复制旧 `hdiutil attach -readonly -nobrowse -mountpoint` 或 `hdiutil detach` 命令。

### 2026-07-22：系统 Ruby 不支持 `YAML.load_file(..., aliases:)`

现象：

```text
psych.rb:576:in `load_file': unknown keyword: aliases (ArgumentError)
```

原因：

- 本机系统 Ruby 2.6 / Psych 的 `YAML.load_file` 不接受新版 Ruby 的 `aliases:` 关键字；失败发生在临时工作流语法检查命令，不是 Release workflow 本身。

处理与防复发：

- 改用系统 Ruby 支持的 `YAML.load_file(".github/workflows/release.yml")` 完成解析；需要别名控制时使用项目固定版本的 YAML 工具，不能假设系统 Ruby API 与新版一致。

### 2026-07-22：前端构建报告主入口 chunk 超过 500 kB

现象：

```text
Some chunks are larger than 500 kB after minification.
dist/assets/index-*.js 571.43 kB (gzip 174.46 kB)
```

原因：

- Vite 的默认未压缩 chunk 告警阈值为 500 kB；页面已分块，剩余主入口包含共享运行时与通用依赖。构建成功，当前 gzip 体积为 174.46 kB，且本轮没有扩大前端运行时代码。

处理与防复发：

- 本次作为非阻断性能信号记录，不为自签名分发改动扩大到无关代码拆分；若主入口继续增长或启动性能实测回退，再单独做依赖分析与共享 chunk 拆分，不通过调高阈值隐藏告警。
- 2026-08-23 已按该条件处理：Vite 8 / Rolldown 通过 `build.rolldownOptions.output.codeSplitting` 把 `node_modules` 归入 vendor 分组，并把分组上限设为 450 KiB。主入口从 578.73 kB 降至 318.93 kB，生产构建不再报警；浏览器 10 个路由和“文件夹导入 → 校对 → 修改 → 保存”均为 0 error / 0 warning，最终 Universal Tauri 包已重新构建并原子安装。
- 仅设置 `maxSize` 而没有 `groups` 会输出 `Manual code splitting options (maxSize) specified without groups`，配置不会生效；不得通过提高 `chunkSizeWarningLimit` 隐藏告警。

### 2026-07-22：提交隐私钩子把公开发布文档误判为个人数据

现象：

```text
privacy guard: README_zh.md: contains labeled address personal data
privacy guard: docs/release-sop.md: contains labeled address/account/user personal data
```

原因：

- 命中内容逐行核对后均为公开的“API 地址”“时间戳地址”、GitHub Release URL、仓库绝对路径和测试命令；没有姓名、联系方式、账号值、凭据、私钥或用户数据。
- 钩子按标签词和绝对路径做保守匹配，无法区分发布文档与个人数据。

处理与防复发：

- 保留全局隐私钩子；完成暂存差异敏感模式扫描和命中行人工复核后，只对本次提交使用钩子输出允许的 `git commit --no-verify` reviewed exception。任何命中真实值、凭据或运行时用户数据的提交都不得使用例外。

### 2026-07-22：隔离下载在放行前执行 FFmpeg 被 macOS 杀死

现象：

```text
quarantined DMG verification exited 137 after deep signature checks
```

原因：

- 客户隔离模拟给 DMG 写入 `com.apple.quarantine` 后，复用了会执行内嵌 FFmpeg 能力探测的完整验收器；macOS 在用户尚未通过“仍要打开”放行应用前终止了该 sidecar。
- 退出发生在 Gatekeeper 已按预期拒绝、签名与 designated requirement 已验证之后，不是 FinalSub 正常启动或转录链崩溃。

处理与防复发：

- 完整二进制能力检查只在未隔离的同 SHA-256 发布资产上运行；隔离副本只检查 quarantine 属性、DMG 完整性、签名和 Gatekeeper 拒绝，放行前禁止启动主程序或任一 sidecar。
- 若隔离测试中断，必须核对临时挂载并用 `diskutil eject` 精确弹出，再清理受控的 `finalsub-self-signed-verify.*` 临时目录。

### 2026-08-23：GitHub API 短暂 TLS 握手超时

现象：

```text
gh auth status
X Timeout trying to log in to github.com account GravityPoet (keyring)

gh repo view
Post "https://api.github.com/graphql": net/http: TLS handshake timeout
```

原因：

- 同一时刻的仓库状态和 SSH 配置没有漂移；随后 `curl https://api.github.com` 返回 HTTP 200，`git ls-remote origin HEAD` 也成功，证明不是 Token、仓库权限或 SSH Key 损坏，而是一次短暂 API TLS 连接故障。

处理：

- 分别检查 GitHub API HTTPS、主页 HTTPS、SSH `ls-remote`、DNS 与代理环境；边界恢复后只重试一次原始 `gh auth status` / `gh repo view`，鉴权与推送均成功。

防复发：

- `gh` 出现 TLS timeout 时先区分 API、SSH、DNS 与代理边界，不立即重新登录、替换 Token 或修改 remote；只有独立连接检查恢复后才重试原命令。

### 2026-08-24：`gh release list --json` 不支持 `url` 字段

现象：

```text
Unknown JSON field: "url"
Available fields: createdAt, isDraft, isImmutable, isLatest, isPrerelease, name, publishedAt, tagName
SyntaxError: Unexpected end of JSON input
```

原因：

- 当前 `gh` 的 `release list` 子命令没有 `url` JSON 字段；上游命令退出后，管道中的 Node 仍尝试解析空输入，产生第二个语法错误。
- `gh release view` 支持 `url`，但它适合读取已存在的目标，不适合要求“目标必须不存在”的无错误碰撞检查。

处理：

- 碰撞检查改为 `gh release list --json tagName,isDraft,isPrerelease`，再按精确 Tag 过滤；本次目标返回 `[]`。
- 仅在 Release 创建后使用 `gh release view <tag> --json url,...` 读取公开 URL。

防复发：

- 为不同 `gh` 子命令分别使用其 `--json` 支持字段；写自动化前先以命令返回的字段列表或 `gh help` 核对，碰撞检查不请求 `url`。

### 2026-08-24：正式 Release 预检拒绝自签名 Tag

现象：

```text
Command: npm run preflight:release -- v1.0.11-self-signed.1
Release tag must be exactly v1.0.11
```

原因：

- `scripts/release-preflight.mjs` 是正式多平台 Release 的严格门禁，读取三处版本后把唯一允许的 Tag 固定为 `v${package.version}`；它还要求正式 updater、Apple、Windows Secret。自签名渠道有意使用不同 Tag 且不注入这些生产 Secret。

处理：

- 停止公开写入；自签名渠道改用 `npm run test:release-preflight`（规则测试）和 `npm run package:release:self-signed:macos`（真实版本/证书/资产/清单验收）。正式渠道仍只对 `v<version>` 运行 `npm run preflight:release -- v<version>`。

防复发：

- 发布前先按渠道路由预检命令，不把自签名 Tag 送入正式 Release 预检；在 SOP 中明确两条 Tag 形态和各自的 Secret/验收边界。

### 2026-08-24：打包清理临时 App 后直接安装缺少 source app

现象：

```text
Command: npm run install:local:universal
Missing source app: /Users/moonlitpoet/Tools/AI-tools/FinalSub/src-tauri/target/universal-apple-darwin/release/bundle/macos/FinalSub.app
```

原因：

- `package:release:self-signed:macos` 成功后按设计调用清理器，删除 `target/.../bundle/macos/FinalSub.app`，只保留已验收的 DMG 与发布目录；`install:local:universal` 的默认输入则正是这个临时 bundle 路径。

处理：

- 先运行 `npm run build:universal:bundle` 重新生成并验签临时 `.app`，再运行 `npm run install:local:universal`；安装脚本随后完成固定路径覆盖、回滚 ZIP、启动与唯一索引验收。

防复发：

- 自签名 DMG 打包与本机覆盖安装是两个独立阶段；打包脚本结束后不得假设临时 `.app` 仍存在。需要安装验收时，显式重建 `build:universal:bundle`，或从已验收 DMG 提取到受控临时输入。

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
