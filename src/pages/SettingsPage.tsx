import { useEffect, useState } from "react";
import { Settings as SettingsIcon, Save, RotateCcw, Download, Upload, FolderOpen, AlertCircle, LockKeyhole, Palette, Sun, Moon, Laptop, RefreshCw, BatteryCharging } from "lucide-react";
import { type TranslationKey, useI18n } from "../lib/i18n";
import { type Theme, useTheme } from "../lib/theme";
import {
  getSettings,
  getPowerSaveStatus,
  saveSettingsCmd,
  resetSettings,
  exportConfigToPath,
  importConfigFromPath,
  exportEncryptedConfigToPath,
  importEncryptedConfigFromPath,
  checkForUpdate,
  downloadAndInstallUpdate,
  openDialog,
  openPath,
  saveDialog,
  type AppUpdateEvent,
  type Settings,
  type PowerSaveStatus,
  type UpdateInfo,
} from "../lib/tauri";

import { Button } from "../components/ui/Button";
import { Input, Select } from "../components/ui/Input";
import { Card } from "../components/ui/Card";

const languages = [
  { value: "zh", label: "language.zh" },
  { value: "en", label: "language.en" },
  { value: "ja", label: "language.ja" },
] as const;

const outputFormats = [
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "VTT" },
  { value: "ass", label: "ASS" },
  { value: "lrc", label: "LRC" },
  { value: "txt", label: "TXT" },
];

const themeOptions: Array<{
  value: Theme;
  labelKey: TranslationKey;
  icon: typeof Sun;
}> = [
  { value: "light", labelKey: "settings.themeLight", icon: Sun },
  { value: "dark", labelKey: "settings.themeDark", icon: Moon },
  { value: "system", labelKey: "settings.themeSystem", icon: Laptop },
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
  <div className="flex flex-col gap-3 py-4 sm:flex-row sm:items-center sm:justify-between sm:gap-5">
    <div className="min-w-0">
      <div className="text-base font-semibold text-text-primary">{label}</div>
      {description && (
        <div className="mt-1 text-sm leading-5 text-text-tertiary">{description}</div>
      )}
    </div>
    <div className="w-full sm:w-auto sm:shrink-0">{children}</div>
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
  <div className="space-y-2.5">
    <div className="mb-2 flex items-center gap-2 px-1">
      <Icon className="size-5 text-text-secondary" />
      <span className="font-display text-h3 font-semibold text-text-secondary">{title}</span>
    </div>
    <Card className="divide-y divide-border-subtle overflow-hidden bg-surface p-0">
      {children}
    </Card>
  </div>
);

export default function SettingsPage() {
  const { t } = useI18n();
  const { theme, setTheme } = useTheme();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const [cryptoDialog, setCryptoDialog] = useState<{ mode: "export" | "import"; path?: string } | null>(null);
  const [configPassphrase, setConfigPassphrase] = useState("");
  const [message, setMessage] = useState<{ type: "ok" | "err"; text: string } | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [availableUpdate, setAvailableUpdate] = useState<UpdateInfo | null>(null);
  const [updateProgress, setUpdateProgress] = useState<AppUpdateEvent | null>(null);
  const [powerSaveStatus, setPowerSaveStatus] = useState<PowerSaveStatus | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch(console.error);
  }, []);

  useEffect(() => {
    let mounted = true;
    const refresh = () => {
      getPowerSaveStatus()
        .then((status) => {
          if (mounted) setPowerSaveStatus(status);
        })
        .catch(console.error);
    };
    refresh();
    const timer = window.setInterval(refresh, 1_500);
    return () => {
      mounted = false;
      window.clearInterval(timer);
    };
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
      const path = await saveDialog({
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
      const selected = await openDialog({
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

  const openEncryptedImport = async () => {
    try {
      const selected = await openDialog({
        multiple: false,
        filters: [{ name: "Encrypted JSON", extensions: ["json"] }],
      });
      if (typeof selected === "string") {
        setConfigPassphrase("");
        setCryptoDialog({ mode: "import", path: selected });
      }
    } catch (err) {
      showMsg("err", `${t("settings.importFailed")}${err}`);
    }
  };

  const handleEncryptedConfig = async () => {
    if (!cryptoDialog || configPassphrase.length < 8) return;
    try {
      if (cryptoDialog.mode === "export") {
        const path = await saveDialog({
          defaultPath: "finalsub-config.encrypted.json",
          filters: [{ name: "Encrypted JSON", extensions: ["json"] }],
        });
        if (!path) return;
        await exportEncryptedConfigToPath(path, configPassphrase);
        showMsg("ok", t("settings.encryptedExported"));
      } else if (cryptoDialog.path) {
        const imported = await importEncryptedConfigFromPath(
          cryptoDialog.path,
          configPassphrase,
        );
        setSettings(imported);
        showMsg("ok", t("settings.encryptedImported"));
        window.dispatchEvent(new CustomEvent("settings-changed"));
      }
      setCryptoDialog(null);
      setConfigPassphrase("");
    } catch (err) {
      showMsg("err", `${t("settings.encryptedConfigFailed")}${err}`);
    }
  };

  const handleSelectModelsPath = async () => {
    const selected = await openDialog({ directory: true });
    if (typeof selected === "string") {
      update("models_path", selected);
    }
  };

  const handleSelectParakeetModelsPath = async () => {
    const selected = await openDialog({ directory: true });
    if (typeof selected === "string") {
      update("parakeet_models_path", selected);
    }
  };

  const handleCheckUpdate = async () => {
    setCheckingUpdate(true);
    try {
      const updateInfo = await checkForUpdate();
      setAvailableUpdate(updateInfo);
      if (!updateInfo) {
        showMsg("ok", t("settings.upToDate"));
      }
    } catch (err) {
      showMsg("err", `${t("settings.updateCheckFailed")}${err}`);
    } finally {
      setCheckingUpdate(false);
    }
  };

  const handleInstallUpdate = async () => {
    if (!availableUpdate) return;
    if (!availableUpdate.install_supported) {
      try {
        await openPath(availableUpdate.url);
      } catch (err) {
        showMsg("err", `${t("settings.updateOpenFailed")}${err}`);
      }
      return;
    }

    setUpdateProgress({ phase: "downloading", downloaded_bytes: 0, total_bytes: null });
    try {
      await downloadAndInstallUpdate(availableUpdate.latest_version, setUpdateProgress);
    } catch (err) {
      setUpdateProgress(null);
      showMsg("err", `${t("settings.updateInstallFailed")}${err}`);
    }
  };

  if (!settings) {
    return (
      <div className="page-shell space-y-6">
        <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{t("settings.title")}</h2>
        <p className="text-text-tertiary text-sm">{t("home.loading")}</p>
      </div>
    );
  }

  return (
    <div className="page-shell space-y-7 pb-12">
      <div className="flex items-center justify-between">
        <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{t("settings.title")}</h2>
        <div className="flex items-center gap-3.5">
          {message && (
            <span
              className={`text-sm font-semibold ${message.type === "ok" ? "text-success" : "text-danger"}`}
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
        <SettingGroup icon={Palette} title={t("settings.theme")}>
          <div className="p-4 sm:p-5">
            <div className="liquid-control grid w-full grid-cols-3 rounded-[1rem] p-1 sm:max-w-sm">
              {themeOptions.map(({ value, labelKey, icon: Icon }) => {
                const isActive = theme === value;
                const label = t(labelKey);
                return (
                  <button
                    key={value}
                    type="button"
                    aria-pressed={isActive}
                    onClick={() => setTheme(value)}
                    className={`flex h-10 min-w-0 items-center justify-center gap-2 rounded-[0.75rem] px-3 text-sm font-semibold transition ${
                      isActive ? "theme-selected text-brand" : "text-text-tertiary hover:text-text-secondary"
                    }`}
                  >
                    <Icon size={16} className="shrink-0" />
                    <span className="truncate">{label}</span>
                  </button>
                );
              })}
            </div>
          </div>
        </SettingGroup>

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
              <div className="flex w-full flex-col gap-2 sm:w-auto sm:flex-row sm:items-center sm:gap-3">
                <span className="min-w-0 max-w-full truncate rounded-xl border border-border-subtle bg-surface-overlay px-3 py-2 font-mono text-sm text-text-secondary sm:max-w-[420px]">
                  {settings.models_path}
                </span>
                <Button
                  onClick={handleSelectModelsPath}
                  variant="secondary"
                  size="sm"
                >
                  {t("settings.change")}
                </Button>
              </div>
            </SettingRow>
            <SettingRow
              label={t("settings.parakeetStorageLabel")}
              description={t("settings.parakeetStorageDesc")}
            >
              <div className="flex w-full flex-col gap-2 sm:w-auto sm:flex-row sm:items-center sm:gap-3">
                <span className="min-w-0 max-w-full truncate rounded-xl border border-border-subtle bg-surface-overlay px-3 py-2 font-mono text-sm text-text-secondary sm:max-w-[420px]">
                  {settings.parakeet_models_path}
                </span>
                <Button
                  onClick={handleSelectParakeetModelsPath}
                  variant="secondary"
                  size="sm"
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
            <SettingRow
              label={t("settings.preventSleepLabel")}
              description={t("settings.preventSleepDesc")}
            >
              <div className="flex flex-col items-start gap-2 sm:items-end">
                <label className="relative inline-flex cursor-pointer items-center">
                  <input
                    type="checkbox"
                    role="switch"
                    aria-label={t("settings.preventSleepLabel")}
                    checked={settings.prevent_sleep_during_tasks}
                    onChange={(e) => update("prevent_sleep_during_tasks", e.target.checked)}
                    className="peer absolute inset-0 z-10 h-full w-full cursor-pointer opacity-0"
                    data-testid="prevent-sleep-toggle"
                  />
                  <div className="pointer-events-none h-5 w-9 rounded-full bg-border-strong ring-offset-2 after:absolute after:left-[2px] after:top-[2px] after:h-4 after:w-4 after:rounded-full after:bg-white after:transition-all peer-checked:bg-brand peer-checked:after:translate-x-full peer-focus-visible:ring-2 peer-focus-visible:ring-brand/40" />
                </label>
                <span
                  className={`inline-flex items-center gap-1.5 text-xs font-medium ${
                    powerSaveStatus?.last_error
                      ? "text-warning"
                      : powerSaveStatus?.active
                        ? "text-success"
                        : "text-text-tertiary"
                  }`}
                  role="status"
                  data-testid="prevent-sleep-status"
                >
                  <BatteryCharging size={13} />
                  {powerSaveStatus?.last_error
                    ? t("settings.preventSleepError")
                    : powerSaveStatus?.active
                      ? t("settings.preventSleepActive", { count: powerSaveStatus.active_count })
                      : powerSaveStatus?.enabled
                        ? t("settings.preventSleepReady")
                        : t("settings.preventSleepOff")}
                </span>
              </div>
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
              <div className="my-3 rounded-xl border border-brand/10 bg-brand-subtle p-3.5 text-sm leading-6 text-brand-text">
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
            <SettingRow
              label={t("settings.checkNowLabel")}
              description={availableUpdate
                ? `${t("settings.updateAvailable")} v${availableUpdate.latest_version}`
                : t("settings.checkNowDesc")}
            >
              <Button
                type="button"
                variant={availableUpdate ? "primary" : "secondary"}
                size="sm"
                disabled={checkingUpdate || Boolean(updateProgress)}
                onClick={() => void (availableUpdate ? handleInstallUpdate() : handleCheckUpdate())}
              >
                {availableUpdate ? <Download size={14} /> : <RefreshCw size={14} />}
                {checkingUpdate
                  ? t("settings.checkingUpdate")
                  : updateProgress
                    ? t(`home.updatePhase.${updateProgress.phase}`)
                    : availableUpdate
                      ? (availableUpdate.install_supported ? t("home.installUpdate") : t("home.goDownload"))
                      : t("settings.checkNow")}
              </Button>
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
              <div className="flex w-full flex-col gap-2 sm:w-[400px] sm:flex-row">
                <Input
                  type="text"
                  placeholder={t("settings.whisperCommandPlaceholder")}
                  value={settings.whisper_command}
                  onChange={(e) => update("whisper_command", e.target.value)}
                  className="min-w-0 flex-1 h-9"
                />
                <Button
                  onClick={async () => {
                    const selected = await openDialog({
                      multiple: false,
                      directory: false,
                    });
                    if (typeof selected === "string") {
                      update("whisper_command", selected);
                    }
                  }}
                  variant="secondary"
                  size="sm"
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
          <div className="flex flex-wrap gap-3 px-5 py-3.5">
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
            <Button
              onClick={() => {
                setConfigPassphrase("");
                setCryptoDialog({ mode: "export" });
              }}
              variant="secondary"
              size="sm"
            >
              <LockKeyhole size={13} />
              <span>{t("settings.encryptedExport")}</span>
            </Button>
            <Button
              onClick={openEncryptedImport}
              variant="secondary"
              size="sm"
            >
              <LockKeyhole size={13} />
              <span>{t("settings.encryptedImport")}</span>
            </Button>
          </div>
          <p className="px-5 pb-4 text-xs leading-5 text-text-tertiary">{t("settings.encryptedConfigHint")}</p>
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
                <p className="text-sm leading-6 text-text-secondary">
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

      {cryptoDialog && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4 backdrop-blur-sm">
          <Card className="w-full max-w-md border border-border-default bg-surface-overlay p-6 shadow-lg">
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-brand/10 p-2 text-brand">
                <LockKeyhole size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="mb-1.5 font-display text-h2 font-semibold text-text-primary">
                  {cryptoDialog.mode === "export"
                    ? t("settings.encryptedExportTitle")
                    : t("settings.encryptedImportTitle")}
                </h3>
                <p className="text-sm leading-6 text-text-secondary">{t("settings.encryptedPassphraseDesc")}</p>
              </div>
            </div>
            <label htmlFor="config-passphrase" className="mb-2 block text-sm font-semibold text-text-secondary">
              {t("settings.encryptedPassphrase")}
            </label>
            <Input
              id="config-passphrase"
              type="password"
              autoFocus
              minLength={8}
              value={configPassphrase}
              onChange={(event) => setConfigPassphrase(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && configPassphrase.length >= 8) {
                  void handleEncryptedConfig();
                }
              }}
            />
            {configPassphrase.length > 0 && configPassphrase.length < 8 && (
              <p className="mt-2 text-xs text-warning">{t("settings.encryptedPassphraseMin")}</p>
            )}
            <div className="mt-5 flex justify-end gap-2.5">
              <Button
                type="button"
                onClick={() => {
                  setCryptoDialog(null);
                  setConfigPassphrase("");
                }}
                variant="secondary"
                size="sm"
              >
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                onClick={() => void handleEncryptedConfig()}
                disabled={configPassphrase.length < 8}
                variant="primary"
                size="sm"
              >
                {cryptoDialog.mode === "export" ? t("settings.encryptedExport") : t("settings.encryptedImport")}
              </Button>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}
