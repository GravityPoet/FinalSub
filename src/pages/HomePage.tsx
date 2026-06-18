import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useNavigate } from "react-router-dom";
import { AlertCircle, CheckCircle, FileVideo, FolderOpen, Play } from "lucide-react";
import { createPreviewTask, getAppInfo, getFfmpegVersion, type AppInfo } from "../lib/tauri";

const mediaExtensions = ["mp4", "mov", "mkv", "webm", "mp3", "wav", "m4a", "aac", "flac"];

function fileNameFromPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export default function HomePage() {
  const navigate = useNavigate();
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [ffmpegVersion, setFfmpegVersion] = useState<string>("检测中...");
  const [selectedPath, setSelectedPath] = useState<string>("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string>("");

  useEffect(() => {
    getAppInfo().then(setAppInfo).catch(console.error);
    getFfmpegVersion().then(setFfmpegVersion).catch(() => setFfmpegVersion("未找到"));
  }, []);

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

  const handleCreatePreviewTask = async () => {
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
        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <div className="mb-5 flex items-center gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300">
              <FileVideo size={20} />
            </div>
            <div className="min-w-0">
              <h3 className="font-semibold text-gray-900 dark:text-white">新建任务</h3>
              <p className="truncate text-sm text-gray-500 dark:text-gray-400">
                {selectedPath ? fileNameFromPath(selectedPath) : "未选择文件"}
              </p>
            </div>
          </div>

          {selectedPath ? (
            <div className="mb-4 rounded-md border border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-900/60">
              <p className="truncate font-mono text-xs text-gray-600 dark:text-gray-300" title={selectedPath}>
                {selectedPath}
              </p>
            </div>
          ) : null}

          {error ? (
            <div className="mb-4 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/30 dark:text-red-300">
              <AlertCircle className="mt-0.5 shrink-0" size={16} />
              <span>{error}</span>
            </div>
          ) : null}

          <div className="flex flex-wrap gap-3">
            <button
              type="button"
              onClick={handleSelectMedia}
              className="inline-flex items-center gap-2 rounded-md border border-gray-300 px-3 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
            >
              <FolderOpen size={16} />
              选择音视频
            </button>
            <button
              type="button"
              onClick={handleCreatePreviewTask}
              disabled={!selectedPath || creating}
              className="inline-flex items-center gap-2 rounded-md bg-blue-600 px-3 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:cursor-not-allowed disabled:bg-gray-300 dark:disabled:bg-gray-700"
            >
              <Play size={16} />
              {creating ? "正在创建..." : "开始预览"}
            </button>
          </div>
        </section>

        <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800">
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
