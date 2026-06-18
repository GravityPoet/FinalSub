import { useEffect, useState } from "react";
import { listTasks, cancelTask, type Task } from "../lib/tauri";
import { XCircle, RefreshCw } from "lucide-react";

function StatusPill({ status }: { status: string }) {
  const colors: Record<string, string> = {
    pending: "bg-gray-100 text-gray-600",
    running: "bg-blue-100 text-blue-700",
    paused: "bg-yellow-100 text-yellow-700",
    cancelled: "bg-gray-100 text-gray-500",
    done: "bg-green-100 text-green-700",
    error: "bg-red-100 text-red-700",
  };
  return (
    <span className={`text-xs px-2 py-0.5 rounded-full font-medium ${colors[status] ?? colors.pending}`}>
      {status}
    </span>
  );
}

export default function TasksPage() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = () => {
    setLoading(true);
    listTasks()
      .then(setTasks)
      .catch(console.error)
      .finally(() => setLoading(false));
  };

  useEffect(() => { refresh(); }, []);

  const handleCancel = async (taskId: string) => {
    await cancelTask(taskId);
    refresh();
  };

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">Tasks</h2>
        <button
          onClick={refresh}
          className="flex items-center gap-2 text-sm text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white"
        >
          <RefreshCw size={16} /> Refresh
        </button>
      </div>

      {loading ? (
        <div className="text-gray-500">Loading tasks...</div>
      ) : tasks.length === 0 ? (
        <div className="text-gray-500 bg-white dark:bg-gray-800 rounded-lg p-8 text-center border border-gray-200 dark:border-gray-700">
          No tasks yet. Create one from the Home page.
        </div>
      ) : (
        <div className="space-y-3">
          {tasks.map((task) => (
            <div
              key={task.id}
              className="bg-white dark:bg-gray-800 rounded-lg p-4 border border-gray-200 dark:border-gray-700"
            >
              <div className="flex items-start justify-between">
                <div>
                  <h4 className="font-medium text-gray-900 dark:text-white">{task.media_name}</h4>
                  <p className="text-xs text-gray-500 mt-1">Engine: {task.engine_id} | Model: {task.model_id}</p>
                </div>
                <div className="flex items-center gap-2">
                  <StatusPill status={task.status} />
                  {task.status === "running" || task.status === "pending" ? (
                    <button onClick={() => handleCancel(task.id)} className="text-red-500 hover:text-red-700">
                      <XCircle size={18} />
                    </button>
                  ) : null}
                </div>
              </div>
              {task.status === "running" && (
                <div className="mt-3">
                  <div className="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                    <div
                      className="bg-blue-600 h-2 rounded-full transition-all"
                      style={{ width: `${task.progress * 100}%` }}
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
