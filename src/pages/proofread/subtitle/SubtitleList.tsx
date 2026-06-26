import React, { useEffect, useRef } from 'react';
import {
  ChevronUp,
  ChevronDown,
  AlertTriangle,
  Sparkles,
  Scissors,
} from 'lucide-react';
import { Subtitle } from '../useStandaloneSubtitles';
import { useI18n } from '../../../lib/i18n';

interface SubtitleListProps {
  mergedSubtitles: Subtitle[];
  currentSubtitleIndex: number;
  shouldShowTranslation: boolean;
  handleSubtitleClick: (index: number) => void;
  handleSubtitleChange: (
    index: number,
    field: 'sourceContent' | 'targetContent',
    value: string,
  ) => void;
  isTranslationFailed: (subtitle: Subtitle) => boolean;
  getFailedTranslationIndices: () => number[];
  goToNextFailedTranslation: () => void;
  goToPreviousFailedTranslation: () => void;
  onCursorPositionChange?: (position: number) => void;
  onAiOptimizeClick?: (index: number) => void;
  onSplitClick?: (index: number) => void;
}

const SubtitleList: React.FC<SubtitleListProps> = ({
  mergedSubtitles,
  currentSubtitleIndex,
  shouldShowTranslation,
  handleSubtitleClick,
  handleSubtitleChange,
  isTranslationFailed,
  getFailedTranslationIndices,
  goToNextFailedTranslation,
  goToPreviousFailedTranslation,
  onCursorPositionChange,
  onAiOptimizeClick,
  onSplitClick,
}) => {
  const { t } = useI18n();
  const failedIndices = getFailedTranslationIndices();
  const hasFailedTranslations = failedIndices.length > 0;
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const skipNextAutoScrollRef = useRef(false);

  const handleSelectionChange = (
    e: React.SyntheticEvent<HTMLTextAreaElement>,
  ) => {
    const target = e.target as HTMLTextAreaElement;
    if (onCursorPositionChange) {
      onCursorPositionChange(target.selectionStart || 0);
    }
  };

  const onSubtitleClick = (index: number) => {
    if (index !== currentSubtitleIndex) {
      skipNextAutoScrollRef.current = true;
    }
    handleSubtitleClick(index);
  };

  useEffect(() => {
    if (skipNextAutoScrollRef.current) {
      skipNextAutoScrollRef.current = false;
      return;
    }
    if (currentSubtitleIndex >= 0 && scrollContainerRef.current) {
      const element = document.getElementById(`subtitle-${currentSubtitleIndex}`);
      if (element) {
        element.scrollIntoView({
          behavior: 'smooth',
          block: 'nearest',
        });
      }
    }
  }, [currentSubtitleIndex]);

  return (
    <div className="h-full flex flex-col bg-surface border border-border-default rounded-xl overflow-hidden shadow-xl">
      {/* 翻译失败导航栏 */}
      {shouldShowTranslation && (
        <div className="flex items-center justify-between px-4 py-2.5 bg-surface-raised border-b border-border-default flex-shrink-0">
          <div className="flex items-center gap-2 text-xs text-text-secondary">
            <AlertTriangle className="h-4 w-4 text-warning flex-shrink-0" />
            <span>
              {t('proofread.list.failedCount')}<strong className="text-warning">{failedIndices.length}</strong> / {mergedSubtitles.length}
            </span>
          </div>
          {hasFailedTranslations && (
            <div className="flex gap-1.5">
              <button
                onClick={goToPreviousFailedTranslation}
                className="p-1 hover:bg-surface-overlay rounded text-text-secondary hover:text-text-primary border border-border-subtle transition-colors"
                title={t('proofread.list.prevFailed')}
              >
                <ChevronUp className="h-3.5 w-3.5" />
              </button>
              <button
                onClick={goToNextFailedTranslation}
                className="p-1 hover:bg-surface-overlay rounded text-text-secondary hover:text-text-primary border border-border-subtle transition-colors"
                title={t('proofread.list.nextFailed')}
              >
                <ChevronDown className="h-3.5 w-3.5" />
              </button>
            </div>
          )}
        </div>
      )}

      {/* 字幕列表 */}
      <div className="flex-1 overflow-y-auto p-3 space-y-2.5" ref={scrollContainerRef}>
        {mergedSubtitles.map((subtitle, index) => {
          const isFailed = isTranslationFailed(subtitle);
          const isCurrent = currentSubtitleIndex === index;

          return (
            <div
              key={`${subtitle.id}-${index}`}
              id={`subtitle-${index}`}
              className={`p-3 rounded-lg border transition-all cursor-pointer group flex flex-col space-y-2 ${
                isCurrent
                  ? 'bg-surface-overlay border-brand/60 shadow-lg'
                  : isFailed
                    ? 'bg-danger/10 hover:bg-danger/15 border-danger/40'
                    : 'bg-surface-raised hover:bg-surface-overlay border-border-subtle'
              }`}
              onClick={() => onSubtitleClick(index)}
            >
              <div className="flex justify-between items-center text-[10px] text-text-secondary">
                <div className="flex items-center gap-1.5 font-medium">
                  {isFailed && <AlertTriangle className="h-3.5 w-3.5 text-danger" />}
                  <span className="font-mono">
                    #{subtitle.id} · {subtitle.startEndTime}
                  </span>
                  {isCurrent && (
                    <span className="text-[9px] bg-brand-subtle text-brand-text border border-border-default px-1 py-0.2 rounded ml-1">
                      {t('proofread.list.playing')}
                    </span>
                  )}
                  {isFailed && (
                    <span className="text-[9px] bg-danger/15 text-danger border border-danger/30 px-1 py-0.2 rounded ml-1">
                      {t('proofread.list.emptyTranslation')}
                    </span>
                  )}
                </div>

                {/* 单条字幕操作按钮 */}
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  {shouldShowTranslation && onAiOptimizeClick && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        onAiOptimizeClick(index);
                      }}
                      className="p-1 hover:bg-surface-overlay rounded transition-colors text-text-secondary hover:text-text-primary"
                      title={t('proofread.list.aiOptimize')}
                    >
                      <Sparkles className="h-3.5 w-3.5" />
                    </button>
                  )}
                  {onSplitClick && (
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        onSplitClick(index);
                      }}
                      className="p-1 hover:bg-surface-overlay rounded transition-colors text-text-secondary hover:text-text-primary"
                      title={t('proofread.list.splitSubtitle')}
                    >
                      <Scissors className="h-3.5 w-3.5" />
                    </button>
                  )}
                </div>
              </div>

              <textarea
                className="w-full bg-app-bg border border-border-default rounded-lg p-2 text-xs text-text-primary focus:outline-none focus:border-brand min-h-[32px] resize-none"
                value={subtitle.sourceContent || ''}
                onChange={(e) => handleSubtitleChange(index, 'sourceContent', e.target.value)}
                onClick={handleSelectionChange}
                onKeyUp={handleSelectionChange}
                placeholder={t('proofread.list.sourcePlaceholder')}
                rows={Math.max(1, (subtitle.sourceContent || '').split('\n').length)}
              />

              {shouldShowTranslation && (
                <textarea
                  className={`w-full bg-app-bg border rounded-lg p-2 text-xs text-text-primary focus:outline-none focus:border-brand min-h-[32px] resize-none ${
                    isFailed ? 'border-danger/40 focus:border-danger' : 'border-border-default'
                  }`}
                  value={subtitle.targetContent || ''}
                  onChange={(e) => handleSubtitleChange(index, 'targetContent', e.target.value)}
                  placeholder={isFailed ? t('proofread.list.failedPlaceholder') : t('proofread.list.targetPlaceholder')}
                  rows={Math.max(1, (subtitle.targetContent || '').split('\n').length)}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default SubtitleList;
