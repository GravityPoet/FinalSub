import React from 'react';
import { FileVideo } from 'lucide-react';
import { formatTime } from '../useVideoPlayer';
import { SubtitleStats } from '../useStandaloneSubtitles';

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
  return (
    <div className="p-4 bg-slate-800/40 border border-slate-700/50 rounded-xl shadow-lg">
      <div className="text-xs font-semibold text-slate-400 mb-2">文件信息</div>
      <div className="grid grid-cols-2 gap-2 text-xs text-slate-300">
        <div className="flex items-center gap-1.5 min-w-0">
          <FileVideo className="h-3.5 w-3.5 text-blue-400 flex-shrink-0" />
          <span className="truncate" title={fileName}>
            {fileName || '未知'} ({extension || '未知'})
          </span>
        </div>
        <div>
          时长: {formatTime(duration)}
        </div>
      </div>
      
      <hr className="border-slate-700/50 my-3" />
      
      <div className="text-xs font-semibold text-slate-400 mb-2">字幕统计</div>
      <div className="grid grid-cols-3 gap-2 text-xs text-slate-300">
        <div>
          总数: {subtitleStats.total}
        </div>
        {shouldShowTranslation && (
          <>
            <div>
              翻译数: {subtitleStats.withTranslation}
            </div>
            <div>
              完成率: {subtitleStats.percent}%
            </div>
          </>
        )}
      </div>
    </div>
  );
};

export default VideoInfo;
