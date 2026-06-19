import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Settings as SettingsIcon, Save, RotateCcw, Download, Upload, FolderOpen } from "lucide-react";
import {
  getSettings,
  saveSettingsCmd,
  resetSettings,
  exportConfigToPath,
  importConfigFromPath,
  type Settings,
} from "../lib/tauri";

const languages = [
  { value: "zh", label: "中文" },
  { value: "en", label: "English" },
];

const outputFormats = [
  { value: "srt", label: "SRT (SubRip)" },
  { value: "vtt", label: "WebVTT" },
  { value: "ass", label: "ASS/SSA" },
  { value: "lrc", label: "LRC 歌词" },
  { value: "txt", label: "纯文本" },
];

const SettingRow = ({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) => (
  <div className="flex items-center justify-between gap-4 py-3">
    <div className="min-w-0">
      <div className="text-sm font-medium text-gray-900 dark:text-gray-100">{label}</div>
      {description && (
        <div className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">{description}</div>
      )}
    </div>
    <div className="shrink-0">{children}</div>
  </div>
);

const SettingGroup = ({
  icon: Icon,
  title,
  children,
}: {
  icon: React.ElementType;
  title: string;
  children: React.ReactNode;
}) => (
  <div>
    <div className="flex items-center gap-2 mb-3 px-1">
      <Icon className="size-4 text-gray-500" />
      <span className="text-sm font-semibold text-gray-700 dark:text-gray-300">{title}</span>
    </div>
    <div className="rounded-lg border border-gray-200 bg-white divide-y divide-gray-100 dark:border-gray-700 dark:bg-gray-800 dark:divide-gray-700">
      {children}
    </div>
  </div>
);

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch(console.error);
  }, []);

  const showMsg = (type: "ok" | "err", text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 3000);
  };

  const update = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
  };

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await saveSettingsCmd(settings);
      showMsg("ok", "设置已保存");
    } catch (err) {
      showMsg("err", `保存失败：${err}`);
    } finally {
      setSaving(false);
    }
  };

  const handleReset = async () => {
    if (!confirm("确定恢复默认设置？当前配置将丢失。")) return;
    try {
      const defaults = await resetSettings();
      setSettings(defaults);
      showMsg("ok", "已恢复默认设置");
    } catch (err) {
      showMsg("err", `重置失败：${err}`);
    }
  };

  const handleExport = async () => {
    try {
      const path = await save({
        defaultPath: "finalsub-config.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (path) {
        await exportConfigToPath(path);
        showMsg("ok", "配置已导出");
      }
    } catch (err) {
      showMsg("err", `导出失败：${err}`);
    }
  };

  const handleImport = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof selected === "string") {
        const imported = await importConfigFromPath(selected);
        setSettings(imported);
        showMsg("ok", "配置已导入");
      }
    } catch (err) {
      showMsg("err", `导入失败：${err}`);
    }
  };

  const handleSelectModelsPath = async () => {
    const selected = await open({ directory: true });
    if (typeof selected === "string") {
      update("models_path", selected);
    }
  };

  if (!settings) {
    return (
      <div className="max-w-4xl">
        <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-white">设置</h2>
        <p className="text-gray-500">加载中...</p>
      </div>
    );
  }

  return (
    <div className="max-w-4xl">
      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">设置</h2>
        <div className="flex items-center gap-2">
          {message && (
            <span
              className={`text-xs ${message.type === "ok" ? "text-green-600" : "text-red-600"}`}
            >
              {message.text}
            </span>
          )}
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex items-center gap-1.5 rounded-md bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
          >
            <Save size={14} />
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </div>

      <div className="space-y-6">
        {/* 语言设置 */}
        <SettingGroup icon={SettingsIcon} title="语言设置">
          <div className="px-4 py-3">
            <SettingRow label="界面语言" description="切换应用界面语言">
              <select
                value={settings.language}
                onChange={(e) => update("language", e.target.value)}
                className="rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
              >
                {languages.map((l) => (
                  <option key={l.value} value={l.value}>
                    {l.label}
                  </option>
                ))}
              </select>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 模型存储 */}
        <SettingGroup icon={FolderOpen} title="模型存储路径">
          <div className="px-4 py-3">
            <SettingRow label="Whisper 模型路径" description="Whisper 模型文件存储目录">
              <div className="flex items-center gap-2">
                <span className="max-w-[300px] truncate text-xs text-gray-500">
                  {settings.models_path}
                </span>
                <button
                  onClick={handleSelectModelsPath}
                  className="rounded border border-gray-300 px-2 py-0.5 text-xs hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
                >
                  更改
                </button>
              </div>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 任务设置 */}
        <SettingGroup icon={SettingsIcon} title="任务设置">
          <div className="px-4 py-3">
            <SettingRow label="最大并发任务数" description="同时处理的任务数量">
              <input
                type="number"
                min={1}
                max={8}
                value={settings.max_concurrent_tasks}
                onChange={(e) => update("max_concurrent_tasks", Number(e.target.value))}
                className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
              />
            </SettingRow>
            <SettingRow label="字幕输出格式" description="默认字幕文件格式">
              <select
                value={settings.subtitle_output_format}
                onChange={(e) => update("subtitle_output_format", e.target.value)}
                className="rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
              >
                {outputFormats.map((f) => (
                  <option key={f.value} value={f.value}>
                    {f.label}
                  </option>
                ))}
              </select>
            </SettingRow>
            <SettingRow label="默认目标语言" description="翻译任务的默认目标语言">
              <input
                type="text"
                value={settings.target_language}
                onChange={(e) => update("target_language", e.target.value)}
                className="w-32 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </SettingRow>
          </div>
        </SettingGroup>

        {/* VAD 设置 */}
        <SettingGroup icon={SettingsIcon} title="VAD 语音活动检测">
          <div className="px-4 py-3">
            <SettingRow label="启用 VAD" description="过滤音频中的静音片段，提升转录质量">
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={settings.use_vad}
                  onChange={(e) => update("use_vad", e.target.checked)}
                  className="peer sr-only"
                />
                <div className="peer h-5 w-9 rounded-full bg-gray-200 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-blue-600 peer-checked:after:translate-x-full dark:bg-gray-600" />
              </label>
            </SettingRow>
            {settings.use_vad && (
              <>
                <SettingRow label="检测阈值" description="语音活动判断阈值 (0-1)">
                  <input
                    type="number"
                    min={0}
                    max={1}
                    step={0.1}
                    value={settings.vad_threshold}
                    onChange={(e) => update("vad_threshold", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
                <SettingRow label="最小语音时长 (ms)">
                  <input
                    type="number"
                    min={0}
                    value={settings.vad_min_speech_duration_ms}
                    onChange={(e) => update("vad_min_speech_duration_ms", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
                <SettingRow label="最小静音时长 (ms)">
                  <input
                    type="number"
                    min={0}
                    value={settings.vad_min_silence_duration_ms}
                    onChange={(e) => update("vad_min_silence_duration_ms", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
              </>
            )}
          </div>
        </SettingGroup>

        {/* 配置导入导出 */}
        <SettingGroup icon={Download} title="配置导入导出">
          <div className="flex gap-3 px-4 py-3">
            <button
              onClick={handleExport}
              className="flex items-center gap-1.5 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
            >
              <Download size={14} />
              导出配置
            </button>
            <button
              onClick={handleImport}
              className="flex items-center gap-1.5 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
            >
              <Upload size={14} />
              导入配置
            </button>
          </div>
        </SettingGroup>

        {/* 危险操作 */}
        <SettingGroup icon={RotateCcw} title="危险操作">
          <div className="px-4 py-3">
            <SettingRow label="恢复默认设置" description="清除所有自定义配置，恢复出厂默认值">
              <button
                onClick={handleReset}
                className="rounded-md border border-red-300 px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 dark:border-red-700 dark:text-red-400 dark:hover:bg-red-900/20"
              >
                恢复默认
              </button>
            </SettingRow>
          </div>
        </SettingGroup>
      </div>
    </div>
  );
}
