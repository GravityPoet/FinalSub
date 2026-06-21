import { useState, useCallback, useEffect, useRef } from 'react';
import ProofreadImport from './ProofreadImport';
import ProofreadFileList from './ProofreadFileList';
import ProofreadEditor from './ProofreadEditor';
import ProofreadTaskList from './ProofreadTaskList';
import { ProofreadTask } from './types';
import { Plus, History } from 'lucide-react';
import {
  PendingFile,
  loadPendingFileFromItem,
  pendingFileToSaveFormat,
} from './proofreadUtils';
import { loadProofreadTasks, saveProofreadTasks } from '../../lib/tauri';
import { ToastProvider } from './Toast';
import { useI18n } from '../../lib/i18n';

type WorkflowStage = 'import' | 'list' | 'edit';

export async function getProofreadTasks(): Promise<ProofreadTask[]> {
  try {
    const raw = await loadProofreadTasks();
    if (!raw || raw.trim() === '') return [];
    return JSON.parse(raw) as ProofreadTask[];
  } catch (e) {
    console.error('Failed to load proofread tasks:', e);
    return [];
  }
}

export async function persistProofreadTasks(tasks: ProofreadTask[]): Promise<void> {
  await saveProofreadTasks(JSON.stringify(tasks));
}

export default function ProofreadPage() {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<'new' | 'history'>('new');
  const [stage, setStage] = useState<WorkflowStage>('import');
  const [pendingFiles, setPendingFiles] = useState<PendingFile[]>([]);
  const [currentEditIndex, setCurrentEditIndex] = useState<number>(-1);
  const [savedTaskId, setSavedTaskId] = useState<string | null>(null);
  const [taskName, setTaskName] = useState<string>('');
  const [importType, setImportType] = useState<'video' | 'subtitle'>('video');

  const handleLoadTask = useCallback(async (task: ProofreadTask) => {
    const files: PendingFile[] = await Promise.all(
      task.items.map((item) => loadPendingFileFromItem(item)),
    );

    const hasVideo = task.items.some((item) => item.videoPath);
    setImportType(hasVideo ? 'video' : 'subtitle');

    setPendingFiles(files);
    setSavedTaskId(task.id);
    setTaskName(task.name);
    setStage('list');
    setActiveTab('new');
  }, []);

  const handleImportComplete = useCallback(
    (files: PendingFile[], type: 'video' | 'subtitle') => {
      setPendingFiles(files);
      setSavedTaskId(null);
      setImportType(type);
      const defaultName = files[0]?.fileName?.replace(/\.[^.]+$/, '') || '';
      setTaskName(defaultName);
      setStage('list');
    },
    [],
  );

  const handleStartProofread = useCallback((index: number) => {
    setCurrentEditIndex(index);
    setPendingFiles((prev) => {
      const next = [...prev];
      next[index] = { ...next[index], status: 'proofreading' };
      return next;
    });
    setStage('edit');
  }, []);

  const handleMarkComplete = useCallback(() => {
    setPendingFiles((prev) => {
      const next = [...prev];
      next[currentEditIndex] = {
        ...next[currentEditIndex],
        status: 'completed',
      };
      return next;
    });
    setCurrentEditIndex(-1);
    setStage('list');
  }, [currentEditIndex]);

  const handleBackToList = useCallback(() => {
    setCurrentEditIndex(-1);
    setStage('list');
  }, []);

  const handleUpdateFile = useCallback(
    (index: number, updates: Partial<PendingFile>) => {
      setPendingFiles((prev) => {
        const next = [...prev];
        next[index] = { ...next[index], ...updates };
        return next;
      });
    },
    [],
  );

  const handleRemoveFile = useCallback((index: number) => {
    setPendingFiles((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const handleAddFiles = useCallback((newFiles: PendingFile[]) => {
    setPendingFiles((prev) => [...prev, ...newFiles]);
  }, []);

  const handleSaveTask = useCallback(async () => {
    const items = pendingFiles.map(pendingFileToSaveFormat);
    const tasks = await getProofreadTasks();

    if (savedTaskId) {
      const idx = tasks.findIndex(t => t.id === savedTaskId);
      if (idx !== -1) {
        tasks[idx] = {
          ...tasks[idx],
          name: taskName,
          items,
          updatedAt: Date.now(),
        };
      }
      await persistProofreadTasks(tasks);
    } else {
      const newId = Math.random().toString(36).substring(2, 11);
      const newTask: ProofreadTask = {
        id: newId,
        name: taskName || pendingFiles[0]?.fileName?.replace(/\.[^.]+$/, '') || t('proofread.unnamedTask'),
        createdAt: Date.now(),
        updatedAt: Date.now(),
        items,
        currentItemIndex: 0,
        status: 'in_progress',
      };
      tasks.unshift(newTask);
      await persistProofreadTasks(tasks);
      setSavedTaskId(newId);
    }
    return true;
  }, [pendingFiles, savedTaskId, taskName]);

  const handleReset = useCallback(() => {
    setPendingFiles([]);
    setCurrentEditIndex(-1);
    setSavedTaskId(null);
    setTaskName('');
    setImportType('video');
    setStage('import');
  }, []);

  const isInitialMount = useRef(true);
  useEffect(() => {
    if (isInitialMount.current) {
      isInitialMount.current = false;
      return;
    }

    if (savedTaskId && pendingFiles.length > 0 && stage === 'list') {
      const autoSaveTimeout = setTimeout(async () => {
        try {
          await handleSaveTask();
        } catch (error) {
          console.error('Auto-save failed:', error);
        }
      }, 500);

      return () => clearTimeout(autoSaveTimeout);
    }
  }, [pendingFiles, savedTaskId, stage]);

  const renderStage = () => {
    switch (stage) {
      case 'import':
        return <ProofreadImport onImportComplete={handleImportComplete} />;

      case 'list':
        return (
          <ProofreadFileList
            files={pendingFiles}
            savedTaskId={savedTaskId}
            taskName={taskName}
            importType={importType}
            onTaskNameChange={setTaskName}
            onStartProofread={handleStartProofread}
            onUpdateFile={handleUpdateFile}
            onRemoveFile={handleRemoveFile}
            onAddFiles={handleAddFiles}
            onSaveTask={handleSaveTask}
            onReset={handleReset}
          />
        );

      case 'edit':
        const currentFile = pendingFiles[currentEditIndex];
        return (
          <ProofreadEditor
            file={currentFile}
            onMarkComplete={handleMarkComplete}
            onBack={handleBackToList}
          />
        );

      default:
        return null;
    }
  };

  return (
    <ToastProvider>
      <div className="h-full p-6 overflow-hidden flex flex-col bg-slate-900 text-slate-100">
        <div className="flex-shrink-0 flex space-x-2 bg-slate-800/80 p-1.5 rounded-lg w-fit border border-slate-700/50 mb-4">
          <button
            onClick={() => setActiveTab('new')}
            className={`flex items-center text-xs px-4 py-2 rounded-md font-medium transition-all ${
              activeTab === 'new'
                ? 'bg-blue-600 text-white shadow-md'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-700/50'
            }`}
          >
            <Plus className="w-3.5 h-3.5 mr-1.5" />
            {t('proofread.newTask')}
          </button>
          <button
            onClick={() => setActiveTab('history')}
            className={`flex items-center text-xs px-4 py-2 rounded-md font-medium transition-all ${
              activeTab === 'history'
                ? 'bg-blue-600 text-white shadow-md'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-700/50'
            }`}
          >
            <History className="w-3.5 h-3.5 mr-1.5" />
            {t('proofread.historyTasks')}
          </button>
        </div>

        <div className="flex-1 min-h-0 overflow-hidden flex flex-col">
          {activeTab === 'new' ? (
            <div className="flex-1 overflow-auto">{renderStage()}</div>
          ) : (
            <div className="flex-1 overflow-auto">
              <ProofreadTaskList onLoadTask={handleLoadTask} />
            </div>
          )}
        </div>
      </div>
    </ToastProvider>
  );
}
