import { editSubtitleDocument } from './subtitleDocument';
/**
 * 独立校对模式的字幕管理 Hook
 * 不依赖 Electron IFiles，直接接收文件路径并使用前端的 srt 解析/序列化和薄 fs IPC 命令
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { readTextFilePath, writeTextFilePath, saveProofreadFiles } from '../../lib/tauri';
import {
  detectSubtitleFormat,
  parseSubtitleEntries,
  convertSubtitleContentForPlayer,
  serializeSubtitleEntries,
} from './subtitleFormats';
import { pathShim } from './subtitleDetector';

import { useToast } from './Toast';
import { useI18n } from '../../lib/i18n';

export interface Subtitle {
  id: string;
  originalId?: string;
  targetOriginalId?: string;
  startEndTime: string;
  content: string[];
  sourceContent?: string;
  targetContent?: string;
  startTimeInSeconds?: number;
  endTimeInSeconds?: number;
  isEditing?: boolean;
}

export interface SubtitleDraft {
  subtitles: Subtitle[];
  lastSavedSubtitles: Subtitle[];
  originalContents: Record<string, string>;
  dirty: boolean;
  reviewVersion?: string;
  backups?: string[];
}
export function isSubtitleDraft(value: unknown): value is SubtitleDraft {
  if (!value || typeof value !== 'object') return false;
  const data = value as Partial<SubtitleDraft>;
  const cues = (items: unknown): items is Subtitle[] => Array.isArray(items) && items.every((cue) => cue && typeof cue.id === 'string' && typeof cue.startEndTime === 'string' && Array.isArray(cue.content) && cue.content.every((line: unknown) => typeof line === 'string') && (cue.sourceContent === undefined || typeof cue.sourceContent === 'string') && (cue.targetContent === undefined || typeof cue.targetContent === 'string'));
  return cues(data.subtitles) && cues(data.lastSavedSubtitles) && typeof data.dirty === 'boolean' && !!data.originalContents && typeof data.originalContents === 'object' && Object.values(data.originalContents).every((text) => typeof text === 'string');
}
const copyCues = (cues: Subtitle[]) => cues.map((cue) => ({ ...cue, content: [...cue.content] }));

export interface SubtitleStats {
  total: number;
  withTranslation: number;
  percent: number;
}

export interface PlayerSubtitleTrack {
  kind: string;
  src: string;
  srcLang: string;
  label: string;
  default?: boolean;
}

interface StandaloneSubtitlesConfig {
  draft?: SubtitleDraft;
  reviewSourceContent?: string;
  reviewTargetContent?: string;
  videoPath?: string;
  sourceSubtitlePath?: string;
  targetSubtitlePath?: string;
  sourceLanguage?: string;
  targetLanguage?: string;
  finalTargetSubtitlePath?: string;
  translateContent?: string;
}

const TRANSLATION_FAILURE_MARKER = '[翻译失败：';

// 将时间字符串转换为秒
const timeToSeconds = (timeStr: string): number => {
  const parts = timeStr.replace(',', '.').split(':');
  if (parts.length !== 3) return 0;
  const hours = parseInt(parts[0], 10);
  const minutes = parseInt(parts[1], 10);
  const seconds = parseFloat(parts[2]);
  return hours * 3600 + minutes * 60 + seconds;
};

// 从时间范围字符串中提取开始和结束时间
const parseTimeRange = (timeRange: string): { start: number; end: number } => {
  const times = timeRange.split(' --> ');
  if (times.length !== 2) return { start: 0, end: 0 };
  return {
    start: timeToSeconds(times[0]),
    end: timeToSeconds(times[1]),
  };
};

export const useStandaloneSubtitles = (
  config: StandaloneSubtitlesConfig,
  isOpen: boolean,
  onDraftChange?: (draft: SubtitleDraft) => void,
) => {
  const { showToast } = useToast();
  const { t } = useI18n();
  const [isDirty, setIsDirty] = useState(false);
  const [mergedSubtitles, setMergedSubtitles] = useState<Subtitle[]>([]);
  const [videoPath, setVideoPath] = useState<string>('');
  const [currentSubtitleIndex, setCurrentSubtitleIndex] = useState(-1);
  const [videoInfo, setVideoInfo] = useState({ fileName: '', extension: '' });
  const [hasTranslationFile, setHasTranslationFile] = useState(false);
  const [subtitleTracksForPlayer, setSubtitleTracksForPlayer] = useState<
    PlayerSubtitleTrack[]
  >([]);
  const [isLoading, setIsLoading] = useState(Boolean(isOpen && config.sourceSubtitlePath));
  const [saveBackups, setSaveBackups] = useState<string[]>(config.draft?.backups ?? []);
  const [loadError, setLoadError] = useState<string | null>(null);

  // 撤销/重做历史
  const [history, setHistory] = useState<Subtitle[][]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const maxHistoryLength = 50;

  // 记录编辑前的快照（用于失焦记录）
  const historyRef = useRef<Subtitle[][]>([]);
  const historyIndexRef = useRef(-1);
  const editGroup = useRef<{ key: string; at: number } | null>(null);
  const originalContents = useRef(new Map(Object.entries(config.draft?.originalContents ?? {})));
  const lastSaved = useRef<Subtitle[]>([]);
  const latestCues = useRef(mergedSubtitles);
  latestCues.current = mergedSubtitles;
  const draftCallback = useRef(onDraftChange);
  draftCallback.current = onDraftChange;

  // 光标位置（用于拆分功能）
  const cursorPositionRef = useRef(0);

  // 是否有翻译字幕
  const shouldShowTranslation = !!config.targetSubtitlePath;

  // 读取字幕文件并解析为 Subtitle 格式
  const readContent = async (path: string): Promise<string> => {
    if (config.draft && originalContents.current.has(path)) return originalContents.current.get(path)!;
    const content = path === config.sourceSubtitlePath && config.reviewSourceContent !== undefined ? config.reviewSourceContent
      : path === config.targetSubtitlePath && config.reviewTargetContent !== undefined ? config.reviewTargetContent
      : await readTextFilePath(path);
    if (!originalContents.current.has(path)) originalContents.current.set(path, content);
    return content;
  };
  const readSubtitleFile = async (filePath: string): Promise<Subtitle[]> => {
    const content = await readContent(filePath);
    const format = detectSubtitleFormat(filePath);
    const entries = parseSubtitleEntries(content, format);
    if (content.trim() !== '' && entries.length === 0) {
      throw new Error(`No valid subtitle entries found in ${filePath}`);
    }
    return entries.map((e) => {
      const { start, end } = parseTimeRange(e.startEndTime);
      return {
        id: e.id,
        originalId: e.id,
        startEndTime: e.startEndTime,
        content: e.content,
        sourceContent: e.content.join('\n'),
        targetContent: '',
        startTimeInSeconds: start,
        endTimeInSeconds: end,
      };
    });
  };

  // 创建播放器字幕轨道（VTT blob URL）
  const createPlayerTrack = async (
    srtPath: string | undefined,
    language: string | undefined,
    isDefault?: boolean,
    layout: 'auto' | 'source' | 'target' | 'bilingual' = 'auto',
  ): Promise<PlayerSubtitleTrack | null> => {
    if (!srtPath) return null;
    try {
      const content = await readContent(srtPath);
      const fromFormat = detectSubtitleFormat(srtPath);
      const vttContent = convertSubtitleContentForPlayer(
        content,
        fromFormat,
        srtPath,
        layout,
      );
      const vttBlob = new Blob([vttContent], { type: 'text/vtt' });
      const vttUrl = URL.createObjectURL(vttBlob);
      return {
        kind: 'subtitles',
        src: vttUrl,
        srcLang: language || 'und',
        label: language ? `(${language})` : '字幕',
        default: isDefault,
      };
    } catch (error) {
      console.error(`Failed to convert subtitle to VTT:`, error);
      return null;
    }
  };

  // 加载文件
  const loadFiles = useCallback(async () => {
    if (!config.sourceSubtitlePath) return;

    setIsLoading(true);
    setLoadError(null);
    setMergedSubtitles([]);
    setIsDirty(false);
    setHistory([]);
    setHistoryIndex(-1);
    historyRef.current = [];
    historyIndexRef.current = -1;
    editGroup.current = null;
    try {
      if (config.videoPath) {
        setVideoPath(config.videoPath);
      }

      const playerTracks: PlayerSubtitleTrack[] = [];

      // 读取源字幕
      const sourceSubtitles = await readSubtitleFile(config.sourceSubtitlePath);
      const sourceTrack = await createPlayerTrack(
        config.sourceSubtitlePath,
        config.sourceLanguage,
        !shouldShowTranslation,
        'auto',
      );
      if (sourceTrack) playerTracks.push(sourceTrack);

      // 读取翻译字幕
      let translatedSubtitles: Subtitle[] = [];
      if (config.targetSubtitlePath) {
        translatedSubtitles = await readSubtitleFile(config.targetSubtitlePath);
        setHasTranslationFile(translatedSubtitles.length > 0);

        const track = await createPlayerTrack(
          config.targetSubtitlePath,
          config.targetLanguage,
          true,
          'target',
        );
        if (track) playerTracks.push(track);
      }

      setSubtitleTracksForPlayer(playerTracks);

      // 合并字幕
      const translatedMap = new Map();
      translatedSubtitles.forEach((sub) => {
        translatedMap.set(sub.startEndTime, sub);
      });

      const merged = sourceSubtitles.map((sub, index) => {
        const translated =
          translatedMap.get(sub.startEndTime) ||
          (index < translatedSubtitles.length
            ? translatedSubtitles[index]
            : null);

        const { start, end } = parseTimeRange(sub.startEndTime);

        return {
          ...sub,
          sourceContent: sub.content.join('\n'),
          targetOriginalId: translated?.originalId,
          targetContent: translated ? translated.sourceContent || translated.content.join('\n') : '',
          isEditing: false,
          startTimeInSeconds: start,
          endTimeInSeconds: end,
        };
      });

      lastSaved.current = copyCues(config.draft?.lastSavedSubtitles ?? merged);
      const restored = config.draft?.subtitles ?? merged;
      setMergedSubtitles(restored);
      setIsDirty(config.draft?.dirty ?? false);
      if (config.draft?.dirty) {
        historyRef.current = [copyCues(lastSaved.current), copyCues(restored)];
        historyIndexRef.current = 1;
        setHistory(historyRef.current);
        setHistoryIndex(1);
      }
    } catch (error) {
      console.error('Error loading files:', error);
      setLoadError(error instanceof Error ? error.message : String(error));
      showToast('error', t('proofread.standalone.loadFailed'));
    } finally {
      setIsLoading(false);
    }
  }, [config, shouldShowTranslation]);

  // 挂载/加载
  useEffect(() => {
    if (isOpen && config.sourceSubtitlePath) {
      loadFiles();
    }

    return () => {
      subtitleTracksForPlayer.forEach((track) => {
        if (track.src && track.src.startsWith('blob:')) {
          URL.revokeObjectURL(track.src);
        }
      });
    };
  }, [isOpen, config.sourceSubtitlePath, config.targetSubtitlePath]);

  // 更新视频信息
  useEffect(() => {
    if (videoPath) {
      const fileName = pathShim.basename(videoPath, pathShim.extname(videoPath));
      const extension = pathShim.extname(videoPath).replace('.', '');
      setVideoInfo({ fileName, extension });
    } else if (config.sourceSubtitlePath) {
      const fileName = pathShim.basename(
        config.sourceSubtitlePath,
        pathShim.extname(config.sourceSubtitlePath),
      );
      setVideoInfo({ fileName, extension: '' });
    }
  }, [videoPath, config.sourceSubtitlePath]);

  // 更新字幕内容（带失焦记录支持）
  const handleSubtitleChange = (
    index: number,
    field: 'sourceContent' | 'targetContent',
    value: string,
  ) => {
    const newSubtitles = mergedSubtitles.map((subtitle, at) => at === index ? {
      ...subtitle, [field]: value, ...(field === 'sourceContent' ? { content: value.split('\n') } : {}),
    } : subtitle);
    pushToHistory(mergedSubtitles, newSubtitles, `${index}:${field}`);
    setMergedSubtitles(newSubtitles);
  };

  // 保存字幕文件
  const handleSave = async (): Promise<boolean> => {
    if (isLoading || loadError !== null) {
      showToast('error', t('proofread.standalone.loadFailed'));
      return false;
    }
    try {
      const buildText = (sub: Subtitle, contentType: string): string => {
        if (contentType === 'source') {
          return sub.sourceContent ?? '';
        }
        const sourceVal = sub.sourceContent ?? '';
        const targetVal = sub.targetContent ?? '';
        if (contentType === 'onlyTranslate') {
          return targetVal;
        } else if (contentType === 'sourceAndTranslate') {
          return `${sourceVal}\n${targetVal}`;
        } else if (contentType === 'translateAndSource') {
          return `${targetVal}\n${sourceVal}`;
        }
        return targetVal;
      };

      const edits = [
        { path: config.sourceSubtitlePath, kind: 'source' },
        ...(shouldShowTranslation ? [{ path: config.targetSubtitlePath, kind: 'onlyTranslate' }] : []),
      ].filter((entry): entry is { path: string; kind: string } => Boolean(entry.path)).map(({ path, kind }) => {
        const template = originalContents.current.get(path);
        if (template === undefined) throw new Error('Original subtitle snapshot is missing. Export a new file instead.');
        const document = (cues: Subtitle[]) => editSubtitleDocument(template, detectSubtitleFormat(path), cues.map((sub) => ({
          id: sub.id, originalId: kind === 'source' ? sub.originalId : sub.targetOriginalId,
          startEndTime: sub.startEndTime, text: buildText(sub, kind),
        })));
        // Anchors belong to the imported document, including rows restored by
        // undo after a save. Keep that template immutable across every write.
        return { path, content: document(mergedSubtitles), expected_content: document(lastSaved.current) };
      });
      const backups = await saveProofreadFiles(edits);
      if (backups.length) setSaveBackups(backups);
      const currentSaved = markSaved();
      if (backups.length) showToast('info', t('proofread.standalone.saveBackups', { paths: backups.join(' · ') }));
      showToast('success', t('proofread.standalone.saveSuccess'));
      return currentSaved;
    } catch (error) {
      console.error('Error saving subtitles:', error);
      showToast('error', String(error).includes('finalsub:subtitle-save-conflict') ? t('proofread.standalone.saveConflict') : `${t('proofread.standalone.saveFailed')} ${String(error)}`);
      return false;
    }
  };

  const handleExport = async (
    filePath: string,
    format: 'srt' | 'vtt' | 'ass' | 'lrc' | 'txt',
    contentType: 'source' | 'onlyTranslate' | 'sourceAndTranslate' | 'translateAndSource',
  ): Promise<boolean> => {
    if (isLoading || loadError !== null) {
      showToast('error', t('proofread.standalone.loadFailed'));
      return false;
    }
    try {
      const buildText = (sub: Subtitle, type: string): string => {
        if (type === 'source') {
          return sub.sourceContent ?? '';
        }
        const sourceVal = sub.sourceContent ?? '';
        const targetVal = sub.targetContent ?? '';
        if (type === 'onlyTranslate') {
          return targetVal;
        } else if (type === 'sourceAndTranslate') {
          return `${sourceVal}\n${targetVal}`;
        } else if (type === 'translateAndSource') {
          return `${targetVal}\n${sourceVal}`;
        }
        return targetVal;
      };

      const entries = mergedSubtitles.map((sub) => ({
        id: sub.id,
        startEndTime: sub.startEndTime,
        text: buildText(sub, contentType),
      }));

      const content = serializeSubtitleEntries(entries, format);
      await writeTextFilePath(filePath, content);
      return true;
    } catch (error) {
      console.error('Error exporting subtitles:', error);
      return false;
    }
  };

  // 字幕统计
  const getSubtitleStats = (): SubtitleStats => {
    const total = mergedSubtitles.length;
    const withTranslation = shouldShowTranslation
      ? mergedSubtitles.filter(
          (sub) =>
            sub.targetContent &&
            sub.targetContent.trim() !== '' &&
            !sub.targetContent.trim().startsWith(TRANSLATION_FAILURE_MARKER),
        ).length
      : 0;
    const percent =
      total > 0 && shouldShowTranslation
        ? Math.round((withTranslation / total) * 100)
        : 0;
    return { total, withTranslation, percent };
  };

  // 检查翻译是否失败
  const isTranslationFailed = (subtitle: Subtitle): boolean => {
    if (!shouldShowTranslation) return false;
    return (
      !!subtitle.sourceContent &&
      subtitle.sourceContent.trim() !== '' &&
      (!subtitle.targetContent ||
        subtitle.targetContent.trim() === '' ||
        subtitle.targetContent.trim().startsWith(TRANSLATION_FAILURE_MARKER))
    );
  };

  // 获取翻译失败的索引
  const getFailedTranslationIndices = (): number[] => {
    if (!shouldShowTranslation) return [];
    return mergedSubtitles
      .map((subtitle, index) => (isTranslationFailed(subtitle) ? index : -1))
      .filter((index) => index !== -1);
  };

  // 导航到下一条失败的翻译
  const goToNextFailedTranslation = (): void => {
    const failedIndices = getFailedTranslationIndices();
    if (failedIndices.length === 0) return;
    const nextIndex = failedIndices.find(
      (index) => index > currentSubtitleIndex,
    );
    if (nextIndex !== undefined) {
      setCurrentSubtitleIndex(nextIndex);
    } else {
      setCurrentSubtitleIndex(failedIndices[0]);
    }
  };

  // 导航到上一条失败的翻译
  const goToPreviousFailedTranslation = (): void => {
    const failedIndices = getFailedTranslationIndices();
    if (failedIndices.length === 0) return;
    const previousIndex = failedIndices
      .slice()
      .reverse()
      .find((index) => index < currentSubtitleIndex);
    if (previousIndex !== undefined) {
      setCurrentSubtitleIndex(previousIndex);
    } else {
      setCurrentSubtitleIndex(failedIndices[failedIndices.length - 1]);
    }
  };

  // 保存到历史记录（用于撤销/重做）
  const pushToHistory = useCallback((oldState: Subtitle[], newState: Subtitle[], key?: string) => {
    const now = Date.now();
    const current = historyRef.current.slice(0, historyIndexRef.current + 1);
    if (current.length === 0) current.push(copyCues(oldState));
    if (key && editGroup.current?.key === key && now - editGroup.current.at < 1000 && current.length > 1) current[current.length - 1] = copyCues(newState);
    else current.push(copyCues(newState));
    while (current.length > maxHistoryLength) current.shift();
    editGroup.current = key ? { key, at: now } : null;
    historyRef.current = current;
    historyIndexRef.current = current.length - 1;
    setHistory(current);
    setHistoryIndex(current.length - 1);
    setIsDirty(true);
  }, []);

  // 更新字幕（带历史记录）
  const updateSubtitles = useCallback(
    (newSubtitles: Subtitle[]) => {
      pushToHistory(mergedSubtitles, newSubtitles);
      setMergedSubtitles(newSubtitles);
    },
    [mergedSubtitles, pushToHistory],
  );

  // 撤销
  const handleUndo = useCallback(() => {
    if (historyIndex > 0 && history.length > 0) {
      const newIndex = historyIndex - 1;
      setHistoryIndex(newIndex);
      historyIndexRef.current = newIndex;
      setMergedSubtitles(JSON.parse(JSON.stringify(history[newIndex])));
      editGroup.current = null;
      setIsDirty(true);
    }
  }, [historyIndex, history]);

  // 重做
  const handleRedo = useCallback(() => {
    if (historyIndex < history.length - 1) {
      const newIndex = historyIndex + 1;
      setHistoryIndex(newIndex);
      historyIndexRef.current = newIndex;
      setMergedSubtitles(JSON.parse(JSON.stringify(history[newIndex])));
      editGroup.current = null;
      setIsDirty(true);
    }
  }, [historyIndex, history]);

  // 是否可以撤销/重做
  const canUndo = historyIndex > 0 && history.length > 1;
  const canRedo = historyIndex < history.length - 1 && historyIndex >= 0;

  const makeDraft = (cues: Subtitle[], dirty: boolean): SubtitleDraft => ({
    subtitles: copyCues(cues), lastSavedSubtitles: copyCues(lastSaved.current),
    originalContents: Object.fromEntries(originalContents.current), dirty, backups: saveBackups,
  });
  const markSaved = () => {
    lastSaved.current = copyCues(mergedSubtitles);
    const changed = JSON.stringify(latestCues.current) !== JSON.stringify(mergedSubtitles);
    setIsDirty(changed);
    draftCallback.current?.(makeDraft(latestCues.current, changed));
    return !changed;
  };
  const discardChanges = () => {
    const cues = copyCues(lastSaved.current);
    setMergedSubtitles(cues);
    setIsDirty(false);
    draftCallback.current?.(makeDraft(cues, false));
  };
  useEffect(() => {
    if (!isLoading && !loadError && mergedSubtitles.length) draftCallback.current?.(makeDraft(mergedSubtitles, isDirty));
  }, [mergedSubtitles, isDirty, isLoading, loadError, saveBackups]);

  // 秒数转时间戳字符串
  const secondsToTime = (seconds: number): string => {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = (seconds % 60).toFixed(3);
    return `${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}:${s.padStart(6, '0').replace('.', ',')}`;
  };

  // 合并字幕
  const handleMergeSubtitles = useCallback(
    (startIndex: number, endIndex: number) => {
      if (
        startIndex < 0 ||
        endIndex > mergedSubtitles.length ||
        startIndex >= endIndex
      )
        return;

      const toMerge = mergedSubtitles.slice(startIndex, endIndex);
      if (toMerge.length < 2) return;

      const mergedContent = toMerge
        .map((s) => s.sourceContent)
        .filter(Boolean)
        .join('\n');
      const mergedTarget = toMerge
        .map((s) => s.targetContent)
        .filter(Boolean)
        .join('\n');

      const startTime = toMerge[0].startTimeInSeconds || 0;
      const endTime = toMerge[toMerge.length - 1].endTimeInSeconds || 0;

      const merged: Subtitle = {
        ...toMerge[0],
        sourceContent: mergedContent,
        targetContent: mergedTarget,
        content: mergedContent.split('\n'),
        startEndTime: `${secondsToTime(startTime)} --> ${secondsToTime(endTime)}`,
        startTimeInSeconds: startTime,
        endTimeInSeconds: endTime,
      };

      const newSubtitles = [
        ...mergedSubtitles.slice(0, startIndex),
        merged,
        ...mergedSubtitles.slice(endIndex),
      ];

      updateSubtitles(newSubtitles.map((sub, index) => ({ ...sub, id: String(index + 1) })));
    },
    [mergedSubtitles, updateSubtitles],
  );

  // 拆分字幕
  const handleSplitSubtitle = useCallback(
    (index: number, splitPoint: number, splitTime?: number) => {
      if (index < 0 || index >= mergedSubtitles.length) return;

      const subtitle = mergedSubtitles[index];
      const content = subtitle.sourceContent || '';
      const targetContent = subtitle.targetContent || '';

      if (content.length < 2) return;

      const content1 = content.slice(0, splitPoint);
      const content2 = content.slice(splitPoint);
      const targetSplitPoint = Math.floor(
        targetContent.length * (splitPoint / Math.max(content.length, 1)),
      );
      const target1 = targetContent.slice(0, targetSplitPoint);
      const target2 = targetContent.slice(targetSplitPoint);

      const startTime = subtitle.startTimeInSeconds || 0;
      const endTime = subtitle.endTimeInSeconds || 0;
      const midTime =
        splitTime !== undefined
          ? splitTime
          : startTime + (endTime - startTime) / 2;

      const sub1: Subtitle = {
        ...subtitle,
        sourceContent: content1,
        targetContent: target1,
        content: content1.split('\n'),
        startEndTime: `${secondsToTime(startTime)} --> ${secondsToTime(midTime)}`,
        startTimeInSeconds: startTime,
        endTimeInSeconds: midTime,
      };

      const sub2: Subtitle = {
        ...subtitle,
        id: String(index + 2),
        sourceContent: content2,
        targetContent: target2,
        content: content2.split('\n'),
        startEndTime: `${secondsToTime(midTime)} --> ${secondsToTime(endTime)}`,
        startTimeInSeconds: midTime,
        endTimeInSeconds: endTime,
      };

      const newSubtitles = [
        ...mergedSubtitles.slice(0, index),
        sub1,
        sub2,
        ...mergedSubtitles.slice(index + 1),
      ];

      updateSubtitles(newSubtitles.map((sub, index) => ({ ...sub, id: String(index + 1) })));
    },
    [mergedSubtitles, updateSubtitles],
  );

  // 光标位置
  const handleCursorPositionChange = useCallback((position: number) => {
    cursorPositionRef.current = position;
  }, []);

  const getCursorPosition = useCallback(() => {
    return cursorPositionRef.current;
  }, []);

  return {
    mergedSubtitles,
    setMergedSubtitles,
    updateSubtitles,
    videoPath,
    currentSubtitleIndex,
    setCurrentSubtitleIndex,
    videoInfo,
    hasTranslationFile,
    shouldShowTranslation,
    subtitleTracksForPlayer,
    isLoading,
    loadError,
    retryLoad: loadFiles,
    handleSubtitleChange,
    handleSave,
    handleExport,
    getSubtitleStats,
    isTranslationFailed,
    getFailedTranslationIndices,
    goToNextFailedTranslation,
    goToPreviousFailedTranslation,
    handleUndo,
    handleRedo,
    canUndo,
    canRedo,
    handleMergeSubtitles,
    handleSplitSubtitle,
    handleCursorPositionChange,
    getCursorPosition,
    isDirty,
    setIsDirty,
    saveBackups,
    markSaved,
    discardChanges,
  };
};
