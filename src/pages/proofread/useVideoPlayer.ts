import { useState, useRef, useEffect } from 'react';
import { Subtitle } from './useStandaloneSubtitles';

// 格式化时间为 MM:SS 格式
export const formatTime = (seconds: number): string => {
  if (!seconds && seconds !== 0) return '--:--';
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
};

export const useVideoPlayer = (
  mergedSubtitles: Subtitle[],
  currentSubtitleIndex: number,
  setCurrentSubtitleIndex: (index: number) => void,
) => {
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [playbackRate, setPlaybackRate] = useState(1);
  const playerRef = useRef<HTMLVideoElement>(null);

  // 根据当前播放时间查找活跃字幕
  useEffect(() => {
    if (currentTime >= 0 && mergedSubtitles.length > 0) {
      const index = mergedSubtitles.findIndex(
        (sub) =>
          (sub.startTimeInSeconds ?? 0) <= currentTime &&
          (sub.endTimeInSeconds ?? 0) > currentTime,
      );
      if (index !== -1 && index !== currentSubtitleIndex) {
        setCurrentSubtitleIndex(index);
      }
    }
  }, [currentTime, mergedSubtitles]);

  // 播放器进度更新（对应原生 video 的 onTimeUpdate）
  const handleTimeUpdate = (e: React.SyntheticEvent<HTMLVideoElement>) => {
    setCurrentTime(e.currentTarget.currentTime);
  };

  // 播放器加载元数据（获取总时长）
  const handleLoadedMetadata = (e: React.SyntheticEvent<HTMLVideoElement>) => {
    setDuration(e.currentTarget.duration);
  };

  const togglePlay = () => {
    if (playerRef.current) {
      if (isPlaying) {
        playerRef.current.pause();
        setIsPlaying(false);
      } else {
        playerRef.current.play().catch(console.error);
        setIsPlaying(true);
      }
    }
  };

  // 点击字幕跳转到对应时间点
  const handleSubtitleClick = (index: number) => {
    if (index >= 0 && index < mergedSubtitles.length) {
      setCurrentSubtitleIndex(index);

      if (playerRef.current) {
        const startTime = mergedSubtitles[index]?.startTimeInSeconds ?? 0;
        playerRef.current.currentTime = startTime + 0.01;
      }
    }
  };

  // 跳转到下一个字幕
  const goToNextSubtitle = () => {
    const nextIndex = Math.min(
      currentSubtitleIndex + 1,
      mergedSubtitles.length - 1,
    );
    if (nextIndex !== currentSubtitleIndex && nextIndex >= 0) {
      handleSubtitleClick(nextIndex);
    }
  };

  // 跳转到上一个字幕
  const goToPreviousSubtitle = () => {
    const prevIndex = Math.max(currentSubtitleIndex - 1, 0);
    if (prevIndex !== currentSubtitleIndex) {
      handleSubtitleClick(prevIndex);
    }
  };

  // 快进快退
  const seekVideo = (seconds: number) => {
    if (playerRef.current) {
      playerRef.current.currentTime = Math.max(
        0,
        Math.min(duration, playerRef.current.currentTime + seconds)
      );
    }
  };

  // 播放速度控制
  const changePlaybackRate = (delta: number) => {
    const newRate = Math.max(0.25, Math.min(2, playbackRate + delta));
    setPlaybackRate(newRate);
    if (playerRef.current) {
      playerRef.current.playbackRate = newRate;
    }
  };

  const handleRateChange = (e: React.SyntheticEvent<HTMLVideoElement>) => {
    setPlaybackRate(e.currentTarget.playbackRate);
  };

  return {
    currentTime,
    duration,
    setDuration,
    isPlaying,
    setIsPlaying,
    playbackRate,
    playerRef,
    handleTimeUpdate,
    handleLoadedMetadata,
    handleRateChange,
    togglePlay,
    handleSubtitleClick,
    goToNextSubtitle,
    goToPreviousSubtitle,
    seekVideo,
    changePlaybackRate,
    setPlaybackRate: (rate: number) => {
      setPlaybackRate(rate);
      if (playerRef.current) {
        playerRef.current.playbackRate = rate;
      }
    },
  };
};
