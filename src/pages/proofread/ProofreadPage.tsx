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
import { useLocation, useSearchParams } from 'react-router-dom';
import { useWorkspaceDraft } from '../../lib/workspaceDraft';
import { Button } from '../../components/ui/Button';
import { isSubtitleDraft, type SubtitleDraft } from './useStandaloneSubtitles';
import { getTaskReviewSource } from '../../lib/tauri';

type WorkflowStage = 'import' | 'list' | 'edit';

export async function getProofreadTasks(): Promise<ProofreadTask[]> {
  const raw = await loadProofreadTasks();
  if (!raw || raw.trim() === '') return [];
  const parsed: unknown = JSON.parse(raw);
  if (!Array.isArray(parsed)) {
    throw new Error('Invalid proofread task history format');
  }
  return parsed as ProofreadTask[];
}

export async function persistProofreadTasks(tasks: ProofreadTask[]): Promise<void> {
  await saveProofreadTasks(JSON.stringify(tasks));
}

interface ProofreadWorkspace {
  activeTab: 'new' | 'history'; stage: WorkflowStage; pendingFiles: PendingFile[];
  currentEditIndex: number; savedTaskId: string | null; taskName: string; importType: 'video' | 'subtitle';
}
const emptyWorkspace: ProofreadWorkspace = { activeTab: 'new', stage: 'import', pendingFiles: [], currentEditIndex: -1, savedTaskId: null, taskName: '', importType: 'video' };
function validWorkspace(value: unknown): value is ProofreadWorkspace {
  if (!value || typeof value !== 'object') return false;
  const data = value as Partial<ProofreadWorkspace>;
  return (data.activeTab === 'new' || data.activeTab === 'history') && ['import', 'list', 'edit'].includes(data.stage ?? '')
    && typeof data.taskName === 'string' && (data.savedTaskId === null || typeof data.savedTaskId === 'string')
    && (data.importType === 'video' || data.importType === 'subtitle') && Number.isInteger(data.currentEditIndex)
    && Array.isArray(data.pendingFiles) && data.pendingFiles.every((file) => file && typeof file.id === 'string' && typeof file.fileName === 'string' && Array.isArray(file.detectedSubtitles)
      && (file.draft === undefined || isSubtitleDraft(file.draft)))
    && (data.stage !== 'edit' || (data.currentEditIndex! >= 0 && data.currentEditIndex! < data.pendingFiles.length));
}

export default function ProofreadPage() {
  const { t } = useI18n();
  const { draft: workspace, setDraft, ready, error: workspaceError, retry } = useWorkspaceDraft('proofread', emptyWorkspace, validWorkspace);
  const { activeTab, stage, pendingFiles, currentEditIndex, savedTaskId, taskName, importType } = workspace;
  const field = <K extends keyof ProofreadWorkspace>(key: K, update: ProofreadWorkspace[K] | ((value: ProofreadWorkspace[K]) => ProofreadWorkspace[K])) => setDraft((previous) => ({ ...previous, [key]: typeof update === 'function' ? update(previous[key]) : update }));
  const setActiveTab = (value: 'new' | 'history') => field('activeTab', value);
  const setStage = (value: WorkflowStage) => field('stage', value);
  const setPendingFiles = useCallback((value: PendingFile[] | ((previous: PendingFile[]) => PendingFile[])) => setDraft((previous) => ({ ...previous, pendingFiles: typeof value === 'function' ? value(previous.pendingFiles) : value })), [setDraft]);
  const setCurrentEditIndex = (value: number) => field('currentEditIndex', value);
  const setSavedTaskId = (value: string | null) => field('savedTaskId', value);
  const setTaskName = (value: string) => field('taskName', value);
  const setImportType = (value: 'video' | 'subtitle') => field('importType', value);
  const location = useLocation();
  const [params] = useSearchParams();
  const [taskLoadError, setTaskLoadError] = useState('');
  const originTaskId = location.pathname === '/proofread' ? params.get('task') : null;
  const workspaceRef = useRef(workspace);
  workspaceRef.current = workspace;
  useEffect(() => {
    if (!ready || !originTaskId) return;
    const cached = workspaceRef.current.pendingFiles.findIndex((file) => file.originTaskId === originTaskId && file.draft);
    if (cached >= 0) { setCurrentEditIndex(cached); setStage('edit'); setActiveTab('new'); return; }
    let active = true;
    setTaskLoadError('');
    getTaskReviewSource(originTaskId).then((source) => {
      if (!active) return;
      setPendingFiles((previous) => [{ id: originTaskId, originTaskId, originTaskVersion: source.version, reviewSourceContent: source.source_content, reviewTargetContent: source.target_content || undefined, fileName: (source.media_path || source.source_path).split(/[\\/]/).pop() || '', videoPath: source.media_path || undefined, selectedSource: source.source_path, selectedTarget: source.target_path || undefined, sourceLanguage: source.source_language, targetLanguage: source.target_language, detectedSubtitles: [], status: 'proofreading' }, ...previous.filter((file) => file.originTaskId !== originTaskId)]);
      setCurrentEditIndex(0);
      setStage('edit');
      setActiveTab('new');
    }).catch((error) => { if (active) setTaskLoadError(String(error)); });
    return () => { active = false; };
  }, [originTaskId, ready]);

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
        const previous = next[index];
        const pathsChanged = ('selectedSource' in updates && updates.selectedSource !== previous.selectedSource) || ('selectedTarget' in updates && updates.selectedTarget !== previous.selectedTarget);
        next[index] = { ...previous, ...updates, ...(pathsChanged ? { draft: undefined } : {}) };
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

    if (ready && savedTaskId && pendingFiles.length > 0 && stage === 'list') {
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
    if (taskLoadError) return <p role="alert" className="m-4 rounded-xl bg-danger/10 p-4 text-danger">{taskLoadError}</p>;
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
            key={currentFile.id}
            active={location.pathname === '/proofread'}
            file={currentFile}
            onDraftChange={(draft: SubtitleDraft) => handleUpdateFile(currentEditIndex, { draft })}
            onMarkComplete={handleMarkComplete}
            onBack={handleBackToList}
          />
        );

      default:
        return null;
    }
  };

  if (!ready) return <div className="space-y-3 p-4"><h2>{t('nav.proofread')}</h2>{workspaceError ? <div role="alert"><p>{t('workspace.loadFailed')}</p><p className="break-words text-sm">{workspaceError}</p><Button onClick={retry}>{t('common.retry')}</Button></div> : <p>{t('home.loading')}</p>}</div>;
  return (
    <ToastProvider>
      <div className="flex h-full flex-col overflow-hidden">
        {workspaceError && <p role="alert" className="rounded-xl bg-danger/10 p-3 text-sm text-danger">{t('workspace.saveFailed')} {workspaceError}</p>}
        <div className="glass-control mb-5 flex w-fit flex-shrink-0 space-x-2 rounded-xl p-1.5">
          <button
            onClick={() => setActiveTab('new')}
            className={`flex min-h-10 items-center rounded-lg px-4 py-2 text-sm font-semibold transition-all duration-150 ${
              activeTab === 'new'
                ? 'bg-brand text-white shadow-sm'
                : 'text-text-secondary hover:text-text-primary hover:bg-surface-overlay'
            }`}
          >
            <Plus className="mr-1.5 h-4 w-4" />
            {t('proofread.newTask')}
          </button>
          <button
            onClick={() => setActiveTab('history')}
            className={`flex min-h-10 items-center rounded-lg px-4 py-2 text-sm font-semibold transition-all duration-150 ${
              activeTab === 'history'
                ? 'bg-brand text-white shadow-sm'
                : 'text-text-secondary hover:text-text-primary hover:bg-surface-overlay'
            }`}
          >
            <History className="mr-1.5 h-4 w-4" />
            {t('proofread.historyTasks')}
          </button>
        </div>

        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
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
