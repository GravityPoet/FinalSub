import React from 'react';
import { formatTime } from '../useVideoPlayer';
import { Subtitle } from '../useStandaloneSubtitles';
import { useI18n } from '../../../lib/i18n';

interface CurrentSubtitleProps {
  currentSubtitleIndex: number;
  currentTime: number;
  duration: number;
  mergedSubtitles: Subtitle[];
  shouldShowTranslation: boolean;
  hasTranslationFile: boolean;
}

const CurrentSubtitle: React.FC<CurrentSubtitleProps> = ({
  currentSubtitleIndex,
  currentTime,
  duration,
  mergedSubtitles,
  shouldShowTranslation,
  hasTranslationFile,
}) => {
  const { t } = useI18n();

  const currentSubtitle =
    currentSubtitleIndex >= 0 && currentSubtitleIndex < mergedSubtitles.length
      ? mergedSubtitles[currentSubtitleIndex]
      : null;

  return (
    <div className="p-4 bg-surface-raised border border-border-default rounded-xl mb-4 shadow-lg">
      <div className="flex justify-between items-center mb-3">
        <div className="text-xs font-semibold text-text-secondary">{t('proofread.current.title')}</div>
        <div className="text-xs font-medium text-text-secondary">
          {formatTime(currentTime)} / {formatTime(duration)}
        </div>
      </div>

      {currentSubtitle ? (
        <div className="space-y-2">
          <div className="text-xs text-text-tertiary font-mono">
            {currentSubtitle.startEndTime}
          </div>
          {currentSubtitle.sourceContent && (
            <div className="p-2.5 bg-app-bg rounded-lg border-l-3 border-brand text-sm text-text-primary font-medium">
              {currentSubtitle.sourceContent}
            </div>
          )}
          {shouldShowTranslation &&
            hasTranslationFile &&
            currentSubtitle.targetContent && (
              <div className="p-2.5 bg-app-bg rounded-lg border-l-3 border-success text-sm text-text-primary font-medium">
                {currentSubtitle.targetContent}
              </div>
            )}
        </div>
      ) : (
        <div className="text-sm text-text-tertiary p-2 italic text-center">
          {t('proofread.current.empty')}
        </div>
      )}
    </div>
  );
};

export default CurrentSubtitle;
