import { useEffect, useState, useRef } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import { useI18n, type TranslationKey } from "../lib/i18n";
import {
  approveTask,
  approveTasks,
  cancelTask,
  deleteTask,
  deleteTasks,
  pauseTask,
  resumeTask,
  retryTask,
  getTaskLogs,
  listTasks,
  listen,
  openPath,
  revealItemInDir,
  TASK_DELETED_EVENT,
  TASK_UPDATED_EVENT,
  type Task,
  type TaskDeletedPayload,
  type PipelineStageKind,
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
  Link2,
  AudioLines,
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
    review: "warning",
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
  return ["review", "done", "error", "cancelled", "paused"].includes(task.status);
}

const pipelineStageKeys: Record<PipelineStageKind, TranslationKey> = {
  transcribe: "home.stageTranscribe",
  translate: "home.stageTranslate",
  "subtitle-review": "home.stageSubtitleReview",
  dub: "home.stageDub",
  "dubbing-review": "home.stageDubbingReview",
  compose: "home.stageCompose",
  done: "home.stageDeliver",
};

function PipelineRail({ task }: { task: Task }) {
  const { t } = useI18n();
  if (!task.pipeline?.stages?.length) return null;
  return (
    <div className="mt-4 rounded-xl border border-border-subtle bg-surface-overlay/45 p-3.5" data-testid="task-pipeline-rail">
      <div className="mb-2.5 flex items-center justify-between gap-3">
        <p className="text-xs font-bold uppercase tracking-[0.1em] text-text-tertiary">{t("tasks.pipelineProgress")}</p>
        {task.status === "review" && <span className="text-xs font-semibold text-warning">{t("tasks.pipelineWaiting")}</span>}
      </div>
      <ol className="flex min-w-0 flex-wrap items-center gap-2">
        {task.pipeline.stages.map((stage, index) => {
          const isCurrent = task.pipeline?.current_stage === stage.kind;
          const color = stage.status === "done"
            ? "border-success/25 bg-success/10 text-success"
            : stage.status === "running"
              ? "border-brand/30 bg-brand/10 text-brand"
              : stage.status === "review"
                ? "border-warning/30 bg-warning/10 text-warning"
                : stage.status === "error"
                  ? "border-danger/30 bg-danger/10 text-danger"
                  : "border-border-subtle bg-surface-overlay text-text-tertiary";
          return (
            <li key={stage.kind} className="flex items-center gap-2">
              <span
                className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1.5 text-[11px] font-semibold ${color} ${isCurrent ? "ring-2 ring-brand/15" : ""}`}
                title={stage.message || undefined}
              >
                <span className={`h-1.5 w-1.5 rounded-full ${stage.status === "done" ? "bg-success" : stage.status === "running" ? "bg-brand animate-pulse" : stage.status === "review" ? "bg-warning" : stage.status === "error" ? "bg-danger" : "bg-border-strong"}`} />
                {t(pipelineStageKeys[stage.kind])}
                {stage.status === "running" && stage.progress > 0 && ` ${Math.round(stage.progress * 100)}%`}
              </span>
              {index < task.pipeline!.stages!.length - 1 && <span className="text-xs text-border-strong">→</span>}
            </li>
          );
        })}
      </ol>
    </div>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export default function TasksPage() {
  const { t } = useI18n();
  const navigate = useNavigate();
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeLogTaskId, setActiveLogTaskId] = useState<string | null>(null);
  const [logsText, setLogsText] = useState("");
  const [copied, setCopied] = useState(false);
  const [selectedTaskIds, setSelectedTaskIds] = useState<string[]>([]);
  const [pendingDeleteTaskIds, setPendingDeleteTaskIds] = useState<string[] | null>(null);
  const [deletingTaskIds, setDeletingTaskIds] = useState<string[]>([]);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [approvingTaskIds, setApprovingTaskIds] = useState<string[]>([]);
  const [actionError, setActionError] = useState<string | null>(null);
  const logContainerRef = useRef<HTMLPreElement>(null);

  const deletableTasks = tasks.filter(canDeleteTask);
  const selectedDeletableIds = selectedTaskIds.filter((taskId) =>
    deletableTasks.some((task) => task.id === taskId),
  );
  const allDeletableSelected =
    deletableTasks.length > 0 && selectedDeletableIds.length === deletableTasks.length;
  const selectedReviewIds = selectedTaskIds.filter((taskId) =>
    tasks.some((task) => task.id === taskId && task.status === "review"),
  );
  const pendingDeleteTasks = pendingDeleteTaskIds
    ? pendingDeleteTaskIds
        .map((taskId) => tasks.find((task) => task.id === taskId))
        .filter((task): task is Task => Boolean(task))
    : [];

  const handleOpenFolder = async (outputPath: string) => {
    setActionError(null);
    try {
      await revealItemInDir(outputPath);
    } catch (e) {
      setActionError(errorMessage(e));
      console.error("Failed to open directory", e);
    }
  };

  const handleOpenFile = async (outputPath: string) => {
    setActionError(null);
    try {
      await openPath(outputPath);
    } catch (e) {
      setActionError(errorMessage(e));
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
    setActionError(null);
    try {
      const task = await cancelTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      setActionError(errorMessage(e));
      console.error("Failed to cancel task", e);
    }
  };

  const handlePause = async (taskId: string) => {
    setActionError(null);
    try {
      const task = await pauseTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      setActionError(errorMessage(e));
      console.error("Failed to pause task", e);
    }
  };

  const handleResume = async (taskId: string) => {
    setActionError(null);
    try {
      const task = await resumeTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      setActionError(errorMessage(e));
      console.error("Failed to resume task", e);
    }
  };

  const handleRetry = async (taskId: string) => {
    setActionError(null);
    try {
      const task = await retryTask(taskId);
      setTasks((currentTasks) => upsertTask(currentTasks, task));
    } catch (e) {
      setActionError(errorMessage(e));
      console.error("Failed to retry task", e);
    }
  };

  const handleApprove = async (taskIds: string[]) => {
    if (taskIds.length === 0) return;
    setApprovingTaskIds(taskIds);
    setActionError(null);
    try {
      const approved = taskIds.length === 1
        ? [await approveTask(taskIds[0])]
        : await approveTasks(taskIds);
      setTasks((currentTasks) => approved.reduce(upsertTask, currentTasks));
      const approvedIds = new Set(approved.map((task) => task.id));
      setSelectedTaskIds((currentIds) => currentIds.filter((taskId) => !approvedIds.has(taskId)));
    } catch (error) {
      setActionError(errorMessage(error));
      console.error("Failed to approve task", error);
    } finally {
      setApprovingTaskIds([]);
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
    <div className="page-shell space-y-7">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h2 className="font-display text-display font-bold tracking-tight text-text-primary">{t("tasks.title")}</h2>
        <div className="flex flex-wrap items-center gap-3">
          {tasks.length > 0 && (
            <>
              <label className="glass-control inline-flex h-9 cursor-pointer select-none items-center gap-2 rounded-xl px-3.5 text-sm font-semibold text-text-secondary transition hover:bg-surface-overlay">
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
                onClick={() => void handleApprove(selectedReviewIds)}
                disabled={selectedReviewIds.length === 0 || approvingTaskIds.length > 0}
                size="sm"
                variant="primary"
              >
                <CheckCircle size={14} />
                <span>{t("tasks.approveSelected") + (selectedReviewIds.length > 0 ? ` (${selectedReviewIds.length})` : "")}</span>
              </Button>
              <Button
                type="button"
                onClick={() => openDeleteDialog(selectedDeletableIds)}
                disabled={selectedDeletableIds.length === 0}
                size="sm"
                variant="danger"
              >
                <Trash2 size={14} />
                <span>{t("tasks.deleteSelected") + (selectedDeletableIds.length > 0 ? ` (${selectedDeletableIds.length})` : "")}</span>
              </Button>
            </>
          )}
          <Button
            onClick={refresh}
            variant="secondary"
            size="sm"
          >
            <RefreshCw size={14} className={loading ? "animate-spin" : ""} />
            <span>{t("tasks.refresh")}</span>
          </Button>
        </div>
      </div>

      {actionError && (
        <div className="rounded-xl border border-danger/20 bg-danger/10 px-3.5 py-3 text-sm text-danger" role="alert">
          {actionError}
        </div>
      )}

      {loading && tasks.length === 0 ? (
        <div className="text-text-tertiary py-16 text-center text-sm">{t("tasks.loading")}</div>
      ) : tasks.length === 0 ? (
        <Card className="py-16 px-6 text-center border-dashed">
          <p className="text-lg font-semibold text-text-primary">{t("tasks.noTasks")}</p>
          <p className="mt-2 text-sm leading-6 text-text-tertiary">
            {t("tasks.noTasksDesc")}
          </p>
        </Card>
      ) : (
        <div className="space-y-4">
          {tasks.map((task) => {
            const isSelected = selectedTaskIds.includes(task.id);
            const reviewStage = task.pipeline?.current_stage;
            const needsAlignmentReview = reviewStage === "dub";
            const reviewTitle = needsAlignmentReview
              ? t("tasks.dubbingAlignmentReviewTitle")
              : reviewStage === "dubbing-review"
              ? t("tasks.dubbingReviewTitle")
              : (task.pipeline ? t("tasks.subtitleReviewTitle") : t("tasks.reviewTitle"));
            const reviewDescription = needsAlignmentReview
              ? t("tasks.dubbingAlignmentReviewDesc")
              : reviewStage === "dubbing-review"
              ? t("tasks.dubbingReviewDesc")
              : (task.pipeline ? t("tasks.subtitleReviewDesc") : t("tasks.reviewDesc"));
            const artifactPaths = [
              { label: t("tasks.artifactSubtitle"), path: task.pipeline?.subtitle_output_path ?? task.output_path },
              { label: t("tasks.artifactDubbing"), path: task.pipeline?.dubbed_audio_path },
              { label: t("tasks.artifactVideo"), path: task.pipeline?.final_video_path },
            ].filter((artifact): artifact is { label: string; path: string } => Boolean(artifact.path));
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
                    <h4 className="truncate font-display text-lg font-semibold tracking-tight text-text-primary">{task.media_name}</h4>
                    <p className="mt-1.5 truncate font-mono text-sm text-text-tertiary" title={task.media_path}>
                      {task.media_path}
                    </p>
                    <div className="mt-3 flex flex-wrap items-center gap-2">
                      <TaskTypeLabel type={task.task_type} />
                      <span className="text-[10px] text-border-strong">|</span>
                      {task.provided_subtitle_path ? (
                        <Badge variant="success" title={task.provided_subtitle_path}>
                          <Link2 size={11} className="mr-1" />
                          {t("tasks.usingPairedSubtitle")}
                        </Badge>
                      ) : (
                        <span className="text-sm text-text-secondary">
                          {task.engine_id} · {task.model_id}
                        </span>
                      )}
                      <span className="text-sm text-text-secondary">
                        {task.source_language && ` · ${task.source_language}`}
                        {task.target_language && ` → ${task.target_language}`}
                        {" · "}{task.output_format.toUpperCase()}
                      </span>
                    </div>
                  </div>

                  <div className="col-span-2 flex items-center justify-between gap-4 border-t border-border-subtle pt-3 sm:col-span-1 sm:justify-end sm:border-0 sm:pt-0">
                    <StatusPill status={task.status} />

                    <div className="flex items-center gap-2">
                      {/* Log button */}
                      <button
                        type="button"
                        title={t("tasks.viewLogs")}
                        aria-label={t("tasks.viewLogs")}
                        onClick={() => setActiveLogTaskId(task.id)}
                        className="rounded-lg p-2 text-text-secondary transition hover:bg-surface-overlay hover:text-text-primary"
                      >
                        <FileText size={16} />
                      </button>

                      {/* Pause button */}
                      {(task.status === "running" || task.status === "pending") && (
                        <button
                          type="button"
                          title={t("tasks.pauseTask")}
                          aria-label={t("tasks.pauseTask")}
                          onClick={() => handlePause(task.id)}
                          className="rounded-lg p-2 text-warning transition hover:bg-warning/10"
                        >
                          <Pause size={16} />
                        </button>
                      )}

                      {/* Resume button */}
                      {task.status === "paused" && (
                        <button
                          type="button"
                          title={t("tasks.resumeTask")}
                          aria-label={t("tasks.resumeTask")}
                          onClick={() => handleResume(task.id)}
                          className="rounded-lg p-2 text-success transition hover:bg-success/10"
                        >
                          <Play size={16} />
                        </button>
                      )}

                      {/* Retry button */}
                      {(task.status === "error" || task.status === "cancelled") && (
                        <button
                          type="button"
                          title={t("tasks.retryTask")}
                          aria-label={t("tasks.retryTask")}
                          onClick={() => handleRetry(task.id)}
                          className="rounded-lg p-2 text-brand transition hover:bg-brand-subtle"
                        >
                          <RotateCcw size={16} />
                        </button>
                      )}

                      {/* Cancel button */}
                      {(task.status === "running" || task.status === "pending" || task.status === "paused") && (
                        <button
                          type="button"
                          title={t("tasks.cancelTask")}
                          aria-label={t("tasks.cancelTask")}
                          onClick={() => handleCancel(task.id)}
                          className="rounded-lg p-2 text-danger transition hover:bg-danger/10"
                        >
                          <XCircle size={16} />
                        </button>
                      )}
                      {canDeleteTask(task) && (
                        <button
                          type="button"
                          title={t("tasks.deleteTaskRecord")}
                          aria-label={t("tasks.deleteTaskRecord")}
                          onClick={() => openDeleteDialog([task.id])}
                          disabled={deletingTaskIds.includes(task.id)}
                          className="rounded-lg p-2 text-text-tertiary transition hover:bg-danger/10 hover:text-danger disabled:opacity-45"
                        >
                          <Trash2 size={16} />
                        </button>
                      )}
                    </div>
                  </div>
                </div>

                {task.status !== "pending" && (
                  <div className="mt-4 space-y-2">
                    <Progress value={Math.round(task.progress * 100)} />
                    <div className="flex items-center justify-between text-sm">
                      <p className="text-text-secondary truncate max-w-[80%]">
                        {task.status_message}
                      </p>
                      <p className="font-semibold text-text-primary shrink-0">
                        {Math.round(task.progress * 100)}%
                      </p>
                    </div>
                  </div>
                )}

                <PipelineRail task={task} />

                {(task.status === "review" || artifactPaths.length > 0) && (
                  <div className="mt-4 space-y-3 rounded-xl border border-border-subtle bg-surface-overlay p-3.5 text-sm">
                    {task.status === "review" && (
                      <div className="flex flex-col gap-3 rounded-xl border border-warning/20 bg-warning/10 p-3 sm:flex-row sm:items-center sm:justify-between">
                        <div className="flex min-w-0 items-start gap-2.5">
                          <AlertCircle size={16} className="mt-0.5 shrink-0 text-warning" />
                          <div>
                            <p className="font-semibold text-text-primary">{reviewTitle}</p>
                            <p className="mt-1 text-xs leading-5 text-text-secondary">{reviewDescription}</p>
                          </div>
                        </div>
                        <div className="flex shrink-0 flex-wrap gap-2">
                          {needsAlignmentReview && task.pipeline?.dubbing_session_id && (
                            <Button
                              type="button"
                              onClick={() => navigate(`/dubbing?session=${encodeURIComponent(task.pipeline!.dubbing_session_id!)}`)}
                              variant="secondary"
                              size="sm"
                            >
                              <AudioLines size={14} />
                              {t("tasks.openDubbingWorkbench")}
                            </Button>
                          )}
                          <Button
                            type="button"
                            onClick={() => void handleApprove([task.id])}
                            disabled={approvingTaskIds.includes(task.id)}
                            variant="primary"
                            size="sm"
                          >
                            <CheckCircle size={14} />
                            {approvingTaskIds.includes(task.id)
                              ? t("tasks.approving")
                              : (needsAlignmentReview
                                ? t("tasks.retryDubbingAlignment")
                                : task.pipeline ? t("tasks.approveAndContinue") : t("tasks.approveTask"))}
                          </Button>
                        </div>
                      </div>
                    )}
                    <div className="space-y-2.5">
                      {artifactPaths.map((artifact) => (
                        <div key={`${artifact.label}-${artifact.path}`} className="flex flex-col gap-2 rounded-lg border border-border-subtle bg-surface px-3 py-2.5 sm:flex-row sm:items-center sm:justify-between">
                          <div className="min-w-0">
                            <p className="text-[10px] font-bold uppercase tracking-[0.08em] text-text-tertiary">{artifact.label}</p>
                            <p className="mt-1 truncate font-mono text-xs font-semibold text-text-secondary" title={artifact.path}>{artifact.path}</p>
                          </div>
                          <div className="flex shrink-0 gap-2">
                            <Button type="button" onClick={() => handleOpenFile(artifact.path)} variant="secondary" size="sm">{t("tasks.openOutputFile")}</Button>
                            <Button type="button" onClick={() => handleOpenFolder(artifact.path)} variant="secondary" size="sm">{t("tasks.openOutputDir")}</Button>
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
                {task.error && (
                  <div className="mt-3.5 break-all rounded-xl border border-danger/20 bg-danger/10 p-3.5 font-mono text-sm leading-6 text-danger">
                    {t("tasks.errorLog")}{task.error}
                  </div>
                )}
              </Card>
            );
          })}
        </div>
      )}

      {/* Delete Confirmation Modal */}
      {pendingDeleteTaskIds && createPortal(
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4" role="presentation">
          <Card
            className="w-full max-w-md bg-surface-overlay p-6 shadow-lg border border-border-default"
            role="dialog"
            aria-modal="true"
            aria-labelledby="delete-task-title"
          >
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-danger/10 p-2 text-danger">
                <AlertCircle size={20} />
              </div>
              <div className="min-w-0">
                <h3 id="delete-task-title" className="font-semibold text-text-primary text-h2 mb-1.5">
                  {t("tasks.deleteModalTitle")}
                </h3>
                <p className="text-sm leading-6 text-text-secondary">
                  {t("tasks.deleteModalDesc")}
                </p>
                {pendingDeleteTasks.length > 0 && (
                  <p className="mt-3 truncate font-mono text-[11px] text-text-tertiary" title={pendingDeleteTasks[0].media_name}>
                    {pendingDeleteTasks[0].media_name}
                    {pendingDeleteTaskIds.length > 1 && ` (+${pendingDeleteTaskIds.length - 1})`}
                  </p>
                )}
                {deleteError && (
                  <p className="mt-3 rounded-xl border border-danger/20 bg-danger/10 px-3.5 py-2.5 text-sm text-danger">
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
        </div>,
        document.body,
      )}

      {/* Logs Modal */}
      {activeLogTaskId && createPortal(
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm" role="presentation">
          <div
            className="bg-surface-overlay rounded-2xl w-full max-w-3xl h-[80vh] flex flex-col shadow-2xl border border-border-default overflow-hidden"
            role="dialog"
            aria-modal="true"
            aria-labelledby="task-log-title"
          >
            {/* Modal Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-border-subtle bg-surface">
              <div className="min-w-0">
                <h3 id="task-log-title" className="text-base font-bold text-text-primary truncate">
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
                  size="sm"
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
                className="h-full w-full overflow-y-auto whitespace-pre-wrap font-mono text-sm leading-relaxed text-green-400 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-gray-800"
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
                  case "review":
                    return t("tasks.log.review");
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
        </div>,
        document.body,
      )}
    </div>
  );
}
