# FinalSubTauri Phase 2 验收报告 (ASR 运行时落地)

ASR (语音转文字) 运行时已完成全部的自包含化、通用包打包以及写死路径清理。本报告记录所有验证闸门输出与业务转录证据。

## 1. 结论

- **任务结果**：成功打通 `whisper-cli` 双架构自包含静态编译与 Tauri sidecar 接驳，迁入了 Parakeet 脚本，并在 `dev` 与 `bundle` 环境下完成完整 SRT 转录测试。
- **写死路径清理**：分发型 sidecar（whisper-cli / ffmpeg / Parakeet 脚本）路径全部清理完毕；`uv` 作为用户环境依赖采用 PATH 优先发现 + 多候选兜底（详见第 6 节复核修正）。
- **构建状态**：通用包（Universal App Bundle）构建正常，签名成功，双侧载（`whisper-cli` 与 `ffmpeg`）均实现 `x86_64` + `arm64` 双架构通用支持。

---

## 2. 关键事实与变更明细

### Git 提交
- **Commit Hash**: `53eee04e57e6bb59794c042b5a10777e2eb26867` (简称 `53eee04`)
- **变更统计**:
  ```
  .gitignore                                         |   3 +-
  package.json                                       |   2 +-
  src-tauri/binaries/whisper-cli-aarch64-apple-darwin      | Bin 0 -> 3197280 bytes
  src-tauri/binaries/whisper-cli-x86_64-apple-darwin | Bin 0 -> 3500752 bytes
  src-tauri/licenses/whisper-NOTICE.md               |  32 +++++
  src-tauri/licenses/whisper.cpp-LICENSE.txt         |  21 +++
  .../resources/parakeet/parakeet_transcribe.py      | 157 +++++++++++++++++++++
  src-tauri/src/commands/mod.rs                      | 130 ++++++++++++++++-
  src-tauri/tauri.conf.json                          |  13 +-
  9 files changed, 347 insertions(+), 11 deletions(-)
  ```

### whisper-cli 二进制参数
- **源版本**: `github.com/ggml-org/whisper.cpp` v1.9.1 (commit `f049fff`)
- **编译参数**: 静态链接 (`-DBUILD_SHARED_LIBS=OFF`)，开启 Metal 与 Accelerate 加速，交叉编译时关闭 host native 优化 (`-DGGML_NATIVE=OFF`)。
- **哈希与文件参数**:
  - `whisper-cli-aarch64-apple-darwin` (arm64, 3.2MB): `c7bf9701e2937f9b9f06b1a0b6c45806c4006990bd85bb89ac32fa6486b8e563`
  - `whisper-cli-x86_64-apple-darwin` (x86_64, 3.5MB): `ace8c282fc11cd1570d79f42f63a0c1e54be2c72ded434166c25dfea5713156e`

---

## 3. 真实闸门输出

### 3.1 写死路径扫描
```bash
$ rg -n '/Users/moonlitpoet|/opt/homebrew/bin/whisper|/opt/homebrew/Cellar' src-tauri/src/
# (无输出)
```
> ⚠️ 复核修正：该正则未覆盖 `/opt/homebrew/bin/uv`，存在假阴性。全仓扫描见第 6 节，残留项已修复。

### 3.2 代码规范与单元测试 (66 tests passed)
```
test result: ok. 66 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 3.3 通用包构建与签名校验
```
Finished 2 bundles at:
    .../bundle/macos/FinalSub Tauri Preview.app
    .../bundle/dmg/FinalSub Tauri Preview_0.1.0_universal.dmg
codesign --verify --deep --strict: valid on disk
satisfies its Designated Requirement
```

### 3.4 侧载程序架构核查 (universal)
```
lipo -info whisper-cli: x86_64 arm64
lipo -info ffmpeg:      x86_64 arm64
```

### 3.5 whisper-cli 依赖核查 (零 Homebrew 依赖)
```
otool -L whisper-cli (arm64 & x86_64):
    仅 Accelerate / Metal / MetalKit / Foundation / CoreFoundation /
    libSystem / libc++ / libobjc —— 全部 macOS 系统库，无 /opt/homebrew 或 /usr/local
```

### 3.6 随包资源文件校验
```
Contents/Resources/resources/parakeet/parakeet_transcribe.py  (5244 bytes)
Contents/Resources/licenses/  ffmpeg-GPLv2.txt / ffmpeg-NOTICE.md /
                              whisper-NOTICE.md / whisper.cpp-LICENSE.txt
```

---

## 4. 业务级转录证据

### 4.1 whisper-cli 实跑 (命中 Metal 加速)
7.4 秒 `/tmp/test.wav` → SRT：
```srt
1
00:00:00,000 --> 00:00:07,000
 Welcome to Final Sub, this is a test of the speech to text runtime, hope it works perfectly.
```

### 4.2 Parakeet 实跑 (命中 sidecar ffmpeg 注入)
```srt
1
00:00:00,000 --> 00:00:05,920
Welcome to Final Sup, this is a test of the speech to text runtime, hope it

2
00:00:05,920 --> 00:00:07,360
works perfectly.
```

---

## 5. 局限性与说明

1. **x86_64 验证**：原报告因本机为 Apple Silicon 未启用 Rosetta 而未实跑；复核阶段已通过 Rosetta 补齐实跑（见第 6 节）。
2. **Parakeet 运行环境**：Parakeet 转录引擎正常执行仍依赖用户本机安装 `uv` 包管理器。
3. **Metal 加速器**：`-DGGML_METAL=ON` 的 Metal 后端在 M4 上成功执行，读入内嵌 `.metallib`，无需外置资源。

---

## 6. 独立复核与修复 (reviewer, commit `b28f975`)

对 b 提交的 18 项声明逐项独立重跑核实，**16 项硬核实通过**（commit / 二进制哈希 / otool / lipo / 签名 / cargo test 66 / clippy 0 / 构建产物 / 代码逻辑 / whisper 实跑），发现并处理 2 处缺口：

### 6.1 写死路径 overclaim → 已修复
- 报告 3.1 的 rg 正则 `'/opt/homebrew/bin/whisper|/opt/homebrew/Cellar'` 未覆盖 `bin/uv`，导致「rg 0 匹配」为假阴性。
- 全仓扫描命中：`src-tauri/src/core/asr/parakeet.rs` 的 `default_uv_bin()` 仍以 `/opt/homebrew/bin/uv` 为首选候选（该文件不在 `53eee04` 变更内，属既有代码）。
- **修复 (`b28f975`)**：改为 **PATH 优先**发现 uv，未命中再回退多候选（`~/.local/bin`、`~/.cargo/bin`、`/opt/homebrew/bin`、`/usr/local/bin`），消除对 Homebrew 路径的字面首选偏向。Intel / 非 brew 环境可正确解析。
- 注：`uv` 是用户自带外部依赖，无法打包进 `.app`，保留已知安装位置作兜底是合理设计；分发型 sidecar（whisper-cli/ffmpeg/脚本）已确认零写死。

### 6.2 重新构建使修复进入分发包
- `default_uv_bin()` 是编译进主程序的逻辑，需重新 `npm run build:universal` 才能让修复进入 `.app`。
- 已重建：产物 19:00 刷新，`codesign --verify --deep --strict` 通过，sidecar `whisper-cli` / `ffmpeg` 均 validated。

### 6.3 双架构 + 三引擎实跑闭环
| 引擎 / 架构 | 方式 | 结果 |
|---|---|---|
| whisper-cli arm64 | Metal 原生 | ✅ 正确递增 SRT |
| whisper-cli x86_64 | Rosetta 实跑（补原报告未做项） | ✅ 正确递增 SRT |
| Parakeet | 重建 `.app` 内脚本 + ffmpeg sidecar | ✅ 2 句递增 SRT |

**复核结论**：Phase 2 经修复 + 重建后，分发型路径零写死、双架构 sidecar 自包含、三引擎业务转录全实跑通过，产物与源码一致。
