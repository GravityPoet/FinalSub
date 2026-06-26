import React from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useI18n } from '../../../lib/i18n';

interface VideoPlayerProps {
  videoPath: string;
  playerRef: React.RefObject<HTMLVideoElement | null>;
  isPlaying: boolean;
  playbackRate: number;
  subtitleTracks?: Array<{
    kind: string;
    src: string;
    srcLang: string;
    default?: boolean;
    label: string;
  }>;
  togglePlay: () => void;
  goToNextSubtitle: () => void;
  goToPreviousSubtitle: () => void;
  seekVideo: (seconds: number) => void;
  handleTimeUpdate: (e: React.SyntheticEvent<HTMLVideoElement>) => void;
  handleLoadedMetadata: (e: React.SyntheticEvent<HTMLVideoElement>) => void;
  handleRateChange: (e: React.SyntheticEvent<HTMLVideoElement>) => void;
  changePlaybackRate: (delta: number) => void;
  setPlaybackRate: (rate: number) => void;
}

const VideoPlayer: React.FC<VideoPlayerProps> = ({
  videoPath,
  playerRef,
  playbackRate: _playbackRate,
  subtitleTracks,
  handleTimeUpdate,
  handleLoadedMetadata,
  handleRateChange,
}) => {
  const { t } = useI18n();
  const videoUrl = videoPath ? convertFileSrc(videoPath) : '';

  return (
    <div className="flex flex-col flex-shrink-0 bg-app-bg rounded-xl overflow-hidden border border-border-default shadow-2xl relative">
      <div className="relative aspect-video max-h-[38.5vh] flex items-center justify-center bg-black">
        {videoUrl ? (
          <video
            ref={playerRef}
            src={videoUrl}
            className="w-full h-full object-contain max-h-[38.5vh]"
            controls
            autoPlay={false}
            onTimeUpdate={handleTimeUpdate}
            onLoadedMetadata={handleLoadedMetadata}
            onRateChange={handleRateChange}
            style={{ outline: 'none' }}
          >
            {subtitleTracks?.map((track, idx) => (
              <track
                key={`${idx}-${track.src}`}
                kind="subtitles"
                src={track.src}
                srcLang={track.srcLang}
                label={track.label}
                default={track.default}
              />
            ))}
          </video>
        ) : (
          <div className="absolute inset-0 flex flex-col items-center justify-center text-text-tertiary gap-2">
            <span className="text-sm">{t('proofread.player.noVideo')}</span>
          </div>
        )}
      </div>
    </div>
  );
};

export default VideoPlayer;
