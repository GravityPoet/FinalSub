import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  AlertCircle,
  AudioLines,
  CheckCircle,
  ChevronRight,
  Cloud,
  Cpu,
  Download,
  FileText,
  FileVideo,
  FolderOpen,
  FolderTree,
  Languages,
  Mic,
  Play,
  Sparkles,
} from "lucide-react";
import { useI18n } from "../lib/i18n";
import {
  createTasks,
  createPreviewTask,
  discoverBatchInputs,
  downloadAndInstallUpdate,
  getAppInfo,
  getFfmpegVersion,
  listAsrModels,
  listenDragDrop,
  getSettings,
  checkForUpdate,
  getVideoMetadata,
  openDialog,
  openPath,
  type AppInfo,
  type AppUpdateEvent,
  type AsrModelInfo,
  type TranslationContentMode,
  type UpdateInfo,
  type VideoMetadata,
} from "../lib/tauri";

import { Button } from "../components/ui/Button";
import { Input, Select } from "../components/ui/Input";
import { Card } from "../components/ui/Card";

const mediaExtensions = [
  "mp4", "mkv", "mov", "avi", "webm", "m4v", "mpeg", "mpg", "ts", "m2ts",
  "mp3", "wav", "m4a", "flac", "aac", "ogg", "opus", "wma",
];

const sourceLanguageOptions = [
  { value: "auto", labelKey: "language.auto" },
  { value: "zh", labelKey: "language.zh" },
  { value: "en", labelKey: "language.en" },
  { value: "ja", labelKey: "language.ja" },
  { value: "ko", labelKey: "language.ko" },
  { value: "yue", labelKey: "language.yue" },
] as const;

const taskTypes = [
  { value: "generate-only", labelKey: "home.genOnlyLabel", descKey: "home.genOnlyDesc", icon: FileText },
  { value: "generate-and-translate", labelKey: "home.genTransLabel", descKey: "home.genTransDesc", icon: Mic },
  { value: "translate-only", labelKey: "home.transOnlyLabel", descKey: "home.transOnlyDesc", icon: Languages },
] as const;

const outputFormats = [
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "VTT" },
  { value: "ass", label: "ASS" },
  { value: "lrc", label: "LRC" },
  { value: "txt", label: "TXT" },
];

const targetLanguageOptions = sourceLanguageOptions.filter(({ value }) => value !== "auto");

const engineLabels: Record<string, string> = {
  "whisper-cpp": "Whisper.cpp",
  "parakeet-mlx": "Parakeet Native",
  sensevoice: "SenseVoice",
  paraformer: "Paraformer",
  "qwen3-asr": "Qwen3-ASR",
  "firered-asr": "FireRedASR2",
  "cloud-asr": "Cloud ASR",
  "custom-command": "Custom Command",
};

function sourceLanguagesForEngine(engineId: string) {
  switch (engineId) {
    case "parakeet-mlx":
      return sourceLanguageOptions.filter(({ value }) => value === "auto" || value === "en");
    case "paraformer":
      return sourceLanguageOptions.filter(({ value }) => value === "auto" || value === "zh");
    case "firered-asr":
      return sourceLanguageOptions.filter(({ value }) => ["auto", "zh", "en", "yue"].includes(value));
    default:
      return sourceLanguageOptions;
  }
}

const waveformBars = [26, 44, 68, 38, 82, 56, 92, 64, 42, 72, 48, 30];

const translationContentModes: Array<{
  value: TranslationContentMode;
  labelKey: "home.subtitleContentTargetOnly" | "home.subtitleContentSourceFirst" | "home.subtitleContentTargetFirst";
  descKey: "home.subtitleContentTargetOnlyDesc" | "home.subtitleContentSourceFirstDesc" | "home.subtitleContentTargetFirstDesc";
}> = [
  {
    value: "target-only",
    labelKey: "home.subtitleContentTargetOnly",
    descKey: "home.subtitleContentTargetOnlyDesc",
  },
  {
    value: "source-and-target",
    labelKey: "home.subtitleContentSourceFirst",
    descKey: "home.subtitleContentSourceFirstDesc",
  },
  {
    value: "target-and-source",
    labelKey: "home.subtitleContentTargetFirst",
    descKey: "home.subtitleContentTargetFirstDesc",
  },
];

function isMediaTaskType(value: string): boolean {
  return value !== "translate-only";
}

function fileNameFromPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export default function HomePage() {
  const navigate = useNavigate();
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [ffmpegVersion, setFfmpegVersion] = useState<string>("detecting");
  const [models, setModels] = useState<AsrModelInfo[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string>("");
  const [bootstrapState, setBootstrapState] = useState<"loading" | "ready" | "error">("loading");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateProgress, setUpdateProgress] = useState<AppUpdateEvent | null>(null);
  const [updateError, setUpdateError] = useState("");
  const [mediaMetadata, setMediaMetadata] = useState<VideoMetadata | null>(null);

  const [taskType, setTaskType] = useState("generate-only");
  const [engineId, setEngineId] = useState("parakeet-mlx");
  const [modelId, setModelId] = useState("parakeet-tdt-0.6b-v2");
  const [sourceLanguage, setSourceLanguage] = useState("auto");
  const [targetLanguage, setTargetLanguage] = useState("zh");
  const [translationContentMode, setTranslationContentMode] =
    useState<TranslationContentMode>("target-only");
  const [outputFormat, setOutputFormat] = useState("srt");
  const [outputName, setOutputName] = useState("");
  const [stripChinesePunctuation, setStripChinesePunctuation] = useState(false);
  const [dragActive, setDragActive] = useState(false);

  const { t } = useI18n();
  const selectedPath = selectedPaths[0] ?? "";

  const loadWorkspace = useCallback(async () => {
    setBootstrapState("loading");
    try {
      const [loadedModels, settings] = await Promise.all([listAsrModels(), getSettings()]);
      if (loadedModels.length === 0) {
        throw new Error("No ASR engines are available");
      }

      const configuredEngineExists = loadedModels.some((model) => model.engine_id === settings.asr_engine);
      const parakeetExists = loadedModels.some((model) => model.engine_id === "parakeet-mlx");
      const nextEngineId = configuredEngineExists
        ? settings.asr_engine
        : (parakeetExists ? "parakeet-mlx" : loadedModels[0].engine_id);
      const preferredModelId = nextEngineId === "parakeet-mlx" ? "parakeet-tdt-0.6b-v2" : "";
      const nextModel = loadedModels.find(
        (model) => model.engine_id === nextEngineId && model.id === preferredModelId,
      ) ?? loadedModels.find((model) => model.engine_id === nextEngineId);

      setModels(loadedModels);
      setEngineId(nextEngineId);
      setModelId(nextModel?.id ?? "");
      setTargetLanguage(
        targetLanguageOptions.some(({ value }) => value === settings.target_language)
          ? settings.target_language
          : "zh",
      );
      setSourceLanguage(
        sourceLanguageOptions.some(({ value }) => value === settings.source_language)
          ? settings.source_language
          : "auto",
      );
      setOutputFormat(
        outputFormats.some(({ value }) => value === settings.subtitle_output_format)
          ? settings.subtitle_output_format
          : "srt",
      );
      setBootstrapState("ready");

      if (settings.check_update_on_startup) {
        checkForUpdate()
          .then((update) => {
            if (update) setUpdateInfo(update);
          })
          .catch(console.error);
      }
    } catch (workspaceError) {
      console.error("Failed to initialize workspace:", workspaceError);
      setBootstrapState("error");
    }
  }, []);

  useEffect(() => {
    getAppInfo().then(setAppInfo).catch(console.error);
    getFfmpegVersion().then(setFfmpegVersion).catch(() => setFfmpegVersion("unavailable"));
    void loadWorkspace();
  }, [loadWorkspace]);

  useEffect(() => {
    if (selectedPath) {
      if (taskType !== "translate-only") {
        getVideoMetadata(selectedPath)
          .then(setMediaMetadata)
          .catch((err) => {
            console.error("加载媒体元数据失败:", err);
            setMediaMetadata(null);
          });
      } else {
        setMediaMetadata(null);
      }
    } else {
      setMediaMetadata(null);
    }
  }, [selectedPath, taskType]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    void listenDragDrop((event) => {
      if (event.type === "enter" || event.type === "over") {
        setDragActive(true);
      } else if (event.type === "leave") {
        setDragActive(false);
      } else if (event.type === "drop") {
        setDragActive(false);
        void discoverBatchInputs(event.paths, taskType, true)
          .then(setSelectedPaths)
          .catch((dropError) => setError(String(dropError)));
      }
    }).then((unlisten) => { stop = unlisten; });

    const onPaste = (event: ClipboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, [contenteditable='true']")) return;
      const paths = event.clipboardData?.getData("text/plain")
        .split(/\r?\n/)
        .map((value) => value.trim())
        .filter((value) => value.startsWith("/") || /^[A-Za-z]:[\\/]/.test(value));
      if (!paths?.length) return;
      event.preventDefault();
      void discoverBatchInputs(paths, taskType, true)
        .then(setSelectedPaths)
        .catch((pasteError) => setError(String(pasteError)));
    };
    window.addEventListener("paste", onPaste);
    return () => {
      stop?.();
      window.removeEventListener("paste", onPaste);
    };
  }, [taskType]);

  const engineModels = models.filter((m) => m.engine_id === engineId);
  const engines = [...new Set(models.map((m) => m.engine_id))];

  const taskNeedsAsr = taskType !== "translate-only";
  const availableSourceLanguages = taskNeedsAsr
    ? sourceLanguagesForEngine(engineId)
    : sourceLanguageOptions;
  const sourceLanguageSupported = availableSourceLanguages.some(
    ({ value }) => value === sourceLanguage,
  );
  const activeModel = models.find((m) => m.id === modelId && m.engine_id === engineId);
  const canStartTask = bootstrapState === "ready"
    && (!taskNeedsAsr || sourceLanguageSupported)
    && (!taskNeedsAsr || Boolean(
      activeModel && (engineId === "custom-command" || activeModel.status === "downloaded")
    ));

  useEffect(() => {
    if (!sourceLanguageSupported) {
      setSourceLanguage("auto");
    }
  }, [sourceLanguageSupported]);

  const selectedFileKind = taskType === "translate-only"
    ? t("home.subFile")
    : t("home.mediaFile");
    
  const missingFileHint = !selectedPath
    ? (taskType === "translate-only" ? t("home.prereqSub") : t("home.prereqMedia"))
    : "";
  const modelPrerequisiteHint = selectedPath && !canStartTask ? t("home.prereqModel") : "";

  const handleSelectMedia = async () => {
    setError("");
    try {
      const isTranslateOnly = taskType === "translate-only";
      const selected = await openDialog({
        multiple: true,
        filters: isTranslateOnly
          ? [{ name: t("home.subFile"), extensions: ["srt", "vtt", "ass", "lrc"] }]
          : [{ name: t("home.mediaFile"), extensions: mediaExtensions }],
      });
      const paths = typeof selected === "string" ? [selected] : selected;
      if (paths?.length) {
        setSelectedPaths(await discoverBatchInputs(paths, taskType, true));
      }
    } catch (dialogError) {
      console.error("Failed to open file picker:", dialogError);
      setError(t("home.selectFileFailed"));
    }
  };

  const handleSelectFolder = async () => {
    setError("");
    try {
      const selected = await openDialog({ directory: true, multiple: true });
      const paths = typeof selected === "string" ? [selected] : selected;
      if (paths?.length) {
        setSelectedPaths(await discoverBatchInputs(paths, taskType, true));
      }
    } catch (dialogError) {
      console.error("Failed to scan selected folder:", dialogError);
      setError(t("home.selectFileFailed"));
    }
  };

  const handleCreate = async () => {
    if (!selectedPath) {
      setError(missingFileHint || (taskType === "translate-only" ? t("home.prereqSub") : t("home.prereqMedia")));
      return;
    }
    if (!canStartTask) {
      setError(modelPrerequisiteHint || t("home.prereqModel"));
      return;
    }
    setCreating(true);
    setError("");
    try {
      const requests = selectedPaths.map((mediaPath, index) => {
        const resolvedOutputName = outputName.trim()
          ? outputName.trim().split("{index}").join(String(index + 1).padStart(2, "0"))
          : undefined;
        return {
          task_type: taskType,
          media_path: mediaPath,
          engine_id: engineId,
          model_id: modelId,
          source_language: sourceLanguage,
          target_language: taskType === "generate-only" ? undefined : targetLanguage,
          translation_content_mode:
            taskType === "generate-only" ? undefined : translationContentMode,
          output_format: outputFormat,
          output_name: resolvedOutputName,
          strip_chinese_punctuation: stripChinesePunctuation,
        };
      });
      await createTasks(requests);
      navigate("/tasks");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const handlePreview = async () => {
    if (taskType === "translate-only") {
      setError(t("home.previewOnlyMedia"));
      return;
    }
    if (!selectedPath) {
      setError(t("home.selectMediaPrereq"));
      return;
    }
    if (selectedPaths.length !== 1) {
      setError(t("home.previewSingleOnly"));
      return;
    }
    setCreating(true);
    setError("");
    try {
      await createPreviewTask(selectedPath);
      navigate("/tasks");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const handleUpdate = async () => {
    if (!updateInfo) return;
    setUpdateError("");
    if (!updateInfo.install_supported) {
      try {
        await openPath(updateInfo.url);
      } catch (openError) {
        setUpdateError(openError instanceof Error ? openError.message : String(openError));
      }
      return;
    }

    setUpdateProgress({ phase: "downloading", downloaded_bytes: 0, total_bytes: null });
    try {
      await downloadAndInstallUpdate(updateInfo.latest_version, setUpdateProgress);
    } catch (installError) {
      setUpdateProgress(null);
      setUpdateError(installError instanceof Error ? installError.message : String(installError));
    }
  };

  const activeTaskType = taskTypes.find((item) => item.value === taskType);
  const sourceLanguageLabel = sourceLanguageOptions.find((item) => item.value === sourceLanguage);
  const targetLanguageLabel = sourceLanguageOptions.find((item) => item.value === targetLanguage);
  const pipelineLanguage = taskType === "generate-only"
    ? (sourceLanguageLabel ? t(sourceLanguageLabel.labelKey) : sourceLanguage)
    : `${sourceLanguageLabel ? t(sourceLanguageLabel.labelKey) : sourceLanguage} → ${targetLanguageLabel ? t(targetLanguageLabel.labelKey) : targetLanguage}`;
  const taskReady = Boolean(selectedPath && canStartTask);
  const readinessHint = !selectedPath
    ? missingFileHint
    : (modelPrerequisiteHint || t("home.readyToStart"));
  const workspaceStatus = bootstrapState === "error" || ffmpegVersion === "unavailable"
    ? "error"
    : (bootstrapState === "loading" || ffmpegVersion === "detecting" ? "loading" : "ready");
  const workspaceStatusLabel = workspaceStatus === "ready"
    ? t("home.readyStatus")
    : (workspaceStatus === "loading" ? t("home.loadingWorkspace") : t("home.workspaceNeedsAttention"));

  return (
    <div className="page-shell space-y-6">
      <section className="hero-panel rounded-[1.9rem] px-5 py-6 sm:px-7 sm:py-8">
        <div className="relative z-10 flex items-end justify-between gap-8">
          <div className="max-w-3xl">
            <div className="eyebrow-chip text-[11px] font-bold uppercase tracking-[0.14em]">
              <Sparkles size={13} />
              {t("home.workspaceEyebrow")}
            </div>
            <h2 className="mt-5 max-w-3xl font-display text-[clamp(2rem,4.6vw,3.65rem)] font-bold leading-[1.04] tracking-[-0.048em] text-text-primary">
              {t("home.workspaceTitle")}
            </h2>
            <p className="mt-4 max-w-2xl text-[0.98rem] leading-7 text-text-secondary sm:text-[1.04rem]">
              {t("home.workspaceSubtitle")}
            </p>
            <div className="mt-6 flex flex-wrap items-center gap-2.5">
              <span className="status-chip text-xs font-semibold">
                <span
                  className={`status-dot ${workspaceStatus === "loading" ? "status-dot-pending" : ""} ${workspaceStatus === "error" ? "status-dot-error" : ""}`}
                  aria-hidden="true"
                />
                {workspaceStatusLabel}
              </span>
              {bootstrapState === "error" && (
                <button
                  type="button"
                  onClick={() => void loadWorkspace()}
                  className="status-chip text-xs font-semibold text-brand transition hover:border-brand/35 hover:text-brand"
                >
                  {t("home.retrySetup")}
                </button>
              )}
              <span className="status-chip max-w-full text-xs font-semibold">
                <Cpu size={13} className="shrink-0 text-brand" />
                <span className="truncate">{engineId}</span>
              </span>
            </div>
          </div>

          <div className="liquid-control relative hidden h-32 w-64 shrink-0 items-center justify-center overflow-hidden rounded-[1.6rem] xl:flex" aria-hidden="true">
            <span className="pipeline-glow" />
            <div className="relative z-10 flex h-[4.8rem] items-center gap-1.5">
              {waveformBars.map((height, index) => (
                <span
                  key={`${height}-${index}`}
                  className="w-1.5 rounded-full bg-brand shadow-[0_0_18px_color-mix(in_srgb,var(--color-brand)_36%,transparent)]"
                  style={{ height: `${height}%`, opacity: 0.48 + (index % 4) * 0.13 }}
                />
              ))}
            </div>
            <AudioLines className="absolute bottom-3 right-4 text-brand/45" size={17} />
          </div>
        </div>
      </section>

      {updateInfo && (
        <div className="liquid-control rounded-[1.25rem] p-4">
          <div className="relative z-10 flex flex-col items-start justify-between gap-3 sm:flex-row sm:items-center">
            <div className="flex min-w-0 items-center gap-3">
              <AlertCircle className="shrink-0 text-info" size={18} />
              <div className="min-w-0 text-sm text-text-secondary">
                <span className="font-semibold text-text-primary">{t("home.newVersionAvailable")}{updateInfo.latest_version}！</span>
                {updateInfo.body && (
                  <span className="ml-1 opacity-90">
                    {t("home.updateNotes")}: {updateInfo.body.slice(0, 100)}{updateInfo.body.length > 100 ? "…" : ""}
                  </span>
                )}
              </div>
            </div>
            <Button
              onClick={() => void handleUpdate()}
              variant="primary"
              size="sm"
              disabled={Boolean(updateProgress)}
            >
              <Download size={15} />
              {updateProgress
                ? t(`home.updatePhase.${updateProgress.phase}`)
                : (updateInfo.install_supported ? t("home.installUpdate") : t("home.goDownload"))}
            </Button>
          </div>
          {updateProgress?.phase === "downloading" && updateProgress.total_bytes && (
            <div
              className="relative z-10 mt-3 h-1.5 overflow-hidden rounded-full bg-border/45"
              role="progressbar"
              aria-label={t("home.updateDownloadProgress")}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.min(100, Math.round((updateProgress.downloaded_bytes / updateProgress.total_bytes) * 100))}
            >
              <span
                className="block h-full rounded-full bg-brand transition-[width] duration-150"
                style={{ width: `${Math.min(100, (updateProgress.downloaded_bytes / updateProgress.total_bytes) * 100)}%` }}
              />
            </div>
          )}
          {updateError && (
            <p className="relative z-10 mt-3 text-sm font-medium text-danger" role="alert">
              {updateError}
            </p>
          )}
        </div>
      )}

      <div className="grid items-start gap-5 min-[1120px]:grid-cols-[minmax(0,1fr)_20rem]">
        <div className="space-y-5">
          <Card className="p-6 sm:p-7">
            <div className="mb-5">
              <span className="step-label">01 · {t("home.sourceStep")}</span>
              <h3 className="mt-3 font-display text-[1.42rem] font-bold tracking-[-0.025em] text-text-primary">
                {taskType === "translate-only" ? t("home.selectSubtitleFile") : t("home.selectMediaFile")}
              </h3>
              <p className="mt-1.5 text-sm leading-6 text-text-secondary">
                {taskType === "translate-only" ? t("home.sourceHintSubtitle") : t("home.sourceHintMedia")}
              </p>
            </div>

            <div className={`file-stage rounded-[1.3rem] p-4 transition sm:p-5 ${dragActive ? "ring-2 ring-brand/70 bg-brand/10" : ""}`}>
              <div className="flex flex-wrap items-center gap-4">
                <span className="file-icon">
                  {taskType === "translate-only" ? <FileText size={24} /> : <FileVideo size={24} />}
                </span>
                <div className="min-w-[12rem] flex-[1_1_14rem]">
                  <p className={`${selectedPath ? "truncate" : "leading-6"} font-semibold text-text-primary`}>
                    {selectedPaths.length > 1
                      ? t("home.batchSelected", { count: selectedPaths.length })
                      : (selectedPath ? fileNameFromPath(selectedPath) : `${t("home.noFileSelected")} · ${selectedFileKind}`)}
                  </p>
                  <p className={`mt-1 font-mono text-xs leading-5 text-text-tertiary ${selectedPath ? "truncate" : ""}`} title={selectedPath || undefined}>
                    {selectedPath || t("home.sourcePathHint")}
                  </p>
                </div>
                <div className="ml-auto flex flex-wrap justify-end gap-2">
                  <Button type="button" onClick={handleSelectMedia} variant="secondary" size="sm">
                    <FolderOpen size={14} />
                    {t("home.selectFile")}
                    <ChevronRight size={14} className="opacity-55" />
                  </Button>
                  <Button type="button" onClick={handleSelectFolder} variant="secondary" size="sm">
                    <FolderTree size={14} />
                    {t("home.selectFolder")}
                  </Button>
                </div>
              </div>

              {selectedPaths.length > 1 && (
                <div className="mt-4 border-t border-border-subtle pt-4">
                  <div className="flex flex-wrap gap-2">
                    {selectedPaths.slice(0, 6).map((path) => (
                      <span
                        key={path}
                        title={path}
                        className="max-w-full truncate rounded-lg border border-border-subtle bg-surface-overlay px-2.5 py-1.5 font-mono text-[11px] text-text-secondary"
                      >
                        {fileNameFromPath(path)}
                      </span>
                    ))}
                    {selectedPaths.length > 6 && (
                      <span className="rounded-lg bg-brand/10 px-2.5 py-1.5 text-[11px] font-semibold text-brand">
                        +{selectedPaths.length - 6}
                      </span>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => setSelectedPaths([])}
                    className="mt-3 text-xs font-semibold text-text-tertiary underline-offset-4 hover:text-text-primary hover:underline"
                  >
                    {t("home.clearSelection")}
                  </button>
                </div>
              )}

              {!selectedPath && (
                <p className="mt-4 border-t border-dashed border-border-subtle pt-4 text-center text-xs text-text-tertiary">
                  {dragActive ? t("home.dropNow") : t("home.dragPasteHint")}
                </p>
              )}

              {mediaMetadata && selectedPaths.length === 1 && (
                <div className="mt-5 border-t border-border-subtle pt-4">
                  <p className="mb-3 text-xs font-bold uppercase tracking-[0.12em] text-text-tertiary">{t("home.mediaInfo")}</p>
                  <div className="grid gap-x-6 gap-y-2 text-xs text-text-secondary sm:grid-cols-2 lg:grid-cols-3">
                    <div>{t("home.miDuration")}: <span className="font-medium text-text-primary">{mediaMetadata.duration_string}</span></div>
                    {mediaMetadata.width > 0 && <div>{t("home.miResolution")}: <span className="font-medium text-text-primary">{mediaMetadata.width}×{mediaMetadata.height}</span></div>}
                    {mediaMetadata.fps > 0 && <div>{t("home.miFps")}: <span className="font-medium text-text-primary">{mediaMetadata.fps.toFixed(2)} fps</span></div>}
                    {mediaMetadata.codec !== "unknown" && <div>{t("home.miVideoCodec")}: <span className="font-medium text-text-primary">{mediaMetadata.codec}</span></div>}
                    {mediaMetadata.audio_codec && <div>{t("home.miAudioCodec")}: <span className="font-medium text-text-primary">{mediaMetadata.audio_codec}</span></div>}
                    {mediaMetadata.audio_sample_rate && (
                      <div className={mediaMetadata.audio_sample_rate !== 16000 ? "font-semibold text-warning" : ""}>
                        {t("home.miSampleRate")}: {mediaMetadata.audio_sample_rate} Hz
                      </div>
                    )}
                    {mediaMetadata.audio_channels && <div>{t("home.miChannels")}: <span className="font-medium text-text-primary">{mediaMetadata.audio_channels} ch</span></div>}
                    <div>{t("home.miAudioTracks")}: <span className="font-medium text-text-primary">{mediaMetadata.audio_tracks}</span></div>
                  </div>
                </div>
              )}
            </div>

            {error && (
              <div className="mt-4 flex items-start gap-2 rounded-xl border border-danger/20 bg-danger/10 px-3.5 py-3 text-sm text-danger" role="alert">
                <AlertCircle className="mt-0.5 shrink-0" size={16} />
                <span>{error}</span>
              </div>
            )}
          </Card>

          <Card className="p-6 sm:p-7">
            <div className="mb-6">
              <span className="step-label">02 · {t("home.workflowStep")}</span>
              <h3 className="mt-3 font-display text-[1.42rem] font-bold tracking-[-0.025em] text-text-primary">{t("home.taskConfig")}</h3>
              <p className="mt-1.5 text-sm leading-6 text-text-secondary">{t("home.workflowDesc")}</p>
            </div>

            <div className="space-y-6">
              <fieldset>
                <legend className="mb-2.5 text-xs font-bold uppercase tracking-[0.1em] text-text-tertiary">{t("home.taskType")}</legend>
                <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3" role="radiogroup">
                  {taskTypes.map((item) => {
                    const Icon = item.icon;
                    const isActive = taskType === item.value;
                    return (
                      <button
                        key={item.value}
                        type="button"
                        role="radio"
                        aria-checked={isActive}
                        tabIndex={isActive ? 0 : -1}
                        onKeyDown={(event) => {
                          if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
                          event.preventDefault();
                          const direction = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
                          const currentIndex = taskTypes.findIndex(({ value }) => value === taskType);
                          const nextIndex = (currentIndex + direction + taskTypes.length) % taskTypes.length;
                          const nextTaskType = taskTypes[nextIndex].value;
                          if (isMediaTaskType(taskType) !== isMediaTaskType(nextTaskType)) setSelectedPaths([]);
                          setTaskType(nextTaskType);
                          setError("");
                          const radios = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>("[role='radio']");
                          requestAnimationFrame(() => radios?.[nextIndex]?.focus());
                        }}
                        onClick={() => {
                          const nextTaskType = item.value;
                          if (isMediaTaskType(taskType) !== isMediaTaskType(nextTaskType)) {
                            setSelectedPaths([]);
                          }
                          setTaskType(nextTaskType);
                          setError("");
                        }}
                        className={`flex min-h-24 items-start gap-3 rounded-[1.15rem] border p-3.5 text-left text-sm transition-all duration-200 ${
                          isActive
                            ? "liquid-selected text-text-primary"
                            : "border-border-default bg-surface-overlay/35 text-text-secondary hover:-translate-y-0.5 hover:border-border-strong hover:bg-surface-overlay hover:text-text-primary"
                        }`}
                      >
                        <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-xl ${isActive ? "bg-brand/15 text-brand" : "bg-surface-overlay text-text-tertiary"}`}>
                          <Icon size={16} />
                        </span>
                        <span className="min-w-0">
                          <span className="block font-semibold text-text-primary">{t(item.labelKey)}</span>
                          <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t(item.descKey)}</span>
                        </span>
                      </button>
                    );
                  })}
                </div>
              </fieldset>

              {taskNeedsAsr && (
                <div className="grid grid-cols-1 gap-4 border-t border-border-subtle pt-6 sm:grid-cols-2">
                  <div>
                    <label htmlFor="task-asr-engine" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.asrEngine")}</label>
                    <Select
                      id="task-asr-engine"
                      value={engineId}
                      onChange={(event) => {
                        setEngineId(event.target.value);
                        const first = models.find((model) => model.engine_id === event.target.value);
                        if (first) setModelId(first.id);
                      }}
                    >
                      {engines.map((engine) => (
                        <option key={engine} value={engine}>{engineLabels[engine] ?? engine}</option>
                      ))}
                    </Select>
                  </div>
                  <div>
                    <label htmlFor="task-asr-model" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.asrModel")}</label>
                    <Select id="task-asr-model" value={modelId} onChange={(event) => setModelId(event.target.value)}>
                      {engineModels.map((model) => <option key={model.id} value={model.id}>{model.name}</option>)}
                    </Select>
                  </div>
                  {engineId === "parakeet-mlx" && (
                    <div className="sm:col-span-2 flex items-start gap-2 rounded-xl border border-info/15 bg-info/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                      <Cpu size={14} className="mt-0.5 shrink-0 text-info" />
                      <span>{t("home.parakeetNotice")}</span>
                    </div>
                  )}
                  {["paraformer", "qwen3-asr", "firered-asr"].includes(engineId) && (
                    <div className="sm:col-span-2 flex items-start gap-2 rounded-xl border border-info/15 bg-info/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                      <Cpu size={14} className="mt-0.5 shrink-0 text-info" />
                      <span>{t("home.sherpaNativeNotice")}</span>
                    </div>
                  )}
                  {engineId === "cloud-asr" && (
                    <div className="sm:col-span-2 flex items-start gap-2 rounded-xl border border-brand/15 bg-brand/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                      <Cloud size={14} className="mt-0.5 shrink-0 text-brand" />
                      <span>{t("home.cloudAsrNotice")}</span>
                    </div>
                  )}
                </div>
              )}

              {taskType === "translate-only" && (
                <div className="rounded-xl border border-border-subtle bg-surface-overlay p-3.5 text-sm leading-6 text-text-secondary">
                  {t("home.transOnlyInfo")}
                </div>
              )}

              <div className={`grid grid-cols-1 gap-4 border-t border-border-subtle pt-6 ${taskType !== "generate-only" ? "sm:grid-cols-2" : ""}`}>
                <div>
                  <label htmlFor="task-source-language" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.sourceLang")}</label>
                  <Select id="task-source-language" value={sourceLanguage} onChange={(event) => setSourceLanguage(event.target.value)}>
                    {availableSourceLanguages.map(({ value, labelKey }) => (
                      <option key={value} value={value}>{t(labelKey)} ({value})</option>
                    ))}
                  </Select>
                </div>
                {taskType !== "generate-only" && (
                  <div>
                    <label htmlFor="task-target-language" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.targetLang")}</label>
                    <Select id="task-target-language" value={targetLanguage} onChange={(event) => setTargetLanguage(event.target.value)}>
                      <option value="zh">{t("language.zh")} (zh)</option>
                      <option value="en">{t("language.en")} (en)</option>
                      <option value="ja">{t("language.ja")} (ja)</option>
                      <option value="ko">{t("language.ko")} (ko)</option>
                      <option value="yue">{t("language.yue")} (yue)</option>
                    </Select>
                  </div>
                )}
              </div>

              {taskType !== "generate-only" && (
                <fieldset>
                  <legend className="mb-2.5 text-sm font-medium text-text-secondary">{t("home.subtitleContent")}</legend>
                  <div className="grid gap-2.5 sm:grid-cols-3" role="radiogroup">
                    {translationContentModes.map((mode) => {
                      const isActive = translationContentMode === mode.value;
                      return (
                        <button
                          key={mode.value}
                          type="button"
                          role="radio"
                          aria-checked={isActive}
                          tabIndex={isActive ? 0 : -1}
                          onKeyDown={(event) => {
                            if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
                            event.preventDefault();
                            const direction = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
                            const currentIndex = translationContentModes.findIndex(({ value }) => value === translationContentMode);
                            const nextIndex = (currentIndex + direction + translationContentModes.length) % translationContentModes.length;
                            setTranslationContentMode(translationContentModes[nextIndex].value);
                            const radios = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>("[role='radio']");
                            requestAnimationFrame(() => radios?.[nextIndex]?.focus());
                          }}
                          onClick={() => setTranslationContentMode(mode.value)}
                          className={`min-h-20 rounded-[1.05rem] border px-3.5 py-3 text-left text-sm transition-all duration-200 ${
                            isActive
                              ? "liquid-selected text-text-primary"
                              : "border-border-default bg-surface-overlay/30 text-text-secondary hover:border-border-strong hover:bg-surface-overlay hover:text-text-primary"
                          }`}
                        >
                          <span className="block font-semibold">{t(mode.labelKey)}</span>
                          <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t(mode.descKey)}</span>
                        </button>
                      );
                    })}
                  </div>
                </fieldset>
              )}

              <div className="grid gap-4 border-t border-border-subtle pt-6 sm:grid-cols-2">
                <div>
                  <label htmlFor="task-output-format" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.outputFormat")}</label>
                  <Select id="task-output-format" value={outputFormat} onChange={(event) => setOutputFormat(event.target.value)}>
                    {outputFormats.map((format) => <option key={format.value} value={format.value}>{format.label}</option>)}
                  </Select>
                </div>
                <div>
                  <label htmlFor="task-output-name" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.outputName")}</label>
                  <Input
                    id="task-output-name"
                    value={outputName}
                    maxLength={180}
                    onChange={(event) => setOutputName(event.target.value)}
                    placeholder={t("home.outputNamePlaceholder")}
                  />
                  <p className="mt-1.5 text-xs leading-5 text-text-tertiary">{t("home.outputNameHint")}</p>
                </div>
                <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-border-subtle bg-surface-overlay px-3.5 py-3 text-sm text-text-secondary sm:col-span-2">
                  <input
                    type="checkbox"
                    checked={stripChinesePunctuation}
                    onChange={(event) => setStripChinesePunctuation(event.target.checked)}
                    className="h-4 w-4 rounded border-border-strong accent-brand"
                  />
                  <span>
                    <span className="block font-semibold text-text-primary">{t("home.stripChinesePunctuation")}</span>
                    <span className="mt-0.5 block text-xs text-text-tertiary">{t("home.stripChinesePunctuationHint")}</span>
                  </span>
                </label>
              </div>

              {modelPrerequisiteHint && (
                <div id="task-prerequisite-hint" className="flex flex-wrap items-center gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm text-warning">
                  <AlertCircle size={14} className="shrink-0" />
                  <span>{modelPrerequisiteHint}</span>
                  <button type="button" onClick={() => navigate("/models")} className="font-semibold underline underline-offset-2 hover:text-warning/80">
                    {t("home.openModelManage")}
                  </button>
                </div>
              )}

              {taskNeedsAsr && mediaMetadata?.audio_sample_rate && mediaMetadata.audio_sample_rate !== 16000 && (
                <div className="flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm text-warning">
                  <AlertCircle className="mt-0.5 shrink-0" size={14} />
                  <span>{t("home.resampleHint", { rate: mediaMetadata.audio_sample_rate })}</span>
                </div>
              )}
            </div>
          </Card>
        </div>

        <aside className="space-y-5 min-[1120px]:sticky min-[1120px]:top-0">
          <Card className="relative overflow-hidden p-5 sm:p-6">
            <span className="pipeline-glow" />
            <div className="relative z-10">
              <span className="step-label">03 · {t("home.summaryStep")}</span>
              <h3 className="mt-3 font-display text-h2 font-bold text-text-primary">{t("home.summaryTitle")}</h3>

              <dl className="mt-5">
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summaryTask")}</dt>
                  <dd className="text-right text-sm font-semibold text-text-primary">{activeTaskType ? t(activeTaskType.labelKey) : taskType}</dd>
                </div>
                {taskNeedsAsr && (
                  <div className="system-row">
                    <dt className="text-xs text-text-tertiary">{t("home.summaryEngine")}</dt>
                    <dd className="max-w-[11rem] truncate text-right font-mono text-xs font-semibold text-text-primary" title={engineId}>{engineId}</dd>
                  </div>
                )}
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summaryLanguage")}</dt>
                  <dd className="text-right text-sm font-semibold text-text-primary">{pipelineLanguage}</dd>
                </div>
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summaryFormat")}</dt>
                  <dd className="font-mono text-sm font-bold uppercase text-brand">{outputFormat}</dd>
                </div>
              </dl>

              <div className={`mt-5 flex items-start gap-2.5 rounded-xl border px-3.5 py-3 text-xs leading-5 ${taskReady ? "border-success/18 bg-success/8 text-success" : "border-warning/18 bg-warning/8 text-warning"}`}>
                <span className={`mt-1.5 h-2 w-2 shrink-0 rounded-full ${taskReady ? "bg-success shadow-[0_0_10px_color-mix(in_srgb,var(--color-success)_55%,transparent)]" : "bg-warning"}`} />
                <span>{readinessHint}</span>
              </div>

              <div className="mt-5 grid gap-2.5">
                <Button
                  type="button"
                  onClick={handleCreate}
                  disabled={creating}
                  aria-describedby={modelPrerequisiteHint ? "task-prerequisite-hint" : undefined}
                  title={modelPrerequisiteHint || undefined}
                  variant="primary"
                  size="lg"
                  className="w-full"
                >
                  <Play size={15} />
                  {creating
                    ? t("home.creating")
                    : (selectedPaths.length > 1
                      ? t("home.batchCreateTask", { count: selectedPaths.length })
                      : t("home.createTask"))}
                </Button>
                {taskType !== "translate-only" && (
                  <Button type="button" onClick={handlePreview} disabled={creating} variant="secondary" className="w-full">
                    {t("home.createPreview")}
                  </Button>
                )}
              </div>
            </div>
          </Card>

          <Card className="h-fit p-5 sm:p-6">
            <span className="step-label">{t("home.systemStep")}</span>
            <h3 className="mt-3 font-display text-h2 font-bold text-text-primary">{t("home.appInfo")}</h3>
            <dl className="mt-4">
              <div className="system-row">
                <dt className="text-xs text-text-tertiary">{t("home.appName")}</dt>
                <dd className="min-w-0 break-all text-right font-mono text-xs font-semibold text-text-primary">{appInfo?.name ?? t("home.loading")}</dd>
              </div>
              <div className="system-row">
                <dt className="text-xs text-text-tertiary">{t("home.version")}</dt>
                <dd className="min-w-0 text-right font-mono text-xs font-semibold text-text-primary">{appInfo?.version ?? t("home.loading")}</dd>
              </div>
              <div className="system-row">
                <dt className="text-xs text-text-tertiary">FFmpeg</dt>
                <dd className="min-w-0 text-right text-xs text-text-primary">
                  {ffmpegVersion === "detecting" ? (
                    <span className="text-text-tertiary">{t("home.detecting")}</span>
                  ) : ffmpegVersion === "unavailable" ? (
                    <span className="inline-flex items-center gap-1 font-semibold text-danger">
                      <AlertCircle size={12} />
                      {t("home.unavailable")}
                    </span>
                  ) : (
                    <span className="inline-flex items-center gap-1 font-semibold text-success" title={ffmpegVersion}>
                      <CheckCircle className="shrink-0" size={12} />
                      {t("home.available")}
                    </span>
                  )}
                </dd>
              </div>
            </dl>
          </Card>
        </aside>
      </div>
    </div>
  );
}
