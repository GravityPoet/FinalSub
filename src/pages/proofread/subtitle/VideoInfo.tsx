import React from 'react';
import { FileVideo } from 'lucide-react';
import { formatTime } from '../useVideoPlayer';
import { SubtitleStats } from '../useStandaloneSubtitles';
import { useI18n } from '../../../lib/i18n';

interface VideoInfoProps {
  fileName: string;
  extension: string;
  duration: number;
  subtitleStats: SubtitleStats;
  shouldShowTranslation: boolean;
}

const VideoInfo: React.FC<VideoInfoProps> = ({
  fileName,
  extension,
  duration,
  subtitleStats,
  shouldShowTranslation,
}) => {
  const { t } = useI18n();

  return (
    <div className="p-4 bg-slate-800/40 border border-slate-700/50 rounded-xl shadow-lg">
      <div className="text-xs font-semibold text-slate-400 mb-2">{t('proofread.info.fileInfo')}</div>
      <div className="grid grid-cols-2 gap-2 text-xs text-slate-300">
        <div className="flex items-center gap-1.5 min-w-0">
          <FileVideo className="h-3.5 w-3.5 text-blue-400 flex-shrink-0" />
          <span className="truncate" title={fileName}>
            {fileName || t('proofread.info.unknown')} ({extension || t('proofread.info.unknown')})
          </span>
        </div>
        <div>
          {t('proofread.info.duration')}{formatTime(duration)}
        </div>
      </div>
      
      <hr className="border-slate-700/50 my-3" />
      
      <div className="text-xs font-semibold text-slate-400 mb-2">{t('proofread.info.stats')}</div>
      <div className="grid grid-cols-3 gap-2 text-xs text-slate-300">
        <div>
          {t('proofread.info.total')}{subtitleStats.total}
        </div>
        {shouldShowTranslation && (
          <>
            <div>
              {t('proofread.info.translationCount')}{subtitleStats.withTranslation}
            </div>
            <div>
              {t('proofread.info.completionRate')}{subtitleStats.percent}%
            </div>
          </>
        )}
      </div>
    </div>
  );
};

export default VideoInfo;
