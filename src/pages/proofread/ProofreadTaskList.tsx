import { useState, useCallback, useEffect } from 'react';
import {
  Play,
  Trash2,
  Clock,
  CheckCircle2,
  Loader2,
  FileText,
  AlertCircle,
} from 'lucide-react';
import { ProofreadTask } from './types';
import { getProofreadTasks, persistProofreadTasks } from './ProofreadPage';
import { useToast } from './Toast';

interface ProofreadTaskListProps {
  onLoadTask: (task: ProofreadTask) => void;
}

export default function ProofreadTaskList({
  onLoadTask,
}: ProofreadTaskListProps) {
  const [tasks, setTasks] = useState<ProofreadTask[]>([]);
  const [loading, setLoading] = useState(true);
  const [pendingDelete, setPendingDelete] = useState<ProofreadTask | null>(null);
  const { showToast } = useToast();

  // 加载任务列表
  const loadTasks = useCallback(async () => {
    setLoading(true);
    try {
      const allTasks = await getProofreadTasks();
      setTasks(allTasks);
    } catch (error) {
      console.error('Failed to load tasks:', error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadTasks();
  }, [loadTasks]);

  // 删除任务
  const handleDeleteTask = useCallback(
    async (taskId: string) => {
      try {
        const updated = tasks.filter((t) => t.id !== taskId);
        await persistProofreadTasks(updated);
        setPendingDelete(null);
        showToast('success', '任务已删除');
        await loadTasks();
      } catch (error) {
        console.error('Failed to delete task:', error);
        showToast('error', '删除任务失败，请稍后重试');
      }
    },
    [tasks, loadTasks, showToast],
  );

  // 计算任务进度
  const getTaskProgress = (task: ProofreadTask) => {
    const completed = task.items.filter((i) => i.status === 'completed').length;
    return {
      completed,
      total: task.items.length,
      percent:
        task.items.length > 0
          ? Math.round((completed / task.items.length) * 100)
          : 0,
    };
  };

  // 格式化时间
  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleString('zh-CN', {
      hour12: false,
    });
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-48">
        <Loader2 className="w-8 h-8 animate-spin text-blue-500" />
      </div>
    );
  }

  if (tasks.length === 0) {
    return (
      <div className="text-center py-16 text-slate-500">
        <FileText className="w-16 h-16 mx-auto mb-4 opacity-30 text-slate-400" />
        <p className="text-base font-medium">暂无历史任务</p>
        <p className="text-xs text-slate-400 mt-2">保存任务后将在此处进行管理</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {tasks.map((task) => {
        const progress = getTaskProgress(task);
        return (
          <div
            key={task.id}
            className="bg-slate-800/40 border border-slate-700/50 rounded-xl p-5 hover:bg-slate-800/60 transition-colors shadow-md flex items-start justify-between gap-5"
          >
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-3 mb-2.5">
                <h3 className="font-semibold text-slate-100 text-base truncate" title={task.name}>
                  {task.name}
                </h3>
                {task.status === 'completed' ? (
                  <span className="flex items-center text-[10px] bg-emerald-950 text-emerald-400 border border-emerald-800/30 px-2 py-0.5 rounded font-medium">
                    <CheckCircle2 className="w-3 h-3 mr-1" />
                    已完成
                  </span>
                ) : (
                  <span className="flex items-center text-[10px] bg-blue-950 text-blue-400 border border-blue-800/30 px-2 py-0.5 rounded font-medium">
                    <Clock className="w-3 h-3 mr-1" />
                    进行中
                  </span>
                )}
              </div>

              <div className="flex items-center gap-4 text-xs text-slate-400 mb-4">
                <span>{task.items.length} 个文件</span>
                <span className="w-1 h-1 bg-slate-600 rounded-full" />
                <span>最后更新: {formatDate(task.updatedAt)}</span>
              </div>

              <div className="flex items-center gap-3">
                <div className="flex-1 bg-slate-900 rounded-full h-2.5 overflow-hidden">
                  <div
                    className="bg-blue-600 h-full rounded-full transition-all duration-300"
                    style={{ width: `${progress.percent}%` }}
                  />
                </div>
                <span className="text-xs text-slate-400 w-24 text-right font-medium">
                  {progress.completed}/{progress.total} ({progress.percent}%)
                </span>
              </div>
            </div>

            <div className="flex items-center gap-2 flex-shrink-0">
              <button
                onClick={() => onLoadTask(task)}
                className="flex items-center text-xs bg-blue-600 hover:bg-blue-700 text-white px-4.5 py-2 rounded-lg transition-colors font-medium shadow-md shadow-blue-500/5"
              >
                <Play className="w-3.5 h-3.5 mr-1.5" />
                {task.status === 'completed' ? '查看' : '继续'}
              </button>
              <button
                onClick={() => setPendingDelete(task)}
                className="p-2 hover:bg-red-950/30 rounded-lg transition-colors text-slate-400 hover:text-red-400"
                title="删除任务"
              >
                <Trash2 className="w-4 h-4 text-red-500" />
              </button>
            </div>
          </div>
        );
      })}

      {pendingDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
          <div className="w-full max-w-md rounded-xl border border-slate-700 bg-slate-900 p-5 shadow-2xl">
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-red-950/50 p-2 text-red-400">
                <AlertCircle className="h-5 w-5" />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-slate-100">删除校对任务</h3>
                <p className="mt-1 text-sm leading-6 text-slate-300">
                  将删除「{pendingDelete.name}」的历史记录，已导出的字幕文件不会被删除。
                </p>
              </div>
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setPendingDelete(null)}
                className="rounded-lg border border-slate-700 px-3 py-1.5 text-sm text-slate-200 hover:bg-slate-800"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => handleDeleteTask(pendingDelete.id)}
                className="rounded-lg bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700"
              >
                删除
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
