import { useMemo, useState, useCallback } from 'react';
import { ArrowLeft, Check, Save, Loader2, AlertTriangle, Download, Languages } from 'lucide-react';
import { save } from "@tauri-apps/plugin-dialog";
import { useToast } from './Toast';
import { Button } from '../../components/ui/Button';

import { useStandaloneSubtitles } from './useStandaloneSubtitles';
import { useVideoPlayer } from './useVideoPlayer';
import VideoPlayer from './subtitle/VideoPlayer';
import CurrentSubtitle from './subtitle/CurrentSubtitle';
import VideoInfo from './subtitle/VideoInfo';
import SubtitleList from './subtitle/SubtitleList';
import SubtitleEditToolbar from './subtitle/SubtitleEditToolbar';
import { PendingFile } from './proofreadUtils';
import { useI18n } from '../../lib/i18n';
import { convertStringsOpencc } from '../../lib/tauri';

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
  const { t } = useI18n();

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
    handleExport,
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

  const { showToast } = useToast();

  const handleExportClick = async (format: 'srt' | 'vtt' | 'ass' | 'lrc' | 'txt') => {
    try {
      const selected = await save({
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
        defaultPath: `subtitle.${format}`,
      });
      if (!selected) return;

      const path = Array.isArray(selected) ? selected[0] : selected;
      const contentType = shouldShowTranslation ? 'sourceAndTranslate' : 'source';
      const success = await handleExport(path, format, contentType);
      if (success) {
        showToast('success', t('proofread.editor.exportSuccess'));
      } else {
        showToast('error', t('proofread.editor.exportFailed'));
      }
    } catch (err: any) {
      console.error(err);
      showToast('error', t('proofread.editor.exportError', { error: err.toString() }));
    }
  };

  const handleOpenccConvert = async (configKey: string) => {
    try {
      const stringsToConvert: string[] = [];
      
      mergedSubtitles.forEach(sub => {
        sub.content.forEach(line => stringsToConvert.push(line));
        if (sub.sourceContent) stringsToConvert.push(sub.sourceContent);
        if (sub.targetContent) stringsToConvert.push(sub.targetContent);
      });
      
      if (stringsToConvert.length === 0) {
        showToast('info', t('proofread.editor.noConvertibleSubtitles'));
        return;
      }
      
      const convertedStrings = await convertStringsOpencc(stringsToConvert, configKey);
      
      let ptr = 0;
      const newSubtitles = mergedSubtitles.map(sub => {
        const newContent = sub.content.map(() => {
          const val = convertedStrings[ptr];
          ptr += 1;
          return val;
        });
        
        let newSourceContent = sub.sourceContent;
        if (sub.sourceContent) {
          newSourceContent = convertedStrings[ptr];
          ptr += 1;
        }
        
        let newTargetContent = sub.targetContent;
        if (sub.targetContent) {
          newTargetContent = convertedStrings[ptr];
          ptr += 1;
        }
        
        return {
          ...sub,
          content: newContent,
          sourceContent: newSourceContent,
          targetContent: newTargetContent,
        };
      });
      
      updateSubtitles(newSubtitles);
      showToast('success', t('proofread.editor.openccConvertSuccess'));
    } catch (err: any) {
      console.error(err);
      showToast('error', t('proofread.editor.openccConvertFailed', { error: err.toString() }));
    }
  };

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
      <div className="h-full flex items-center justify-center bg-app-bg">
        <Loader2 className="w-8 h-8 animate-spin text-brand" />
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-app-bg text-text-primary overflow-hidden relative">
      {/* 顶部工具栏 */}
      <div className="flex items-center justify-between px-6 py-4 bg-surface/50 border-b border-border-subtle flex-shrink-0 backdrop-blur-md">
        <div className="flex items-center gap-4">
          <Button
            variant="secondary"
            onClick={handleBackClick}
            size="sm"
          >
            <ArrowLeft className="w-4 h-4" />
            {t('proofread.editor.backToList')}
          </Button>
          <div className="text-sm font-medium text-text-secondary truncate max-w-[320px]" title={file.fileName}>
            {file.fileName}
          </div>
        </div>
        <div className="flex items-center gap-2.5">
          <Button
            variant="secondary"
            onClick={handleSave}
            size="sm"
          >
            <Save className="w-4 h-4 text-text-secondary" />
            {t('proofread.editor.saveChanges')}
          </Button>
          
          <div className="relative group">
            <Button
              variant="secondary"
              size="sm"
            >
              <Download className="w-4 h-4 text-text-secondary" />
              {t('proofread.editor.exportSubtitle')}
            </Button>
            <div className="absolute right-0 mt-1.5 hidden group-hover:block bg-surface border border-border-default rounded-xl shadow-lg z-50 w-32 overflow-hidden backdrop-blur-md">
              <button
                type="button"
                onClick={() => handleExportClick('srt')}
                className="w-full text-left px-4 py-2.5 text-xs hover:bg-surface-raised text-text-primary transition-colors cursor-pointer font-medium"
              >
                {t('proofread.editor.formatSrt')}
              </button>
              <button
                type="button"
                onClick={() => handleExportClick('vtt')}
                className="w-full text-left px-4 py-2.5 text-xs hover:bg-surface-raised text-text-primary transition-colors cursor-pointer font-medium"
              >
                {t('proofread.editor.formatVtt')}
              </button>
              <button
                type="button"
                onClick={() => handleExportClick('ass')}
                className="w-full text-left px-4 py-2.5 text-xs hover:bg-surface-raised text-text-primary transition-colors cursor-pointer font-medium"
              >
                {t('proofread.editor.formatAss')}
              </button>
            </div>
          </div>

          <div className="relative group">
            <Button
              variant="secondary"
              size="sm"
            >
              <Languages className="w-4 h-4 text-text-secondary" />
              {t('proofread.editor.openccConvert')}
            </Button>
            <div className="absolute right-0 mt-1.5 hidden group-hover:block bg-surface border border-border-default rounded-xl shadow-lg z-50 w-36 overflow-hidden backdrop-blur-md">
              <button
                type="button"
                onClick={() => handleOpenccConvert('s2t')}
                className="w-full text-left px-4 py-2.5 text-xs hover:bg-surface-raised text-text-primary transition-colors cursor-pointer font-medium"
              >
                {t('proofread.editor.openccS2t')}
              </button>
              <button
                type="button"
                onClick={() => handleOpenccConvert('t2s')}
                className="w-full text-left px-4 py-2.5 text-xs hover:bg-surface-raised text-text-primary transition-colors cursor-pointer font-medium"
              >
                {t('proofread.editor.openccT2s')}
              </button>
              <button
                type="button"
                onClick={() => handleOpenccConvert('s2twp')}
                className="w-full text-left px-4 py-2.5 text-xs hover:bg-surface-raised text-text-primary transition-colors cursor-pointer font-medium"
              >
                {t('proofread.editor.openccS2twp')}
              </button>
              <button
                type="button"
                onClick={() => handleOpenccConvert('s2hk')}
                className="w-full text-left px-4 py-2.5 text-xs hover:bg-surface-raised text-text-primary transition-colors cursor-pointer font-medium"
              >
                {t('proofread.editor.openccS2hk')}
              </button>
            </div>
          </div>

          <Button
            variant="primary"
            onClick={onMarkComplete}
            size="sm"
          >
            <Check className="w-4 h-4" />
            {t('proofread.editor.markComplete')}
          </Button>
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
        <div className="fixed inset-0 z-50 bg-black/40 backdrop-blur-md flex items-center justify-center p-4">
          <div className="bg-surface border border-border-default rounded-xl w-full max-w-md shadow-lg overflow-hidden text-text-primary font-sans p-6 space-y-6">
            <div className="flex items-start gap-4">
              <div className="p-3 bg-warning/10 border border-warning/20 rounded-xl text-warning shrink-0">
                <AlertTriangle size={24} />
              </div>
              <div className="space-y-1.5">
                <h3 className="text-base font-bold text-text-primary">{t('proofread.editor.unsavedTitle')}</h3>
                <p className="text-xs text-text-secondary leading-relaxed">
                  {t('proofread.editor.unsavedDesc')}
                </p>
              </div>
            </div>

            <div className="flex flex-col gap-2.5">
              <Button
                variant="primary"
                onClick={handleSaveAndExit}
                className="w-full"
              >
                {t('proofread.editor.saveExit')}
              </Button>
              <Button
                variant="danger"
                onClick={handleDiscardAndExit}
                className="w-full"
              >
                {t('proofread.editor.discardExit')}
              </Button>
              <Button
                variant="ghost"
                onClick={() => setShowUnsavedDialog(false)}
                className="w-full"
              >
                {t('proofread.editor.cancel')}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
