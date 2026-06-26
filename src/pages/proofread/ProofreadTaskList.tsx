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
import { useI18n } from '../../lib/i18n';

import { Button } from '../../components/ui/Button';
import { Card } from '../../components/ui/Card';
import { Badge } from '../../components/ui/Badge';
import { Progress } from '../../components/ui/Progress';

interface ProofreadTaskListProps {
  onLoadTask: (task: ProofreadTask) => void;
}

export default function ProofreadTaskList({
  onLoadTask,
}: ProofreadTaskListProps) {
  const { locale, t } = useI18n();
  const [tasks, setTasks] = useState<ProofreadTask[]>([]);
  const [loading, setLoading] = useState(true);
  const [pendingDelete, setPendingDelete] = useState<ProofreadTask | null>(null);
  const { showToast } = useToast();

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

  const handleDeleteTask = useCallback(
    async (taskId: string) => {
      try {
        const updated = tasks.filter((t) => t.id !== taskId);
        await persistProofreadTasks(updated);
        setPendingDelete(null);
        showToast('success', t('proofread.tasks.deleteSuccess'));
        await loadTasks();
      } catch (error) {
        console.error('Failed to delete task:', error);
        showToast('error', t('proofread.tasks.deleteFailed'));
      }
    },
    [tasks, loadTasks, showToast, t],
  );

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

  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleString(locale === 'en' ? 'en-US' : 'zh-CN', {
      hour12: false,
    });
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-48">
        <Loader2 className="w-8 h-8 animate-spin text-brand" />
      </div>
    );
  }

  if (tasks.length === 0) {
    return (
      <div className="text-center py-16 text-text-tertiary">
        <FileText className="w-16 h-16 mx-auto mb-4 opacity-30 text-text-tertiary" />
        <p className="text-base font-medium text-text-secondary">{t('proofread.tasks.noHistory')}</p>
        <p className="text-xs text-text-tertiary mt-2">{t('proofread.tasks.noHistoryDesc')}</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {tasks.map((task) => {
        const progress = getTaskProgress(task);
        return (
          <Card
            key={task.id}
            className="p-5 flex items-start justify-between gap-5"
          >
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-3 mb-2.5">
                <h3 className="font-semibold text-text-primary text-base truncate" title={task.name}>
                  {task.name}
                </h3>
                {task.status === 'completed' ? (
                  <Badge variant="success">
                    <CheckCircle2 className="w-3 h-3 mr-1" />
                    {t('proofread.tasks.completed')}
                  </Badge>
                ) : (
                  <Badge variant="info">
                    <Clock className="w-3 h-3 mr-1" />
                    {t('proofread.tasks.inProgress')}
                  </Badge>
                )}
              </div>

              <div className="flex items-center gap-4 text-xs text-text-secondary mb-4">
                <span>{t('proofread.tasks.filesCount').replace('{count}', String(task.items.length))}</span>
                <span className="w-1 h-1 bg-border-strong rounded-full" />
                <span>{t('proofread.tasks.lastUpdated')}{formatDate(task.updatedAt)}</span>
              </div>

              <div className="flex items-center gap-3">
                <Progress value={progress.percent} className="flex-1" />
                <span className="text-xs text-text-secondary w-24 text-right font-medium">
                  {progress.completed}/{progress.total} ({progress.percent}%)
                </span>
              </div>
            </div>

            <div className="flex items-center gap-2 flex-shrink-0">
              <Button
                onClick={() => onLoadTask(task)}
                variant="primary"
                size="sm"
              >
                <Play size={13} />
                <span>{task.status === 'completed' ? t('proofread.tasks.view') : t('proofread.tasks.continue')}</span>
              </Button>
              <button
                onClick={() => setPendingDelete(task)}
                className="p-2 hover:bg-danger/10 rounded-lg transition-colors text-text-tertiary hover:text-danger"
                title={t('proofread.tasks.delete')}
              >
                <Trash2 className="w-4 h-4 text-danger" />
              </button>
            </div>
          </Card>
        );
      })}

      {pendingDelete && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
          <Card className="w-full max-w-md bg-surface-overlay p-6 shadow-lg border border-border-default">
            <div className="mb-5 flex items-start gap-3">
              <div className="rounded-full bg-danger/10 p-2 text-danger">
                <AlertCircle className="h-5 w-5" />
              </div>
              <div className="min-w-0">
                <h3 className="font-semibold text-text-primary text-h2 mb-1.5">{t('proofread.tasks.deleteModalTitle')}</h3>
                <p className="text-xs text-text-secondary leading-5">
                  {t('proofread.tasks.deleteModalDesc').replace('{name}', pendingDelete.name)}
                </p>
              </div>
            </div>
            <div className="flex justify-end gap-2.5">
              <Button
                type="button"
                onClick={() => setPendingDelete(null)}
                variant="secondary"
                size="sm"
              >
                {t('proofread.tasks.cancel')}
              </Button>
              <Button
                type="button"
                onClick={() => handleDeleteTask(pendingDelete.id)}
                variant="danger"
                size="sm"
              >
                {t('proofread.tasks.deleteConfirm')}
              </Button>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}
