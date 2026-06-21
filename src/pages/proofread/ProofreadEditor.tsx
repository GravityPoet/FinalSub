import { useMemo, useState, useCallback } from 'react';
import { ArrowLeft, Check, Save, Loader2, AlertTriangle } from 'lucide-react';

import { useStandaloneSubtitles } from './useStandaloneSubtitles';
import { useVideoPlayer } from './useVideoPlayer';
import VideoPlayer from './subtitle/VideoPlayer';
import CurrentSubtitle from './subtitle/CurrentSubtitle';
import VideoInfo from './subtitle/VideoInfo';
import SubtitleList from './subtitle/SubtitleList';
import SubtitleEditToolbar from './subtitle/SubtitleEditToolbar';
import { PendingFile } from './proofreadUtils';

interface ProofreadEditorProps {
  file: PendingFile;
  onMarkComplete: () => void;
  onBack: () => void;
}

export default function ProofreadEditor({
  file,
  onMarkComplete,
  onBack,
}: ProofreadEditorProps) {

  // 构建配置
  const config = useMemo(
    () => ({
      videoPath: file.videoPath,
      sourceSubtitlePath: file.selectedSource,
      targetSubtitlePath: file.selectedTarget,
      sourceLanguage: file.sourceLanguage,
      targetLanguage: file.targetLanguage,
      finalTargetSubtitlePath: file.selectedTarget, // 兼容用 selectedTarget 代替 finalTargetSubtitlePath
      translateContent: 'onlyTranslate',
    }),
    [file],
  );

  // 使用独立的字幕 hook
  const {
    mergedSubtitles,
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
    // 编辑增强
    handleUndo,
    handleRedo,
    canUndo,
    canRedo,
    handleMergeSubtitles,
    handleSplitSubtitle,
    // 光标位置
    handleCursorPositionChange,
    getCursorPosition,
    isDirty,
    setIsDirty,
  } = useStandaloneSubtitles(config, true);

  // 使用视频播放器 hook
  const {
    currentTime,
    duration,
    isPlaying,
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
    setPlaybackRate,
  } = useVideoPlayer(
    mergedSubtitles,
    currentSubtitleIndex,
    setCurrentSubtitleIndex,
  );

  // 是否有视频
  const hasVideo = !!videoPath;

  // 外部触发器状态
  const [triggerAiOptimize, setTriggerAiOptimize] = useState(false);
  const [triggerSplit, setTriggerSplit] = useState(false);

  // 冲突挽救确认框状态
  const [showUnsavedDialog, setShowUnsavedDialog] = useState(false);

  // 返回处理
  const handleBackClick = useCallback(() => {
    if (isDirty) {
      setShowUnsavedDialog(true);
    } else {
      onBack();
    }
  }, [isDirty, onBack]);

  const handleSaveAndExit = useCallback(async () => {
    setShowUnsavedDialog(false);
    const success = await handleSave();
    if (success) {
      onBack();
    }
  }, [handleSave, onBack]);

  const handleDiscardAndExit = useCallback(() => {
    setShowUnsavedDialog(false);
    setIsDirty(false);
    onBack();
  }, [onBack, setIsDirty]);

  // 处理从字幕列表点击 AI 优化按钮
  const handleAiOptimizeClick = useCallback(
    (index: number) => {
      handleSubtitleClick(index);
      setTimeout(() => {
        setTriggerAiOptimize(true);
      }, 0);
    },
    [handleSubtitleClick],
  );

  // 处理从字幕列表点击拆分按钮
  const handleSplitClick = useCallback(
    (index: number) => {
      handleSubtitleClick(index);
      setTimeout(() => {
        setTriggerSplit(true);
      }, 0);
    },
    [handleSubtitleClick],
  );

  // 重置触发器
  const handleTriggerHandled = useCallback(() => {
    setTriggerAiOptimize(false);
    setTriggerSplit(false);
  }, []);

  if (isLoading) {
    return (
      <div className="h-full flex items-center justify-center bg-slate-900">
        <Loader2 className="w-8 h-8 animate-spin text-blue-500" />
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-slate-900 text-slate-100 overflow-hidden relative">
      {/* 顶部工具栏 */}
      <div className="flex items-center justify-between px-6 py-4 bg-slate-800/60 border-b border-slate-700/50 flex-shrink-0">
        <div className="flex items-center gap-4">
          <button
            onClick={handleBackClick}
            className="flex items-center text-xs text-slate-300 hover:text-white bg-slate-700 hover:bg-slate-650 px-3.5 py-1.5 rounded-lg transition-colors font-medium border border-slate-600/30"
          >
            <ArrowLeft className="w-4 h-4 mr-1.5 text-slate-400" />
            返回列表
          </button>
          <div className="text-sm font-medium text-slate-300 truncate max-w-[320px]" title={file.fileName}>
            {file.fileName}
          </div>
        </div>
        <div className="flex items-center gap-2.5">
          <button
            onClick={handleSave}
            className="flex items-center text-xs bg-slate-700 hover:bg-slate-650 text-slate-200 border border-slate-650 px-4 py-2 rounded-lg transition-colors font-medium"
          >
            <Save className="w-4 h-4 mr-1.5 text-slate-400" />
            保存修改
          </button>
          <button
            onClick={onMarkComplete}
            className="flex items-center text-xs bg-blue-600 hover:bg-blue-700 text-white px-4.5 py-2 rounded-lg transition-colors font-medium shadow-md shadow-blue-500/10"
          >
            <Check className="w-4 h-4 mr-1.5" />
            标记完成并返回
          </button>
        </div>
      </div>

      {/* 编辑工具栏 */}
      <SubtitleEditToolbar
        subtitles={mergedSubtitles}
        onSubtitlesChange={updateSubtitles}
        onUndo={handleUndo}
        onRedo={handleRedo}
        canUndo={canUndo}
        canRedo={canRedo}
        currentSubtitleIndex={currentSubtitleIndex}
        onMergeSubtitles={handleMergeSubtitles}
        onSplitSubtitle={handleSplitSubtitle}
        shouldShowTranslation={shouldShowTranslation}
        getCursorPosition={getCursorPosition}
        triggerAiOptimize={triggerAiOptimize}
        triggerSplit={triggerSplit}
        onTriggerHandled={handleTriggerHandled}
      />

      {/* 主内容区 */}
      <div
        className={`grid gap-4 flex-1 overflow-hidden min-h-0 p-6 ${
          hasVideo ? 'grid-cols-1 lg:grid-cols-2' : 'grid-cols-1'
        }`}
      >
        {/* 左侧：视频播放器和控制区域 */}
        {hasVideo && (
          <div className="flex flex-col min-h-0 overflow-hidden space-y-4">
            {/* 视频播放器组件 */}
            <VideoPlayer
              videoPath={videoPath}
              playerRef={playerRef}
              isPlaying={isPlaying}
              playbackRate={playbackRate}
              togglePlay={togglePlay}
              goToNextSubtitle={goToNextSubtitle}
              goToPreviousSubtitle={goToPreviousSubtitle}
              seekVideo={seekVideo}
              handleTimeUpdate={handleTimeUpdate}
              handleLoadedMetadata={handleLoadedMetadata}
              handleRateChange={handleRateChange}
              changePlaybackRate={changePlaybackRate}
              setPlaybackRate={setPlaybackRate}
              subtitleTracks={subtitleTracksForPlayer}
            />

            {/* 当前字幕预览组件 */}
            <CurrentSubtitle
              currentSubtitleIndex={currentSubtitleIndex}
              currentTime={currentTime}
              duration={duration}
              mergedSubtitles={mergedSubtitles}
              shouldShowTranslation={shouldShowTranslation}
              hasTranslationFile={hasTranslationFile}
            />

            {/* 视频信息和字幕统计组件 */}
            <VideoInfo
              fileName={videoInfo.fileName}
              extension={videoInfo.extension}
              duration={duration}
              subtitleStats={getSubtitleStats()}
              shouldShowTranslation={shouldShowTranslation}
            />
          </div>
        )}

        {/* 右侧/全屏：字幕列表组件 */}
        <div className="min-h-0 overflow-hidden">
          <SubtitleList
            mergedSubtitles={mergedSubtitles}
            currentSubtitleIndex={currentSubtitleIndex}
            shouldShowTranslation={shouldShowTranslation}
            handleSubtitleClick={handleSubtitleClick}
            handleSubtitleChange={handleSubtitleChange}
            isTranslationFailed={isTranslationFailed}
            getFailedTranslationIndices={getFailedTranslationIndices}
            goToNextFailedTranslation={goToNextFailedTranslation}
            goToPreviousFailedTranslation={goToPreviousFailedTranslation}
            onCursorPositionChange={handleCursorPositionChange}
            onAiOptimizeClick={handleAiOptimizeClick}
            onSplitClick={handleSplitClick}
          />
        </div>
      </div>

      {/* 冲突挽救 Dialog */}
      {showUnsavedDialog && (
        <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
          <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-md shadow-2xl overflow-hidden text-slate-100 font-sans p-6 space-y-6">
            <div className="flex items-start gap-4">
              <div className="p-3 bg-amber-500/10 border border-amber-500/20 rounded-xl text-amber-500 shrink-0">
                <AlertTriangle size={24} />
              </div>
              <div className="space-y-1.5">
                <h3 className="text-base font-bold text-slate-100">您有未保存的修改</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  检测到您对字幕进行了修改。如果在没有保存的情况下退出，这些修改将会丢失。请选择您的操作：
                </p>
              </div>
            </div>

            <div className="flex flex-col gap-2.5">
              <button
                onClick={handleSaveAndExit}
                className="w-full text-xs font-semibold bg-blue-600 hover:bg-blue-700 text-white py-2.5 rounded-lg transition-colors shadow-lg shadow-blue-500/10"
              >
                保存并退出
              </button>
              <button
                onClick={handleDiscardAndExit}
                className="w-full text-xs font-semibold bg-slate-850 hover:bg-slate-800 text-slate-200 border border-slate-800 py-2.5 rounded-lg transition-colors"
              >
                放弃修改
              </button>
              <button
                onClick={() => setShowUnsavedDialog(false)}
                className="w-full text-xs font-semibold bg-transparent hover:bg-slate-850 text-slate-400 py-2.5 rounded-lg transition-colors"
              >
                取消
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
