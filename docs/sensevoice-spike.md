# SenseVoice Spike Results

## 测试时间
2026-06-19

## 环境检查

- macOS Apple Silicon (M4)
- Python 3.11+ (via Homebrew)
- uv 0.8.11

## 检查结果

| 候选方案 | 状态 | 说明 |
|---------|------|------|
| FunASR (pip) | ❌ 未安装 | `pip3 list` 无 funasr |
| sherpa-onnx | ❌ 未安装 | `pip3 list` 无 sherpa-onnx |
| SenseVoiceSmall 模型 | ❌ 未下载 | `/Users/moonlitpoet/Tools/Local-LLM/` 无 sensevoice 相关目录 |

## 结论

SenseVoice 在当前环境不可用。UI 保持 "待接入" 状态，不标记为 "已支持"。

## 后续步骤

1. 用户确认是否需要安装 FunASR 或 sherpa-onnx
2. 若需要，先做 30 秒中文/粤语/英文样本验证
3. 验证通过后再接入 ASR 引擎 trait
4. 验证内容：安装方式、模型大小、离线可用性、SRT 输出、速度、内存占用、许可证
