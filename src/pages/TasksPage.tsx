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

import { Button } from "../components/ui/Button";
import { Card } from "../components/ui/Card";
import { Badge } from "../components/ui/Badge";
import { Progress } from "../components/ui/Progress";

function StatusPill({ status }: { status: string }) {
  const { t } = useI18n();
  const variants: Record<string, "default" | "success" | "warning" | "danger" | "info"> = {
    pending: "default",
    running: "info",
    paused: "warning",
    cancelled: "default",
    done: "success",
    error: "danger",
  };
  return (
    <Badge variant={variants[status] ?? "default"}>
      {t(`tasks.status.${status}` as any)}
    </Badge>
  );
}

function TaskTypeLabel({ type }: { type: string }) {
  const { t } = useI18n();
  return (
    <Badge variant="default" className="font-normal border-none bg-surface-overlay text-text-secondary">
      {t(`tasks.type.${type}` as any)}
    </Badge>
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
  const { t } = useI18n();
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
      console.error("Failed to open directory", e);
    }
  };

  const handleOpenFile = async (outputPath: string) => {
    try {
      await openPath(outputPath);
    } catch (e) {
      console.error("Failed to open file", e);
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
      console.error("Failed to cancel task", e);
    }
  };

  const handlePause = async (taskId: string) => {
    try {
      const task = await pauseTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      console.error("Failed to pause task", e);
    }
  };

  const handleResume = async (taskId: string) => {
    try {
      const task = await resumeTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      console.error("Failed to resume task", e);
    }
  };

  const handleRetry = async (taskId: string) => {
    try {
      const task = await retryTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      console.error("Failed to retry task", e);
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
      console.error("Failed to delete task", error);
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
    <div className="max-w-5xl space-y-6">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h2 className="text-display font-bold tracking-tight text-text-primary">{t("tasks.title")}</h2>
        <div className="flex flex-wrap items-center gap-3">
          {tasks.length > 0 && (
            <>
              <label className="inline-flex h-8 items-center gap-2 rounded-lg border border-border-default bg-surface px-3 text-xs font-medium text-text-secondary shadow-sm cursor-pointer hover:bg-surface-overlay select-none transition">
                <input
                  type="checkbox"
                  checked={allDeletableSelected}
                  disabled={deletableTasks.length === 0}
                  onChange={handleToggleSelectAll}
                  className="h-3.5 w-3.5 rounded border-border-default text-brand focus:ring-0 cursor-pointer disabled:opacity-40"
                />
                <span>{t("tasks.selectAllDeletable")}</span>
              </label>
              <Button
                type="button"
                onClick={() => openDeleteDialog(selectedDeletableIds)}
                disabled={selectedDeletableIds.length === 0}
                className="h-8 py-0 px-3 text-xs"
                variant="danger"
              >
                <Trash2 size={12} />
                <span>{t("tasks.deleteSelected") + (selectedDeletableIds.length > 0 ? ` (${selectedDeletableIds.length})` : "")}</span>
              </Button>
            </>
          )}
          <Button
            onClick={refresh}
            variant="secondary"
            className="h-8 py-0 px-3 text-xs"
          >
            <RefreshCw size={12} className={loading ? "animate-spin" : ""} />
            <span>{t("tasks.refresh")}</span>
          </Button>
        </div>
      </div>

      {loading && tasks.length === 0 ? (
        <div className="text-text-tertiary py-16 text-center text-sm">{t("tasks.loading")}</div>
      ) : tasks.length === 0 ? (
        <Card className="py-16 px-6 text-center border-dashed">
          <p className="text-base font-semibold text-text-primary">{t("tasks.noTasks")}</p>
          <p className="text-xs text-text-tertiary mt-1.5 leading-5">
            {t("tasks.noTasksDesc")}
          </p>
        </Card>
      ) : (
        <div className="space-y-4">
          {tasks.map((task) => {
            const isSelected = selectedTaskIds.includes(task.id);
            return (
              <Card
                key={task.id}
                className={`p-5 transition-all duration-150 ${
                  isSelected
                    ? "border-brand bg-brand-subtle/20 shadow-sm"
                    : "border-border-subtle bg-surface"
                }`}
              >
                <div className="grid grid-cols-[auto_minmax(0,1fr)] gap-3 sm:grid-cols-[auto_minmax(0,1fr)_auto]">
                  <div className="flex items-start pt-1.5">
                    <input
                      type="checkbox"
                      checked={isSelected}
                      disabled={!canDeleteTask(task)}
                      onChange={() => handleToggleSelectTask(task.id)}
                      aria-label={t('tasks.selectTaskAria', { name: task.media_name })}
                      title={canDeleteTask(task) ? t("tasks.selectTask") : t("tasks.deleteRunningPrereq")}
                      className="h-3.5 w-3.5 rounded border-border-default text-brand focus:ring-0 cursor-pointer disabled:cursor-not-allowed disabled:opacity-35"
                    />
                  </div>
                  <div className="min-w-0">
                    <h4 className="font-semibold text-text-primary truncate text-base">{task.media_name}</h4>
                    <p className="mt-1.5 truncate font-mono text-xs text-text-tertiary" title={task.media_path}>
                      {task.media_path}
                    </p>
                    <div className="mt-3 flex flex-wrap items-center gap-2">
                      <TaskTypeLabel type={task.task_type} />
                      <span className="text-[10px] text-border-strong">|</span>
                      <span className="text-xs text-text-secondary">
                        {task.engine_id} · {task.model_id}
                        {task.source_language && ` · ${task.source_language}`}
                        {task.target_language && ` → ${task.target_language}`}
                        {" · "}{task.output_format.toUpperCase()}
                      </span>
                    </div>
                  </div>

                  <div className="col-span-2 flex items-center justify-between gap-4 border-t border-border-subtle pt-3 sm:col-span-1 sm:justify-end sm:border-0 sm:pt-0">
                    <StatusPill status={task.status} />

                    <div className="flex items-center gap-1.5">
                      {/* Log button */}
                      <button
                        type="button"
                        title={t("tasks.viewLogs")}
                        onClick={() => setActiveLogTaskId(task.id)}
                        className="p-1.5 rounded-lg text-text-secondary hover:text-text-primary hover:bg-surface-overlay transition"
                      >
                        <FileText size={15} />
                      </button>

                      {/* Pause button */}
                      {(task.status === "running" || task.status === "pending") && (
                        <button
                          type="button"
                          title={t("tasks.pauseTask")}
                          onClick={() => handlePause(task.id)}
                          className="p-1.5 rounded-lg text-warning hover:bg-warning/10 transition"
                        >
                          <Pause size={15} />
                        </button>
                      )}

                      {/* Resume button */}
                      {task.status === "paused" && (
                        <button
                          type="button"
                          title={t("tasks.resumeTask")}
                          onClick={() => handleResume(task.id)}
                          className="p-1.5 rounded-lg text-success hover:bg-success/10 transition"
                        >
                          <Play size={15} />
                        </button>
                      )}

                      {/* Retry button */}
                      {(task.status === "error" || task.status === "cancelled") && (
                        <button
                          type="button"
                          title={t("tasks.retryTask")}
                          onClick={() => handleRetry(task.id)}
                          className="p-1.5 rounded-lg text-brand hover:bg-brand-subtle transition"
                        >
                          <RotateCcw size={15} />
                        </button>
                      )}

                      {/* Cancel button */}
                      {(task.status === "running" || task.status === "pending" || task.status === "paused") && (
                        <button
                          type="button"
                          title={t("tasks.cancelTask")}
                          onClick={() => handleCancel(task.id)}
                          className="p-1.5 rounded-lg text-danger hover:bg-danger/10 transition"
                        >
                          <XCircle size={15} />
                        </button>
                      )}
                      {canDeleteTask(task) && (
                        <button
                          type="button"
                          title={t("tasks.deleteTaskRecord")}
                          onClick={() => openDeleteDialog([task.id])}
                          disabled={deletingTaskIds.includes(task.id)}
                          className="p-1.5 rounded-lg text-text-tertiary hover:text-danger hover:bg-danger/10 disabled:opacity-45 transition"
                        >
                          <Trash2 size={15} />
                        </button>
                      )}
                    </div>
                  </div>
                </div>

                {task.status !== "pending" && (
                  <div className="mt-4 space-y-2">
                    <Progress value={Math.round(task.progress * 100)} />
                    <div className="flex justify-between items-center text-xs">
                      <p className="text-text-secondary truncate max-w-[80%]">
                        {task.status_message}
                      </p>
                      <p className="font-semibold text-text-primary shrink-0">
                        {Math.round(task.progress * 100)}%
                      </p>
                    </div>
                  </div>
                )}

                {task.status === "done" && task.output_path && (
                  <div className="mt-4 bg-surface-overlay border border-border-subtle p-3 rounded-lg text-xs space-y-2.5">
                    <p className="font-semibold text-text-secondary truncate" title={task.output_path}>
                      {t("tasks.outputPath")}{task.output_path}
                    </p>
                    <div className="flex gap-2">
                      <Button
                        type="button"
                        onClick={() => handleOpenFile(task.output_path!)}
                        variant="secondary"
                        size="sm"
                      >
                        {t("tasks.openOutputFile")}
                      </Button>
                      <Button
                        type="button"
                        onClick={() => handleOpenFolder(task.output_path!)}
                        variant="secondary"
                        size="sm"
                      >
                        {t("tasks.openOutputDir")}
                      </Button>
                    </div>
                  </div>
                )}
                {task.error && (
                  <div className="mt-3.5 p-3 rounded-lg bg-danger/10 border border-danger/20 text-xs text-danger font-mono break-all leading-5">
                    {t("tasks.errorLog")}{task.error}
                  </div>
                )}
              </Card>
            );
          })}
        </div>
      )}

      {/* Delete Confirmation Modal */}
      {pendingDeleteTaskIds && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <Card className="w-full max-w-md bg-surface-overlay p-6 shadow-lg border border-border-default">
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-danger/10 p-2 text-danger">
                <AlertCircle size={20} />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-text-primary text-h2 mb-1.5">
                  {t("tasks.deleteModalTitle")}
                </h3>
                <p className="text-xs text-text-secondary leading-5">
                  {t("tasks.deleteModalDesc")}
                </p>
                {pendingDeleteTasks.length > 0 && (
                  <p className="mt-3 truncate font-mono text-[11px] text-text-tertiary" title={pendingDeleteTasks[0].media_name}>
                    {pendingDeleteTasks[0].media_name}
                    {pendingDeleteTaskIds.length > 1 && ` (+${pendingDeleteTaskIds.length - 1})`}
                  </p>
                )}
                {deleteError && (
                  <p className="mt-3 rounded-lg bg-danger/10 border border-danger/20 px-3 py-2 text-xs text-danger">
                    {deleteError}
                  </p>
                )}
              </div>
            </div>
            <div className="flex justify-end gap-2.5">
              <Button
                type="button"
                onClick={() => setPendingDeleteTaskIds(null)}
                disabled={deletingTaskIds.length > 0}
                variant="secondary"
                size="sm"
              >
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                onClick={handleConfirmDelete}
                disabled={deletingTaskIds.length > 0}
                variant="danger"
                size="sm"
              >
                {deletingTaskIds.length > 0 ? t("tasks.deleting") : t("tasks.deleteModalConfirm")}
              </Button>
            </div>
          </Card>
        </div>
      )}

      {/* Logs Modal */}
      {activeLogTaskId && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm">
          <div className="bg-surface-overlay rounded-2xl w-full max-w-3xl h-[80vh] flex flex-col shadow-2xl border border-border-default overflow-hidden">
            {/* Modal Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-border-subtle bg-surface">
              <div className="min-w-0">
                <h3 className="text-base font-bold text-text-primary truncate">
                  {t("tasks.modal.title")}
                </h3>
                <p className="text-[11px] text-text-tertiary font-mono truncate mt-0.5">
                  ID: {activeLogTaskId}
                </p>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  onClick={handleCopyLogs}
                  disabled={!logsText}
                  variant="secondary"
                  className="h-8 py-0 px-3 text-xs"
                >
                  {copied ? (
                    <>
                      <CheckCircle size={12} className="text-success" />
                      <span>{t("tasks.modal.copied")}</span>
                    </>
                  ) : (
                    <>
                      <Copy size={12} />
                      <span>{t("tasks.modal.copy")}</span>
                    </>
                  )}
                </Button>
                <button
                  onClick={() => setActiveLogTaskId(null)}
                  className="p-1.5 rounded-lg text-text-tertiary hover:text-text-primary hover:bg-surface transition"
                >
                  <X size={18} />
                </button>
              </div>
            </div>

            {/* Modal Body (Logs) */}
            <div className="flex-1 p-6 overflow-hidden bg-black">
              <pre
                ref={logContainerRef}
                className="w-full h-full overflow-y-auto text-xs text-green-400 font-mono whitespace-pre-wrap select-text leading-relaxed scrollbar-thin scrollbar-thumb-gray-800 scrollbar-track-transparent"
              >
                {logsText || t("tasks.logModalNoLogs")}
              </pre>
            </div>

            {/* Modal Footer */}
            <div className="px-6 py-3 border-t border-border-subtle bg-surface flex justify-end text-[10px] text-text-tertiary font-mono">
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
