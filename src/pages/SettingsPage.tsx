import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Settings as SettingsIcon, Save, RotateCcw, Download, Upload, FolderOpen, AlertCircle } from "lucide-react";
import { useI18n } from "../lib/i18n";
import {
  getSettings,
  saveSettingsCmd,
  resetSettings,
  exportConfigToPath,
  importConfigFromPath,
  type Settings,
} from "../lib/tauri";

const languages = [
  { value: "zh", label: "language.zh" },
  { value: "en", label: "language.en" },
] as const;

const outputFormats = [
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "VTT" },
  { value: "ass", label: "ASS" },
  { value: "lrc", label: "LRC" },
  { value: "txt", label: "TXT" },
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
  const { t } = useI18n();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
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
      showMsg("ok", t("settings.saved"));
      window.dispatchEvent(new CustomEvent("settings-changed"));
    } catch (err) {
      showMsg("err", `${t("settings.saveFailed")}${err}`);
    } finally {
      setSaving(false);
    }
  };

  const handleReset = async () => {
    try {
      const defaults = await resetSettings();
      setSettings(defaults);
      setConfirmReset(false);
      showMsg("ok", t("settings.restored"));
      window.dispatchEvent(new CustomEvent("settings-changed"));
    } catch (err) {
      showMsg("err", `${t("settings.resetFailed")}${err}`);
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
        showMsg("ok", t("settings.exported"));
      }
    } catch (err) {
      showMsg("err", `${t("settings.exportFailed")}${err}`);
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
        showMsg("ok", t("settings.imported"));
        window.dispatchEvent(new CustomEvent("settings-changed"));
      }
    } catch (err) {
      showMsg("err", `${t("settings.importFailed")}${err}`);
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
        <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-white">{t("settings.title")}</h2>
        <p className="text-gray-500">{t("home.loading")}</p>
      </div>
    );
  }

  return (
    <div className="max-w-4xl">
      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">{t("settings.title")}</h2>
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
            {saving ? t("settings.saving") : t("common.save")}
          </button>
        </div>
      </div>

      <div className="space-y-6">
        {/* 语言设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.langGroup")}>
          <div className="px-4 py-3">
            <SettingRow label={t("settings.langLabel")} description={t("settings.langDesc")}>
              <select
                value={settings.language}
                onChange={(e) => update("language", e.target.value)}
                className="rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
              >
                {languages.map((l) => (
                  <option key={l.value} value={l.value}>
                    {t(l.label)}
                  </option>
                ))}
              </select>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 模型存储 */}
        <SettingGroup icon={FolderOpen} title={t("settings.modelStorageGroup")}>
          <div className="px-4 py-3">
            <SettingRow label={t("settings.modelStorageLabel")} description={t("settings.modelStorageDesc")}>
              <div className="flex items-center gap-2">
                <span className="max-w-[300px] truncate text-xs text-gray-500">
                  {settings.models_path}
                </span>
                <button
                  onClick={handleSelectModelsPath}
                  className="rounded border border-gray-300 px-2 py-0.5 text-xs hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
                >
                  {t("settings.change")}
                </button>
              </div>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 任务设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.taskGroup")}>
          <div className="px-4 py-3">
            <SettingRow label={t("settings.concurrentLabel")} description={t("settings.concurrentDesc")}>
              <input
                type="number"
                min={1}
                max={8}
                value={settings.max_concurrent_tasks}
                onChange={(e) => update("max_concurrent_tasks", Number(e.target.value))}
                className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
              />
            </SettingRow>
            <SettingRow label={t("settings.outputLabel")} description={t("settings.outputDesc")}>
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
            <SettingRow
              label={t("settings.defaultTargetLanguageLabel")}
              description={t("settings.defaultTargetLanguageDesc")}
            >
              <input
                type="text"
                value={settings.target_language}
                onChange={(e) => update("target_language", e.target.value)}
                className="w-32 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
              />
            </SettingRow>
          </div>
        </SettingGroup>

        {/* VAD 设置（仅 whisper-cpp 引擎支持，parakeet-mlx 无 VAD，故按引擎隐藏） */}
        {settings.asr_engine === "whisper-cpp" && (
        <SettingGroup icon={SettingsIcon} title={t("settings.vadGroup")}>
          <div className="px-4 py-3">
            <div className="mb-3 rounded-lg bg-blue-50/50 p-3 text-xs text-blue-700 dark:bg-blue-950/20 dark:text-blue-300">
              {t("settings.vadGroupDesc")}
            </div>
            <SettingRow label={t("settings.useVadLabel")} description={t("settings.useVadDesc")}>
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
                <SettingRow label={t("settings.vadThresholdLabel")} description={t("settings.vadThresholdDesc")}>
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
                <SettingRow label={t("settings.vadMinSpeechLabel")} description={t("settings.vadMinSpeechDesc")}>
                  <input
                    type="number"
                    min={0}
                    value={settings.vad_min_speech_duration_ms}
                    onChange={(e) => update("vad_min_speech_duration_ms", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
                <SettingRow label={t("settings.vadMinSilenceLabel")} description={t("settings.vadMinSilenceDesc")}>
                  <input
                    type="number"
                    min={0}
                    value={settings.vad_min_silence_duration_ms}
                    onChange={(e) => update("vad_min_silence_duration_ms", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
                <SettingRow label={t("settings.vadMaxSpeechLabel")} description={t("settings.vadMaxSpeechDesc")}>
                  <input
                    type="number"
                    min={0}
                    max={3600}
                    value={settings.vad_max_speech_duration_s}
                    onChange={(e) => update("vad_max_speech_duration_s", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
                <SettingRow label={t("settings.vadSpeechPadLabel")} description={t("settings.vadSpeechPadDesc")}>
                  <input
                    type="number"
                    min={0}
                    max={5000}
                    value={settings.vad_speech_pad_ms}
                    onChange={(e) => update("vad_speech_pad_ms", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
                <SettingRow label={t("settings.vadSamplesOverlapLabel")} description={t("settings.vadSamplesOverlapDesc")}>
                  <input
                    type="number"
                    min={0}
                    max={1}
                    step={0.05}
                    value={settings.vad_samples_overlap}
                    onChange={(e) => update("vad_samples_overlap", Number(e.target.value))}
                    className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
                  />
                </SettingRow>
              </>
            )}
          </div>
        </SettingGroup>
        )}

        {/* 通用与更新设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.updateGroup")}>
          <div className="px-4 py-3">
            <SettingRow label={t("settings.updateLabel")} description={t("settings.updateDesc")}>
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={settings.check_update_on_startup}
                  onChange={(e) => update("check_update_on_startup", e.target.checked)}
                  className="peer sr-only"
                />
                <div className="peer h-5 w-9 rounded-full bg-gray-200 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-blue-600 peer-checked:after:translate-x-full dark:bg-gray-600" />
              </label>
            </SettingRow>
            <SettingRow label={t("settings.telemetryLabel")} description={t("settings.telemetryDesc")}>
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={settings.enable_telemetry}
                  onChange={(e) => update("enable_telemetry", e.target.checked)}
                  className="peer sr-only"
                />
                <div className="peer h-5 w-9 rounded-full bg-gray-200 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-blue-600 peer-checked:after:translate-x-full dark:bg-gray-600" />
              </label>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 转录高级设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.advancedGroup")}>
          <div className="px-4 py-3">
            <SettingRow
              label={t("settings.whisperCommandLabel")}
              description={t("settings.whisperCommandDesc")}
            >
              <div className="flex gap-2">
                <input
                  type="text"
                  placeholder={t("settings.whisperCommandPlaceholder")}
                  value={settings.whisper_command}
                  onChange={(e) => update("whisper_command", e.target.value)}
                  className="w-80 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
                />
                <button
                  onClick={async () => {
                    const selected = await open({
                      multiple: false,
                      directory: false,
                    });
                    if (typeof selected === "string") {
                      update("whisper_command", selected);
                    }
                  }}
                  className="rounded-md border border-gray-300 px-2.5 py-1 text-xs hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
                >
                  {t("common.browse")}
                </button>
              </div>
            </SettingRow>
            <SettingRow
              label={t("settings.maxContextLabel")}
              description={t("settings.maxContextDesc")}
            >
              <input
                type="number"
                min={-1}
                max={65536}
                value={settings.max_context}
                onChange={(e) => update("max_context", Number(e.target.value))}
                className="w-20 rounded-md border border-gray-300 bg-white px-2.5 py-1 text-sm text-right dark:border-gray-600 dark:bg-gray-700"
              />
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 配置导入导出 */}
        <SettingGroup icon={Download} title={t("settings.importExportGroup")}>
          <div className="flex gap-3 px-4 py-3">
            <button
              onClick={handleExport}
              className="flex items-center gap-1.5 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
            >
              <Download size={14} />
              {t("settings.export")}
            </button>
            <button
              onClick={handleImport}
              className="flex items-center gap-1.5 rounded-md border border-gray-300 px-3 py-1.5 text-sm hover:bg-gray-50 dark:border-gray-600 dark:hover:bg-gray-700"
            >
              <Upload size={14} />
              {t("settings.import")}
            </button>
          </div>
        </SettingGroup>

        {/* 危险操作 */}
        <SettingGroup icon={RotateCcw} title={t("settings.dangerGroup")}>
          <div className="px-4 py-3">
            <SettingRow label={t("settings.resetLabel")} description={t("settings.resetDesc")}>
              <button
                onClick={() => setConfirmReset(true)}
                className="rounded-md border border-red-300 px-3 py-1.5 text-sm text-red-600 hover:bg-red-50 dark:border-red-700 dark:text-red-400 dark:hover:bg-red-900/20"
              >
                {t("settings.reset")}
              </button>
            </SettingRow>
          </div>
        </SettingGroup>
      </div>

      {confirmReset && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 p-4">
          <div className="w-full max-w-md rounded-lg border border-gray-200 bg-white p-5 shadow-xl dark:border-gray-700 dark:bg-gray-800">
            <div className="mb-4 flex items-start gap-3">
              <div className="rounded-full bg-red-50 p-2 text-red-600 dark:bg-red-950/40 dark:text-red-300">
                <AlertCircle size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-gray-900 dark:text-white">{t("settings.resetConfirmTitle")}</h3>
                <p className="mt-1 text-sm text-gray-600 dark:text-gray-300">
                  {t("settings.resetConfirmDesc")}
                </p>
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmReset(false)}
                className="rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
              >
                {t("common.cancel")}
              </button>
              <button
                type="button"
                onClick={handleReset}
                className="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700"
              >
                {t("settings.reset")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
