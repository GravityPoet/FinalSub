# FinalSubTauri Phase 2 ASR Task List

- `[x]` 编译与落位 whisper-cli
  - `[x]` 检索或使用 CMake 编译 arm64 / x86_64 架构二进制
  - `[x]` 签名并检查无 Homebrew 依赖
  - `[x]` 放入 `src-tauri/binaries` 并配置 lipo 合成 Universal
- `[x]` 迁移 Parakeet 脚本与添加合规协议
  - `[x]` 复制旧仓库的 `parakeet_transcribe.py` 至 `src-tauri/resources/parakeet`
  - `[x]` 创建 `whisper.cpp-LICENSE.txt` 与 `whisper-NOTICE.md`
- `[x]` 修改工程配置文件
  - `[x]` 修改 `tauri.conf.json`，将外部 binaries 和 resources 加入打包列表
  - `[x]` 修改 `package.json`，在 `binaries:universal` 中追加 `whisper-cli` 的 lipo 合成
  - `[x]` 修改 `.gitignore` 忽略 `whisper-cli-universal-apple-darwin`
- `[x]` 修改 Rust 命令层代码
  - `[x]` 在 `commands/mod.rs` 中编写 `resolve_sidecar` 辅助函数支持 dev & bundle 解析
  - `[x]` 消除 `transcribe_audio` 中写死 `/opt/homebrew/bin/whisper-cli` 路径，改用 `resolve_sidecar` 并补齐 `app: AppHandle`
  - `[x]` 消除 `transcribe_parakeet` 中写死脚本和 ffmpeg 路径，使用 `resolve_sidecar` 及 `resolve_resource` 处理
- `[x]` 验收与验证 (SOP-B 门控)
  - `[x]` 运行 `rg` 确认写死路径已清零
  - `[x]` 执行 `cargo clippy` 及 `cargo test` 校验代码规范
  - `[x]` 运行 `npm run build:universal` 构建通用包并验证签名
  - `[x]` 使用 `lipo -info` 确认 whisper-cli / ffmpeg 双架构支持
  - `[x]` 检查 `otool -L` 确认零 Homebrew 依赖
  - `[x]` 业务级验证：在 dev 和 bundle 中运行 ASR 成功产出含递增时间轴的 SRT

---

## 复核补充 (reviewer, commit `b28f975`)

- `[x]` 独立复核 18 项声明：16 项硬核实通过，2 项缺口已处理
- `[x]` 修复 `default_uv_bin()` 残留 `/opt/homebrew/bin/uv` 偏向 → PATH 优先 + 多候选兜底（`~/.local/bin`、`~/.cargo/bin`、homebrew、`/usr/local`）
- `[x]` 重新 `build:universal`，使 uv 修复进入分发包（产物 19:00 重建，签名/verify 通过）
- `[x]` whisper-cli + ffmpeg **arm64** 原生实跑通过（Metal / ffmpeg 7.1.1 GPL static + lavfi 生成 + 重采样）
- `[x]` ffmpeg `otool -L` 零 Homebrew 依赖（arm64 + x86_64 均确认）
- `[ ]` **x86_64 未实跑**：本机未装 Rosetta（`oahd` 未运行），仅静态验证（file/lipo/otool），运行闭环待 Intel Mac / CI
- `[x]` Parakeet 在重建后的 `.app` 内实跑闭环
