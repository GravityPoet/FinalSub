import React from 'react';
import { formatTime } from '../useVideoPlayer';
import { Subtitle } from '../useStandaloneSubtitles';

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
  const currentSubtitle =
    currentSubtitleIndex >= 0 && currentSubtitleIndex < mergedSubtitles.length
      ? mergedSubtitles[currentSubtitleIndex]
      : null;

  return (
    <div className="p-4 bg-slate-800/40 border border-slate-700/50 rounded-xl mb-4 shadow-lg">
      <div className="flex justify-between items-center mb-3">
        <div className="text-xs font-semibold text-slate-400">当前字幕</div>
        <div className="text-xs font-medium text-slate-400">
          {formatTime(currentTime)} / {formatTime(duration)}
        </div>
      </div>

      {currentSubtitle ? (
        <div className="space-y-2">
          <div className="text-xs text-slate-500 font-mono">
            {currentSubtitle.startEndTime}
          </div>
          {currentSubtitle.sourceContent && (
            <div className="p-2.5 bg-slate-900/60 rounded-lg border-l-3 border-blue-500 text-sm text-slate-200 font-medium">
              {currentSubtitle.sourceContent}
            </div>
          )}
          {shouldShowTranslation &&
            hasTranslationFile &&
            currentSubtitle.targetContent && (
              <div className="p-2.5 bg-slate-900/60 rounded-lg border-l-3 border-emerald-500 text-sm text-slate-200 font-medium">
                {currentSubtitle.targetContent}
              </div>
            )}
        </div>
      ) : (
        <div className="text-sm text-slate-500 p-2 italic text-center">
          当前时间点无字幕
        </div>
      )}
    </div>
  );
};

export default CurrentSubtitle;
