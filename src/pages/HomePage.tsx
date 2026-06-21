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
  type AppInfo,
  type AsrModelInfo,
  type UpdateInfo,
} from "../lib/tauri";

const mediaExtensions = ["mp4", "mov", "mkv", "webm", "mp3", "wav", "m4a", "aac", "flac"];

const taskTypes = [
  { value: "generate-only", label: "仅生成字幕", icon: FileText, desc: "从音视频生成字幕文件" },
  { value: "generate-and-translate", label: "生成并翻译", icon: Mic, desc: "生成字幕并翻译为目标语言" },
  { value: "translate-only", label: "仅翻译", icon: Languages, desc: "翻译已有字幕文件" },
];

const outputFormats = [
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "VTT" },
  { value: "ass", label: "ASS" },
  { value: "lrc", label: "LRC" },
  { value: "txt", label: "TXT" },
];

function fileNameFromPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export default function HomePage() {
  const navigate = useNavigate();
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [ffmpegVersion, setFfmpegVersion] = useState<string>("检测中...");
  const [models, setModels] = useState<AsrModelInfo[]>([]);
  const [selectedPath, setSelectedPath] = useState<string>("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string>("");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);

  const [taskType, setTaskType] = useState("generate-only");
  const [engineId, setEngineId] = useState("parakeet-mlx");
  const [modelId, setModelId] = useState("parakeet-tdt-0.6b-v2");
  const [sourceLanguage, setSourceLanguage] = useState("auto");
  const [targetLanguage, setTargetLanguage] = useState("zh");
  const [outputFormat, setOutputFormat] = useState("srt");

  useEffect(() => {
    getAppInfo().then(setAppInfo).catch(console.error);
    getFfmpegVersion().then(setFfmpegVersion).catch(() => setFfmpegVersion("未找到"));
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

  const engineModels = models.filter((m) => m.engine_id === engineId);
  const engines = [...new Set(models.map((m) => m.engine_id))].filter(
    (id) => id !== "sensevoice" && id !== "custom-command"
  );

  const taskNeedsAsr = taskType !== "translate-only";
  const activeModel = models.find((m) => m.id === modelId && m.engine_id === engineId);
  const canStartTask = !taskNeedsAsr || Boolean(activeModel && (engineId !== "whisper-cpp" || activeModel.status === "downloaded"));
  const { t, locale } = useI18n();

  const selectedFileKind = taskType === "translate-only"
    ? t("home.subFile", "字幕文件")
    : t("home.mediaFile", "音视频文件");
    
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
        ? [{ name: "字幕文件", extensions: ["srt"] }]
        : [{ name: "音视频文件", extensions: mediaExtensions }],
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
      setError("快速预览仅支持音视频任务；仅翻译请使用“开始任务”。");
      return;
    }
    if (!selectedPath) {
      setError("请先选择音视频文件。");
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
    <div className="max-w-5xl">
      <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-white">任务</h2>

      {updateInfo && (
        <div className="mb-6 flex items-center justify-between rounded-lg bg-blue-50 border border-blue-200 p-4 dark:bg-blue-950/20 dark:border-blue-900/30">
          <div className="flex items-center gap-3">
            <AlertCircle className="text-blue-600 dark:text-blue-400 shrink-0" size={18} />
            <div className="text-sm text-blue-700 dark:text-blue-300">
              <span className="font-semibold">发现新版本 v{updateInfo.latest_version}！</span>
              {updateInfo.body && <span className="ml-1 opacity-90">更新说明: {updateInfo.body.slice(0, 100)}...</span>}
            </div>
          </div>
          <button
            onClick={() => {
              openPath(updateInfo.url);
            }}
            className="rounded bg-blue-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-blue-700 transition shrink-0"
          >
            前往下载
          </button>
        </div>
      )}

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
        <div className="space-y-5">
          {/* 文件选择 */}
          <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
            <div className="mb-4 flex items-center gap-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300">
                <FileVideo size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-gray-900 dark:text-white">{t("home.newTask")}</h3>
                <p className="truncate text-sm text-gray-500">
                  {selectedPath ? fileNameFromPath(selectedPath) : (locale === "en" ? `No ${selectedFileKind} selected` : `未选择${selectedFileKind}`)}
                </p>
              </div>
            </div>

            {selectedPath && (
              <div className="mb-4 rounded-md border border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-900/60">
                <p className="truncate font-mono text-xs text-gray-600 dark:text-gray-300" title={selectedPath}>
                  {selectedPath}
                </p>
              </div>
            )}

            {error && (
              <div className="mb-4 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
                <AlertCircle className="mt-0.5 shrink-0" size={16} />
                <span>{error}</span>
              </div>
            )}

            <button
              type="button"
              onClick={handleSelectMedia}
              className="inline-flex items-center gap-2 rounded-md border border-gray-300 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
            >
              <FolderOpen size={16} />
              {taskType === "translate-only" ? (locale === "en" ? "Select Subtitle File" : "选择字幕文件") : (locale === "en" ? "Select Media" : "选择音视频")}
            </button>
          </section>

          {/* 任务配置 */}
          <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
            <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">{locale === "en" ? "Task Configuration" : "任务配置"}</h3>

            {/* 任务类型 */}
            <div className="mb-4">
              <label className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">{t("home.taskType")}</label>
              <div className="grid grid-cols-3 gap-2">
                {taskTypes.map((t) => {
                  const Icon = t.icon;
                  return (
                    <button
                      key={t.value}
                      onClick={() => {
                        setTaskType(t.value);
                        setSelectedPath("");
                        setError("");
                      }}
                      className={`flex flex-col items-center gap-1 rounded-lg border p-3 text-xs transition ${
                        taskType === t.value
                          ? "border-blue-500 bg-blue-50 text-blue-700 dark:border-blue-400 dark:bg-blue-900/30 dark:text-blue-300"
                          : "border-gray-200 text-gray-600 hover:border-gray-300 dark:border-gray-600 dark:text-gray-400"
                      }`}
                    >
                      <Icon size={16} />
                      <span className="font-medium">{getTaskTypeLabel(t.value)}</span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* 引擎 */}
            <div className="mb-4 grid grid-cols-2 gap-4">
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">{t("home.asrEngine")}</label>
                <select
                  value={engineId}
                  onChange={(e) => {
                    setEngineId(e.target.value);
                    const first = models.find((m) => m.engine_id === e.target.value);
                    if (first) setModelId(first.id);
                  }}
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  {engines.map((e) => (
                    <option key={e} value={e}>{e}</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">{t("home.asrModel")}</label>
                <select
                  value={modelId}
                  onChange={(e) => setModelId(e.target.value)}
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  {engineModels.map((m) => (
                    <option key={m.id} value={m.id}>{m.name}</option>
                  ))}
                </select>
              </div>
            </div>

            {taskType === "translate-only" && (
              <div className="mb-4 rounded-md bg-gray-50 p-2.5 text-xs text-gray-600 dark:bg-gray-900/50 dark:text-gray-300">
                {locale === "en" ? "Translate-only mode reads the selected SRT subtitle file, and does not use ASR engine or model." : "仅翻译模式会读取已选择的 SRT 字幕文件，不使用 ASR 引擎或模型。"}
              </div>
            )}

            {/* 语言 */}
            <div className="mb-4 grid grid-cols-2 gap-4">
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">{t("home.sourceLang")}</label>
                <select
                  value={sourceLanguage}
                  onChange={(e) => setSourceLanguage(e.target.value)}
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  <option value="auto">{locale === "en" ? "Auto Detect" : "自动检测"} (auto)</option>
                  <option value="zh">{locale === "en" ? "Chinese" : "中文"} (zh)</option>
                  <option value="en">{locale === "en" ? "English" : "英文"} (en)</option>
                  <option value="ja">{locale === "en" ? "Japanese" : "日文"} (ja)</option>
                  <option value="ko">{locale === "en" ? "Korean" : "韩文"} (ko)</option>
                  <option value="yue">{locale === "en" ? "Cantonese" : "粤语"} (yue)</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">{t("home.targetLang")}</label>
                <select
                  value={targetLanguage}
                  onChange={(e) => setTargetLanguage(e.target.value)}
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  <option value="zh">{locale === "en" ? "Chinese" : "中文"} (zh)</option>
                  <option value="en">{locale === "en" ? "English" : "英文"} (en)</option>
                  <option value="ja">{locale === "en" ? "Japanese" : "日文"} (ja)</option>
                  <option value="ko">{locale === "en" ? "Korean" : "韩文"} (ko)</option>
                  <option value="yue">{locale === "en" ? "Cantonese" : "粤语"} (yue)</option>
                </select>
              </div>
            </div>

            {/* 输出格式 */}
            <div className="mb-4">
              <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">{t("home.outputFormat")}</label>
              <select
                value={outputFormat}
                onChange={(e) => setOutputFormat(e.target.value)}
                className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
              >
                {outputFormats.map((f) => (
                  <option key={f.value} value={f.value}>{f.label}</option>
                ))}
              </select>
            </div>

            {engineId === "parakeet-mlx" && (
              <div className="mb-4 text-xs text-blue-600 bg-blue-50 dark:bg-blue-900/20 dark:text-blue-300 p-2.5 rounded-md">
                {locale === "en"
                  ? "Tip: Parakeet-MLX engine relies on local `uv` for the first run and will automatically cache models from Hugging Face in the background. Please ensure internet access."
                  : "提示：Parakeet-MLX 引擎首次运行依赖本地 `uv` 并会在后台自动从 Hugging Face 缓存模型，请确保网络通畅。"}
              </div>
            )}

            {prerequisiteHint && (
              <div
                id="task-prerequisite-hint"
                className="mb-4 flex flex-wrap items-center gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-200"
              >
                <AlertCircle size={14} className="shrink-0" />
                <span>{prerequisiteHint}</span>
                {!selectedPath ? (
                  <button
                    type="button"
                    onClick={handleSelectMedia}
                    className="font-medium text-amber-900 underline-offset-2 hover:underline dark:text-amber-100"
                  >
                    现在选择
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={() => navigate("/models")}
                    className="font-medium text-amber-900 underline-offset-2 hover:underline dark:text-amber-100"
                  >
                    {locale === "en" ? "Open Model Management" : "打开模型管理"}
                  </button>
                )}
              </div>
            )}

            {/* 操作按钮 */}
            <div className="flex flex-wrap gap-3">
              <button
                type="button"
                onClick={handleCreate}
                disabled={creating}
                aria-describedby={prerequisiteHint ? "task-prerequisite-hint" : undefined}
                className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300 dark:disabled:bg-gray-700"
                title={prerequisiteHint || undefined}
              >
                <Play size={16} />
                {creating ? t("home.creating") : t("home.createTask")}
              </button>
              <button
                type="button"
                onClick={handlePreview}
                disabled={creating}
                className="inline-flex items-center gap-2 rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-600 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
                title={taskType === "translate-only" ? (locale === "en" ? "Preview supports audio/video only" : "快速预览仅支持音视频任务") : undefined}
              >
                {t("home.createPreview")}
              </button>
            </div>
          </section>
        </div>

        {/* 系统信息 */}
        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800 h-fit">
          <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">{locale === "en" ? "System Info" : "系统信息"}</h3>
          <dl className="space-y-3">
            <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-3">
              <dt className="text-sm text-gray-500">{locale === "en" ? "App Name" : "应用名称"}</dt>
              <dd className="min-w-0 break-words text-right font-mono text-sm text-gray-900 dark:text-gray-100">
                {appInfo?.name ?? (locale === "en" ? "Loading..." : "加载中...")}
              </dd>
            </div>
            <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-3">
              <dt className="text-sm text-gray-500">{locale === "en" ? "Version" : "版本"}</dt>
              <dd className="min-w-0 text-right font-mono text-sm text-gray-900 dark:text-gray-100">
                {appInfo?.version ?? (locale === "en" ? "Loading..." : "加载中...")}
              </dd>
            </div>
            <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-3">
              <dt className="text-sm text-gray-500">FFmpeg</dt>
              <dd className="min-w-0 text-right text-sm text-gray-900 dark:text-gray-100">
                {ffmpegVersion === "检测中..." ? (
                  <span className="text-gray-500">{t("home.detecting")}</span>
                ) : ffmpegVersion === "未找到" ? (
                  <span className="inline-flex items-center gap-1 text-red-600 dark:text-red-300">
                    <AlertCircle size={14} />
                    {locale === "en" ? "Unavailable" : "不可用"}
                  </span>
                ) : (
                  <span
                    className="inline-flex items-center gap-1 text-green-700 dark:text-green-300"
                    title={ffmpegVersion}
                  >
                    <CheckCircle className="shrink-0" size={14} />
                    {locale === "en" ? "Available" : "可用"}
                  </span>
                )}
              </dd>
            </div>
          </dl>
        </section>
      </div>
    </div>
  );
}
