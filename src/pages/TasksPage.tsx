import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { cancelTask, listTasks, TASK_UPDATED_EVENT, type Task } from "../lib/tauri";
import { RefreshCw, XCircle } from "lucide-react";

function StatusPill({ status }: { status: string }) {
  const colors: Record<string, string> = {
    pending: "bg-gray-100 text-gray-600",
    running: "bg-blue-100 text-blue-700",
    paused: "bg-yellow-100 text-yellow-700",
    cancelled: "bg-gray-100 text-gray-500",
    done: "bg-green-100 text-green-700",
    error: "bg-red-100 text-red-700",
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
    <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${colors[status] ?? colors.pending}`}>
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

  const handleCancel = async (taskId: string) => {
    const task = await cancelTask(taskId);
    setTasks((currentTasks) => upsertTask(currentTasks, task));
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">任务队列</h2>
        <button
          onClick={refresh}
          className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white"
        >
          <RefreshCw size={16} /> 刷新
        </button>
      </div>

      {loading ? (
        <div className="text-gray-500">正在加载任务...</div>
      ) : tasks.length === 0 ? (
        <div className="text-gray-500 bg-white dark:bg-gray-800 rounded-lg p-8 text-center border border-gray-200 dark:border-gray-700">
          暂无任务。
        </div>
      ) : (
        <div className="space-y-3">
          {tasks.map((task) => (
            <div
              key={task.id}
              className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700"
            >
              <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto]">
                <div className="min-w-0">
                  <h4 className="font-medium text-gray-900 dark:text-white">{task.media_name}</h4>
                  <p className="mt-1 truncate text-xs text-gray-500" title={task.media_path}>
                    {task.media_path}
                  </p>
                  <p className="mt-1 text-xs text-gray-500">
                    <TaskTypeLabel type={task.task_type} />
                    {" · "}{task.engine_id} · {task.model_id}
                    {task.source_language && ` · ${task.source_language}`}
                    {task.target_language && ` → ${task.target_language}`}
                    {" · "}{task.output_format.toUpperCase()}
                  </p>
                </div>
                <div className="flex items-center justify-end gap-2">
                  <StatusPill status={task.status} />
                  {task.status === "running" || task.status === "pending" ? (
                    <button
                      type="button"
                      aria-label="Cancel task"
                      onClick={() => handleCancel(task.id)}
                      className="text-red-500 hover:text-red-700"
                    >
                      <XCircle size={18} />
                    </button>
                  ) : null}
                </div>
              </div>
              {task.status !== "pending" && (
                <div className="mt-3">
                  <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                    <div
                      className="bg-blue-600 h-2 rounded-full transition-all"
                      style={{ width: `${Math.round(task.progress * 100)}%` }}
                    />
                  </div>
                  <p className="text-xs text-gray-500 mt-1">{task.status_message}</p>
                </div>
              )}
              {task.error && <p className="text-sm text-red-600 mt-2">{task.error}</p>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
