import { useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { useI18n } from "../lib/i18n";
import {
  cancelTask,
  deleteTask,
  deleteTasks,
  pauseTask,
  resumeTask,
  retryTask,
  getTaskLogs,
  listTasks,
  TASK_DELETED_EVENT,
  TASK_UPDATED_EVENT,
  type Task,
  type TaskDeletedPayload,
} from "../lib/tauri";
import {
  AlertCircle,
  RefreshCw,
  XCircle,
  Play,
  Pause,
  RotateCcw,
  FileText,
  Trash2,
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

function canDeleteTask(task: Task): boolean {
  return ["done", "error", "cancelled", "paused"].includes(task.status);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export default function TasksPage() {
  const { t, locale } = useI18n();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeLogTaskId, setActiveLogTaskId] = useState<string | null>(null);
  const [logsText, setLogsText] = useState("");
  const [copied, setCopied] = useState(false);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([]);
  const [pendingDeleteTaskIds, setPendingDeleteTaskIds] = useState<string[] | null>(null);
  const [deletingTaskIds, setDeletingTaskIds] = useState<string[]>([]);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const logContainerRef = useRef<HTMLPreElement>(null);

  const deletableTasks = tasks.filter(canDeleteTask);
  const selectedDeletableIds = selectedTaskIds.filter((taskId) =>
    deletableTasks.some((task) => task.id === taskId),
  );
  const allDeletableSelected =
    deletableTasks.length > 0 && selectedDeletableIds.length === deletableTasks.length;
  const pendingDeleteTasks = pendingDeleteTaskIds
    ? pendingDeleteTaskIds
        .map((taskId) => tasks.find((task) => task.id === taskId))
        .filter((task): task is Task => Boolean(task))
    : [];

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
    let unlistenDelete: (() => void) | undefined;

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
    listen<TaskDeletedPayload>(TASK_DELETED_EVENT, (event) => {
      const deletedTaskId = event.payload.task_id;
      setTasks((currentTasks) => currentTasks.filter((task) => task.id !== deletedTaskId));
      setSelectedTaskIds((currentIds) => currentIds.filter((taskId) => taskId !== deletedTaskId));
      setActiveLogTaskId((currentTaskId) =>
        currentTaskId === deletedTaskId ? null : currentTaskId,
      );
    }).then((cleanup) => {
      if (mounted) {
        unlistenDelete = cleanup;
      } else {
        cleanup();
      }
    }).catch(console.error);

    return () => {
      mounted = false;
      unlisten?.();
      unlistenDelete?.();
    };
  }, []);

  useEffect(() => {
    const deletableIds = new Set(tasks.filter(canDeleteTask).map((task) => task.id));
    setSelectedTaskIds((currentIds) => currentIds.filter((taskId) => deletableIds.has(taskId)));
  }, [tasks]);

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

  const removeDeletedTasksFromView = (deletedTaskIds: string[]) => {
    const deletedSet = new Set(deletedTaskIds);
    setTasks((currentTasks) => currentTasks.filter((task) => !deletedSet.has(task.id)));
    setSelectedTaskIds((currentIds) => currentIds.filter((taskId) => !deletedSet.has(taskId)));
    setActiveLogTaskId((currentTaskId) =>
      currentTaskId && deletedSet.has(currentTaskId) ? null : currentTaskId,
    );
  };

  const handleToggleSelectTask = (taskId: string) => {
    setSelectedTaskIds((currentIds) =>
      currentIds.includes(taskId)
        ? currentIds.filter((selectedTaskId) => selectedTaskId !== taskId)
        : [...currentIds, taskId],
    );
  };

  const handleToggleSelectAll = () => {
    setSelectedTaskIds(allDeletableSelected ? [] : deletableTasks.map((task) => task.id));
  };

  const openDeleteDialog = (taskIds: string[]) => {
    setDeleteError(null);
    setPendingDeleteTaskIds(taskIds);
  };

  const handleConfirmDelete = async () => {
    if (!pendingDeleteTaskIds || pendingDeleteTaskIds.length === 0) {
      return;
    }

    setDeletingTaskIds(pendingDeleteTaskIds);
    setDeleteError(null);
    try {
      const deletedTaskIds =
        pendingDeleteTaskIds.length === 1
          ? [await deleteTask(pendingDeleteTaskIds[0])]
          : await deleteTasks(pendingDeleteTaskIds);
      removeDeletedTasksFromView(deletedTaskIds);
      setPendingDeleteTaskIds(null);
    } catch (error) {
      const message = errorMessage(error);
      setDeleteError(message);
      console.error("删除任务失败", error);
    } finally {
      setDeletingTaskIds([]);
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
      <div className="flex flex-col gap-3 mb-6 sm:flex-row sm:items-center sm:justify-between">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white font-sans tracking-tight">{t("tasks.title")}</h2>
        <div className="flex flex-wrap items-center gap-2">
          {tasks.length > 0 && (
            <>
              <label className="inline-flex h-8 items-center gap-2 rounded-lg border border-gray-200 bg-white px-3 text-xs font-medium text-gray-600 shadow-sm dark:border-gray-700 dark:bg-gray-800 dark:text-gray-300">
                <input
                  type="checkbox"
                  checked={allDeletableSelected}
                  disabled={deletableTasks.length === 0}
                  onChange={handleToggleSelectAll}
                  className="h-4 w-4 rounded border-gray-300 text-blue-600 disabled:opacity-40"
                />
                {locale === "en" ? "Select all deletable" : "全选可删除"}
              </label>
              <button
                type="button"
                onClick={() => openDeleteDialog(selectedDeletableIds)}
                disabled={selectedDeletableIds.length === 0}
                className="inline-flex h-8 items-center gap-1.5 rounded-lg border border-red-200 bg-red-50 px-3 text-xs font-medium text-red-700 shadow-sm transition hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-45 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300 dark:hover:bg-red-950/50"
              >
                <Trash2 size={14} />
                {(locale === "en" ? "Delete selected" : "删除选中") + (selectedDeletableIds.length > 0 ? ` ${selectedDeletableIds.length}` : "")}
              </button>
            </>
          )}
          <button
            onClick={refresh}
            className="flex h-8 items-center gap-1.5 rounded-lg border border-gray-200 bg-white px-3 text-sm text-gray-600 shadow-sm transition hover:border-gray-300 hover:text-gray-900 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-400 dark:hover:border-gray-600 dark:hover:text-white"
          >
            <RefreshCw size={14} className={loading ? "animate-spin" : ""} /> {locale === "en" ? "Refresh" : "刷新"}
          </button>
        </div>
      </div>

      {loading && tasks.length === 0 ? (
        <div className="text-gray-500 py-10 text-center dark:text-gray-400">{locale === "en" ? "Loading tasks..." : "正在加载任务..."}</div>
      ) : tasks.length === 0 ? (
        <div className="text-gray-500 bg-white dark:bg-gray-800 rounded-xl py-12 px-6 text-center border border-gray-200 dark:border-gray-700 shadow-sm">
          <p className="text-base font-medium">{locale === "en" ? "No Tasks" : "暂无任务"}</p>
          <p className="text-xs text-gray-400 mt-1">
            {locale === "en" ? "Progress of media transcription or subtitle translation will be shown here." : "提交音视频文件转录或字幕翻译后将在此处显示进度。"}
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {tasks.map((task) => (
            <div
              key={task.id}
              className={`rounded-xl border p-5 shadow-sm transition hover:shadow-md ${
                selectedTaskIds.includes(task.id)
                  ? "border-blue-200 bg-blue-50/40 dark:border-blue-900/50 dark:bg-blue-950/20"
                  : "border-gray-200 bg-white dark:border-gray-700 dark:bg-gray-800"
              }`}
            >
              <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-3 sm:grid-cols-[auto_minmax(0,1fr)_auto]">
                <div className="flex items-start pt-0.5">
                  <input
                    type="checkbox"
                    checked={selectedTaskIds.includes(task.id)}
                    disabled={!canDeleteTask(task)}
                    onChange={() => handleToggleSelectTask(task.id)}
                    aria-label={`选择任务 ${task.media_name}`}
                    title={canDeleteTask(task) ? "选择任务" : "运行中任务需先暂停或取消"}
                    className="h-4 w-4 rounded border-gray-300 text-blue-600 disabled:cursor-not-allowed disabled:opacity-35"
                  />
                </div>
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

                <div className="col-span-2 flex items-center justify-between gap-3.5 border-t border-gray-100 pt-3 dark:border-gray-700/60 sm:col-span-1 sm:justify-end sm:border-0 sm:pt-0">
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
                    {canDeleteTask(task) && (
                      <button
                        type="button"
                        title="删除任务记录"
                        onClick={() => openDeleteDialog([task.id])}
                        disabled={deletingTaskIds.includes(task.id)}
                        className="p-1.5 rounded-lg text-gray-400 hover:text-red-700 hover:bg-red-50 disabled:opacity-45 dark:text-gray-500 dark:hover:text-red-300 dark:hover:bg-red-950/30 transition"
                      >
                        <Trash2 size={16} />
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

      {pendingDeleteTaskIds && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 p-4">
          <div className="w-full max-w-md rounded-lg border border-gray-200 bg-white p-5 shadow-xl dark:border-gray-700 dark:bg-gray-800">
            <div className="mb-4 flex items-start gap-3">
              <div className="rounded-full bg-red-50 p-2 text-red-600 dark:bg-red-950/40 dark:text-red-300">
                <AlertCircle size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-gray-900 dark:text-white">
                  删除{pendingDeleteTaskIds.length > 1 ? `${pendingDeleteTaskIds.length} 个` : ""}任务记录
                </h3>
                <p className="mt-1 text-sm text-gray-600 dark:text-gray-300">
                  只会移除队列记录、日志和临时工作目录；原始媒体和已导出的文件不会被删除。
                </p>
                {pendingDeleteTasks.length > 0 && (
                  <p className="mt-2 truncate text-xs text-gray-400" title={pendingDeleteTasks[0].media_name}>
                    {pendingDeleteTasks[0].media_name}
                    {pendingDeleteTaskIds.length > 1 && ` 等 ${pendingDeleteTaskIds.length} 个任务`}
                  </p>
                )}
                {deleteError && (
                  <p className="mt-2 rounded-md bg-red-50 px-2 py-1.5 text-xs text-red-600 dark:bg-red-950/30 dark:text-red-300">
                    {deleteError}
                  </p>
                )}
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setPendingDeleteTaskIds(null)}
                disabled={deletingTaskIds.length > 0}
                className="rounded-md border border-gray-300 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-50 disabled:opacity-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
              >
                取消
              </button>
              <button
                type="button"
                onClick={handleConfirmDelete}
                disabled={deletingTaskIds.length > 0}
                className="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                {deletingTaskIds.length > 0 ? "删除中..." : "删除"}
              </button>
            </div>
          </div>
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
                  {t("tasks.modal.title")}
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
                      <CheckCircle size={12} className="text-green-500" /> {t("tasks.modal.copied")}
                    </>
                  ) : (
                    <>
                      <Copy size={12} /> {t("tasks.modal.copy")}
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
                {logsText || (locale === "en" ? "Loading logs or no logs available..." : "正在加载日志或暂无日志...")}
              </pre>
            </div>

            {/* Modal Footer */}
            <div className="px-6 py-3 border-t border-gray-200 dark:border-gray-800 bg-gray-50/50 dark:bg-gray-900/50 flex justify-end text-[10px] text-gray-400 dark:text-gray-500">
              {(() => {
                const activeLogTask = tasks.find((t) => t.id === activeLogTaskId);
                switch (activeLogTask?.status) {
                  case "running":
                    return t("tasks.log.streaming");
                  case "pending":
                    return t("tasks.log.pending");
                  case "paused":
                    return t("tasks.log.paused");
                  case "done":
                    return t("tasks.log.done");
                  case "error":
                    return t("tasks.log.error");
                  case "cancelled":
                    return t("tasks.log.cancelled");
                  default:
                    return t("tasks.log.streaming");
                }
              })()}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
