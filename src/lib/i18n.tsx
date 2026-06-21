import React, { createContext, useContext, useState, useEffect } from "react";
import { getSettings } from "./tauri";

const zh = {
  // Navigation
  "nav.tasks": "新建任务",
  "nav.queue": "任务队列",
  "nav.models": "模型管理",
  "nav.translation": "翻译管理",
  "nav.proofread": "字幕校对",
  "nav.merge": "视频合字幕",
  "nav.settings": "设置",

  // Common
  "common.save": "保存",
  "common.cancel": "取消",
  "common.confirm": "确认",
  "common.delete": "删除",
  "common.error": "错误",
  "common.success": "成功",
  "common.browse": "浏览",

  // Settings
  "settings.title": "设置",
  "settings.saveSettings": "保存设置",
  "settings.saving": "正在保存...",
  "settings.reset": "重置默认",
  "settings.export": "导出配置",
  "settings.import": "导入配置",
  "settings.langGroup": "语言设置",
  "settings.langLabel": "界面语言",
  "settings.langDesc": "切换应用界面语言",
  "settings.modelStorageGroup": "模型存储路径",
  "settings.modelStorageLabel": "本地模型存储路径",
  "settings.modelStorageDesc": "存放 ASR 转录模型和本地 LLM 翻译模型的目录",
  "settings.modelStorageSelect": "选择目录",
  "settings.taskGroup": "任务队列设置",
  "settings.concurrentLabel": "最大并发任务数",
  "settings.concurrentDesc": "允许同时运行的转录/翻译任务上限 (1-8，改并发数对之后新建的任务生效)",
  "settings.outputGroup": "输出格式设置",
  "settings.outputLabel": "默认字幕输出格式",
  "settings.outputDesc": "转录生成字幕时的默认文件格式",
  "settings.vadGroup": "VAD 语音活动检测 (Silero VAD)",
  "settings.vadGroupDesc": "提示：VAD 仅有效于 whisper-cpp 引擎；Parakeet 暂不支持。",
  "settings.useVadLabel": "启用 VAD",
  "settings.useVadDesc": "过滤音频中的静音片段，提升转录质量",
  "settings.vadThresholdLabel": "检测阈值",
  "settings.vadThresholdDesc": "语音活动判断阈值 (0-1，默认 0.5)",
  "settings.vadMinSpeechLabel": "最小语音时长 (ms)",
  "settings.vadMinSpeechDesc": "短于此毫秒数的片段将被忽略 (默认 250)",
  "settings.vadMinSilenceLabel": "最小静音时长 (ms)",
  "settings.vadMinSilenceDesc": "长于此毫秒数的静音将触发切片 (默认 100)",
  "settings.vadMaxSpeechLabel": "最大语音时长 (s)",
  "settings.vadMaxSpeechDesc": "0 表示不限制，最大可设 3600",
  "settings.vadSpeechPadLabel": "语音扩展边距 (ms)",
  "settings.vadSpeechPadDesc": "前后扩展检测到的语音 (最大 5000)",
  "settings.vadSamplesOverlapLabel": "样本重叠度 (秒)",
  "settings.vadSamplesOverlapDesc": "片段间的时间重叠度 (0-1，默认 0.1)",
  "settings.updateGroup": "通用与更新设置",
  "settings.updateLabel": "启动时检查更新",
  "settings.updateDesc": "开启后，每次应用启动将自动检查 GitHub 上的最新版本",
  "settings.telemetryLabel": "上报崩溃与错误（匿名）",
  "settings.telemetryDesc": "开启后将崩溃/错误信息匿名上报到 Sentry，帮助定位问题。默认关闭，不上报用户 IP；报错可能含文件路径，介意请保持关闭",
  "settings.advancedGroup": "转录高级设置 (仅 whisper-cpp 生效)",
  "settings.whisperCommandLabel": "自定义 whisper-cli 路径",
  "settings.whisperCommandDesc": "留空使用内置执行文件。若填写则覆盖默认路径。",
  "settings.maxContextLabel": "最大上下文 Token 数",
  "settings.maxContextDesc": "转录时参考的最大上下文 Token 长度，-1 为默认值。",
  "settings.translationRetryLabel": "翻译重试次数",
  "settings.translationRetryDesc": "翻译失败时的重试次数上限",
  "settings.defaultTargetLanguageLabel": "默认目标语言",
  "settings.defaultTargetLanguageDesc": "翻译任务的默认目标语言",
  "settings.importExportGroup": "配置导入导出",
  "settings.dangerGroup": "危险操作",
  "settings.resetLabel": "恢复默认设置",
  "settings.resetDesc": "清除所有自定义配置，恢复出厂默认值",
  "settings.resetConfirmTitle": "确认恢复默认设置？",
  "settings.resetConfirmDesc": "此操作将清除所有自定义设置并恢复为出厂默认配置，此操作不可逆。",

  // HomePage
  "home.title": "任务",
  "home.newTask": "新建任务",
  "home.selectFile": "选择文件",
  "home.selectedFile": "已选择文件",
  "home.subFile": "字幕文件",
  "home.mediaFile": "音视频文件",
  "home.taskType": "任务类型",
  "home.asrEngine": "ASR 引擎",
  "home.asrModel": "ASR 模型",
  "home.sourceLang": "源语言",
  "home.targetLang": "目标语言",
  "home.outputFormat": "字幕格式",
  "home.createTask": "开始任务",
  "home.createPreview": "快速预览",
  "home.creating": "正在创建...",
  "home.prereqMedia": "请选择音视频文件后再开始任务。",
  "home.prereqSub": "请选择字幕文件后再开始任务。",
  "home.prereqModel": "请先在模型管理页下载 Whisper 模型，或切换到已安装模型。",
  "home.detecting": "检测中...",
  "home.notFound": "未找到",
  "home.transOnlyLabel": "仅翻译",
  "home.transOnlyDesc": "翻译已有字幕文件",
  "home.genOnlyLabel": "仅生成字幕",
  "home.genOnlyDesc": "从音视频生成字幕文件",
  "home.genTransLabel": "生成并翻译",
  "home.genTransDesc": "生成字幕并翻译为目标语言",
  "home.newVersion": "发现新版本 v",
  "home.updateNotes": "更新说明",

  // TasksPage
  "tasks.title": "任务队列",
  "tasks.status.all": "全部",
  "tasks.status.running": "进行中",
  "tasks.status.completed": "已完成",
  "tasks.status.failed": "已失败",
  "tasks.status.paused": "已暂停",
  "tasks.modal.title": "任务日志",
  "tasks.modal.copy": "复制日志",
  "tasks.modal.copied": "已复制",
  "tasks.log.streaming": "日志实时流式更新中",
  "tasks.log.pending": "等待队列中，准备启动...",
  "tasks.log.paused": "任务已暂停",
  "tasks.log.done": "任务已完成",
  "tasks.log.error": "任务失败",
  "tasks.log.cancelled": "任务已取消",
};

const en = {
  // Navigation
  "nav.tasks": "New Task",
  "nav.queue": "Queue",
  "nav.models": "Models",
  "nav.translation": "Translation",
  "nav.proofread": "Proofread",
  "nav.merge": "Merge",
  "nav.settings": "Settings",

  // Common
  "common.save": "Save",
  "common.cancel": "Cancel",
  "common.confirm": "Confirm",
  "common.delete": "Delete",
  "common.error": "Error",
  "common.success": "Success",
  "common.browse": "Browse",

  // Settings
  "settings.title": "Settings",
  "settings.saveSettings": "Save Settings",
  "settings.saving": "Saving...",
  "settings.reset": "Reset Default",
  "settings.export": "Export Config",
  "settings.import": "Import Config",
  "settings.langGroup": "Language Settings",
  "settings.langLabel": "UI Language",
  "settings.langDesc": "Switch application interface language",
  "settings.modelStorageGroup": "Model Storage Path",
  "settings.modelStorageLabel": "Local Model Storage Path",
  "settings.modelStorageDesc": "Directory to store ASR models and local LLM translation models",
  "settings.modelStorageSelect": "Select Folder",
  "settings.taskGroup": "Task Queue Settings",
  "settings.concurrentLabel": "Max Concurrent Tasks",
  "settings.concurrentDesc": "Max limit of running tasks (1-8, applies to tasks created hereafter)",
  "settings.outputGroup": "Output Format Settings",
  "settings.outputLabel": "Default Subtitle Format",
  "settings.outputDesc": "Default file format when generating subtitles",
  "settings.vadGroup": "VAD Silence Detection (Silero VAD)",
  "settings.vadGroupDesc": "Note: VAD is only effective for whisper-cpp engine; Parakeet is not supported.",
  "settings.useVadLabel": "Enable VAD",
  "settings.useVadDesc": "Filter silent segments to improve transcription quality",
  "settings.vadThresholdLabel": "VAD Threshold",
  "settings.vadThresholdDesc": "Confidence threshold to detect speech (0-1, default: 0.5)",
  "settings.vadMinSpeechLabel": "Min Speech Duration (ms)",
  "settings.vadMinSpeechDesc": "Segments shorter than this ms will be ignored (default: 250)",
  "settings.vadMinSilenceLabel": "Min Silence Duration (ms)",
  "settings.vadMinSilenceDesc": "Silence longer than this ms will trigger segmentation (default: 100)",
  "settings.vadMaxSpeechLabel": "Max Speech Duration (s)",
  "settings.vadMaxSpeechDesc": "0 for no limit, max allowed is 3600",
  "settings.vadSpeechPadLabel": "Speech Padding (ms)",
  "settings.vadSpeechPadDesc": "Extend the detected speech boundaries (max: 5000)",
  "settings.vadSamplesOverlapLabel": "Samples Overlap",
  "settings.vadSamplesOverlapDesc": "Time overlap between segments (0-1, default: 0.1)",
  "settings.updateGroup": "General & Update Settings",
  "settings.updateLabel": "Check for Update on Startup",
  "settings.updateDesc": "Enable to automatically check for the latest version on GitHub at startup",
  "settings.telemetryLabel": "Report Crashes & Errors (Anonymous)",
  "settings.telemetryDesc": "Sends crash/error reports anonymously to Sentry to help diagnose issues. Off by default, no user IP collected; reports may contain file paths — keep off if sensitive",
  "settings.advancedGroup": "Advanced Settings (whisper-cpp only)",
  "settings.whisperCommandLabel": "Custom whisper-cli Path",
  "settings.whisperCommandDesc": "Leave empty for built-in. Overrides default whisper executable path.",
  "settings.maxContextLabel": "Max Context Tokens",
  "settings.maxContextDesc": "Max text context tokens during transcription, -1 for default.",
  "settings.translationRetryLabel": "Translation Retry Times",
  "settings.translationRetryDesc": "Max retry attempts on translation failure",
  "settings.defaultTargetLanguageLabel": "Default Target Language",
  "settings.defaultTargetLanguageDesc": "Default target language for translation tasks",
  "settings.importExportGroup": "Import / Export Config",
  "settings.dangerGroup": "Danger Zone",
  "settings.resetLabel": "Restore Default Settings",
  "settings.resetDesc": "Clear all custom settings and restore to factory defaults",
  "settings.resetConfirmTitle": "Reset settings to default?",
  "settings.resetConfirmDesc": "This will clear all custom configurations. This action is irreversible.",

  // HomePage
  "home.title": "Tasks",
  "home.newTask": "New Task",
  "home.selectFile": "Select File",
  "home.selectedFile": "Selected File",
  "home.subFile": "Subtitle File",
  "home.mediaFile": "Media File",
  "home.taskType": "Task Type",
  "home.asrEngine": "ASR Engine",
  "home.asrModel": "ASR Model",
  "home.sourceLang": "Source Language",
  "home.targetLang": "Target Language",
  "home.outputFormat": "Format",
  "home.createTask": "Start Task",
  "home.createPreview": "Quick Preview",
  "home.creating": "Creating...",
  "home.prereqMedia": "Please select a media file before starting.",
  "home.prereqSub": "Please select a subtitle file before starting.",
  "home.prereqModel": "Please download a Whisper model first in Model Management, or switch to an installed model.",
  "home.detecting": "Detecting...",
  "home.notFound": "Not Found",
  "home.transOnlyLabel": "Translate Only",
  "home.transOnlyDesc": "Translate existing subtitle files",
  "home.genOnlyLabel": "Generate Only",
  "home.genOnlyDesc": "Generate subtitle file from audio/video",
  "home.genTransLabel": "Generate & Translate",
  "home.genTransDesc": "Generate subtitle and translate to target language",
  "home.newVersion": "New version available v",
  "home.updateNotes": "Release notes",

  // TasksPage
  "tasks.title": "Queue",
  "tasks.status.all": "All",
  "tasks.status.running": "Active",
  "tasks.status.completed": "Done",
  "tasks.status.failed": "Failed",
  "tasks.status.paused": "Paused",
  "tasks.modal.title": "Task Logs",
  "tasks.modal.copy": "Copy Logs",
  "tasks.modal.copied": "Copied",
  "tasks.log.streaming": "Logs updating in real-time",
  "tasks.log.pending": "Queued, preparing to start...",
  "tasks.log.paused": "Task paused",
  "tasks.log.done": "Task completed",
  "tasks.log.error": "Task failed",
  "tasks.log.cancelled": "Task cancelled",
};

export type Locale = "zh" | "en";
export type TranslationKey = keyof typeof zh;

interface I18nContextProps {
  locale: Locale;
  t: (key: TranslationKey, defaultText?: string) => string;
}

const I18nContext = createContext<I18nContextProps | undefined>(undefined);

export const I18nProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [locale, setLocale] = useState<Locale>("zh");

  const loadLang = () => {
    getSettings()
      .then((s) => {
        if (s.language === "en" || s.language === "zh") {
          setLocale(s.language);
        }
      })
      .catch(console.error);
  };

  useEffect(() => {
    loadLang();

    window.addEventListener("settings-changed", loadLang);
    return () => {
      window.removeEventListener("settings-changed", loadLang);
    };
  }, []);

  const t = (key: TranslationKey, defaultText?: string): string => {
    const dict = locale === "en" ? en : zh;
    return dict[key] || defaultText || key;
  };

  return (
    <I18nContext.Provider value={{ locale, t }}>
      {children}
    </I18nContext.Provider>
  );
};

export const useI18n = () => {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used within an I18nProvider");
  }
  return context;
};
