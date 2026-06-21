import { useCallback, useState } from 'react';
import { useToast } from './Toast';
import {
  Play,
  Trash2,
  Save,
  CheckCircle2,
  Circle,
  Upload,
  RotateCcw,
  Loader2,
  Edit2,
  Plus,
  Check,
} from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import {
  PendingFile,
  createPendingFileFromVideo,
  createPendingFileFromSubtitle,
} from './proofreadUtils';
import { detectLanguageFromFilename } from './languageDetector';

interface ProofreadFileListProps {
  files: PendingFile[];
  savedTaskId: string | null;
  taskName: string;
  importType: 'video' | 'subtitle';
  onTaskNameChange: (name: string) => void;
  onStartProofread: (index: number) => void;
  onUpdateFile: (index: number, updates: Partial<PendingFile>) => void;
  onRemoveFile: (index: number) => void;
  onAddFiles: (files: PendingFile[]) => void;
  onSaveTask: () => Promise<boolean>;
  onReset: () => void;
}

export default function ProofreadFileList({
  files,
  savedTaskId,
  taskName,
  importType,
  onTaskNameChange,
  onStartProofread,
  onUpdateFile,
  onRemoveFile,
  onAddFiles,
  onSaveTask,
  onReset,
}: ProofreadFileListProps) {
  const [saving, setSaving] = useState(false);
  const [editingName, setEditingName] = useState(false);
  const [tempTaskName, setTempTaskName] = useState(taskName);
  const { showToast } = useToast();

  // 手动选择源字幕
  const handleSelectSourceSubtitle = useCallback(
    async (index: number) => {
      try {
        const selected = await open({
          multiple: false,
          directory: false,
          filters: [{ name: '字幕文件', extensions: ['srt', 'vtt', 'ass', 'ssa', 'lrc'] }],
        });
        if (!selected || Array.isArray(selected)) return;
        const filePath = selected;

        const langInfo = detectLanguageFromFilename(filePath);
        const language = langInfo?.code;

        const file = files[index];
        const exists = file.detectedSubtitles.some((s) => s.filePath === filePath);

        const updates: Partial<PendingFile> = {
          selectedSource: filePath,
          sourceLanguage: language,
        };

        if (!exists) {
          updates.detectedSubtitles = [
            ...file.detectedSubtitles,
            {
              filePath,
              type: 'source' as const,
              language,
              confidence: 100,
            },
          ];
        }

        onUpdateFile(index, updates);
      } catch (error) {
        console.error('Failed to select source subtitle:', error);
      }
    },
    [files, onUpdateFile],
  );

  // 手动选择翻译字幕
  const handleSelectTargetSubtitle = useCallback(
    async (index: number) => {
      try {
        const selected = await open({
          multiple: false,
          directory: false,
          filters: [{ name: '字幕文件', extensions: ['srt', 'vtt', 'ass', 'ssa', 'lrc'] }],
        });
        if (!selected || Array.isArray(selected)) return;
        const filePath = selected;

        const langInfo = detectLanguageFromFilename(filePath);
        const language = langInfo?.code;

        const file = files[index];
        const exists = file.detectedSubtitles.some((s) => s.filePath === filePath);

        const updates: Partial<PendingFile> = {
          selectedTarget: filePath,
          targetLanguage: language,
        };

        if (!exists) {
          updates.detectedSubtitles = [
            ...file.detectedSubtitles,
            {
              filePath,
              type: 'translated' as const,
              language,
              confidence: 100,
            },
          ];
        }

        onUpdateFile(index, updates);
      } catch (error) {
        console.error('Failed to select target subtitle:', error);
      }
    },
    [files, onUpdateFile],
  );

  // 从下拉菜单选择字幕
  const handleSelectFromDropdown = useCallback(
    (index: number, type: 'source' | 'target', filePath: string) => {
      const file = files[index];
      const subtitle = file.detectedSubtitles.find((s) => s.filePath === filePath);

      if (type === 'source') {
        onUpdateFile(index, {
          selectedSource: filePath,
          sourceLanguage: subtitle?.language,
        });
      } else {
        onUpdateFile(index, {
          selectedTarget: filePath === 'none' ? undefined : filePath,
          targetLanguage: filePath === 'none' ? undefined : subtitle?.language,
        });
      }
    },
    [files, onUpdateFile],
  );

  // 保存任务
  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      const success = await onSaveTask();
      if (success) {
        showToast('success', '任务保存成功');
      }
    } catch (error) {
      console.error(error);
      showToast('error', '保存失败');
    } finally {
      setSaving(false);
    }
  }, [onSaveTask, showToast]);

  // 追加文件
  const handleAppendFiles = useCallback(async () => {
    try {
      if (importType === 'video') {
        const selected = await open({
          multiple: true,
          directory: false,
          filters: [
            {
              name: '视频文件',
              extensions: ['mp4', 'avi', 'mov', 'mkv', 'flv', 'wmv', 'webm', '3gp', 'ts', 'm4v'],
            },
          ],
        });

        if (!selected) return;
        const filePaths = Array.isArray(selected) ? selected : [selected];

        const newFiles = await Promise.all(
          filePaths.map((videoPath: string) => createPendingFileFromVideo(videoPath)),
        );

        if (newFiles.length > 0) {
          onAddFiles(newFiles);
        }
      } else {
        const selected = await open({
          multiple: true,
          directory: false,
          filters: [{ name: '字幕文件', extensions: ['srt', 'vtt', 'ass', 'ssa', 'lrc'] }],
        });

        if (!selected) return;
        const filePaths = Array.isArray(selected) ? selected : [selected];

        const newFiles = await Promise.all(
          filePaths.map((filePath: string) => createPendingFileFromSubtitle(filePath)),
        );

        if (newFiles.length > 0) {
          onAddFiles(newFiles);
        }
      }
    } catch (error) {
      console.error('Failed to append files:', error);
    }
  }, [importType, onAddFiles]);

  const handleSaveTaskName = () => {
    onTaskNameChange(tempTaskName);
    setEditingName(false);
  };

  const getStatusDisplay = (status: PendingFile['status']) => {
    switch (status) {
      case 'completed':
        return (
          <div className="flex items-center gap-1.5 text-emerald-500 whitespace-nowrap">
            <CheckCircle2 className="w-4 h-4 flex-shrink-0" />
            <span className="text-xs">已完成</span>
          </div>
        );
      case 'proofreading':
        return (
          <div className="flex items-center gap-1.5 text-blue-500 whitespace-nowrap">
            <Loader2 className="w-4 h-4 animate-spin flex-shrink-0" />
            <span className="text-xs">校对中</span>
          </div>
        );
      default:
        return (
          <div className="flex items-center gap-1.5 text-slate-400 whitespace-nowrap">
            <Circle className="w-4 h-4 flex-shrink-0" />
            <span className="text-xs">待校对</span>
          </div>
        );
    }
  };

  const formatFileName = (filePath: string) => {
    const parts = filePath.replace(/\\/g, '/').split('/');
    const name = parts[parts.length - 1] || '';
    return name.length > 30 ? name.slice(0, 27) + '...' : name;
  };

  const completedCount = files.filter((f) => f.status === 'completed').length;

  return (
    <div className="space-y-6">
      {/* 顶部工具栏 */}
      <div className="flex items-center justify-between bg-slate-800/40 p-4 rounded-xl border border-slate-700/50">
        <div className="flex items-center gap-4">
          {/* 任务名称编辑 */}
          {editingName ? (
            <div className="flex items-center gap-2">
              <input
                type="text"
                value={tempTaskName}
                onChange={(e) => setTempTaskName(e.target.value)}
                className="bg-slate-900 border border-slate-700 rounded-lg px-3 py-1 text-sm text-slate-200 focus:outline-none focus:border-blue-500"
                placeholder="请输入任务名称"
              />
              <button
                onClick={handleSaveTaskName}
                className="bg-blue-600 hover:bg-blue-700 text-white p-1 rounded-md transition-colors"
              >
                <Check className="w-4 h-4" />
              </button>
            </div>
          ) : (
            <div
              className="flex items-center gap-2 cursor-pointer hover:bg-slate-700/30 px-3 py-1.5 rounded-lg transition-colors"
              onClick={() => {
                setTempTaskName(taskName);
                setEditingName(true);
              }}
            >
              <h3 className="font-semibold text-slate-100 max-w-[240px] truncate" title={taskName}>
                {taskName || '未命名任务'}
              </h3>
              <Edit2 className="w-3.5 h-3.5 text-slate-400" />
            </div>
          )}

          <span className="text-xs bg-slate-700 text-slate-300 px-2.5 py-1 rounded-full font-medium">
            已完成: {completedCount}/{files.length}
          </span>
          {savedTaskId && (
            <span className="text-xs bg-emerald-950 text-emerald-400 border border-emerald-800/50 px-2.5 py-1 rounded-full font-medium">
              已保存
            </span>
          )}
        </div>

        <div className="flex items-center gap-2.5">
          <button
            onClick={handleAppendFiles}
            className="flex items-center text-xs bg-slate-700/80 hover:bg-slate-750 text-slate-200 border border-slate-650 px-4 py-2 rounded-lg transition-colors font-medium"
          >
            <Plus className="w-4 h-4 mr-1.5 text-slate-400" />
            {importType === 'video' ? '追加视频' : '追加字幕'}
          </button>
          <button
            onClick={onReset}
            className="flex items-center text-xs bg-slate-700/80 hover:bg-slate-750 text-slate-200 border border-slate-650 px-4 py-2 rounded-lg transition-colors font-medium"
          >
            <RotateCcw className="w-4 h-4 mr-1.5 text-slate-400" />
            重新导入
          </button>
          <button
            onClick={handleSave}
            disabled={saving || files.length === 0}
            className="flex items-center text-xs bg-blue-600 hover:bg-blue-700 text-white px-5 py-2 rounded-lg transition-colors font-medium disabled:opacity-50 disabled:cursor-not-allowed shadow-lg shadow-blue-500/10"
          >
            {saving ? (
              <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
            ) : (
              <Save className="w-4 h-4 mr-1.5" />
            )}
            {savedTaskId ? '更新任务' : '保存任务'}
          </button>
        </div>
      </div>

      {/* 文件列表表格 */}
      <div className="bg-slate-800/30 border border-slate-700/50 rounded-xl overflow-hidden shadow-xl">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="bg-slate-800/60 border-b border-slate-700/50 text-xs font-semibold text-slate-400 uppercase tracking-wider">
              <th className="py-4.5 px-6 w-32">状态</th>
              <th className="py-4.5 px-6">文件名</th>
              <th className="py-4.5 px-6">源字幕</th>
              <th className="py-4.5 px-6">翻译字幕</th>
              <th className="py-4.5 px-6 w-36 text-right">操作</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-700/40">
            {files.map((file, index) => {
              const sourceOptions = file.detectedSubtitles.filter(
                (s) => s.type === 'source' || s.type === 'unknown',
              );
              const effectiveSourceOptions =
                sourceOptions.length > 0 ? sourceOptions : file.detectedSubtitles;

              const targetOptions = file.detectedSubtitles.filter(
                (s) => s.filePath !== file.selectedSource,
              );

              return (
                <tr key={file.id} className="hover:bg-slate-850/20 transition-colors">
                  <td className="py-4 px-6">{getStatusDisplay(file.status)}</td>
                  <td className="py-4 px-6">
                    <div className="font-medium text-slate-200 truncate max-w-[200px]" title={file.fileName}>
                      {file.fileName}
                    </div>
                    {file.videoPath && (
                      <span className="text-[10px] bg-blue-900/40 text-blue-400 border border-blue-800/30 px-1.5 py-0.5 rounded font-medium inline-block mt-1">
                        关联视频
                      </span>
                    )}
                  </td>
                  <td className="py-4 px-6">
                    <div className="flex items-center gap-2">
                      {file.isSubtitleOnlyMode ? (
                        <div className="flex items-center gap-2">
                          <span className="text-sm text-slate-300 truncate max-w-[160px]" title={file.selectedSource}>
                            {formatFileName(file.selectedSource || '')}
                          </span>
                          {file.sourceLanguage && (
                            <span className="text-[10px] bg-slate-700 text-slate-300 px-1.5 py-0.5 rounded">
                              {file.sourceLanguage}
                            </span>
                          )}
                        </div>
                      ) : effectiveSourceOptions.length > 0 ? (
                        <select
                          value={file.selectedSource || ''}
                          onChange={(e) => handleSelectFromDropdown(index, 'source', e.target.value)}
                          className="bg-slate-900 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-blue-500 w-[180px]"
                        >
                          {effectiveSourceOptions.map((s, idx) => (
                            <option key={`source-${idx}-${s.filePath}`} value={s.filePath}>
                              {formatFileName(s.filePath)} ({s.language || '未知'} - {s.confidence}%)
                            </option>
                          ))}
                        </select>
                      ) : file.selectedSource ? (
                        <span className="text-sm text-slate-300 truncate max-w-[160px]" title={file.selectedSource}>
                          {formatFileName(file.selectedSource)}
                        </span>
                      ) : (
                        <span className="text-slate-500 text-xs">无字幕</span>
                      )}

                      {!file.isSubtitleOnlyMode && (
                        <button
                          onClick={() => handleSelectSourceSubtitle(index)}
                          className="p-1.5 hover:bg-slate-700/50 rounded-lg transition-colors text-slate-400 hover:text-slate-200"
                          title="选择本地字幕"
                        >
                          <Upload className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>
                  </td>
                  <td className="py-4 px-6">
                    <div className="flex items-center gap-2">
                      <select
                        value={file.selectedTarget || 'none'}
                        onChange={(e) => handleSelectFromDropdown(index, 'target', e.target.value)}
                        className="bg-slate-900 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-blue-500 w-[180px]"
                      >
                        <option value="none">无翻译字幕</option>
                        {targetOptions.map((s, idx) => (
                          <option key={`target-${idx}-${s.filePath}`} value={s.filePath}>
                            {formatFileName(s.filePath)} ({s.language || '未知'} - {s.confidence}%)
                          </option>
                        ))}
                      </select>
                      <button
                        onClick={() => handleSelectTargetSubtitle(index)}
                        className="p-1.5 hover:bg-slate-700/50 rounded-lg transition-colors text-slate-400 hover:text-slate-200"
                        title="选择本地字幕"
                      >
                        <Upload className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </td>
                  <td className="py-4 px-6 text-right">
                    <div className="flex items-center justify-end gap-1.5">
                      <button
                        onClick={() => onStartProofread(index)}
                        disabled={!file.selectedSource}
                        className="flex items-center text-xs bg-slate-700 hover:bg-slate-650 text-slate-200 px-3.5 py-1.5 rounded-lg transition-colors font-medium disabled:opacity-40 disabled:cursor-not-allowed"
                      >
                        <Play className="w-3.5 h-3.5 mr-1 text-slate-400" />
                        {file.status === 'completed' ? '查看' : '校对'}
                      </button>
                      <button
                        onClick={() => onRemoveFile(index)}
                        className="p-1.5 hover:bg-red-950/30 rounded-lg transition-colors text-slate-400 hover:text-red-400"
                        title="移除文件"
                      >
                        <Trash2 className="w-4 h-4 text-red-500" />
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {files.length === 0 && (
          <div className="text-center py-16 text-slate-500 text-sm">暂无校对文件，请重新导入</div>
        )}
      </div>
    </div>
  );
}
