import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useNavigate } from "react-router-dom";
import { AlertCircle, CheckCircle, FileText, FileVideo, FolderOpen, Languages, Mic, Play } from "lucide-react";
import { useI18n } from "../lib/i18n";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  createTask,
  createPreviewTask,
  getAppInfo,
  getFfmpegVersion,
  listAsrModels,
  getSettings,
  checkForUpdate,
  getVideoMetadata,
  type AppInfo,
  type AsrModelInfo,
  type TranslationContentMode,
  type UpdateInfo,
  type VideoMetadata,
} from "../lib/tauri";

import { Button } from "../components/ui/Button";
import { Select } from "../components/ui/Input";
import { Card } from "../components/ui/Card";

const mediaExtensions = ["mp4", "mov", "mkv", "webm", "mp3", "wav", "m4a", "aac", "flac"];

const taskTypes = [
  { value: "generate-only", icon: FileText },
  { value: "generate-and-translate", icon: Mic },
  { value: "translate-only", icon: Languages },
];

const outputFormats = [
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "VTT" },
  { value: "ass", label: "ASS" },
  { value: "lrc", label: "LRC" },
  { value: "txt", label: "TXT" },
];

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
  const [selectedPath, setSelectedPath] = useState<string>("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string>("");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [mediaMetadata, setMediaMetadata] = useState<VideoMetadata | null>(null);

  const [taskType, setTaskType] = useState("generate-only");
  const [engineId, setEngineId] = useState("parakeet-mlx");
  const [modelId, setModelId] = useState("parakeet-tdt-0.6b-v2");
  const [sourceLanguage, setSourceLanguage] = useState("auto");
  const [targetLanguage, setTargetLanguage] = useState("zh");
  const [translationContentMode, setTranslationContentMode] =
    useState<TranslationContentMode>("target-only");
  const [outputFormat, setOutputFormat] = useState("srt");

  const { t } = useI18n();

  useEffect(() => {
    getAppInfo().then(setAppInfo).catch(console.error);
    getFfmpegVersion().then(setFfmpegVersion).catch(() => setFfmpegVersion("unavailable"));
    listAsrModels().then(setModels).catch(console.error);
    getSettings()
      .then((s) => {
        const nextEngineId = s.asr_engine || "parakeet-mlx";
        setEngineId(nextEngineId);
        if (nextEngineId === "parakeet-mlx") {
          setModelId("parakeet-tdt-0.6b-v2");
        }
        setTargetLanguage(s.target_language);
        setSourceLanguage(s.source_language || "auto");
        setOutputFormat(s.subtitle_output_format);
        
        if (s.check_update_on_startup) {
          checkForUpdate()
            .then((update) => {
              if (update) {
                setUpdateInfo(update);
              }
            })
            .catch(console.error);
        }
      })
      .catch(console.error);
  }, []);

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

  const engineModels = models.filter((m) => m.engine_id === engineId);
  const engines = [...new Set(models.map((m) => m.engine_id))];

  const taskNeedsAsr = taskType !== "translate-only";
  const activeModel = models.find((m) => m.id === modelId && m.engine_id === engineId);
  const canStartTask = !taskNeedsAsr || Boolean(
    activeModel && (
      (engineId !== "whisper-cpp" && engineId !== "sensevoice") ||
      activeModel.status === "downloaded"
    )
  );

  const selectedFileKind = taskType === "translate-only"
    ? t("home.subFile")
    : t("home.mediaFile");
    
  const prerequisiteHint = !selectedPath
    ? (taskType === "translate-only" ? t("home.prereqSub") : t("home.prereqMedia"))
    : !canStartTask
      ? t("home.prereqModel")
      : "";

  const getTaskTypeLabel = (val: string) => {
    switch (val) {
      case "generate-only": return t("home.genOnlyLabel");
      case "generate-and-translate": return t("home.genTransLabel");
      case "translate-only": return t("home.transOnlyLabel");
      default: return val;
    }
  };

  const handleSelectMedia = async () => {
    setError("");
    const isTranslateOnly = taskType === "translate-only";
    const selected = await open({
      multiple: false,
      filters: isTranslateOnly
        ? [{ name: t("home.subFile"), extensions: ["srt", "vtt", "ass", "lrc"] }]
        : [{ name: t("home.mediaFile"), extensions: mediaExtensions }],
    });
    if (typeof selected === "string") {
      setSelectedPath(selected);
    }
  };

  const handleCreate = async () => {
    if (!selectedPath) {
      setError(prerequisiteHint || (taskType === "translate-only" ? t("home.prereqSub") : t("home.prereqMedia")));
      return;
    }
    if (!canStartTask) {
      setError(prerequisiteHint || t("home.prereqModel"));
      return;
    }
    setCreating(true);
    setError("");
    try {
      await createTask({
        task_type: taskType,
        media_path: selectedPath,
        engine_id: engineId,
        model_id: modelId,
        source_language: sourceLanguage,
        target_language: taskType === "generate-only" ? undefined : targetLanguage,
        translation_content_mode:
          taskType === "generate-only" ? undefined : translationContentMode,
        output_format: outputFormat,
      });
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

  return (
    <div className="max-w-5xl space-y-6">
      <h2 className="text-display font-bold tracking-tight text-text-primary">{t("home.title")}</h2>

      {updateInfo && (
        <div className="flex items-center justify-between rounded-xl bg-info/10 border border-info/20 p-4">
          <div className="flex items-center gap-3">
            <AlertCircle className="text-info shrink-0" size={18} />
            <div className="text-sm text-text-secondary">
              <span className="font-semibold text-text-primary">{t("home.newVersionAvailable")}{updateInfo.latest_version}！</span>
              {updateInfo.body && <span className="ml-1 opacity-90">{t("home.updateNotes")}: {updateInfo.body.slice(0, 100)}...</span>}
            </div>
          </div>
          <Button
            onClick={() => {
              openPath(updateInfo.url);
            }}
            variant="primary"
            size="sm"
          >
            {t("home.goDownload")}
          </Button>
        </div>
      )}

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
        <div className="space-y-6">
          {/* 文件选择 */}
          <Card className="p-5">
            <div className="mb-4 flex items-center gap-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-brand-subtle text-brand">
                <FileVideo size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-text-primary text-h2">{t("home.newTask")}</h3>
                <p className="truncate text-xs text-text-secondary mt-0.5">
                  {selectedPath ? fileNameFromPath(selectedPath) : `${t("home.noFileSelected")} (${selectedFileKind})`}
                </p>
              </div>
            </div>

            {selectedPath && (
              <div className="mb-4 rounded-lg border border-border-subtle bg-surface-overlay px-3 py-2.5">
                <p className="truncate font-mono text-xs text-text-secondary" title={selectedPath}>
                  {selectedPath}
                </p>
                {mediaMetadata && (
                  <div className="mt-2.5 border-t border-border-default pt-2.5 text-xs text-text-secondary space-y-1">
                    <div className="font-semibold text-text-primary mb-1">{t("home.mediaInfo")}:</div>
                    <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
                      <div>{t("home.miDuration")}: {mediaMetadata.duration_string} ({mediaMetadata.duration_seconds.toFixed(1)}s)</div>
                      {mediaMetadata.width > 0 && <div>{t("home.miResolution")}: {mediaMetadata.width}x{mediaMetadata.height}</div>}
                      {mediaMetadata.fps > 0 && <div>{t("home.miFps")}: {mediaMetadata.fps.toFixed(2)} fps</div>}
                      {mediaMetadata.codec !== "unknown" && <div>{t("home.miVideoCodec")}: {mediaMetadata.codec}</div>}
                      {mediaMetadata.audio_codec && <div>{t("home.miAudioCodec")}: {mediaMetadata.audio_codec}</div>}
                      {mediaMetadata.audio_sample_rate && (
                        <div className={mediaMetadata.audio_sample_rate !== 16000 ? "text-warning font-semibold" : ""}>
                          {t("home.miSampleRate")}: {mediaMetadata.audio_sample_rate} Hz {mediaMetadata.audio_sample_rate !== 16000 && ` (${t("home.miNot16k")})`}
                        </div>
                      )}
                      {mediaMetadata.audio_channels && <div>{t("home.miChannels")}: {mediaMetadata.audio_channels} ch</div>}
                      <div>{t("home.miAudioTracks")}: {mediaMetadata.audio_tracks}</div>
                    </div>
                  </div>
                )}
              </div>
            )}

            {error && (
              <div className="mb-4 flex items-start gap-2 rounded-lg border border-danger/20 bg-danger/10 px-3 py-2 text-sm text-danger">
                <AlertCircle className="mt-0.5 shrink-0" size={16} />
                <span>{error}</span>
              </div>
            )}

            <Button
              type="button"
              onClick={handleSelectMedia}
              variant="secondary"
              size="sm"
            >
              <FolderOpen size={14} />
              {taskType === "translate-only" ? t("home.selectSubtitleFile") : t("home.selectMediaFile")}
            </Button>
          </Card>

          {/* 任务配置 */}
          <Card className="p-5">
            <h3 className="mb-4 font-semibold text-text-primary text-h2">{t("home.taskConfig")}</h3>

            {/* 任务类型 */}
            <div className="mb-4">
              <label className="mb-2 block text-sm font-medium text-text-secondary">{t("home.taskType")}</label>
              <div className="grid grid-cols-3 gap-2.5">
                {taskTypes.map((t) => {
                  const Icon = t.icon;
                  return (
                    <button
                      key={t.value}
                      onClick={() => {
                        const nextTaskType = t.value;
                        if (isMediaTaskType(taskType) !== isMediaTaskType(nextTaskType)) {
                          setSelectedPath("");
                        }
                        setTaskType(nextTaskType);
                        setError("");
                      }}
                      className={`flex flex-col items-center gap-1.5 rounded-lg border p-3.5 text-xs transition-all duration-150 ${
                        taskType === t.value
                          ? "border-brand bg-brand-subtle text-brand-text shadow-sm font-semibold"
                          : "border-border-default text-text-secondary hover:border-border-strong hover:bg-surface-overlay hover:text-text-primary"
                      }`}
                    >
                      <Icon size={16} />
                      <span>{getTaskTypeLabel(t.value)}</span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* 引擎 */}
            <div className="mb-4 grid grid-cols-2 gap-4">
              <div>
                <label className="mb-1 block text-sm font-medium text-text-secondary">{t("home.asrEngine")}</label>
                <Select
                  value={engineId}
                  onChange={(e) => {
                    setEngineId(e.target.value);
                    const first = models.find((m) => m.engine_id === e.target.value);
                    if (first) setModelId(first.id);
                  }}
                >
                  {engines.map((e) => (
                    <option key={e} value={e}>{e}</option>
                  ))}
                </Select>
              </div>
              <div>
                <label className="mb-1 block text-sm font-medium text-text-secondary">{t("home.asrModel")}</label>
                <Select
                  value={modelId}
                  onChange={(e) => setModelId(e.target.value)}
                >
                  {engineModels.map((m) => (
                    <option key={m.id} value={m.id}>{m.name}</option>
                  ))}
                </Select>
              </div>
            </div>

            {taskType === "translate-only" && (
              <div className="mb-4 rounded-lg bg-surface-overlay border border-border-subtle p-3 text-xs text-text-secondary leading-5">
                {t("home.transOnlyInfo")}
              </div>
            )}

            {/* 语言 */}
            <div className="mb-4 grid grid-cols-2 gap-4">
              <div>
                <label className="mb-1 block text-sm font-medium text-text-secondary">{t("home.sourceLang")}</label>
                <Select
                  value={sourceLanguage}
                  onChange={(e) => setSourceLanguage(e.target.value)}
                >
                  <option value="auto">{t("language.auto")} (auto)</option>
                  <option value="zh">{t("language.zh")} (zh)</option>
                  <option value="en">{t("language.en")} (en)</option>
                  <option value="ja">{t("language.ja")} (ja)</option>
                  <option value="ko">{t("language.ko")} (ko)</option>
                  <option value="yue">{t("language.yue")} (yue)</option>
                </Select>
              </div>
              <div>
                <label className="mb-1 block text-sm font-medium text-text-secondary">{t("home.targetLang")}</label>
                <Select
                  value={targetLanguage}
                  onChange={(e) => setTargetLanguage(e.target.value)}
                >
                  <option value="zh">{t("language.zh")} (zh)</option>
                  <option value="en">{t("language.en")} (en)</option>
                  <option value="ja">{t("language.ja")} (ja)</option>
                  <option value="ko">{t("language.ko")} (ko)</option>
                  <option value="yue">{t("language.yue")} (yue)</option>
                </Select>
              </div>
            </div>

            {taskType !== "generate-only" && (
              <div className="mb-4">
                <label className="mb-2 block text-sm font-medium text-text-secondary">{t("home.subtitleContent")}</label>
                <div className="grid gap-2.5 sm:grid-cols-3">
                  {translationContentModes.map((mode) => (
                    <button
                      key={mode.value}
                      type="button"
                      aria-pressed={translationContentMode === mode.value}
                      onClick={() => setTranslationContentMode(mode.value)}
                      className={`min-h-[4.75rem] rounded-lg border px-3 py-2.5 text-left text-xs transition-all duration-150 ${
                        translationContentMode === mode.value
                          ? "border-brand bg-brand-subtle text-brand-text shadow-sm"
                          : "border-border-default text-text-secondary hover:border-border-strong hover:bg-surface-overlay hover:text-text-primary"
                      }`}
                    >
                      <span className="block font-semibold">{t(mode.labelKey)}</span>
                      <span className="mt-1 block leading-5 text-text-tertiary">
                        {t(mode.descKey)}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* 输出格式 */}
            <div className="mb-4">
              <label className="mb-1 block text-sm font-medium text-text-secondary">{t("home.outputFormat")}</label>
              <Select
                value={outputFormat}
                onChange={(e) => setOutputFormat(e.target.value)}
              >
                {outputFormats.map((f) => (
                  <option key={f.value} value={f.value}>{f.label}</option>
                ))}
              </Select>
            </div>

            {engineId === "parakeet-mlx" && (
              <div className="mb-4 text-xs text-brand-text bg-brand-subtle/40 border border-brand/10 p-3 rounded-lg leading-5">
                {t("home.parakeetNotice")}
              </div>
            )}

            {prerequisiteHint && (
              <div
                id="task-prerequisite-hint"
                className="mb-4 flex flex-wrap items-center gap-2 rounded-lg border border-warning/20 bg-warning/10 px-3 py-2.5 text-xs text-warning"
              >
                <AlertCircle size={14} className="shrink-0" />
                <span>{prerequisiteHint}</span>
                {!selectedPath ? (
                  <button
                    type="button"
                    onClick={handleSelectMedia}
                    className="font-medium underline hover:text-warning/80"
                  >
                    {t("home.selectModelNow")}
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => navigate("/models")}
                    className="font-medium underline hover:text-warning/80"
                  >
                    {t("home.openModelManage")}
                  </button>
                )}
              </div>
            )}

            {taskNeedsAsr && mediaMetadata && mediaMetadata.audio_sample_rate && mediaMetadata.audio_sample_rate !== 16000 && (
              <div className="mb-4 flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/10 px-3 py-2.5 text-xs text-warning">
                <AlertCircle className="mt-0.5 shrink-0" size={14} />
                <span>
                  {t("home.resampleHint", { rate: mediaMetadata.audio_sample_rate })}
                </span>
              </div>
            )}

            {/* 操作按钮 */}
            <div className="flex flex-wrap gap-3 mt-5">
              <Button
                type="button"
                onClick={handleCreate}
                disabled={creating}
                aria-describedby={prerequisiteHint ? "task-prerequisite-hint" : undefined}
                title={prerequisiteHint || undefined}
                variant="primary"
              >
                <Play size={14} />
                {creating ? t("home.creating") : t("home.createTask")}
              </Button>
              <Button
                type="button"
                onClick={handlePreview}
                disabled={creating}
                variant="secondary"
                title={taskType === "translate-only" ? t("home.previewOnlyMedia") : undefined}
              >
                {t("home.createPreview")}
              </Button>
            </div>
          </Card>
        </div>

        {/* 系统信息 */}
        <Card className="p-5 h-fit">
          <h3 className="mb-4 font-semibold text-text-primary text-h2">{t("home.appInfo")}</h3>
          <dl className="space-y-3.5">
            <div className="flex justify-between items-center gap-3">
              <dt className="text-xs text-text-secondary">{t("home.appName")}</dt>
              <dd className="min-w-0 break-all text-right font-mono text-xs text-text-primary">
                {appInfo?.name ?? t("home.loading")}
              </dd>
            </div>
            <div className="flex justify-between items-center gap-3">
              <dt className="text-xs text-text-secondary">{t("home.version")}</dt>
              <dd className="min-w-0 text-right font-mono text-xs text-text-primary">
                {appInfo?.version ?? t("home.loading")}
              </dd>
            </div>
            <div className="flex justify-between items-center gap-3">
              <dt className="text-xs text-text-secondary">FFmpeg</dt>
              <dd className="min-w-0 text-right text-xs text-text-primary">
                {ffmpegVersion === "detecting" ? (
                  <span className="text-text-tertiary">{t("home.detecting")}</span>
                ) : ffmpegVersion === "unavailable" ? (
                  <span className="inline-flex items-center gap-1 text-danger font-medium">
                    <AlertCircle size={12} />
                    {t("home.unavailable")}
                  </span>
                ) : (
                  <span
                    className="inline-flex items-center gap-1 text-success font-medium"
                    title={ffmpegVersion}
                  >
                    <CheckCircle className="shrink-0" size={12} />
                    {t("home.available")}
                  </span>
                )}
              </dd>
            </div>
          </dl>
        </Card>
      </div>
    </div>
  );
}
