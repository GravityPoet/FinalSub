import { useState, useCallback, useEffect } from 'react';
import {
  Search,
  Clock,
  Undo2,
  Redo2,
  Combine,
  Split,
  Sparkles,
  Loader2,
  Wand2,
} from 'lucide-react';
import { Subtitle } from '../useStandaloneSubtitles';
import BatchAiOptimizeDialog from './BatchAiOptimizeDialog';
import { listTranslationProviders, testTranslation } from '../../../lib/tauri';

interface SubtitleEditToolbarProps {
  subtitles: Subtitle[];
  onSubtitlesChange: (subtitles: Subtitle[]) => void;
  onUndo: () => void;
  onRedo: () => void;
  canUndo: boolean;
  canRedo: boolean;
  currentSubtitleIndex: number;
  onMergeSubtitles: (startIndex: number, endIndex: number) => void;
  onSplitSubtitle: (
    index: number,
    splitPoint: number,
    splitTime?: number,
  ) => void;
  shouldShowTranslation: boolean;
  getCursorPosition?: () => number;
  triggerAiOptimize?: boolean;
  triggerSplit?: boolean;
  onTriggerHandled?: () => void;
}

export default function SubtitleEditToolbar({
  subtitles,
  onSubtitlesChange,
  onUndo,
  onRedo,
  canUndo,
  canRedo,
  currentSubtitleIndex,
  onMergeSubtitles,
  onSplitSubtitle,
  shouldShowTranslation,
  getCursorPosition,
  triggerAiOptimize,
  triggerSplit,
  onTriggerHandled,
}: SubtitleEditToolbarProps) {
  // 搜索替换状态
  const [showSearchReplace, setShowSearchReplace] = useState(false);
  const [searchText, setSearchText] = useState('');
  const [replaceText, setReplaceText] = useState('');
  const [searchTarget, setSearchTarget] = useState<'source' | 'target' | 'both'>('both');
  const [matchCount, setMatchCount] = useState(0);

  // 拆分对话框状态
  const [showSplit, setShowSplit] = useState(false);
  const [splitPosition, setSplitPosition] = useState(0);
  const [splitTimePercent, setSplitTimePercent] = useState(50);

  // 时间轴偏移状态
  const [showTimeOffset, setShowTimeOffset] = useState(false);
  const [timeOffset, setTimeOffset] = useState('0');
  const [offsetDirection, setOffsetDirection] = useState<'forward' | 'backward'>('forward');

  // 合并状态
  const [showMerge, setShowMerge] = useState(false);
  const [mergeStart, setMergeStart] = useState(0);
  const [mergeEnd, setMergeEnd] = useState(0);

  // AI 优化状态
  const [showAiOptimize, setShowAiOptimize] = useState(false);
  const [aiOptimizing, setAiOptimizing] = useState(false);
  const [optimizedText, setOptimizedText] = useState('');
  const [aiProviders, setAiProviders] = useState<any[]>([]);
  const [selectedProviderId, setSelectedProviderId] = useState('');
  const [showCustomPrompt, setShowCustomPrompt] = useState(false);
  const [customPrompt, setCustomPrompt] = useState('');
  const [isCustomPromptLoaded, setIsCustomPromptLoaded] = useState(false);

  // 批量 AI 优化状态
  const [showBatchOptimize, setShowBatchOptimize] = useState(false);

  const defaultOptimizePrompt = `You are a professional subtitle translator and proofreader.

Original text:
{{sourceText}}

Current translation:
{{targetText}}

Please improve the translation:
1. Accurately convey the meaning of the original.
2. Use natural and fluent target expressions.
3. Be appropriate for subtitle display.
4. Maintain the tone and style.

Only respond with the translated/improved text, nothing else.`;

  const PROMPT_CACHE_KEY = 'ai_optimize_custom_prompt';

  // 搜索匹配数量统计
  const handleSearch = useCallback(() => {
    if (!searchText) {
      setMatchCount(0);
      return;
    }

    let count = 0;
    subtitles.forEach((sub) => {
      if (searchTarget === 'source' || searchTarget === 'both') {
        if (sub.sourceContent?.includes(searchText)) count++;
      }
      if (
        (searchTarget === 'target' || searchTarget === 'both') &&
        shouldShowTranslation
      ) {
        if (sub.targetContent?.includes(searchText)) count++;
      }
    });
    setMatchCount(count);
  }, [searchText, searchTarget, subtitles, shouldShowTranslation]);

  // 执行替换
  const handleReplace = useCallback(() => {
    if (!searchText) return;

    const newSubtitles = subtitles.map((sub) => {
      const newSub = { ...sub };
      if (searchTarget === 'source' || searchTarget === 'both') {
        if (newSub.sourceContent) {
          newSub.sourceContent = newSub.sourceContent.split(searchText).join(replaceText);
          newSub.content = newSub.sourceContent.split('\n');
        }
      }
      if (
        (searchTarget === 'target' || searchTarget === 'both') &&
        shouldShowTranslation
      ) {
        if (newSub.targetContent) {
          newSub.targetContent = newSub.targetContent.split(searchText).join(replaceText);
        }
      }
      return newSub;
    });

    onSubtitlesChange(newSubtitles);
    alert(`已成功替换 ${matchCount} 处文本`);
    setShowSearchReplace(false);
    setSearchText('');
    setReplaceText('');
    setMatchCount(0);
  }, [searchText, replaceText, searchTarget, subtitles, matchCount, onSubtitlesChange, shouldShowTranslation]);

  const timeToSeconds = (timeStr: string): number => {
    const parts = timeStr.replace(',', '.').split(':');
    if (parts.length !== 3) return 0;
    const hours = parseInt(parts[0], 10);
    const minutes = parseInt(parts[1], 10);
    const seconds = parseFloat(parts[2]);
    return hours * 3600 + minutes * 60 + seconds;
  };

  const secondsToTime = (seconds: number): string => {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = (seconds % 60).toFixed(3);
    return `${h.toString().padStart(2, '0')}:${m.toString().padStart(2, '0')}:${s.padStart(6, '0').replace('.', ',')}`;
  };

  // 执行时间偏移
  const handleTimeOffset = useCallback(() => {
    const offsetSeconds =
      parseFloat(timeOffset) * (offsetDirection === 'forward' ? 1 : -1);
    if (isNaN(offsetSeconds) || offsetSeconds === 0) return;

    const newSubtitles = subtitles.map((sub) => {
      const newSub = { ...sub };
      const times = sub.startEndTime.split(' --> ');
      if (times.length === 2) {
        const startSeconds = Math.max(0, timeToSeconds(times[0]) + offsetSeconds);
        const endSeconds = Math.max(0, timeToSeconds(times[1]) + offsetSeconds);

        newSub.startEndTime = `${secondsToTime(startSeconds)} --> ${secondsToTime(endSeconds)}`;
        newSub.startTimeInSeconds = startSeconds;
        newSub.endTimeInSeconds = endSeconds;
      }
      return newSub;
    });

    onSubtitlesChange(newSubtitles);
    alert('时间轴调整完成');
    setShowTimeOffset(false);
  }, [timeOffset, offsetDirection, subtitles, onSubtitlesChange]);

  // 执行合并
  const handleMerge = useCallback(() => {
    if (
      mergeStart >= mergeEnd ||
      mergeStart < 0 ||
      mergeEnd > subtitles.length
    ) {
      alert('无效的合并范围');
      return;
    }
    // 我们的 index 都是 0-based，UI 传入的可能需要微调。
    // 在这里，onMergeSubtitles 接收的是 0-based 索引范围 [startIndex, endIndex)
    onMergeSubtitles(mergeStart, mergeEnd);
    setShowMerge(false);
  }, [mergeStart, mergeEnd, subtitles.length, onMergeSubtitles]);

  // 加载 AI 服务商列表
  const loadAiProviders = useCallback(async () => {
    try {
      const providers = await listTranslationProviders();
      const aiOnly = providers.filter((p: any) => p.is_ai);
      setAiProviders(aiOnly);
      if (aiOnly.length > 0 && !selectedProviderId) {
        setSelectedProviderId(aiOnly[0].id);
      }
    } catch (error) {
      console.error('Failed to load AI providers:', error);
    }
  }, [selectedProviderId]);

  // 加载缓存的提示词
  const loadCachedPrompt = useCallback(() => {
    if (isCustomPromptLoaded) return;
    try {
      const cached = localStorage.getItem(PROMPT_CACHE_KEY);
      if (cached) {
        setCustomPrompt(cached);
        setShowCustomPrompt(true);
      } else {
        setCustomPrompt(defaultOptimizePrompt);
      }
      setIsCustomPromptLoaded(true);
    } catch {
      setCustomPrompt(defaultOptimizePrompt);
      setIsCustomPromptLoaded(true);
    }
  }, [isCustomPromptLoaded, defaultOptimizePrompt]);

  const handlePromptChange = useCallback((value: string) => {
    setCustomPrompt(value);
    try {
      if (value.trim() !== defaultOptimizePrompt.trim()) {
        localStorage.setItem(PROMPT_CACHE_KEY, value);
      } else {
        localStorage.removeItem(PROMPT_CACHE_KEY);
      }
    } catch {}
  }, [defaultOptimizePrompt]);

  const handleOpenAiOptimize = useCallback(() => {
    if (currentSubtitleIndex >= 0) {
      setOptimizedText('');
      loadAiProviders();
      loadCachedPrompt();
      setShowAiOptimize(true);
    }
  }, [currentSubtitleIndex, loadAiProviders, loadCachedPrompt]);

  const handleOpenSplit = useCallback(() => {
    if (currentSubtitleIndex >= 0 && currentSubtitleIndex < subtitles.length) {
      const subtitle = subtitles[currentSubtitleIndex];
      const content = subtitle.sourceContent || '';
      const cursorPos = getCursorPosition
        ? getCursorPosition()
        : Math.floor(content.length / 2);
      setSplitPosition(Math.max(1, Math.min(cursorPos, content.length - 1)));
      setSplitTimePercent(50);
      setShowSplit(true);
    }
  }, [currentSubtitleIndex, subtitles, getCursorPosition]);

  useEffect(() => {
    if (triggerAiOptimize && currentSubtitleIndex >= 0) {
      handleOpenAiOptimize();
      onTriggerHandled?.();
    }
  }, [triggerAiOptimize, currentSubtitleIndex, handleOpenAiOptimize, onTriggerHandled]);

  useEffect(() => {
    if (triggerSplit && currentSubtitleIndex >= 0) {
      handleOpenSplit();
      onTriggerHandled?.();
    }
  }, [triggerSplit, currentSubtitleIndex, handleOpenSplit, onTriggerHandled]);

  // AI 优化单条字幕
  const handleAiOptimize = useCallback(async () => {
    if (currentSubtitleIndex < 0 || currentSubtitleIndex >= subtitles.length) return;

    const subtitle = subtitles[currentSubtitleIndex];
    const sourceText = subtitle.sourceContent || '';
    const targetText = subtitle.targetContent || '';

    if (aiProviders.length === 0) {
      alert('请先配置并开启 AI 翻译服务');
      return;
    }

    setAiOptimizing(true);
    setOptimizedText('');

    try {
      const formattedPrompt = customPrompt
        .replace('{{sourceText}}', sourceText)
        .replace('{{targetText}}', targetText);

      const res = await testTranslation({
        text: formattedPrompt,
        source_language: 'auto',
        target_language: 'auto',
        provider: selectedProviderId
      });

      if (res.success && res.translated_text) {
        setOptimizedText(res.translated_text.trim());
      } else {
        alert(res.error || 'AI 优化未返回结果');
      }
    } catch (error: any) {
      console.error('AI optimize error:', error);
      alert('AI 优化请求错误: ' + error.toString());
    } finally {
      setAiOptimizing(false);
    }
  }, [currentSubtitleIndex, subtitles, aiProviders, selectedProviderId, customPrompt]);

  const handleAcceptOptimization = useCallback(() => {
    if (!optimizedText || currentSubtitleIndex < 0) return;

    const newSubtitles = [...subtitles];
    newSubtitles[currentSubtitleIndex] = {
      ...newSubtitles[currentSubtitleIndex],
      targetContent: optimizedText,
    };
    onSubtitlesChange(newSubtitles);
    setShowAiOptimize(false);
    setOptimizedText('');
  }, [optimizedText, currentSubtitleIndex, subtitles, onSubtitlesChange]);

  const handleApplyBatchOptimizations = useCallback(
    (optimizations: Array<{ index: number; targetContent: string }>) => {
      const newSubtitles = [...subtitles];
      optimizations.forEach(({ index, targetContent }) => {
        if (index >= 0 && index < newSubtitles.length) {
          newSubtitles[index] = {
            ...newSubtitles[index],
            targetContent,
          };
        }
      });
      onSubtitlesChange(newSubtitles);
    },
    [subtitles, onSubtitlesChange],
  );

  const handleOpenMergeDialog = () => {
    setMergeStart(Math.max(0, currentSubtitleIndex));
    setMergeEnd(Math.min(subtitles.length, currentSubtitleIndex + 2));
    setShowMerge(true);
  };

  const executeSplit = () => {
    if (currentSubtitleIndex < 0 || currentSubtitleIndex >= subtitles.length) return;
    const sub = subtitles[currentSubtitleIndex];
    const startTime = sub.startTimeInSeconds || 0;
    const endTime = sub.endTimeInSeconds || 0;
    const splitTime = startTime + (endTime - startTime) * (splitTimePercent / 100);

    onSplitSubtitle(currentSubtitleIndex, splitPosition, splitTime);
    setShowSplit(false);
  };

  const closeAllPopovers = () => {
    setShowSearchReplace(false);
    setShowTimeOffset(false);
    setShowMerge(false);
    setShowSplit(false);
    setShowAiOptimize(false);
  };

  return (
    <div className="flex items-center gap-1.5 px-6 py-2.5 bg-slate-800/35 border-b border-slate-700/50 flex-shrink-0 relative">
      {/* 撤销/重做 */}
      <button
        onClick={onUndo}
        disabled={!canUndo}
        className="p-1.5 hover:bg-slate-700/50 rounded-lg text-slate-400 hover:text-slate-200 transition-colors disabled:opacity-40"
        title="撤销"
      >
        <Undo2 className="h-4 w-4" />
      </button>
      <button
        onClick={onRedo}
        disabled={!canRedo}
        className="p-1.5 hover:bg-slate-700/50 rounded-lg text-slate-400 hover:text-slate-200 transition-colors disabled:opacity-40"
        title="重做"
      >
        <Redo2 className="h-4 w-4" />
      </button>

      <div className="w-px h-5 bg-slate-750 mx-1.5" />

      {/* 搜索替换 */}
      <div className="relative">
        <button
          onClick={() => {
            const state = !showSearchReplace;
            closeAllPopovers();
            setShowSearchReplace(state);
          }}
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent ${
            showSearchReplace
              ? 'bg-blue-600 text-white shadow-md shadow-blue-500/5'
              : 'text-slate-300 hover:text-white hover:bg-slate-700/50'
          }`}
        >
          <Search className="h-3.5 w-3.5 mr-1.5" />
          搜索替换
        </button>
        {showSearchReplace && (
          <div className="absolute top-10 left-0 z-40 bg-slate-900 border border-slate-750 p-4 rounded-xl shadow-2xl w-80 text-xs space-y-3">
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium">查找内容</label>
              <input
                type="text"
                value={searchText}
                onChange={(e) => setSearchText(e.target.value)}
                onKeyUp={handleSearch}
                placeholder="输入要查找的字符"
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium">替换为</label>
              <input
                type="text"
                value={replaceText}
                onChange={(e) => setReplaceText(e.target.value)}
                placeholder="输入要替换的字符"
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-blue-500"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium">替换范围</label>
              <select
                value={searchTarget}
                onChange={(e) => setSearchTarget(e.target.value as any)}
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-blue-500"
              >
                <option value="both">原文和翻译</option>
                <option value="source">仅原文</option>
                {shouldShowTranslation && <option value="target">仅翻译</option>}
              </select>
            </div>
            {matchCount > 0 && <p className="text-[10px] text-amber-500">找到 {matchCount} 处匹配</p>}
            <div className="flex justify-end gap-2 pt-1">
              <button
                onClick={handleSearch}
                className="bg-slate-800 hover:bg-slate-750 px-3 py-1.5 rounded-lg text-slate-200 font-medium transition-colors border border-slate-700/50"
              >
                查找
              </button>
              <button
                onClick={handleReplace}
                disabled={matchCount === 0}
                className="bg-blue-600 hover:bg-blue-750 px-3 py-1.5 rounded-lg text-white font-medium transition-colors disabled:opacity-40"
              >
                替换全部
              </button>
            </div>
          </div>
        )}
      </div>

      {/* 时间轴偏移 */}
      <div className="relative">
        <button
          onClick={() => {
            const state = !showTimeOffset;
            closeAllPopovers();
            setShowTimeOffset(state);
          }}
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent ${
            showTimeOffset
              ? 'bg-blue-600 text-white shadow-md shadow-blue-500/5'
              : 'text-slate-300 hover:text-white hover:bg-slate-700/50'
          }`}
        >
          <Clock className="h-3.5 w-3.5 mr-1.5" />
          时间轴微调
        </button>
        {showTimeOffset && (
          <div className="absolute top-10 left-0 z-40 bg-slate-900 border border-slate-750 p-4 rounded-xl shadow-2xl w-72 text-xs space-y-3.5">
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium">调整方向</label>
              <div className="flex bg-slate-950 p-0.5 rounded-lg border border-slate-800">
                <button
                  onClick={() => setOffsetDirection('forward')}
                  className={`flex-1 py-1 rounded-md text-[10px] font-medium transition-all ${
                    offsetDirection === 'forward' ? 'bg-slate-800 text-white' : 'text-slate-400'
                  }`}
                >
                  向前延后 (延迟)
                </button>
                <button
                  onClick={() => setOffsetDirection('backward')}
                  className={`flex-1 py-1 rounded-md text-[10px] font-medium transition-all ${
                    offsetDirection === 'backward' ? 'bg-slate-800 text-white' : 'text-slate-400'
                  }`}
                >
                  向后提前 (赶前)
                </button>
              </div>
            </div>
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium">偏移秒数</label>
              <input
                type="number"
                step="0.1"
                value={timeOffset}
                onChange={(e) => setTimeOffset(e.target.value)}
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-blue-500 text-center"
              />
            </div>
            <div className="flex justify-end gap-2 pt-1">
              <button
                onClick={handleTimeOffset}
                className="w-full bg-blue-600 hover:bg-blue-700 text-white py-1.5 rounded-lg transition-colors font-medium"
              >
                确认应用到全部
              </button>
            </div>
          </div>
        )}
      </div>

      <div className="w-px h-5 bg-slate-750 mx-1.5" />

      {/* 合并 */}
      <div className="relative">
        <button
          onClick={() => {
            const state = !showMerge;
            closeAllPopovers();
            if (!state) {
              setShowMerge(false);
            } else {
              handleOpenMergeDialog();
            }
          }}
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent ${
            showMerge
              ? 'bg-blue-600 text-white shadow-md shadow-blue-500/5'
              : 'text-slate-300 hover:text-white hover:bg-slate-700/50'
          }`}
        >
          <Combine className="h-3.5 w-3.5 mr-1.5" />
          合并字幕
        </button>
        {showMerge && (
          <div className="absolute top-10 left-0 z-40 bg-slate-900 border border-slate-750 p-4 rounded-xl shadow-2xl w-80 text-xs space-y-3.5">
            <div className="p-3 bg-slate-955 rounded-lg border border-slate-800 text-[10px] text-slate-400 leading-relaxed">
              输入要合并的字幕序号范围。例如，输入 [1, 3] 将把序号为 1 和 2 的字幕合并为一条。
            </div>
            <div className="grid grid-cols-2 gap-3.5">
              <div className="space-y-1.5">
                <label className="text-slate-400 font-medium">开始序号 (包含)</label>
                <input
                  type="number"
                  min={1}
                  max={subtitles.length}
                  value={mergeStart + 1}
                  onChange={(e) => setMergeStart(Math.max(0, parseInt(e.target.value) - 1))}
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 text-center focus:outline-none"
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-slate-400 font-medium">结束序号 (不含)</label>
                <input
                  type="number"
                  min={2}
                  max={subtitles.length + 1}
                  value={mergeEnd + 1}
                  onChange={(e) => setMergeEnd(Math.max(1, parseInt(e.target.value) - 1))}
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 text-center focus:outline-none"
                />
              </div>
            </div>
            <button
              onClick={handleMerge}
              className="w-full bg-blue-600 hover:bg-blue-700 text-white py-1.5 rounded-lg transition-colors font-medium"
            >
              合并字幕
            </button>
          </div>
        )}
      </div>

      {/* 拆分 */}
      <div className="relative">
        <button
          onClick={() => {
            const state = !showSplit;
            closeAllPopovers();
            if (!state) {
              setShowSplit(false);
            } else {
              handleOpenSplit();
            }
          }}
          disabled={currentSubtitleIndex < 0}
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent disabled:opacity-40 ${
            showSplit
              ? 'bg-blue-600 text-white shadow-md shadow-blue-500/5'
              : 'text-slate-300 hover:text-white hover:bg-slate-700/50'
          }`}
        >
          <Split className="h-3.5 w-3.5 mr-1.5" />
          拆分字幕
        </button>
        {showSplit && (
          <div className="absolute top-10 left-0 z-40 bg-slate-900 border border-slate-750 p-4 rounded-xl shadow-2xl w-80 text-xs space-y-3.5">
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium">文字拆分位置 (字符光标数)</label>
              <input
                type="number"
                min={1}
                max={(subtitles[currentSubtitleIndex]?.sourceContent || '').length - 1}
                value={splitPosition}
                onChange={(e) => setSplitPosition(parseInt(e.target.value) || 1)}
                className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium flex justify-between">
                <span>时间拆分比率</span>
                <span className="text-blue-500">{splitTimePercent}% : {100 - splitTimePercent}%</span>
              </label>
              <input
                type="range"
                min={10}
                max={90}
                value={splitTimePercent}
                onChange={(e) => setSplitTimePercent(parseInt(e.target.value))}
                className="w-full h-1.5 bg-slate-955 rounded-lg appearance-none cursor-pointer accent-blue-600"
              />
            </div>
            <button
              onClick={executeSplit}
              className="w-full bg-blue-600 hover:bg-blue-700 text-white py-1.5 rounded-lg transition-colors font-medium"
            >
              拆分字幕
            </button>
          </div>
        )}
      </div>

      <div className="w-px h-5 bg-slate-750 mx-1.5" />

      {/* AI 优化 (单条) */}
      <div className="relative">
        <button
          onClick={() => {
            const state = !showAiOptimize;
            closeAllPopovers();
            if (!state) {
              setShowAiOptimize(false);
            } else {
              handleOpenAiOptimize();
            }
          }}
          disabled={currentSubtitleIndex < 0}
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent disabled:opacity-40 ${
            showAiOptimize
              ? 'bg-blue-600 text-white shadow-md shadow-blue-500/5'
              : 'text-slate-300 hover:text-white hover:bg-slate-700/50'
          }`}
        >
          <Wand2 className="h-3.5 w-3.5 mr-1.5" />
          AI 优化单条
        </button>
        {showAiOptimize && (
          <div className="absolute top-10 left-0 z-40 bg-slate-900 border border-slate-750 p-4 rounded-xl shadow-2xl w-[360px] text-xs space-y-3.5">
            <div className="space-y-1.5">
              <label className="text-slate-400 font-medium">选择 AI 翻译服务</label>
              {aiProviders.length === 0 ? (
                <div className="p-2 border border-slate-800 text-[10px] text-slate-400 italic rounded bg-slate-950/50">
                  未配置 AI 翻译服务，请先在设置中添加
                </div>
              ) : (
                <select
                  value={selectedProviderId}
                  onChange={(e) => setSelectedProviderId(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none"
                >
                  {aiProviders.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              )}
            </div>

            <div className="space-y-1.5">
              <div className="flex justify-between items-center">
                <label className="text-slate-400 font-medium">自定义优化提示词</label>
                <button
                  onClick={() => setShowCustomPrompt(!showCustomPrompt)}
                  className="text-[10px] text-blue-500 hover:underline"
                >
                  {showCustomPrompt ? '收起' : '展开自定义'}
                </button>
              </div>
              {showCustomPrompt && (
                <textarea
                  value={customPrompt}
                  onChange={(e) => handlePromptChange(e.target.value)}
                  className="w-full bg-slate-950 font-mono border border-slate-800 text-[10px] text-slate-350 p-2.5 rounded-lg min-h-[120px] focus:outline-none"
                />
              )}
            </div>

            <div className="flex gap-2">
              <button
                onClick={handleAiOptimize}
                disabled={aiOptimizing || aiProviders.length === 0}
                className="w-full bg-blue-600 hover:bg-blue-700 text-white py-2 rounded-lg transition-colors font-medium flex items-center justify-center gap-1 disabled:opacity-40"
              >
                {aiOptimizing ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    正在优化...
                  </>
                ) : (
                  <>
                    <Sparkles className="h-3.5 w-3.5 text-blue-200" />
                    开始生成 AI 优化
                  </>
                )}
              </button>
            </div>

            {optimizedText && (
              <div className="space-y-1.5 border-t border-slate-800 pt-3">
                <label className="text-emerald-400 font-semibold flex items-center gap-1">
                  <span>AI 优化建议:</span>
                </label>
                <div className="p-2.5 bg-slate-950/70 border border-slate-850 rounded-lg text-slate-200 break-words leading-relaxed">
                  {optimizedText}
                </div>
                <button
                  onClick={handleAcceptOptimization}
                  className="w-full bg-emerald-600 hover:bg-emerald-700 text-white py-1.5 rounded-lg transition-colors font-medium"
                >
                  采纳该优化译文
                </button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* 批量 AI 优化 */}
      <button
        onClick={() => setShowBatchOptimize(true)}
        className="flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent text-slate-300 hover:text-white hover:bg-slate-700/50"
      >
        <Sparkles className="h-3.5 w-3.5 mr-1.5 text-blue-500" />
        全文 AI 优化
      </button>

      {/* 批量 AI 优化对话框 */}
      <BatchAiOptimizeDialog
        open={showBatchOptimize}
        onOpenChange={setShowBatchOptimize}
        subtitles={subtitles}
        onApplyOptimizations={handleApplyBatchOptimizations}
        shouldShowTranslation={shouldShowTranslation}
      />
    </div>
  );
}
