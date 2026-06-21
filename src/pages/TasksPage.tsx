import { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  cancelTask,
  pauseTask,
  resumeTask,
  retryTask,
  getTaskLogs,
  listTasks,
  TASK_UPDATED_EVENT,
  type Task,
} from "../lib/tauri";
import {
  RefreshCw,
  XCircle,
  Play,
  Pause,
  RotateCcw,
  FileText,
  X,
  Copy,
  CheckCircle,
} from "lucide-react";

function StatusPill({ status }: { status: string }) {
  const colors: Record<string, string> = {
    pending: "bg-gray-100 text-gray-600 dark:bg-gray-700/60 dark:text-gray-400",
    running: "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
    paused: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
    cancelled: "bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400",
    done: "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400",
    error: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  };
  const labels: Record<string, string> = {
    pending: "等待中",
    running: "运行中",
    paused: "已暂停",
    cancelled: "已取消",
    done: "已完成",
    error: "错误",
  };
  return (
    <span className={`text-xs px-2.5 py-0.5 rounded-full font-medium ${colors[status] ?? colors.pending}`}>
      {labels[status] ?? status}
    </span>
  );
}

function TaskTypeLabel({ type }: { type: string }) {
  const labels: Record<string, string> = {
    "generate-and-translate": "生成并翻译",
    "generate-only": "仅生成",
    "translate-only": "仅翻译",
  };
  return (
    <span className="text-xs px-1.5 py-0.5 rounded bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-300">
      {labels[type] ?? type}
    </span>
  );
}

function sortTasks(tasks: Task[]): Task[] {
  return [...tasks].sort((a, b) => Date.parse(b.created_at) - Date.parse(a.created_at));
}

function upsertTask(tasks: Task[], task: Task): Task[] {
  const existingIndex = tasks.findIndex((item) => item.id === task.id);
  if (existingIndex === -1) {
    return sortTasks([task, ...tasks]);
  }

  const next = [...tasks];
  next[existingIndex] = task;
  return sortTasks(next);
}

export default function TasksPage() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeLogTaskId, setActiveLogTaskId] = useState<string | null>(null);
  const [logsText, setLogsText] = useState("");
  const [copied, setCopied] = useState(false);
  const logContainerRef = useRef<HTMLPreElement>(null);

  const handleOpenFolder = async (outputPath: string) => {
    try {
      await revealItemInDir(outputPath);
    } catch (e) {
      console.error("无法打开目录", e);
    }
  };

  const handleOpenFile = async (outputPath: string) => {
    try {
      await openPath(outputPath);
    } catch (e) {
      console.error("无法打开文件", e);
    }
  };

  const refresh = () => {
    setLoading(true);
    listTasks()
      .then((nextTasks) => setTasks(sortTasks(nextTasks)))
      .catch(console.error)
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined;

    refresh();
    listen<Task>(TASK_UPDATED_EVENT, (event) => {
      setTasks((currentTasks) => upsertTask(currentTasks, event.payload));
      setLoading(false);
    }).then((cleanup) => {
      if (mounted) {
        unlisten = cleanup;
      } else {
        cleanup();
      }
    }).catch(console.error);

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);

  // Listen to logs when activeLogTaskId is set
  useEffect(() => {
    if (!activeLogTaskId) return;

    setLogsText("");
    getTaskLogs(activeLogTaskId)
      .then((existingLogs) => {
        setLogsText(existingLogs);
      })
      .catch(console.error);

    let unlistenLogs: (() => void) | undefined;
    listen<{ task_id: string; message: string }>("task-log", (event) => {
      if (event.payload.task_id === activeLogTaskId) {
        setLogsText((prev) => prev + event.payload.message);
      }
    }).then((cleanup) => {
      unlistenLogs = cleanup;
    }).catch(console.error);

    return () => {
      unlistenLogs?.();
    };
  }, [activeLogTaskId]);

  // Auto-scroll logs to bottom
  useEffect(() => {
    if (logContainerRef.current) {
      logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
    }
  }, [logsText]);

  const handleCancel = async (taskId: string) => {
    try {
      const task = await cancelTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      console.error("取消任务失败", e);
    }
  };

  const handlePause = async (taskId: string) => {
    try {
      const task = await pauseTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      console.error("暂停任务失败", e);
    }
  };

  const handleResume = async (taskId: string) => {
    try {
      const task = await resumeTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      console.error("恢复任务失败", e);
    }
  };

  const handleRetry = async (taskId: string) => {
    try {
      const task = await retryTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      console.error("重试任务失败", e);
    }
  };

  const handleCopyLogs = () => {
    navigator.clipboard.writeText(logsText).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div className="max-w-5xl">
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white font-sans tracking-tight">任务队列</h2>
        <button
          onClick={refresh}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-gray-200 hover:border-gray-300 dark:border-gray-700 dark:hover:border-gray-600 bg-white dark:bg-gray-800 text-sm text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white transition shadow-sm"
        >
          <RefreshCw size={14} className={loading ? "animate-spin" : ""} /> 刷新
        </button>
      </div>

      {loading && tasks.length === 0 ? (
        <div className="text-gray-500 py-10 text-center dark:text-gray-400">正在加载任务...</div>
      ) : tasks.length === 0 ? (
        <div className="text-gray-500 bg-white dark:bg-gray-800 rounded-xl py-12 px-6 text-center border border-gray-200 dark:border-gray-700 shadow-sm">
          <p className="text-base font-medium">暂无任务</p>
          <p className="text-xs text-gray-400 mt-1">提交音视频文件转录或字幕翻译后将在此处显示进度。</p>
        </div>
      ) : (
        <div className="space-y-4">
          {tasks.map((task) => (
            <div
              key={task.id}
              className="bg-white dark:bg-gray-800 rounded-xl p-5 border border-gray-200 dark:border-gray-700 shadow-sm transition hover:shadow-md"
            >
              <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto]">
                <div className="min-w-0">
                  <h4 className="font-semibold text-gray-900 dark:text-white truncate text-base">{task.media_name}</h4>
                  <p className="mt-1.5 truncate text-xs text-gray-500 dark:text-gray-400" title={task.media_path}>
                    {task.media_path}
                  </p>
                  <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
                    <TaskTypeLabel type={task.task_type} />
                    <span className="text-[10px] text-gray-300 dark:text-gray-600">|</span>
                    <span className="text-xs text-gray-500 dark:text-gray-400">
                      {task.engine_id} · {task.model_id}
                      {task.source_language && ` · ${task.source_language}`}
                      {task.target_language && ` → ${task.target_language}`}
                      {" · "}{task.output_format.toUpperCase()}
                    </span>
                  </div>
                </div>

                <div className="flex items-center justify-between sm:justify-end gap-3.5 border-t border-gray-100 dark:border-gray-700/60 pt-3 sm:pt-0 sm:border-0">
                  <StatusPill status={task.status} />

                  <div className="flex items-center gap-1">
                    {/* Log button */}
                    <button
                      type="button"
                      title="查看日志"
                      onClick={() => setActiveLogTaskId(task.id)}
                      className="p-1.5 rounded-lg text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 transition"
                    >
                      <FileText size={16} />
                    </button>

                    {/* Pause button */}
                    {(task.status === "running" || task.status === "pending") && (
                      <button
                        type="button"
                        title="暂停任务"
                        onClick={() => handlePause(task.id)}
                        className="p-1.5 rounded-lg text-amber-600 hover:text-amber-700 hover:bg-amber-50 dark:text-amber-400 dark:hover:text-amber-300 dark:hover:bg-amber-950/30 transition"
                      >
                        <Pause size={16} />
                      </button>
                    )}

                    {/* Resume button */}
                    {task.status === "paused" && (
                      <button
                        type="button"
                        title="继续任务"
                        onClick={() => handleResume(task.id)}
                        className="p-1.5 rounded-lg text-green-600 hover:text-green-700 hover:bg-green-50 dark:text-green-400 dark:hover:text-green-300 dark:hover:bg-green-950/30 transition"
                      >
                        <Play size={16} />
                      </button>
                    )}

                    {/* Retry button */}
                    {(task.status === "error" || task.status === "cancelled") && (
                      <button
                        type="button"
                        title="重试任务"
                        onClick={() => handleRetry(task.id)}
                        className="p-1.5 rounded-lg text-blue-600 hover:text-blue-700 hover:bg-blue-50 dark:text-blue-400 dark:hover:text-blue-300 dark:hover:bg-blue-950/30 transition"
                      >
                        <RotateCcw size={16} />
                      </button>
                    )}

                    {/* Cancel button */}
                    {(task.status === "running" || task.status === "pending" || task.status === "paused") && (
                      <button
                        type="button"
                        title="取消任务"
                        onClick={() => handleCancel(task.id)}
                        className="p-1.5 rounded-lg text-red-500 hover:text-red-700 hover:bg-red-50 dark:text-red-400 dark:hover:text-red-300 dark:hover:bg-red-950/30 transition"
                      >
                        <XCircle size={16} />
                      </button>
                    )}
                  </div>
                </div>
              </div>

              {task.status !== "pending" && (
                <div className="mt-4">
                  <div className="w-full bg-gray-100 dark:bg-gray-700 rounded-full h-1.5 overflow-hidden">
                    <div
                      className={`h-full rounded-full transition-all duration-300 ${
                        task.status === "error"
                          ? "bg-red-500"
                          : task.status === "paused"
                          ? "bg-amber-400"
                          : task.status === "cancelled"
                          ? "bg-gray-400"
                          : task.status === "done"
                          ? "bg-green-500"
                          : "bg-blue-600"
                      }`}
                      style={{ width: `${Math.round(task.progress * 100)}%` }}
                    />
                  </div>
                  <div className="flex justify-between items-center mt-2">
                    <p className="text-xs text-gray-500 dark:text-gray-400 truncate max-w-[80%]">
                      {task.status_message}
                    </p>
                    <p className="text-xs font-semibold text-gray-600 dark:text-gray-300 shrink-0">
                      {Math.round(task.progress * 100)}%
                    </p>
                  </div>
                </div>
              )}

              {task.status === "done" && task.output_path && (
                <div className="mt-4 bg-gray-50 dark:bg-gray-900/30 p-3 rounded-lg border border-gray-100 dark:border-gray-800 text-xs">
                  <p className="font-medium text-gray-700 dark:text-gray-300 truncate mb-2.5" title={task.output_path}>
                    输出路径：{task.output_path}
                  </p>
                  <div className="flex gap-2.5">
                    <button
                      type="button"
                      onClick={() => handleOpenFile(task.output_path!)}
                      className="inline-flex items-center gap-1.5 rounded-lg bg-blue-50 hover:bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300 px-3 py-1.5 font-medium transition shadow-sm"
                    >
                      打开输出文件
                    </button>
                    <button
                      type="button"
                      onClick={() => handleOpenFolder(task.output_path!)}
                      className="inline-flex items-center gap-1.5 rounded-lg bg-gray-100 hover:bg-gray-200 text-gray-700 dark:bg-gray-700 dark:text-gray-300 px-3 py-1.5 font-medium transition shadow-sm"
                    >
                      打开所在目录
                    </button>
                  </div>
                </div>
              )}
              {task.error && (
                <div className="mt-3.5 p-3 rounded-lg bg-red-50 dark:bg-red-950/20 border border-red-100 dark:border-red-900/30 text-xs text-red-600 dark:text-red-400 font-mono break-all">
                  错误日志: {task.error}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Logs Modal */}
      {activeLogTaskId && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm animate-fade-in">
          <div className="bg-white dark:bg-gray-900 rounded-2xl w-full max-w-3xl h-[80vh] flex flex-col shadow-2xl border border-gray-200 dark:border-gray-800 overflow-hidden">
            {/* Modal Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-800 bg-gray-50/50 dark:bg-gray-900/50">
              <div className="min-w-0">
                <h3 className="text-base font-bold text-gray-900 dark:text-white truncate">
                  任务日志
                </h3>
                <p className="text-xs text-gray-400 font-mono truncate mt-0.5">
                  ID: {activeLogTaskId}
                </p>
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={handleCopyLogs}
                  disabled={!logsText}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-xs text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white transition disabled:opacity-50"
                >
                  {copied ? (
                    <>
                      <CheckCircle size={12} className="text-green-500" /> 已复制
                    </>
                  ) : (
                    <>
                      <Copy size={12} /> 复制日志
                    </>
                  )}
                </button>
                <button
                  onClick={() => setActiveLogTaskId(null)}
                  className="p-1.5 rounded-lg text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 hover:bg-gray-150 dark:hover:bg-gray-800 transition"
                >
                  <X size={18} />
                </button>
              </div>
            </div>

            {/* Modal Body (Logs) */}
            <div className="flex-1 p-6 overflow-hidden bg-gray-950 dark:bg-black">
              <pre
                ref={logContainerRef}
                className="w-full h-full overflow-y-auto text-xs text-green-400 font-mono whitespace-pre-wrap select-text leading-relaxed scrollbar-thin scrollbar-thumb-gray-800 scrollbar-track-transparent"
              >
                {logsText || "正在加载日志或暂无日志..."}
              </pre>
            </div>

            {/* Modal Footer */}
            <div className="px-6 py-3 border-t border-gray-200 dark:border-gray-800 bg-gray-50/50 dark:bg-gray-900/50 flex justify-end text-[10px] text-gray-400 dark:text-gray-500">
              日志实时流式更新中
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
