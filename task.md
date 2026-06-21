# FinalSub Phase 2 ASR Task List

- `[x]` 编译与落位 whisper-cli
  - `[x]` 检索或使用 CMake 编译 arm64 / x86_64 架构二进制
  - `[x]` 签名并检查无 Homebrew 依赖
  - `[x]` 放入 `src-tauri/binaries` 并配置 lipo 合成 Universal
- `[x]` 迁移 Parakeet 脚本与添加合规协议
  - `[x]` 复制旧仓库的 `parakeet_transcribe.py` 至 `src-tauri/resources/parakeet`
  - `[x]` 创建 `whisper.cpp-LICENSE.txt` 与 `whisper-NOTICE.md`
- `[x]` 修改工程配置文件
  - `[x]` 修改 `tauri.conf.json`，将外部 binaries 和 resources 加入打包列表
  - `[x]` G1: 修复 FFmpeg 未找到（改为 tokio::process::Command 绕过 shell-plugin）
- `[x]` G2: 修改 Rust get_app_info 命令与 Cargo.toml 版本为 2.17.0
- `[x]` G3: 移除 Layout.tsx 中的 "Tauri 预览版" 副标题
- `[x]` G4: 修改 ModelsPage 与 Placeholder 中的 "待迁移/待接入" 等开发文案为 "敬请期待"
- `[x]` G5: 修改 models/mod.rs 主推 Whisper Large V3 中文，SenseVoice 改为 "敬请期待"
- `[x]` G6: 混合翻译设置与 Keychain 密钥安全存储
  - `[x]` Rust 引入 keyring 依赖
  - `[x]` Settings struct 增加 translate_endpoints 和 translate_models 字典
  - `[x]` TranslationProvider struct 增加能力属性并在 builtin_providers 填充
  - `[x]` 实现 Rust Keychain commands: set/get/delete_provider_secret
  - `[x]` 在后端 test_translation 自动从 settings 和 Keychain 拼接缺失的配置
  - `[x]` 前端 tauri.ts 暴露相应属性与 Keychain 接口
  - `[x]` 前端 TranslationPage.tsx 支持动态配置表单 and Keychain 存储
- `[x]` G7: 字幕校对功能完整移植
  - `[x]` 实现 Rust 端任务数据持久化 load/save_proofread_tasks
  - `[x]` 实现 Rust 端薄文件系统命令 fs_read_dir/fs_exists/fs_read_text/fs_write_text
  - `[x]` 注册新 commands
  - `[x]` 前端 tauri.ts 声明新接口
  - `[x]` 在前端 pages/proofread/ 移植 detector, language_detector, hooks, editor 与 sub-components shim
  - `[x]` 在 App.tsx 中挂载 ProofreadPage 路由
- `[x]` G8: HomePage 源/目标语言文本框改为下拉 Select 框
- `[x]` G9: 字幕合并页增加实时 ASS 转 CSS 字幕样式预览
- `[x]` Build & Verify: 双架构打包、ad-hoc 签名与业务功能复验产出含递增时间轴的 SRT

---

## 复核补充 (reviewer, commit `b28f975`)

- `[x]` 独立复核 18 项声明：16 项硬核实通过，2 项缺口已处理
- `[x]` 修复 `default_uv_bin()` 残留 `/opt/homebrew/bin/uv` 偏向 → PATH 优先 + 多候选兜底（`~/.local/bin`、`~/.cargo/bin`、homebrew、`/usr/local`）
- `[x]` 重新 `build:universal`，使 uv 修复进入分发包（产物 19:00 重建，签名/verify 通过）
- `[x]` whisper-cli + ffmpeg **arm64** 原生实跑通过（Metal / ffmpeg 7.1.1 GPL static + lavfi 生成 + 重采样）
- `[x]` ffmpeg `otool -L` 零 Homebrew 依赖（arm64 + x86_64 均确认）
- `[ ]` **x86_64 未实跑**：本机未装 Rosetta（`oahd` 未运行），仅静态验证（file/lipo/otool），运行闭环待 Intel Mac / CI
- `[x]` Parakeet 在重建后的 `.app` 内实跑闭环

---

## 2026-06-21 审计修复：Keychain 后端与迁移矩阵

- `[x]` A0: 改前基线 `cargo test --manifest-path src-tauri/Cargo.toml` 通过，83 passed
- `[x]` A1: `keyring` 启用 `apple-native` / `windows-native` feature，避免无平台后端退化为 mock
- `[x]` A2: `cargo build --manifest-path src-tauri/Cargo.toml` 通过，`Cargo.lock` 新增 `security-framework`
- `[x]` A3: 新增 ignored 真实 Keychain 往返测试，并显式运行通过
- `[x]` B1: 将 `implemented_providers_match_dispatch_table` 从假断言改为检查 `translate_text` dispatch arm
- `[x]` B2: 反向实验临时移除 `qwen` dispatch arm，目标测试按预期失败；恢复后目标测试通过
- `[x]` D1: 按源码现状修正 `MIGRATION_MATRIX.md` 中 provider 数、Keychain 后端、任务持久化/暂停/恢复/重试/日志流状态
- `[ ]` C1: provider `implemented` 策略仍需用户拍板：维持全 true、未 E2E 的复杂 provider 暂置 false，或补真实 E2E
- `[ ]` BIZ1: App 内重存真实 `custom-openai` API Key 并跑真实翻译任务，需要用户真实密钥和交互环境

---

## 2026-06-21 任务队列删除能力

- `[x]` 后端新增 `delete_task` / `delete_tasks`，删除队列记录并清理对应日志与临时工作目录
- `[x]` 删除保护：仅允许删除 `done/error/cancelled/paused`，`pending/running` 需先暂停或取消
- `[x]` 前端任务队列新增单条删除按钮、可删除任务复选框、全选可删除、删除选中和确认弹窗
- `[x]` 明确删除范围：不删除原始媒体文件，也不删除已导出的输出文件
- `[x]` 验收：`cargo test --manifest-path src-tauri/Cargo.toml` 84 passed / 1 ignored，`npm run build` 通过，开发版已重新启动
