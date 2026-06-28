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

import { Button } from "../components/ui/Button";
import { Input, Select } from "../components/ui/Input";
import { Card } from "../components/ui/Card";

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

function clampInteger(value: unknown, min: number, max: number): number {
  const numeric = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(numeric)) {
    return min;
  }
  return Math.min(max, Math.max(min, Math.trunc(numeric)));
}

function normalizeMaxContext(value: unknown): number {
  const numeric = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(numeric) || numeric < 0) {
    return -1;
  }
  return clampInteger(numeric, 0, 65_536);
}

function normalizeSettings(settings: Settings): Settings {
  return {
    ...settings,
    max_concurrent_tasks: clampInteger(settings.max_concurrent_tasks, 1, 8),
    max_context: normalizeMaxContext(settings.max_context),
  };
}

const SettingRow = ({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) => (
  <div className="flex items-center justify-between gap-4 py-3.5">
    <div className="min-w-0">
      <div className="text-sm font-semibold text-text-primary">{label}</div>
      {description && (
        <div className="mt-1 text-xs text-text-tertiary leading-4">{description}</div>
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
  <div className="space-y-2">
    <div className="flex items-center gap-2 mb-2 px-1">
      <Icon className="size-4 text-text-secondary" />
      <span className="text-sm font-semibold text-text-secondary">{title}</span>
    </div>
    <Card className="p-0 bg-surface divide-y divide-border-subtle overflow-hidden">
      {children}
    </Card>
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
      const normalized = normalizeSettings(settings);
      const savedSettings = await saveSettingsCmd(normalized);
      setSettings(savedSettings);
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
      <div className="max-w-4xl space-y-6">
        <h2 className="text-display font-bold tracking-tight text-text-primary">{t("settings.title")}</h2>
        <p className="text-text-tertiary text-sm">{t("home.loading")}</p>
      </div>
    );
  }

  return (
    <div className="max-w-4xl space-y-6 pb-12">
      <div className="flex items-center justify-between">
        <h2 className="text-display font-bold tracking-tight text-text-primary">{t("settings.title")}</h2>
        <div className="flex items-center gap-3.5">
          {message && (
            <span
              className={`text-xs font-semibold ${message.type === "ok" ? "text-success" : "text-danger"}`}
            >
              {message.text}
            </span>
          )}
          <Button
            onClick={handleSave}
            disabled={saving}
            variant="primary"
            className="h-9"
          >
            <Save size={14} />
            <span>{saving ? t("settings.saving") : t("common.save")}</span>
          </Button>
        </div>
      </div>

      <div className="space-y-6">
        {/* 语言设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.langGroup")}>
          <div className="px-5">
            <SettingRow label={t("settings.langLabel")} description={t("settings.langDesc")}>
              <Select
                value={settings.language}
                onChange={(e) => update("language", e.target.value)}
                className="w-32 h-9 py-1"
              >
                {languages.map((l) => (
                  <option key={l.value} value={l.value}>
                    {t(l.label)}
                  </option>
                ))}
              </Select>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 模型存储 */}
        <SettingGroup icon={FolderOpen} title={t("settings.modelStorageGroup")}>
          <div className="px-5">
            <SettingRow label={t("settings.modelStorageLabel")} description={t("settings.modelStorageDesc")}>
              <div className="flex items-center gap-3">
                <span className="max-w-[300px] truncate text-xs text-text-secondary font-mono bg-surface-overlay px-2.5 py-1.5 rounded-lg border border-border-subtle">
                  {settings.models_path}
                </span>
                <Button
                  onClick={handleSelectModelsPath}
                  variant="secondary"
                  size="sm"
                  className="h-8 py-0"
                >
                  {t("settings.change")}
                </Button>
              </div>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 任务设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.taskGroup")}>
          <div className="px-5">
            <SettingRow label={t("settings.concurrentLabel")} description={t("settings.concurrentDesc")}>
              <Input
                type="number"
                min={1}
                max={8}
                value={settings.max_concurrent_tasks}
                onChange={(e) =>
                  update("max_concurrent_tasks", clampInteger(e.target.value, 1, 8))
                }
                className="w-20 text-right h-9"
              />
            </SettingRow>
            <SettingRow label={t("settings.outputLabel")} description={t("settings.outputDesc")}>
              <Select
                value={settings.subtitle_output_format}
                onChange={(e) => update("subtitle_output_format", e.target.value)}
                className="w-28 h-9 py-1"
              >
                {outputFormats.map((f) => (
                  <option key={f.value} value={f.value}>
                    {f.label}
                  </option>
                ))}
              </Select>
            </SettingRow>
            <SettingRow
              label={t("settings.defaultTargetLanguageLabel")}
              description={t("settings.defaultTargetLanguageDesc")}
            >
              <Input
                type="text"
                value={settings.target_language}
                onChange={(e) => update("target_language", e.target.value)}
                className="w-32 h-9"
              />
            </SettingRow>
          </div>
        </SettingGroup>

        {/* VAD 设置（whisper-cpp 与自定义命令支持） */}
        {(settings.asr_engine === "whisper-cpp" || settings.asr_engine === "custom-command") && (
          <SettingGroup icon={SettingsIcon} title={t("settings.vadGroup")}>
            <div className="px-5 py-2 space-y-1">
              <div className="my-3 rounded-lg bg-brand-subtle border border-brand/10 p-3 text-xs text-brand-text leading-5">
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
                  <div className="peer h-5 w-9 rounded-full bg-border-strong after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-brand peer-checked:after:translate-x-full" />
                </label>
              </SettingRow>
              {settings.use_vad && (
                <div className="divide-y divide-border-subtle">
                  <SettingRow label={t("settings.vadThresholdLabel")} description={t("settings.vadThresholdDesc")}>
                    <Input
                      type="number"
                      min={0}
                      max={1}
                      step={0.1}
                      value={settings.vad_threshold}
                      onChange={(e) => update("vad_threshold", Number(e.target.value))}
                      className="w-24 text-right h-9"
                    />
                  </SettingRow>
                  <SettingRow label={t("settings.vadMinSpeechLabel")} description={t("settings.vadMinSpeechDesc")}>
                    <Input
                      type="number"
                      min={0}
                      value={settings.vad_min_speech_duration_ms}
                      onChange={(e) => update("vad_min_speech_duration_ms", Number(e.target.value))}
                      className="w-24 text-right h-9"
                    />
                  </SettingRow>
                  <SettingRow label={t("settings.vadMinSilenceLabel")} description={t("settings.vadMinSilenceDesc")}>
                    <Input
                      type="number"
                      min={0}
                      value={settings.vad_min_silence_duration_ms}
                      onChange={(e) => update("vad_min_silence_duration_ms", Number(e.target.value))}
                      className="w-24 text-right h-9"
                    />
                  </SettingRow>
                  <SettingRow label={t("settings.vadMaxSpeechLabel")} description={t("settings.vadMaxSpeechDesc")}>
                    <Input
                      type="number"
                      min={0}
                      max={3600}
                      value={settings.vad_max_speech_duration_s}
                      onChange={(e) => update("vad_max_speech_duration_s", Number(e.target.value))}
                      className="w-24 text-right h-9"
                    />
                  </SettingRow>
                  <SettingRow label={t("settings.vadSpeechPadLabel")} description={t("settings.vadSpeechPadDesc")}>
                    <Input
                      type="number"
                      min={0}
                      max={5000}
                      value={settings.vad_speech_pad_ms}
                      onChange={(e) => update("vad_speech_pad_ms", Number(e.target.value))}
                      className="w-24 text-right h-9"
                    />
                  </SettingRow>
                  <SettingRow label={t("settings.vadSamplesOverlapLabel")} description={t("settings.vadSamplesOverlapDesc")}>
                    <Input
                      type="number"
                      min={0}
                      max={1}
                      step={0.05}
                      value={settings.vad_samples_overlap}
                      onChange={(e) => update("vad_samples_overlap", Number(e.target.value))}
                      className="w-24 text-right h-9"
                    />
                  </SettingRow>
                </div>
              )}
            </div>
          </SettingGroup>
        )}

        {/* 通用与更新设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.updateGroup")}>
          <div className="px-5">
            <SettingRow label={t("settings.updateLabel")} description={t("settings.updateDesc")}>
              <label className="relative inline-flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={settings.check_update_on_startup}
                  onChange={(e) => update("check_update_on_startup", e.target.checked)}
                  className="peer sr-only"
                />
                <div className="peer h-5 w-9 rounded-full bg-border-strong after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-brand peer-checked:after:translate-x-full" />
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
                <div className="peer h-5 w-9 rounded-full bg-border-strong after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-brand peer-checked:after:translate-x-full" />
              </label>
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 转录高级设置 */}
        <SettingGroup icon={SettingsIcon} title={t("settings.advancedGroup")}>
          <div className="px-5">
            <SettingRow
              label={t("settings.whisperCommandLabel")}
              description={t("settings.whisperCommandDesc")}
            >
              <div className="flex gap-2 w-[400px]">
                <Input
                  type="text"
                  placeholder={t("settings.whisperCommandPlaceholder")}
                  value={settings.whisper_command}
                  onChange={(e) => update("whisper_command", e.target.value)}
                  className="flex-1 h-9"
                />
                <Button
                  onClick={async () => {
                    const selected = await open({
                      multiple: false,
                      directory: false,
                    });
                    if (typeof selected === "string") {
                      update("whisper_command", selected);
                    }
                  }}
                  variant="secondary"
                  className="h-9 px-3 text-xs"
                >
                  {t("common.browse")}
                </Button>
              </div>
            </SettingRow>
            <SettingRow
              label={t("settings.maxContextLabel")}
              description={t("settings.maxContextDesc")}
            >
              <Input
                type="number"
                min={0}
                max={65536}
                placeholder={t("settings.maxContextPlaceholder")}
                value={settings.max_context === -1 ? "" : settings.max_context}
                onChange={(e) => {
                  const rawValue = e.target.value.trim();
                  update(
                    "max_context",
                    rawValue === "" ? -1 : normalizeMaxContext(rawValue)
                  );
                }}
                className="w-28 text-right h-9"
              />
            </SettingRow>
          </div>
        </SettingGroup>

        {/* 配置导入导出 */}
        <SettingGroup icon={Download} title={t("settings.importExportGroup")}>
          <div className="flex gap-3 px-5 py-3.5">
            <Button
              onClick={handleExport}
              variant="secondary"
              size="sm"
            >
              <Download size={13} />
              <span>{t("settings.export")}</span>
            </Button>
            <Button
              onClick={handleImport}
              variant="secondary"
              size="sm"
            >
              <Upload size={13} />
              <span>{t("settings.import")}</span>
            </Button>
          </div>
        </SettingGroup>

        {/* 危险操作 */}
        <SettingGroup icon={RotateCcw} title={t("settings.dangerGroup")}>
          <div className="px-5">
            <SettingRow label={t("settings.resetLabel")} description={t("settings.resetDesc")}>
              <Button
                onClick={() => setConfirmReset(true)}
                variant="danger"
                size="sm"
              >
                {t("settings.reset")}
              </Button>
            </SettingRow>
          </div>
        </SettingGroup>
      </div>

      {confirmReset && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <Card className="w-full max-w-md bg-surface-overlay p-6 shadow-lg border border-border-default animate-fade-in">
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-danger/10 p-2 text-danger">
                <AlertCircle size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-text-primary text-h2 mb-1.5">{t("settings.resetConfirmTitle")}</h3>
                <p className="text-xs text-text-secondary leading-5">
                  {t("settings.resetConfirmDesc")}
                </p>
              </div>
            </div>
            <div className="flex justify-end gap-2.5">
              <Button
                type="button"
                onClick={() => setConfirmReset(false)}
                variant="secondary"
                size="sm"
              >
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                onClick={handleReset}
                variant="danger"
                size="sm"
              >
                {t("settings.reset")}
              </Button>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}
