/**
 * 独立校对模式的字幕管理 Hook
 * 不依赖 Electron IFiles，直接接收文件路径并使用前端的 srt 解析/序列化和薄 fs IPC 命令
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import {
  detectSubtitleFormat,
  parseSubtitleEntries,
  convertSubtitleContent,
  serializeSubtitleEntries,
} from './subtitleFormats';
import { pathShim } from './subtitleDetector';

// 定义 alert 弹窗兜底 toast
const toast = {
  success: (msg: string) => alert(msg),
  error: (msg: string) => alert(msg),
};

export interface Subtitle {
  id: string;
  startEndTime: string;
  content: string[];
  sourceContent?: string;
  targetContent?: string;
  startTimeInSeconds?: number;
  endTimeInSeconds?: number;
  isEditing?: boolean;
}

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
  videoPath?: string;
  sourceSubtitlePath?: string;
  targetSubtitlePath?: string;
  sourceLanguage?: string;
  targetLanguage?: string;
  finalTargetSubtitlePath?: string;
  translateContent?: string;
}

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
) => {
  const [mergedSubtitles, setMergedSubtitles] = useState<Subtitle[]>([]);
  const [videoPath, setVideoPath] = useState<string>('');
  const [currentSubtitleIndex, setCurrentSubtitleIndex] = useState(-1);
  const [previousSubtitleIndex, setPreviousSubtitleIndex] = useState(-1);
  const [videoInfo, setVideoInfo] = useState({ fileName: '', extension: '' });
  const [hasTranslationFile, setHasTranslationFile] = useState(false);
  const [subtitleTracksForPlayer, setSubtitleTracksForPlayer] = useState<
    PlayerSubtitleTrack[]
  >([]);
  const [isLoading, setIsLoading] = useState(false);

  // 撤销/重做历史
  const [history, setHistory] = useState<Subtitle[][]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const maxHistoryLength = 50;

  // 记录编辑前的快照（用于失焦记录）
  const [editSnapshot, setEditSnapshot] = useState<Subtitle[] | null>(null);

  // 光标位置（用于拆分功能）
  const cursorPositionRef = useRef(0);

  // 是否有翻译字幕
  const shouldShowTranslation = !!config.targetSubtitlePath;

  // 读取字幕文件并解析为 Subtitle 格式
  const readSubtitleFile = async (filePath: string): Promise<Subtitle[]> => {
    try {
      const content = await readTextFile(filePath);
      const format = detectSubtitleFormat(filePath);
      const entries = parseSubtitleEntries(content, format);
      return entries.map((e) => {
        const { start, end } = parseTimeRange(e.startEndTime);
        return {
          id: e.id,
          startEndTime: e.startEndTime,
          content: e.content,
          sourceContent: e.content.join('\n'),
          targetContent: '',
          startTimeInSeconds: start,
          endTimeInSeconds: end,
        };
      });
    } catch (error) {
      console.error('Error reading subtitle file:', error);
      return [];
    }
  };

  // 创建播放器字幕轨道（VTT blob URL）
  const createPlayerTrack = async (
    srtPath: string | undefined,
    language: string,
    isDefault?: boolean,
  ): Promise<PlayerSubtitleTrack | null> => {
    if (!srtPath) return null;
    try {
      const content = await readTextFile(srtPath);
      const fromFormat = detectSubtitleFormat(srtPath);
      const vttContent = convertSubtitleContent(content, fromFormat, 'vtt');
      const vttBlob = new Blob([vttContent], { type: 'text/vtt' });
      const vttUrl = URL.createObjectURL(vttBlob);
      return {
        kind: 'subtitles',
        src: vttUrl,
        srcLang: language,
        label: `(${language})`,
        default: isDefault,
      };
    } catch (error) {
      console.error(`转换字幕到 VTT 失败:`, error);
      return null;
    }
  };

  // 加载文件
  const loadFiles = useCallback(async () => {
    if (!config.sourceSubtitlePath) return;

    setIsLoading(true);
    try {
      if (config.videoPath) {
        setVideoPath(config.videoPath);
      }

      const playerTracks: PlayerSubtitleTrack[] = [];

      // 读取源字幕
      const sourceSubtitles = await readSubtitleFile(config.sourceSubtitlePath);
      if (config.sourceLanguage) {
        const track = await createPlayerTrack(
          config.sourceSubtitlePath,
          config.sourceLanguage,
          !shouldShowTranslation,
        );
        if (track) playerTracks.push(track);
      }

      // 读取翻译字幕
      let translatedSubtitles: Subtitle[] = [];
      if (config.targetSubtitlePath) {
        translatedSubtitles = await readSubtitleFile(config.targetSubtitlePath);
        setHasTranslationFile(translatedSubtitles.length > 0);

        if (config.targetLanguage) {
          const track = await createPlayerTrack(
            config.targetSubtitlePath,
            config.targetLanguage,
            true,
          );
          if (track) playerTracks.push(track);
        }
      }

      setSubtitleTracksForPlayer(playerTracks);

      // 合并字幕
      if (sourceSubtitles.length > 0) {
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
            targetContent: translated ? translated.sourceContent || translated.content.join('\n') : '',
            isEditing: false,
            startTimeInSeconds: start,
            endTimeInSeconds: end,
          };
        });

        setMergedSubtitles(merged);
      }
    } catch (error) {
      console.error('Error loading files:', error);
      toast.error('加载文件失败');
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
    if (!editSnapshot) {
      setEditSnapshot(JSON.parse(JSON.stringify(mergedSubtitles)));
    }

    const newSubtitles = [...mergedSubtitles];
    newSubtitles[index][field] = value;
    newSubtitles[index].content =
      field === 'sourceContent'
        ? value.split('\n')
        : newSubtitles[index].content;
    setMergedSubtitles(newSubtitles);
  };

  // 保存字幕文件
  const handleSave = async () => {
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

      // 保存源字幕
      if (config.sourceSubtitlePath) {
        const format = detectSubtitleFormat(config.sourceSubtitlePath);
        const entries = mergedSubtitles.map((sub) => ({
          id: sub.id,
          startEndTime: sub.startEndTime,
          text: buildText(sub, 'source'),
        }));
        const content = serializeSubtitleEntries(entries, format);
        await writeTextFile(config.sourceSubtitlePath, content);
      }

      // 保存翻译字幕
      if (config.targetSubtitlePath && shouldShowTranslation) {
        const format = detectSubtitleFormat(config.targetSubtitlePath);
        const entries = mergedSubtitles.map((sub) => ({
          id: sub.id,
          startEndTime: sub.startEndTime,
          text: buildText(sub, 'onlyTranslate'),
        }));
        const content = serializeSubtitleEntries(entries, format);
        await writeTextFile(config.targetSubtitlePath, content);
      }

      // 保存到目标翻译文件（如双语）
      if (config.finalTargetSubtitlePath && shouldShowTranslation) {
        const format = detectSubtitleFormat(config.finalTargetSubtitlePath);
        const contentType = config.translateContent || 'onlyTranslate';
        const entries = mergedSubtitles.map((sub) => ({
          id: sub.id,
          startEndTime: sub.startEndTime,
          text: buildText(sub, contentType),
        }));
        const content = serializeSubtitleEntries(entries, format);
        await writeTextFile(config.finalTargetSubtitlePath, content);
      }

      toast.success('字幕保存成功');
    } catch (error) {
      console.error('Error saving subtitles:', error);
      toast.error('保存失败');
    }
  };

  // 字幕统计
  const getSubtitleStats = (): SubtitleStats => {
    const total = mergedSubtitles.length;
    const withTranslation = shouldShowTranslation
      ? mergedSubtitles.filter(
          (sub) => sub.targetContent && sub.targetContent.trim() !== '',
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
      (!subtitle.targetContent || subtitle.targetContent.trim() === '')
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
  const pushToHistory = useCallback(
    (oldState: Subtitle[], newState: Subtitle[]) => {
      setHistory((prev) => {
        const newHistory = prev.slice(0, historyIndex + 1);
        if (newHistory.length === 0) {
          newHistory.push(JSON.parse(JSON.stringify(oldState)));
        }
        newHistory.push(JSON.parse(JSON.stringify(newState)));
        while (newHistory.length > maxHistoryLength) {
          newHistory.shift();
        }
        return newHistory;
      });
      setHistoryIndex((prev) => {
        if (prev === -1) return 1;
        return Math.min(prev + 1, maxHistoryLength - 1);
      });
    },
    [historyIndex],
  );

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
      setMergedSubtitles(JSON.parse(JSON.stringify(history[newIndex])));
      setEditSnapshot(null);
    }
  }, [historyIndex, history]);

  // 重做
  const handleRedo = useCallback(() => {
    if (historyIndex < history.length - 1) {
      const newIndex = historyIndex + 1;
      setHistoryIndex(newIndex);
      setMergedSubtitles(JSON.parse(JSON.stringify(history[newIndex])));
      setEditSnapshot(null);
    }
  }, [historyIndex, history]);

  // 是否可以撤销/重做
  const canUndo = historyIndex > 0 && history.length > 1;
  const canRedo = historyIndex < history.length - 1 && historyIndex >= 0;

  // 失焦记录
  useEffect(() => {
    if (
      previousSubtitleIndex !== -1 &&
      previousSubtitleIndex !== currentSubtitleIndex &&
      editSnapshot
    ) {
      const hasChanged =
        JSON.stringify(editSnapshot) !== JSON.stringify(mergedSubtitles);
      if (hasChanged) {
        pushToHistory(editSnapshot, mergedSubtitles);
      }
      setEditSnapshot(null);
    }
    setPreviousSubtitleIndex(currentSubtitleIndex);
  }, [currentSubtitleIndex, editSnapshot, mergedSubtitles, pushToHistory]);

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

      newSubtitles.forEach((sub, idx) => {
        sub.id = String(idx + 1);
      });

      updateSubtitles(newSubtitles);
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

      newSubtitles.forEach((sub, idx) => {
        sub.id = String(idx + 1);
      });

      updateSubtitles(newSubtitles);
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
    handleSubtitleChange,
    handleSave,
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
  };
};
