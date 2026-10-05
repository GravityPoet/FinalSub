import { useMemo, useState, useCallback, useRef, useEffect } from 'react';
import { ArrowLeft, Check, Save, Loader2, AlertTriangle, Download, Languages, ChevronDown } from 'lucide-react';
import { useToast } from './Toast';
import { Button } from '../../components/ui/Button';
import { ActionMenu } from '../../components/ui/ActionMenu';

import { useStandaloneSubtitles } from './useStandaloneSubtitles';
import { useVideoPlayer } from './useVideoPlayer';
import VideoPlayer from './subtitle/VideoPlayer';
import CurrentSubtitle from './subtitle/CurrentSubtitle';
import VideoInfo from './subtitle/VideoInfo';
import SubtitleList from './subtitle/SubtitleList';
import SubtitleEditToolbar from './subtitle/SubtitleEditToolbar';
import { PendingFile } from './proofreadUtils';
import { useI18n } from '../../lib/i18n';
import { convertStringsOpencc, saveDialog, publishTaskReview, revealItemInDir } from '../../lib/tauri';
import QualityPanel from './QualityPanel';

interface ProofreadEditorProps {
  file: PendingFile;
  active: boolean;
  onDraftChange: (draft: import('./useStandaloneSubtitles').SubtitleDraft) => void;
  onMarkComplete: () => void;
  onBack: () => void;
}

export default function ProofreadEditor({
  file,
  active,
  onDraftChange,
  onMarkComplete,
  onBack,
}: ProofreadEditorProps) {
  const { t } = useI18n();

  // 构建配置
  const config = useMemo(
    () => ({
      draft: file.draft,
      videoPath: file.videoPath,
      sourceSubtitlePath: file.selectedSource,
      targetSubtitlePath: file.selectedTarget,
      sourceLanguage: file.sourceLanguage,
      targetLanguage: file.targetLanguage,
      finalTargetSubtitlePath: file.selectedTarget, // 兼容用 selectedTarget 代替 finalTargetSubtitlePath
      translateContent: 'onlyTranslate',
      reviewSourceContent: file.reviewSourceContent,
      reviewTargetContent: file.reviewTargetContent,
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
    loadError,
    retryLoad,
    handleSubtitleChange,
    handleSave: saveSubtitleDraft,
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
    saveBackups,
    discardChanges,
    markSaved,
  } = useStandaloneSubtitles(config, true, (draft) => onDraftChange({ ...draft, reviewVersion: reviewVersion.current }));

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
    handlePlaybackState,
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

  useEffect(() => { if (!active) playerRef.current?.pause(); }, [active, playerRef]);
  // 是否有视频
  const hasVideo = !!videoPath;

  // 外部触发器状态
  const [triggerAiOptimize, setTriggerAiOptimize] = useState(false);
  const [triggerSplit, setTriggerSplit] = useState(false);

  // 冲突挽救确认框状态
  const [showUnsavedDialog, setShowUnsavedDialog] = useState(false);
  const [isCompleting, setIsCompleting] = useState(false);

  const { showToast } = useToast();
  const reviewVersion = useRef(file.draft?.reviewVersion ?? file.originTaskVersion);
  const handleSave = async () => {
    if (file.originTaskId) {
      try {
        const cues = mergedSubtitles.map((cue) => ({
          start_ms: Math.round((cue.startTimeInSeconds || 0) * 1000),
          end_ms: Math.round((cue.endTimeInSeconds || 0) * 1000),
          source: cue.sourceContent ?? cue.content.join('\n'),
          target: shouldShowTranslation ? cue.targetContent ?? '' : null,
        }));
        if (!reviewVersion.current) throw new Error(t('quality.changed'));
        reviewVersion.current = await publishTaskReview(file.originTaskId, cues, reviewVersion.current);
        const currentSaved = markSaved();
        showToast('success', t('proofread.standalone.saveSuccess'));
        return currentSaved;
      }
      catch (error) { setIsDirty(true); showToast('error', String(error)); return false; }
    }
    return saveSubtitleDraft();
  };

  const handleExportClick = async (format: 'srt' | 'vtt' | 'ass' | 'lrc' | 'txt') => {
    try {
      const selected = await saveDialog({
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
    discardChanges();
    onBack();
  }, [onBack, discardChanges]);

  const handleMarkCompleteClick = useCallback(async () => {
    setIsCompleting(true);
    try {
      const success = await handleSave();
      if (success) {
        onMarkComplete();
      }
    } finally {
      setIsCompleting(false);
    }
  }, [handleSave, onMarkComplete]);

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

  if (loadError) {
    return (
      <div className="flex h-full items-center justify-center bg-app-bg p-6 text-text-primary">
        <div className="w-full max-w-lg rounded-xl border border-danger/25 bg-danger/10 p-6 text-center">
          <AlertTriangle className="mx-auto mb-3 h-8 w-8 text-danger" />
          <h2 className="mb-2 text-base font-semibold">{t('proofread.standalone.loadFailed')}</h2>
          <p className="mb-5 break-words text-xs leading-5 text-text-secondary">{loadError}</p>
          <div className="flex justify-center gap-2.5">
            <Button variant="secondary" onClick={onBack}>
              {t('proofread.editor.backToList')}
            </Button>
            <Button variant="primary" onClick={retryLoad}>
              {t('proofread.standalone.retry')}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col bg-app-bg text-text-primary overflow-hidden relative">
      {/* 顶部工具栏 */}
      <div className="flex flex-wrap items-center justify-between gap-3 px-3 py-4 sm:px-6 bg-surface/50 border-b border-border-subtle flex-shrink-0 backdrop-blur-md">
        <div className="flex min-w-0 flex-wrap items-center gap-3">
          <Button
            variant="secondary"
            onClick={handleBackClick}
            size="sm"
          >
            <ArrowLeft className="w-4 h-4" />
            {t('proofread.editor.backToList')}
          </Button>
          <div className="text-sm font-medium text-text-secondary truncate max-w-[min(320px,calc(100vw-4rem))]" title={file.fileName}>
            {file.fileName}
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2.5">
          <Button
            variant="secondary"
            onClick={handleSave}
            size="sm"
          >
            <Save className="w-4 h-4 text-text-secondary" />
            {t('proofread.editor.saveChanges')}
          </Button>
          
          <Button variant="secondary" size="sm" onClick={() => void handleExportClick('srt')}><Download size={16} />{t('proofread.editor.exportSubtitle')}</Button>
          <ActionMenu label={t('proofread.editor.exportFormats')} items={['srt', 'vtt', 'ass', 'lrc', 'txt'].map((format) => ({
            label: format.toUpperCase(), action: () => void handleExportClick(format as 'srt' | 'vtt' | 'ass' | 'lrc' | 'txt'),
          }))}><ChevronDown size={16} /></ActionMenu>
          <ActionMenu label={t('proofread.editor.openccConvert')} items={[
            { label: t('proofread.editor.openccS2t'), action: () => void handleOpenccConvert('s2t') },
            { label: t('proofread.editor.openccT2s'), action: () => void handleOpenccConvert('t2s') },
            { label: t('proofread.editor.openccS2twp'), action: () => void handleOpenccConvert('s2twp') },
            { label: t('proofread.editor.openccS2hk'), action: () => void handleOpenccConvert('s2hk') },
          ]}><Languages size={16} />{t('proofread.editor.openccConvert')}</ActionMenu>
          <Button
            variant="primary"
            onClick={handleMarkCompleteClick}
            disabled={isCompleting}
            size="sm"
          >
            {isCompleting ? <Loader2 className="h-4 w-4 animate-spin" /> : <Check className="w-4 h-4" />}
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

      {saveBackups.length > 0 && <div className="flex flex-wrap items-center gap-2 px-3 py-2 text-xs text-text-secondary"><span>{t('proofread.standalone.backupReady')}</span><Button variant="ghost" size="sm" onClick={() => { void revealItemInDir(saveBackups[0]).catch((error) => showToast('error', String(error))); }}>{t('proofread.standalone.openBackup')}</Button></div>}
      {/* 主内容区 */}
      <QualityPanel subtitles={mergedSubtitles} sourceLanguage={file.sourceLanguage || 'auto'} targetLanguage={file.targetLanguage || 'zh'} bilingual={shouldShowTranslation} onChange={updateSubtitles}
        onLocate={(index, play) => { handleSubtitleClick(index); if (play && playerRef.current) { playerRef.current.currentTime = Math.max(0, (mergedSubtitles[index]?.startTimeInSeconds || 0) - 0.5); void playerRef.current.play().catch(() => undefined); } }}
        onSplit={handleSplitClick} onMerge={(index) => handleMergeSubtitles(index, index + 2)} />
      <div
        className={`grid gap-4 flex-1 overflow-hidden min-h-0 p-3 sm:p-6 ${
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
              handlePlaybackState={handlePlaybackState}
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
            isPlaying={hasVideo && isPlaying}
            playbackTime={currentTime}
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
