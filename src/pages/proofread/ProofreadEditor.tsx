import { useMemo, useState, useCallback } from 'react';
import { ArrowLeft, Check, Save, Loader2, AlertTriangle, Download, Languages } from 'lucide-react';
import { save } from "@tauri-apps/plugin-dialog";
import { useToast } from './Toast';

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
        showToast('success', '字幕导出成功');
      } else {
        showToast('error', '字幕导出失败');
      }
    } catch (err) {
      console.error(err);
      showToast('error', `导出失败: ${err}`);
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
        showToast('info', '没有可转换的字幕内容');
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
      showToast('success', '简繁/地域词汇转换成功');
    } catch (err) {
      console.error(err);
      showToast('error', `转换失败: ${err}`);
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
            className="flex items-center text-xs text-slate-300 hover:text-white bg-slate-700 hover:bg-slate-655 px-3.5 py-1.5 rounded-lg transition-colors font-medium border border-slate-600/30"
          >
            <ArrowLeft className="w-4 h-4 mr-1.5 text-slate-400" />
            {t('proofread.editor.backToList')}
          </button>
          <div className="text-sm font-medium text-slate-300 truncate max-w-[320px]" title={file.fileName}>
            {file.fileName}
          </div>
        </div>
        <div className="flex items-center gap-2.5">
          <button
            onClick={handleSave}
            className="flex items-center text-xs bg-slate-700 hover:bg-slate-655 text-slate-200 border border-slate-655 px-4 py-2 rounded-lg transition-colors font-medium"
          >
            <Save className="w-4 h-4 mr-1.5 text-slate-400" />
            {t('proofread.editor.saveChanges')}
          </button>
          
          <div className="relative group">
            <button
              className="flex items-center text-xs bg-slate-700 hover:bg-slate-655 text-slate-200 border border-slate-655 px-4 py-2 rounded-lg transition-colors font-medium"
            >
              <Download className="w-4 h-4 mr-1.5 text-slate-400" />
              导出字幕
            </button>
            <div className="absolute right-0 mt-1 hidden group-hover:block bg-slate-800 border border-slate-700 rounded-lg shadow-xl z-50 w-32 overflow-hidden">
              <button
                type="button"
                onClick={() => handleExportClick('srt')}
                className="w-full text-left px-4 py-2 text-xs hover:bg-slate-700 text-slate-200 transition-colors"
              >
                SRT 格式
              </button>
              <button
                type="button"
                onClick={() => handleExportClick('vtt')}
                className="w-full text-left px-4 py-2 text-xs hover:bg-slate-700 text-slate-200 transition-colors"
              >
                VTT 格式
              </button>
              <button
                type="button"
                onClick={() => handleExportClick('ass')}
                className="w-full text-left px-4 py-2 text-xs hover:bg-slate-700 text-slate-200 transition-colors"
              >
                ASS 格式
              </button>
            </div>
          </div>

          <div className="relative group">
            <button
              className="flex items-center text-xs bg-slate-700 hover:bg-slate-655 text-slate-200 border border-slate-655 px-4 py-2 rounded-lg transition-colors font-medium"
            >
              <Languages className="w-4 h-4 mr-1.5 text-slate-400" />
              简繁转换
            </button>
            <div className="absolute right-0 mt-1 hidden group-hover:block bg-slate-800 border border-slate-700 rounded-lg shadow-xl z-50 w-36 overflow-hidden">
              <button
                type="button"
                onClick={() => handleOpenccConvert('s2t')}
                className="w-full text-left px-4 py-2 text-xs hover:bg-slate-700 text-slate-200 transition-colors"
              >
                简体 → 繁体 (S2T)
              </button>
              <button
                type="button"
                onClick={() => handleOpenccConvert('t2s')}
                className="w-full text-left px-4 py-2 text-xs hover:bg-slate-700 text-slate-200 transition-colors"
              >
                繁体 → 简体 (T2S)
              </button>
              <button
                type="button"
                onClick={() => handleOpenccConvert('s2twp')}
                className="w-full text-left px-4 py-2 text-xs hover:bg-slate-700 text-slate-200 transition-colors"
              >
                简体 → 台湾繁体 (S2TWP)
              </button>
              <button
                type="button"
                onClick={() => handleOpenccConvert('s2hk')}
                className="w-full text-left px-4 py-2 text-xs hover:bg-slate-700 text-slate-200 transition-colors"
              >
                简体 → 香港繁体 (S2HK)
              </button>
            </div>
          </div>

          <button
            onClick={onMarkComplete}
            className="flex items-center text-xs bg-blue-600 hover:bg-blue-700 text-white px-4.5 py-2 rounded-lg transition-colors font-medium shadow-md shadow-blue-500/10"
          >
            <Check className="w-4 h-4 mr-1.5" />
            {t('proofread.editor.markComplete')}
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
                <h3 className="text-base font-bold text-slate-100">{t('proofread.editor.unsavedTitle')}</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  {t('proofread.editor.unsavedDesc')}
                </p>
              </div>
            </div>

            <div className="flex flex-col gap-2.5">
              <button
                onClick={handleSaveAndExit}
                className="w-full text-xs font-semibold bg-blue-600 hover:bg-blue-700 text-white py-2.5 rounded-lg transition-colors shadow-lg shadow-blue-500/10"
              >
                {t('proofread.editor.saveExit')}
              </button>
              <button
                onClick={handleDiscardAndExit}
                className="w-full text-xs font-semibold bg-slate-850 hover:bg-slate-800 text-slate-200 border border-slate-800 py-2.5 rounded-lg transition-colors"
              >
                {t('proofread.editor.discardExit')}
              </button>
              <button
                onClick={() => setShowUnsavedDialog(false)}
                className="w-full text-xs font-semibold bg-transparent hover:bg-slate-850 text-slate-400 py-2.5 rounded-lg transition-colors"
              >
                {t('proofread.editor.cancel')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
