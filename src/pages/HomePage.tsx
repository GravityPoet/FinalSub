import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useNavigate } from "react-router-dom";
import { AlertCircle, CheckCircle, FileText, FileVideo, FolderOpen, Languages, Mic, Play } from "lucide-react";
import {
  createTask,
  createPreviewTask,
  getAppInfo,
  getFfmpegVersion,
  listAsrModels,
  getSettings,
  type AppInfo,
  type AsrModelInfo,
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

  const [taskType, setTaskType] = useState("generate-only");
  const [engineId, setEngineId] = useState("whisper-cpp");
  const [modelId, setModelId] = useState("large-v3-turbo");
  const [sourceLanguage, setSourceLanguage] = useState("auto");
  const [targetLanguage, setTargetLanguage] = useState("zh");
  const [outputFormat, setOutputFormat] = useState("srt");

  useEffect(() => {
    getAppInfo().then(setAppInfo).catch(console.error);
    getFfmpegVersion().then(setFfmpegVersion).catch(() => setFfmpegVersion("未找到"));
    listAsrModels().then(setModels).catch(console.error);
    getSettings()
      .then((s) => {
        setTargetLanguage(s.target_language);
        setOutputFormat(s.subtitle_output_format);
      })
      .catch(console.error);
  }, []);

  const engineModels = models.filter((m) => m.engine_id === engineId);
  const engines = [...new Set(models.map((m) => m.engine_id))];

  const handleSelectMedia = async () => {
    setError("");
    const selected = await open({
      multiple: false,
      filters: [{ name: "音视频文件", extensions: mediaExtensions }],
    });
    if (typeof selected === "string") {
      setSelectedPath(selected);
    }
  };

  const handleCreate = async () => {
    if (!selectedPath) {
      setError("请先选择音视频文件。");
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

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_22rem]">
        <div className="space-y-5">
          {/* 文件选择 */}
          <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
            <div className="mb-4 flex items-center gap-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300">
                <FileVideo size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-gray-900 dark:text-white">新建任务</h3>
                <p className="truncate text-sm text-gray-500">
                  {selectedPath ? fileNameFromPath(selectedPath) : "未选择文件"}
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
              选择音视频
            </button>
          </section>

          {/* 任务配置 */}
          <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
            <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">任务配置</h3>

            {/* 任务类型 */}
            <div className="mb-4">
              <label className="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-300">任务类型</label>
              <div className="grid grid-cols-3 gap-2">
                {taskTypes.map((t) => {
                  const Icon = t.icon;
                  return (
                    <button
                      key={t.value}
                      onClick={() => setTaskType(t.value)}
                      className={`flex flex-col items-center gap-1 rounded-lg border p-3 text-xs transition ${
                        taskType === t.value
                          ? "border-blue-500 bg-blue-50 text-blue-700 dark:border-blue-400 dark:bg-blue-900/30 dark:text-blue-300"
                          : "border-gray-200 text-gray-600 hover:border-gray-300 dark:border-gray-600 dark:text-gray-400"
                      }`}
                    >
                      <Icon size={16} />
                      <span className="font-medium">{t.label}</span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* 引擎 */}
            <div className="mb-4 grid grid-cols-2 gap-4">
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">ASR 引擎</label>
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
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">模型</label>
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

            {/* 语言 */}
            <div className="mb-4 grid grid-cols-2 gap-4">
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">源语言</label>
                <select
                  value={sourceLanguage}
                  onChange={(e) => setSourceLanguage(e.target.value)}
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  <option value="auto">自动检测 (auto)</option>
                  <option value="zh">中文 (zh)</option>
                  <option value="en">英文 (en)</option>
                  <option value="ja">日文 (ja)</option>
                  <option value="ko">韩文 (ko)</option>
                  <option value="yue">粤语 (yue)</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">目标语言</label>
                <select
                  value={targetLanguage}
                  onChange={(e) => setTargetLanguage(e.target.value)}
                  className="w-full rounded-md border border-gray-300 bg-white px-2.5 py-1.5 text-sm dark:border-gray-600 dark:bg-gray-700"
                >
                  <option value="zh">中文 (zh)</option>
                  <option value="en">英文 (en)</option>
                  <option value="ja">日文 (ja)</option>
                  <option value="ko">韩文 (ko)</option>
                  <option value="yue">粤语 (yue)</option>
                </select>
              </div>
            </div>

            {/* 输出格式 */}
            <div className="mb-4">
              <label className="mb-1 block text-sm font-medium text-gray-700 dark:text-gray-300">输出格式</label>
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

            {/* 操作按钮 */}
            <div className="flex flex-wrap gap-3">
              <button
                type="button"
                onClick={handleCreate}
                disabled={!selectedPath || creating}
                className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300 dark:disabled:bg-gray-700"
              >
                <Play size={16} />
                {creating ? "正在创建..." : "开始任务"}
              </button>
              <button
                type="button"
                onClick={handlePreview}
                disabled={!selectedPath || creating}
                className="inline-flex items-center gap-2 rounded-md border border-gray-300 px-3 py-2 text-sm text-gray-600 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-700"
              >
                快速预览
              </button>
            </div>
          </section>
        </div>

        {/* 系统信息 */}
        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800 h-fit">
          <h3 className="mb-4 font-semibold text-gray-900 dark:text-white">系统信息</h3>
          <dl className="space-y-3">
            <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-3">
              <dt className="text-sm text-gray-500">应用名称</dt>
              <dd className="min-w-0 break-words text-right font-mono text-sm text-gray-900 dark:text-gray-100">
                {appInfo?.name ?? "加载中..."}
              </dd>
            </div>
            <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-3">
              <dt className="text-sm text-gray-500">版本</dt>
              <dd className="min-w-0 text-right font-mono text-sm text-gray-900 dark:text-gray-100">
                {appInfo?.version ?? "加载中..."}
              </dd>
            </div>
            <div className="grid grid-cols-[7rem_minmax(0,1fr)] gap-3">
              <dt className="text-sm text-gray-500">FFmpeg</dt>
              <dd className="min-w-0 break-words text-right font-mono text-sm text-gray-900 dark:text-gray-100">
                {ffmpegVersion === "未找到" ? (
                  <span className="inline-flex items-center gap-1 text-red-600 dark:text-red-300">
                    <AlertCircle size={14} />
                    未找到
                  </span>
                ) : (
                  <span className="inline-flex items-start gap-1">
                    <CheckCircle className="mt-0.5 shrink-0 text-green-600" size={14} />
                    <span>{ffmpegVersion}</span>
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
