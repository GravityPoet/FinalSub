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
import { useToast } from '../Toast';
import { useI18n } from '../../../lib/i18n';
import { Button } from '../../../components/ui/Button';
import { Input, Select, Textarea } from '../../../components/ui/Input';

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
  const { t } = useI18n();
  const { showToast } = useToast();
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
    showToast('success', t('proofread.toolbar.replaceDone', { count: matchCount }));
    setShowSearchReplace(false);
    setSearchText('');
    setReplaceText('');
    setMatchCount(0);
  }, [searchText, replaceText, searchTarget, subtitles, matchCount, onSubtitlesChange, shouldShowTranslation, showToast]);

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
    showToast('success', t('proofread.toolbar.timeOffsetDone'));
    setShowTimeOffset(false);
  }, [timeOffset, offsetDirection, subtitles, onSubtitlesChange, showToast]);

  // 执行合并
  const handleMerge = useCallback(() => {
    if (
      mergeStart >= mergeEnd ||
      mergeStart < 0 ||
      mergeEnd > subtitles.length
    ) {
      showToast('error', t('proofread.toolbar.invalidMergeRange'));
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
      showToast('error', t('proofread.toolbar.aiNotConfigured'));
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
        showToast('error', res.error || t('proofread.toolbar.aiNoResult'));
      }
    } catch (error: any) {
      console.error('AI optimize error:', error);
      showToast('error', `${t('proofread.toolbar.aiRequestError')}: ${error.toString()}`);
    } finally {
      setAiOptimizing(false);
    }
  }, [currentSubtitleIndex, subtitles, aiProviders, selectedProviderId, customPrompt, showToast]);

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
    <div className="flex items-center gap-1.5 px-6 py-2 bg-surface/50 backdrop-blur-md border-b border-border-subtle flex-shrink-0 relative">
      {/* 撤销/重做 */}
      <button
        onClick={onUndo}
        disabled={!canUndo}
        className="p-1.5 hover:bg-surface-raised rounded-lg text-text-secondary hover:text-text-primary transition-colors disabled:opacity-40 cursor-pointer"
        title={t('proofread.toolbar.undo')}
      >
        <Undo2 className="h-4 w-4" />
      </button>
      <button
        onClick={onRedo}
        disabled={!canRedo}
        className="p-1.5 hover:bg-surface-raised rounded-lg text-text-secondary hover:text-text-primary transition-colors disabled:opacity-40 cursor-pointer"
        title={t('proofread.toolbar.redo')}
      >
        <Redo2 className="h-4 w-4" />
      </button>

      <div className="w-px h-5 bg-border-subtle mx-1.5" />

      {/* 搜索替换 */}
      <div className="relative">
        <button
          onClick={() => {
            const state = !showSearchReplace;
            closeAllPopovers();
            setShowSearchReplace(state);
          }}
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent cursor-pointer ${
            showSearchReplace
              ? 'bg-brand text-white shadow-brand-glow'
              : 'text-text-secondary hover:text-text-primary hover:bg-surface-overlay'
          }`}
        >
          <Search className="h-3.5 w-3.5 mr-1.5" />
          {t('proofread.toolbar.searchReplace')}
        </button>
        {showSearchReplace && (
          <div className="absolute top-10 left-0 z-40 bg-surface/95 border border-border-default p-4 rounded-xl shadow-lg w-80 text-xs space-y-3 backdrop-blur-md">
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium">{t('proofread.toolbar.findContent')}</label>
              <Input
                type="text"
                value={searchText}
                onChange={(e) => setSearchText(e.target.value)}
                onKeyUp={handleSearch}
                placeholder={t('proofread.toolbar.findPlaceholder')}
                className="py-1 px-2.5 text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium">{t('proofread.toolbar.replaceWith')}</label>
              <Input
                type="text"
                value={replaceText}
                onChange={(e) => setReplaceText(e.target.value)}
                placeholder={t('proofread.toolbar.replacePlaceholder')}
                className="py-1 px-2.5 text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium">{t('proofread.toolbar.replaceScope')}</label>
              <Select
                value={searchTarget}
                onChange={(e) => setSearchTarget(e.target.value as any)}
                className="py-1 px-2.5 text-xs"
              >
                <option value="both">{t('proofread.toolbar.scopeBoth')}</option>
                <option value="source">{t('proofread.toolbar.scopeSource')}</option>
                {shouldShowTranslation && <option value="target">{t('proofread.toolbar.scopeTarget')}</option>}
              </Select>
            </div>
            {matchCount > 0 && <p className="text-[10px] text-warning font-medium">{t('proofread.toolbar.matchCount').replace('{count}', String(matchCount))}</p>}
            <div className="flex justify-end gap-2 pt-1">
              <Button
                variant="secondary"
                onClick={handleSearch}
                size="sm"
              >
                {t('proofread.toolbar.find')}
              </Button>
              <Button
                variant="primary"
                onClick={handleReplace}
                disabled={matchCount === 0}
                size="sm"
              >
                {t('proofread.toolbar.replaceAll')}
              </Button>
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
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent cursor-pointer ${
            showTimeOffset
              ? 'bg-brand text-white shadow-brand-glow'
              : 'text-text-secondary hover:text-text-primary hover:bg-surface-overlay'
          }`}
        >
          <Clock className="h-3.5 w-3.5 mr-1.5" />
          {t('proofread.toolbar.timelineShift')}
        </button>
        {showTimeOffset && (
          <div className="absolute top-10 left-0 z-40 bg-surface/95 border border-border-default p-4 rounded-xl shadow-lg w-72 text-xs space-y-3.5 backdrop-blur-md">
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium">{t('proofread.toolbar.direction')}</label>
              <div className="flex bg-surface-raised p-0.5 rounded-lg border border-border-subtle">
                <button
                  onClick={() => setOffsetDirection('forward')}
                  className={`flex-1 py-1 rounded-md text-[10px] font-medium transition-all cursor-pointer ${
                    offsetDirection === 'forward' ? 'bg-surface text-text-primary shadow-sm border border-border-subtle/50' : 'text-text-tertiary'
                  }`}
                >
                  {t('proofread.toolbar.delay')}
                </button>
                <button
                  onClick={() => setOffsetDirection('backward')}
                  className={`flex-1 py-1 rounded-md text-[10px] font-medium transition-all cursor-pointer ${
                    offsetDirection === 'backward' ? 'bg-surface text-text-primary shadow-sm border border-border-subtle/50' : 'text-text-tertiary'
                  }`}
                >
                  {t('proofread.toolbar.advance')}
                </button>
              </div>
            </div>
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium">{t('proofread.toolbar.offsetSeconds')}</label>
              <Input
                type="number"
                step="0.1"
                value={timeOffset}
                onChange={(e) => setTimeOffset(e.target.value)}
                className="py-1 px-2.5 text-xs text-center font-mono"
              />
            </div>
            <div className="flex justify-end gap-2 pt-1">
              <Button
                variant="primary"
                onClick={handleTimeOffset}
                className="w-full"
                size="sm"
              >
                {t('proofread.toolbar.applyAll')}
              </Button>
            </div>
          </div>
        )}
      </div>

      <div className="w-px h-5 bg-border-subtle mx-1.5" />

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
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent cursor-pointer ${
            showMerge
              ? 'bg-brand text-white shadow-brand-glow'
              : 'text-text-secondary hover:text-text-primary hover:bg-surface-overlay'
          }`}
        >
          <Combine className="h-3.5 w-3.5 mr-1.5" />
          {t('proofread.toolbar.merge')}
        </button>
        {showMerge && (
          <div className="absolute top-10 left-0 z-40 bg-surface/95 border border-border-default p-4 rounded-xl shadow-lg w-80 text-xs space-y-3.5 backdrop-blur-md">
            <div className="p-3 bg-surface-overlay rounded-lg border border-border-subtle text-[10px] text-text-secondary leading-relaxed">
              {t('proofread.toolbar.mergeDesc')}
            </div>
            <div className="grid grid-cols-2 gap-3.5">
              <div className="space-y-1.5">
                <label className="text-text-secondary font-medium">{t('proofread.toolbar.startIndex')}</label>
                <Input
                  type="number"
                  min={1}
                  max={subtitles.length}
                  value={mergeStart + 1}
                  onChange={(e) => setMergeStart(Math.max(0, parseInt(e.target.value) - 1))}
                  className="py-1 px-2.5 text-xs text-center font-mono"
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-text-secondary font-medium">{t('proofread.toolbar.endIndex')}</label>
                <Input
                  type="number"
                  min={2}
                  max={subtitles.length + 1}
                  value={mergeEnd + 1}
                  onChange={(e) => setMergeEnd(Math.max(1, parseInt(e.target.value) - 1))}
                  className="py-1 px-2.5 text-xs text-center font-mono"
                />
              </div>
            </div>
            <Button
              variant="primary"
              onClick={handleMerge}
              className="w-full"
              size="sm"
            >
              {t('proofread.toolbar.merge')}
            </Button>
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
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent disabled:opacity-40 cursor-pointer ${
            showSplit
              ? 'bg-brand text-white shadow-brand-glow'
              : 'text-text-secondary hover:text-text-primary hover:bg-surface-overlay'
          }`}
        >
          <Split className="h-3.5 w-3.5 mr-1.5" />
          {t('proofread.toolbar.split')}
        </button>
        {showSplit && (
          <div className="absolute top-10 left-0 z-40 bg-surface/95 border border-border-default p-4 rounded-xl shadow-lg w-80 text-xs space-y-3.5 backdrop-blur-md">
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium">{t('proofread.toolbar.splitPos')}</label>
              <Input
                type="number"
                min={1}
                max={(subtitles[currentSubtitleIndex]?.sourceContent || '').length - 1}
                value={splitPosition}
                onChange={(e) => setSplitPosition(parseInt(e.target.value) || 1)}
                className="py-1 px-2.5 text-xs font-mono"
              />
            </div>
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium flex justify-between">
                <span>{t('proofread.toolbar.timeRatio')}</span>
                <span className="text-brand font-mono font-bold">{splitTimePercent}% : {100 - splitTimePercent}%</span>
              </label>
              <input
                type="range"
                min={10}
                max={90}
                value={splitTimePercent}
                onChange={(e) => setSplitTimePercent(parseInt(e.target.value))}
                className="w-full h-1.5 bg-surface-raised border border-border-subtle rounded-lg appearance-none cursor-pointer accent-brand"
              />
            </div>
            <Button
              variant="primary"
              onClick={executeSplit}
              className="w-full"
              size="sm"
            >
              {t('proofread.toolbar.split')}
            </Button>
          </div>
        )}
      </div>

      <div className="w-px h-5 bg-border-subtle mx-1.5" />

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
          className={`flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent disabled:opacity-40 cursor-pointer ${
            showAiOptimize
              ? 'bg-brand text-white shadow-brand-glow'
              : 'text-text-secondary hover:text-text-primary hover:bg-surface-overlay'
          }`}
        >
          <Wand2 className="h-3.5 w-3.5 mr-1.5" />
          {t('proofread.toolbar.aiOptimize')}
        </button>
        {showAiOptimize && (
          <div className="absolute top-10 left-0 z-40 bg-surface/95 border border-border-default p-4 rounded-xl shadow-lg w-[360px] text-xs space-y-3.5 backdrop-blur-md">
            <div className="space-y-1.5">
              <label className="text-text-secondary font-medium">{t('proofread.toolbar.selectAiService')}</label>
              {aiProviders.length === 0 ? (
                <div className="p-2 border border-border-subtle text-[10px] text-text-tertiary italic rounded bg-surface-overlay">
                  {t('proofread.toolbar.toastNoServiceInSettings')}
                </div>
              ) : (
                <Select
                  value={selectedProviderId}
                  onChange={(e) => setSelectedProviderId(e.target.value)}
                  className="py-1 px-2.5 text-xs"
                >
                  {aiProviders.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </Select>
              )}
            </div>

            <div className="space-y-1.5">
              <div className="flex justify-between items-center">
                <label className="text-text-secondary font-medium">{t('proofread.toolbar.customPrompt')}</label>
                <button
                  onClick={() => setShowCustomPrompt(!showCustomPrompt)}
                  className="text-[10px] text-brand hover:underline cursor-pointer"
                >
                  {showCustomPrompt ? t('proofread.toolbar.hideCustom') : t('proofread.toolbar.showCustom')}
                </button>
              </div>
              {showCustomPrompt && (
                <Textarea
                  value={customPrompt}
                  onChange={(e) => handlePromptChange(e.target.value)}
                  className="font-mono text-[10px] min-h-[120px] py-1.5 px-2.5"
                />
              )}
            </div>

            <div className="flex gap-2">
              <Button
                variant="primary"
                onClick={handleAiOptimize}
                disabled={aiOptimizing || aiProviders.length === 0}
                className="w-full"
                size="sm"
              >
                {aiOptimizing ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    {t('proofread.toolbar.optimizing')}
                  </>
                ) : (
                  <>
                    <Sparkles className="h-3.5 w-3.5" />
                    {t('proofread.toolbar.startOptimize')}
                  </>
                )}
              </Button>
            </div>

            {optimizedText && (
              <div className="space-y-1.5 border-t border-border-subtle pt-3">
                <label className="text-success font-semibold flex items-center gap-1">
                  <span>{t('proofread.toolbar.aiSuggestion')}</span>
                </label>
                <div className="p-2.5 bg-brand-subtle border border-brand/10 rounded-lg text-brand-text break-words leading-relaxed">
                  {optimizedText}
                </div>
                <Button
                  variant="primary"
                  onClick={handleAcceptOptimization}
                  className="w-full"
                  size="sm"
                >
                  {t('proofread.toolbar.applySuggestion')}
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* 批量 AI 优化 */}
      <button
        onClick={() => setShowBatchOptimize(true)}
        className="flex items-center text-xs px-3 py-1.5 rounded-lg transition-colors font-medium border border-transparent text-text-secondary hover:text-text-primary hover:bg-surface-overlay cursor-pointer"
      >
        <Sparkles className="h-3.5 w-3.5 mr-1.5 text-brand" />
        {t('proofread.toolbar.fullAiOptimize')}
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
