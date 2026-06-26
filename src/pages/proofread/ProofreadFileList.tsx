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
import { detectLanguageFromFilename, getLanguageName } from './languageDetector';
import { useI18n } from '../../lib/i18n';

import { Button } from '../../components/ui/Button';
import { Card } from '../../components/ui/Card';
import { Input, Select } from '../../components/ui/Input';
import { Badge } from '../../components/ui/Badge';

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
  const { locale, t } = useI18n();
  const [saving, setSaving] = useState(false);
  const [editingName, setEditingName] = useState(false);
  const [tempTaskName, setTempTaskName] = useState(taskName);
  const { showToast } = useToast();

  const handleSelectSourceSubtitle = useCallback(
    async (index: number) => {
      try {
        const selected = await open({
          multiple: false,
          directory: false,
          filters: [{ name: t('proofread.list.subtitleFilterName'), extensions: ['srt', 'vtt', 'ass', 'ssa', 'lrc'] }],
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
    [files, onUpdateFile, t],
  );

  const handleSelectTargetSubtitle = useCallback(
    async (index: number) => {
      try {
        const selected = await open({
          multiple: false,
          directory: false,
          filters: [{ name: t('proofread.list.subtitleFilterName'), extensions: ['srt', 'vtt', 'ass', 'ssa', 'lrc'] }],
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
    [files, onUpdateFile, t],
  );

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

  const handleSave = useCallback(async () => {
    setSaving(true);
    try {
      const success = await onSaveTask();
      if (success) {
        showToast('success', t('proofread.list.saveSuccess'));
      }
    } catch (error) {
      console.error(error);
      showToast('error', t('proofread.list.saveFailed'));
    } finally {
      setSaving(false);
    }
  }, [onSaveTask, showToast, t]);

  const handleAppendFiles = useCallback(async () => {
    try {
      if (importType === 'video') {
        const selected = await open({
          multiple: true,
          directory: false,
          filters: [
            {
              name: t('proofread.list.videoFilterName'),
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
          filters: [{ name: t('proofread.list.subtitleFilterName'), extensions: ['srt', 'vtt', 'ass', 'ssa', 'lrc'] }],
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
  }, [importType, onAddFiles, t]);

  const handleSaveTaskName = () => {
    onTaskNameChange(tempTaskName);
    setEditingName(false);
  };

  const getStatusDisplay = (status: PendingFile['status']) => {
    switch (status) {
      case 'completed':
        return (
          <div className="flex items-center gap-1.5 text-success whitespace-nowrap">
            <CheckCircle2 className="w-4 h-4 flex-shrink-0" />
            <span className="text-xs">{t('proofread.list.statusCompleted')}</span>
          </div>
        );
      case 'proofreading':
        return (
          <div className="flex items-center gap-1.5 text-brand whitespace-nowrap">
            <Loader2 className="w-4 h-4 animate-spin flex-shrink-0" />
            <span className="text-xs">{t('proofread.list.statusProofreading')}</span>
          </div>
        );
      default:
        return (
          <div className="flex items-center gap-1.5 text-text-tertiary whitespace-nowrap">
            <Circle className="w-4 h-4 flex-shrink-0" />
            <span className="text-xs">{t('proofread.list.statusPending')}</span>
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
      <Card className="flex items-center justify-between p-4 bg-surface">
        <div className="flex items-center gap-4">
          {/* 任务名称编辑 */}
          {editingName ? (
            <div className="flex items-center gap-2">
              <Input
                type="text"
                value={tempTaskName}
                onChange={(e) => setTempTaskName(e.target.value)}
                className="w-48 h-8 px-2.5 text-xs"
                placeholder={t('proofread.list.inputPlaceholder')}
              />
              <button
                onClick={handleSaveTaskName}
                className="bg-brand hover:bg-brand-hover text-white p-1.5 rounded-md transition-all duration-150"
              >
                <Check className="w-3.5 h-3.5" />
              </button>
            </div>
          ) : (
            <div
              className="flex items-center gap-2 cursor-pointer hover:bg-surface-overlay px-3 py-1.5 rounded-lg transition-colors"
              onClick={() => {
                setTempTaskName(taskName);
                setEditingName(true);
              }}
            >
              <h3 className="font-semibold text-text-primary max-w-[240px] truncate" title={taskName}>
                {taskName || t('proofread.list.unnamedTask')}
              </h3>
              <Edit2 className="w-3.5 h-3.5 text-text-tertiary" />
            </div>
          )}

          <Badge variant="default" className="font-normal border-none bg-surface-overlay text-text-secondary px-3 py-1">
            {t('proofread.list.completedCount')
              .replace('{completed}', String(completedCount))
              .replace('{total}', String(files.length))}
          </Badge>
          {savedTaskId && (
            <Badge variant="success" className="px-3 py-1">
              {t('proofread.list.saved')}
            </Badge>
          )}
        </div>

        <div className="flex items-center gap-2.5">
          <Button
            onClick={handleAppendFiles}
            variant="secondary"
            size="sm"
            className="h-9 px-3.5"
          >
            <Plus className="w-4 h-4 text-text-tertiary" />
            <span>{importType === 'video' ? t('proofread.list.appendVideo') : t('proofread.list.appendSubtitle')}</span>
          </Button>
          <Button
            onClick={onReset}
            variant="secondary"
            size="sm"
            className="h-9 px-3.5"
          >
            <RotateCcw className="w-4 h-4 text-text-tertiary" />
            <span>{t('proofread.list.reset')}</span>
          </Button>
          <Button
            onClick={handleSave}
            disabled={saving || files.length === 0}
            variant="primary"
            size="sm"
            className="h-9 px-4"
          >
            {saving ? (
              <Loader2 className="w-4 h-4 animate-spin" />
            ) : (
              <Save className="w-4 h-4" />
            )}
            <span>{savedTaskId ? t('proofread.list.updateTask') : t('proofread.list.saveTask')}</span>
          </Button>
        </div>
      </Card>

      {/* 文件列表表格 */}
      <Card className="p-0 overflow-hidden shadow-md">
        <table className="w-full text-left border-collapse">
          <thead>
            <tr className="bg-surface-raised border-b border-border-subtle text-xs font-semibold text-text-secondary uppercase tracking-wider">
              <th className="py-4.5 px-6 w-32">{t('proofread.list.thStatus')}</th>
              <th className="py-4.5 px-6">{t('proofread.list.thFilename')}</th>
              <th className="py-4.5 px-6">{t('proofread.list.thSourceSub')}</th>
              <th className="py-4.5 px-6">{t('proofread.list.thTargetSub')}</th>
              <th className="py-4.5 px-6 w-36 text-right">{t('proofread.list.thActions')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-border-subtle">
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
                <tr key={file.id} className="hover:bg-surface-overlay/30 transition-colors">
                  <td className="py-4 px-6">{getStatusDisplay(file.status)}</td>
                  <td className="py-4 px-6">
                    <div className="font-medium text-text-primary truncate max-w-[200px]" title={file.fileName}>
                      {file.fileName}
                    </div>
                    {file.videoPath && (
                      <Badge variant="info" className="mt-1 text-[9px] px-1.5 py-0">
                        {t('proofread.list.associatedVideo')}
                      </Badge>
                    )}
                  </td>
                  <td className="py-4 px-6">
                    <div className="flex items-center gap-2">
                      {file.isSubtitleOnlyMode ? (
                        <div className="flex items-center gap-2">
                          <span className="text-sm text-text-secondary truncate max-w-[160px] font-mono" title={file.selectedSource}>
                            {formatFileName(file.selectedSource || '')}
                          </span>
                          {file.sourceLanguage && (
                            <Badge variant="default" className="text-[10px] px-1.5 py-0">
                              {getLanguageName(file.sourceLanguage, locale)}
                            </Badge>
                          )}
                        </div>
                      ) : effectiveSourceOptions.length > 0 ? (
                        <Select
                           value={file.selectedSource || ''}
                           onChange={(e) => handleSelectFromDropdown(index, 'source', e.target.value)}
                           className="py-1 px-2.5 text-xs w-[180px]"
                        >
                          {effectiveSourceOptions.map((s, idx) => (
                            <option key={`source-${idx}-${s.filePath}`} value={s.filePath}>
                              {formatFileName(s.filePath)} ({s.language ? getLanguageName(s.language, locale) : t('proofread.list.unknownLang')} - {s.confidence}%)
                            </option>
                          ))}
                        </Select>
                      ) : file.selectedSource ? (
                        <span className="text-sm text-text-secondary truncate max-w-[160px] font-mono" title={file.selectedSource}>
                          {formatFileName(file.selectedSource)}
                        </span>
                      ) : (
                        <span className="text-text-tertiary text-xs">{t('proofread.list.noSubtitle')}</span>
                      )}

                      {!file.isSubtitleOnlyMode && (
                        <button
                          onClick={() => handleSelectSourceSubtitle(index)}
                          className="p-1.5 hover:bg-surface-overlay rounded-lg transition-colors text-text-tertiary hover:text-text-primary"
                          title={t('proofread.list.selectLocalSub')}
                        >
                          <Upload className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </div>
                  </td>
                  <td className="py-4 px-6">
                    <div className="flex items-center gap-2">
                      <Select
                        value={file.selectedTarget || 'none'}
                        onChange={(e) => handleSelectFromDropdown(index, 'target', e.target.value)}
                        className="py-1 px-2.5 text-xs w-[180px]"
                      >
                        <option value="none">{t('proofread.list.noTargetSubOption')}</option>
                        {targetOptions.map((s, idx) => (
                          <option key={`target-${idx}-${s.filePath}`} value={s.filePath}>
                            {formatFileName(s.filePath)} ({s.language ? getLanguageName(s.language, locale) : t('proofread.list.unknownLang')} - {s.confidence}%)
                          </option>
                        ))}
                      </Select>
                      <button
                        onClick={() => handleSelectTargetSubtitle(index)}
                        className="p-1.5 hover:bg-surface-overlay rounded-lg transition-colors text-text-tertiary hover:text-text-primary"
                        title={t('proofread.list.selectLocalSub')}
                      >
                        <Upload className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </td>
                  <td className="py-4 px-6 text-right">
                    <div className="flex items-center justify-end gap-2">
                      <Button
                        onClick={() => onStartProofread(index)}
                        disabled={!file.selectedSource}
                        variant="secondary"
                        size="sm"
                        className="h-8"
                      >
                        <Play size={12} className="text-text-tertiary" />
                        <span>{file.status === 'completed' ? t('proofread.list.view') : t('proofread.list.proofread')}</span>
                      </Button>
                      <button
                        onClick={() => onRemoveFile(index)}
                        className="p-1.5 hover:bg-danger/10 rounded-lg transition-colors text-text-tertiary hover:text-danger"
                        title={t('proofread.list.removeFile')}
                      >
                        <Trash2 className="w-4 h-4 text-danger" />
                      </button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {files.length === 0 && (
          <div className="text-center py-16 text-text-tertiary text-sm">{t('proofread.list.noFiles')}</div>
        )}
      </Card>
    </div>
  );
}
