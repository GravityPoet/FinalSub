import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import {
  AlertCircle,
  CheckCircle,
  ChevronDown,
  ChevronRight,
  Cloud,
  Cpu,
  Download,
  FileText,
  FileVideo,
  FolderOpen,
  FolderTree,
  Film,
  Languages,
  Link2,
  Mic,
  Pencil,
  Play,
  Save,
  ShieldCheck,
  Sparkles,
  Trash2,
  Volume2,
} from "lucide-react";
import { useI18n } from "../lib/i18n";
import {
  createTasks,
  createPreviewTask,
  discoverMixedBatchInputs,
  downloadAndInstallUpdate,
  getFfmpegVersion,
  listAsrModels,
  listTtsModels,
  listTtsProviders,
  listenDragDrop,
  getSettings,
  checkForUpdate,
  getVideoMetadata,
  deleteTaskRecipe,
  listTaskRecipes,
  listSubtitleStylePresets,
  openDialog,
  openPath,
  saveTaskRecipe,
  type AppUpdateEvent,
  type AsrModelInfo,
  type TranslationContentMode,
  type TaskRecipe,
  type TaskRecipeSnapshot,
  type SubtitleStyle,
  type SubtitleStylePreset,
  type TtsModelInfo,
  type TtsProviderProfile,
  type UpdateInfo,
  type VideoMetadata,
} from "../lib/tauri";
import { pairMediaWithSubtitles } from "../lib/filePairing";
import {
  BUILT_IN_SUBTITLE_STYLE_PRESETS,
  DEFAULT_SUBTITLE_STYLE,
  subtitleStylesEqual,
} from "../lib/subtitleStyles";

import { Button } from "../components/ui/Button";
import { Input, Select } from "../components/ui/Input";
import { Card } from "../components/ui/Card";
import { Badge } from "../components/ui/Badge";

const mediaExtensions = [
  "mp4", "mkv", "mov", "avi", "webm", "m4v", "mpeg", "mpg", "ts", "m2ts",
  "mp3", "wav", "m4a", "flac", "aac", "ogg", "opus", "wma",
];

const videoExtensions = [
  "mp4", "mkv", "mov", "avi", "webm", "m4v", "mpeg", "mpg", "ts", "m2ts",
];

const subtitleExtensions = ["srt", "vtt", "ass", "lrc"];

type SourceInputKind = "media" | "subtitle";

const sourceLanguageOptions = [
  { value: "auto", labelKey: "language.auto" },
  { value: "zh", labelKey: "language.zh" },
  { value: "en", labelKey: "language.en" },
  { value: "ja", labelKey: "language.ja" },
  { value: "ko", labelKey: "language.ko" },
  { value: "yue", labelKey: "language.yue" },
] as const;

const taskTypes = [
  { value: "generate-and-translate", labelKey: "home.genTransLabel", descKey: "home.genTransDesc", icon: Mic },
  { value: "generate-only", labelKey: "home.genOnlyLabel", descKey: "home.genOnlyDesc", icon: FileText },
  { value: "translate-only", labelKey: "home.transOnlyLabel", descKey: "home.transOnlyDesc", icon: Languages },
] as const;

const outputFormats = [
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "VTT" },
  { value: "ass", label: "ASS" },
  { value: "lrc", label: "LRC" },
  { value: "txt", label: "TXT" },
];

const targetLanguageOptions = sourceLanguageOptions.filter(({ value }) => value !== "auto");

const engineLabels: Record<string, string> = {
  "whisper-cpp": "Whisper.cpp",
  "parakeet-mlx": "Parakeet MLX V2（Native 兜底）",
  sensevoice: "SenseVoice",
  paraformer: "Paraformer",
  "qwen3-asr": "Qwen3-ASR",
  "firered-asr": "FireRedASR2",
  "cloud-asr": "Cloud ASR",
  "custom-command": "Custom Command",
};

function sourceLanguagesForEngine(engineId: string) {
  switch (engineId) {
    case "parakeet-mlx":
      return sourceLanguageOptions.filter(({ value }) => value === "auto" || value === "en");
    case "paraformer":
      return sourceLanguageOptions.filter(({ value }) => value === "auto" || value === "zh");
    case "firered-asr":
      return sourceLanguageOptions.filter(({ value }) => ["auto", "zh", "en", "yue"].includes(value));
    default:
      return sourceLanguageOptions;
  }
}

const translationContentModes: Array<{
  value: TranslationContentMode;
  labelKey: "home.subtitleContentTargetOnly" | "home.subtitleContentSourceFirst" | "home.subtitleContentTargetFirst";
  descKey: "home.subtitleContentTargetOnlyDesc" | "home.subtitleContentSourceFirstDesc" | "home.subtitleContentTargetFirstDesc";
}> = [
  {
    value: "target-only",
    labelKey: "home.subtitleContentTargetOnly",
    descKey: "home.subtitleContentTargetOnlyDesc",
  },
  {
    value: "source-and-target",
    labelKey: "home.subtitleContentSourceFirst",
    descKey: "home.subtitleContentSourceFirstDesc",
  },
  {
    value: "target-and-source",
    labelKey: "home.subtitleContentTargetFirst",
    descKey: "home.subtitleContentTargetFirstDesc",
  },
];

function fileNameFromPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function fileExtensionFromPath(path: string): string {
  const fileName = fileNameFromPath(path);
  const dotIndex = fileName.lastIndexOf(".");
  return dotIndex >= 0 ? fileName.slice(dotIndex + 1).toLowerCase() : "";
}

function isSubtitleInputPath(path: string): boolean {
  return subtitleExtensions.includes(fileExtensionFromPath(path));
}

function sourceInputKindForPath(path: string): SourceInputKind {
  return isSubtitleInputPath(path) ? "subtitle" : "media";
}

function sourceInputKindForTaskType(taskType: string): SourceInputKind {
  return taskType === "translate-only" ? "subtitle" : "media";
}

export default function HomePage() {
  const navigate = useNavigate();
  const [ffmpegVersion, setFfmpegVersion] = useState<string>("detecting");
  const [models, setModels] = useState<AsrModelInfo[]>([]);
  const [ttsModels, setTtsModels] = useState<TtsModelInfo[]>([]);
  const [ttsProviders, setTtsProviders] = useState<TtsProviderProfile[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [pairedSubtitlePaths, setPairedSubtitlePaths] = useState<string[]>([]);
  const [manualSubtitlePairs, setManualSubtitlePairs] = useState<Record<string, string>>({});
  // The imported source is an asset, while taskType is only a processing
  // recipe. Keep the asset kind separately so changing the recipe cannot
  // accidentally replace/clear the uploaded path.
  const [selectedInputKind, setSelectedInputKind] = useState<SourceInputKind | null>(null);
  const selectionRequestRef = useRef(0);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string>("");
  const [bootstrapState, setBootstrapState] = useState<"loading" | "ready" | "error">("loading");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateProgress, setUpdateProgress] = useState<AppUpdateEvent | null>(null);
  const [updateError, setUpdateError] = useState("");
  const [mediaMetadata, setMediaMetadata] = useState<VideoMetadata | null>(null);
  const [mediaMetadataError, setMediaMetadataError] = useState("");

  const [taskType, setTaskType] = useState("generate-and-translate");
  const [engineId, setEngineId] = useState("parakeet-mlx");
  const [modelId, setModelId] = useState("parakeet-tdt-0.6b-v2");
  const [sourceLanguage, setSourceLanguage] = useState("auto");
  const [targetLanguage, setTargetLanguage] = useState("zh");
  const [translationContentMode, setTranslationContentMode] =
    useState<TranslationContentMode>("target-only");
  const [outputFormat, setOutputFormat] = useState("srt");
  const [outputName, setOutputName] = useState("");
  const [maxSubtitleChars, setMaxSubtitleChars] = useState(0);
  const [customSubtitleChars, setCustomSubtitleChars] = useState(40);
  const [customSubtitleDraft, setCustomSubtitleDraft] = useState("40");
  const [stripChinesePunctuation, setStripChinesePunctuation] = useState(false);
  const [reviewRequired, setReviewRequired] = useState(false);
  const [enableDubbing, setEnableDubbing] = useState(false);
  const [enableCompose, setEnableCompose] = useState(false);
  const [dubbingReview, setDubbingReview] = useState(false);
  const [dubbingEngine, setDubbingEngine] = useState<"local" | "cloud">("local");
  const [dubbingTargetId, setDubbingTargetId] = useState("");
  const [dubbingVoice, setDubbingVoice] = useState("");
  const [dubbingSpeed, setDubbingSpeed] = useState(1);
  const [dubbingConcurrency, setDubbingConcurrency] = useState(1);
  const [composeSoftSubtitle, setComposeSoftSubtitle] = useState(false);
  const [composeAudioMode, setComposeAudioMode] = useState<"replace" | "mix" | "add-track">("replace");
  const [composeEncoderMode, setComposeEncoderMode] = useState<"auto" | "cpu" | "hardware">("auto");
  const [composeStyle, setComposeStyle] = useState<SubtitleStyle>({ ...DEFAULT_SUBTITLE_STYLE });
  const [subtitleStylePresets, setSubtitleStylePresets] = useState<SubtitleStylePreset[]>([]);
  const [dragActive, setDragActive] = useState(false);
  const [recipes, setRecipes] = useState<TaskRecipe[]>([]);
  const [recipeNotice, setRecipeNotice] = useState("");
  const [recipeBusy, setRecipeBusy] = useState(false);
  const [recipeName, setRecipeName] = useState("");
  const [recipeDialog, setRecipeDialog] = useState<{
    mode: "create" | "rename" | "delete";
    recipe?: TaskRecipe;
  } | null>(null);

  const { t } = useI18n();
  const selectedPath = selectedPaths[0] ?? "";

  const commitSelectedPaths = useCallback((paths: string[], subtitles: string[] = []) => {
    if (paths.length === 0) return;
    const nextKind = sourceInputKindForPath(paths[0]);
    setSelectedPaths(paths);
    setSelectedInputKind(nextKind);
    setTaskType((current) => (
      nextKind === "subtitle"
        ? "translate-only"
        : (current === "translate-only" ? "generate-and-translate" : current)
    ));
    if (nextKind === "subtitle") {
      setEnableDubbing(false);
      setEnableCompose(false);
      setDubbingReview(false);
    }
    setPairedSubtitlePaths(nextKind === "media" ? subtitles : []);
    setManualSubtitlePairs({});
  }, []);

  const clearSelectedPaths = useCallback(() => {
    selectionRequestRef.current += 1;
    setSelectedPaths([]);
    setSelectedInputKind(null);
    setPairedSubtitlePaths([]);
    setManualSubtitlePairs({});
  }, []);

  const discoverAndCommit = useCallback(async (paths: string[], kind: SourceInputKind) => {
    const requestId = ++selectionRequestRef.current;
    const discovered = await discoverMixedBatchInputs(paths, true);
    // A stale drag/paste/dialog response must never overwrite a newer source.
    if (requestId !== selectionRequestRef.current) return;
    if (kind === "media" && discovered.media.length > 0) {
      commitSelectedPaths(discovered.media, discovered.subtitles);
      return;
    }
    if (
      kind === "media"
      && discovered.subtitles.length > 0
      && selectedInputKind === "media"
      && selectedPaths.length > 0
    ) {
      setPairedSubtitlePaths((current) => [...new Set([...current, ...discovered.subtitles])]);
      return;
    }
    const fallback = kind === "subtitle"
      ? (discovered.subtitles.length > 0 ? discovered.subtitles : discovered.media)
      : (discovered.media.length > 0 ? discovered.media : discovered.subtitles);
    commitSelectedPaths(fallback);
  }, [commitSelectedPaths, selectedInputKind, selectedPaths.length]);

  const handleTaskTypeChange = useCallback((nextTaskType: string) => {
    // Deliberately do not touch selectedPaths here. The same uploaded asset
    // can be inspected against another workflow and remains available when
    // the user switches back.
    setTaskType(nextTaskType);
    if (nextTaskType === "translate-only") {
      setEnableDubbing(false);
      setEnableCompose(false);
      setDubbingReview(false);
    }
    setError("");
  }, []);

  const loadWorkspace = useCallback(async () => {
    setBootstrapState("loading");
    const settingsPromise = getSettings();
    void settingsPromise
      .then((settings) => {
        if (!settings.check_update_on_startup) return;
        return checkForUpdate().then((update) => {
          if (update) setUpdateInfo(update);
        });
      })
      .catch((updateError) => {
        console.error("Failed to check for updates:", updateError);
      });
    try {
      const [loadedModels, loadedTtsModels, loadedTtsProviders, settings, loadedRecipes, loadedStylePresets] = await Promise.all([
        listAsrModels(),
        listTtsModels(),
        listTtsProviders(),
        settingsPromise,
        listTaskRecipes().catch((recipeError) => {
          console.error("Failed to load task recipes:", recipeError);
          return [];
        }),
        listSubtitleStylePresets().catch((presetError) => {
          console.error("Failed to load subtitle style presets:", presetError);
          return [];
        }),
      ]);
      if (loadedModels.length === 0) {
        throw new Error("No ASR engines are available");
      }

      const configuredEngineExists = loadedModels.some((model) => model.engine_id === settings.asr_engine);
      const parakeetExists = loadedModels.some((model) => model.engine_id === "parakeet-mlx");
      const nextEngineId = configuredEngineExists
        ? settings.asr_engine
        : (parakeetExists ? "parakeet-mlx" : loadedModels[0].engine_id);
      const preferredModelId = nextEngineId === "parakeet-mlx" ? "parakeet-tdt-0.6b-v2" : "";
      const nextModel = loadedModels.find(
        (model) => model.engine_id === nextEngineId && model.id === preferredModelId,
      ) ?? loadedModels.find((model) => model.engine_id === nextEngineId);

      setModels(loadedModels);
      setTtsModels(loadedTtsModels);
      setTtsProviders(loadedTtsProviders);
      setRecipes(loadedRecipes);
      setSubtitleStylePresets(loadedStylePresets);
      setEngineId(nextEngineId);
      setModelId(nextModel?.id ?? "");
      setTargetLanguage(
        targetLanguageOptions.some(({ value }) => value === settings.target_language)
          ? settings.target_language
          : "zh",
      );
      setSourceLanguage(
        sourceLanguageOptions.some(({ value }) => value === settings.source_language)
          ? settings.source_language
          : "auto",
      );
      setOutputFormat(
        outputFormats.some(({ value }) => value === settings.subtitle_output_format)
          ? settings.subtitle_output_format
          : "srt",
      );
      const readyLocalTts = loadedTtsModels.find(
        (model) => model.status === "ready" && !model.clone_only,
      );
      const readyCloudTts = loadedTtsProviders.find((provider) => provider.text_upload_consent);
      if (readyLocalTts) {
        setDubbingEngine("local");
        setDubbingTargetId(readyLocalTts.id);
        setDubbingVoice(readyLocalTts.default_voice_id);
      } else if (readyCloudTts) {
        setDubbingEngine("cloud");
        setDubbingTargetId(readyCloudTts.id);
        setDubbingVoice(readyCloudTts.voice);
      }
      setBootstrapState("ready");
    } catch (workspaceError) {
      console.error("Failed to initialize workspace:", workspaceError);
      setBootstrapState("error");
    }
  }, []);

  useEffect(() => {
    getFfmpegVersion().then(setFfmpegVersion).catch(() => setFfmpegVersion("unavailable"));
    void loadWorkspace();
  }, [loadWorkspace]);

  useEffect(() => {
    if (selectedPath) {
      if (taskType !== "translate-only") {
        setMediaMetadataError("");
        getVideoMetadata(selectedPath)
          .then((meta) => {
            setMediaMetadata(meta);
            setMediaMetadataError("");
          })
          .catch((err) => {
            console.error("加载媒体元数据失败:", err);
            setMediaMetadata(null);
            setMediaMetadataError(String(err));
          });
      } else {
        setMediaMetadata(null);
        setMediaMetadataError("");
      }
    } else {
      setMediaMetadata(null);
      setMediaMetadataError("");
    }
  }, [selectedPath, taskType]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    void listenDragDrop((event) => {
      if (event.type === "enter" || event.type === "over") {
        setDragActive(true);
      } else if (event.type === "leave") {
        setDragActive(false);
      } else if (event.type === "drop") {
        setDragActive(false);
        void discoverAndCommit(event.paths, sourceInputKindForTaskType(taskType))
          .catch((dropError) => setError(String(dropError)));
      }
    }).then((unlisten) => { stop = unlisten; });

    const onPaste = (event: ClipboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, [contenteditable='true']")) return;
      const paths = event.clipboardData?.getData("text/plain")
        .split(/\r?\n/)
        .map((value) => value.trim())
        .filter((value) => value.startsWith("/") || /^[A-Za-z]:[\\/]/.test(value));
      if (!paths?.length) return;
      event.preventDefault();
      void discoverAndCommit(paths, sourceInputKindForTaskType(taskType))
        .catch((pasteError) => setError(String(pasteError)));
    };
    window.addEventListener("paste", onPaste);
    return () => {
      stop?.();
      window.removeEventListener("paste", onPaste);
    };
  }, [discoverAndCommit, taskType]);

  const engineModels = models.filter((m) => m.engine_id === engineId);
  const engines = [...new Set(models.map((m) => m.engine_id))];
  const readyTtsModels = ttsModels.filter((model) => model.status === "ready" && !model.clone_only);
  const selectedTtsModel = ttsModels.find((model) => model.id === dubbingTargetId);
  const selectedTtsProvider = ttsProviders.find((provider) => provider.id === dubbingTargetId);
  const matchingBuiltInStyle = BUILT_IN_SUBTITLE_STYLE_PRESETS.find((preset) => subtitleStylesEqual(preset.style, composeStyle));
  const matchingUserStyle = subtitleStylePresets.find((preset) => subtitleStylesEqual(preset.style, composeStyle));
  const composeStylePresetValue = matchingBuiltInStyle?.id ?? matchingUserStyle?.id ?? "snapshot";

  const handleComposeStyleChange = (presetId: string) => {
    const builtIn = BUILT_IN_SUBTITLE_STYLE_PRESETS.find((preset) => preset.id === presetId);
    const personal = subtitleStylePresets.find((preset) => preset.id === presetId);
    const style = builtIn?.style ?? personal?.style;
    if (style) setComposeStyle({ ...style });
  };
  const dubbingReady = !enableDubbing || (
    dubbingEngine === "local"
      ? Boolean(selectedTtsModel?.status === "ready" && !selectedTtsModel.clone_only)
      : Boolean(selectedTtsProvider?.text_upload_consent)
  );
  const subtitlePairing = pairMediaWithSubtitles(
    selectedInputKind === "media" ? selectedPaths : [],
    pairedSubtitlePaths,
    manualSubtitlePairs,
  );
  const pairedSubtitleCount = subtitlePairing.pairedByMedia.size;
  const asrFallbackCount = subtitlePairing.unpairedMedia.length;
  const allSelectedMediaPaired = selectedInputKind === "media"
    && selectedPaths.length > 0
    && asrFallbackCount === 0;
  const visiblePairingMedia = selectedPaths.slice(0, 100);
  const visiblePairingSubtitles = pairedSubtitlePaths.slice(0, 200);
  const selectedSourcesAreVideo = selectedPaths.every((path) => videoExtensions.includes(fileExtensionFromPath(path)));
  const composeSourceReady = !enableCompose
    || !selectedPath
    || (selectedSourcesAreVideo && (selectedPaths.length > 1 || Boolean(mediaMetadata?.width)));
  const timedSubtitleReady = (!enableDubbing && !enableCompose) || outputFormat !== "txt";
  const downstreamInputReady = (
    (!enableDubbing && !enableCompose) || taskType !== "translate-only"
  ) && composeSourceReady;
  const pipelineReady = dubbingReady && downstreamInputReady && timedSubtitleReady;

  const taskNeedsAsr = taskType !== "translate-only"
    && (!allSelectedMediaPaired || selectedPaths.length === 0);
  const availableSourceLanguages = taskNeedsAsr
    ? sourceLanguagesForEngine(engineId)
    : sourceLanguageOptions;
  const sourceLanguageSupported = availableSourceLanguages.some(
    ({ value }) => value === sourceLanguage,
  );
  const activeModel = models.find((m) => m.id === modelId && m.engine_id === engineId);
  const inputTypeMismatch = selectedPaths.length > 0
    && selectedInputKind !== null
    && selectedInputKind !== sourceInputKindForTaskType(taskType);
  const inputTypeMismatchHint = inputTypeMismatch
    ? (taskType === "translate-only" ? t("home.inputMismatchSubtitle") : t("home.inputMismatchMedia"))
    : "";
  const modelReady = !taskNeedsAsr || Boolean(
    activeModel && (engineId === "custom-command" || activeModel.status === "downloaded")
  );
  const canStartTask = bootstrapState === "ready"
    && (!taskNeedsAsr || sourceLanguageSupported)
    && modelReady
    && pipelineReady
    && !inputTypeMismatch;

  const deliveryTargets = [
    t("home.targetSubtitle"),
    ...(enableDubbing ? [t("home.targetDubbing")] : []),
    ...(enableCompose ? [t("home.targetVideo")] : []),
  ].join(" · ");

  useEffect(() => {
    if (!sourceLanguageSupported) {
      setSourceLanguage("auto");
    }
  }, [sourceLanguageSupported]);

  const selectedFileKind = (selectedInputKind ?? sourceInputKindForTaskType(taskType)) === "subtitle"
    ? t("home.subFile")
    : t("home.mediaFile");
  const displayedInputKind = selectedInputKind ?? sourceInputKindForTaskType(taskType);
    
  const missingFileHint = !selectedPath
    ? (taskType === "translate-only" ? t("home.prereqSub") : t("home.prereqMedia"))
    : "";
  const modelPrerequisiteHint = selectedPath && !modelReady ? t("home.prereqModel") : "";
  const pipelinePrerequisiteHint = !composeSourceReady
    // 元数据探测失败时给出真实原因，而不是误导性的“需要有画面的源”。
    ? (mediaMetadataError
      ? t("home.metadataError", { error: mediaMetadataError })
      : t("home.pipelineNeedsVideo"))
    : (!downstreamInputReady
      ? t("home.pipelineNeedsMedia")
      : (enableDubbing && !dubbingReady
        ? t("home.pipelineNeedsTts")
        : (!timedSubtitleReady ? t("home.pipelineNeedsTimedSubtitle") : "")));

  const handleSelectMedia = async () => {
    setError("");
    try {
      const selectionKind = sourceInputKindForTaskType(taskType);
      const selected = await openDialog({
        multiple: true,
        filters: selectionKind === "subtitle"
          ? [{ name: t("home.subFile"), extensions: ["srt", "vtt", "ass", "lrc"] }]
          : [{ name: t("home.mediaFile"), extensions: mediaExtensions }],
      });
      const paths = typeof selected === "string" ? [selected] : selected;
      if (paths?.length) {
        await discoverAndCommit(paths, selectionKind);
      }
    } catch (dialogError) {
      console.error("Failed to open file picker:", dialogError);
      setError(t("home.selectFileFailed"));
    }
  };

  const handleSelectFolder = async () => {
    setError("");
    try {
      const selectionKind = sourceInputKindForTaskType(taskType);
      const selected = await openDialog({ directory: true, multiple: true });
      const paths = typeof selected === "string" ? [selected] : selected;
      if (paths?.length) {
        await discoverAndCommit(paths, selectionKind);
      }
    } catch (dialogError) {
      console.error("Failed to scan selected folder:", dialogError);
      setError(t("home.selectFileFailed"));
    }
  };

  const handleSelectPairedSubtitles = async () => {
    setError("");
    try {
      const selected = await openDialog({
        multiple: true,
        filters: [{ name: t("home.subFile"), extensions: subtitleExtensions }],
      });
      const paths = (typeof selected === "string" ? [selected] : selected)
        ?.filter(isSubtitleInputPath);
      if (paths?.length) {
        setPairedSubtitlePaths((current) => [...new Set([...current, ...paths])]);
      }
    } catch (dialogError) {
      console.error("Failed to select paired subtitles:", dialogError);
      setError(t("home.selectFileFailed"));
    }
  };

  const handlePairChoice = (mediaPath: string, choice: string) => {
    setManualSubtitlePairs((current) => {
      const next = { ...current };
      if (choice === "__auto__") {
        delete next[mediaPath];
        return next;
      }
      const subtitlePath = choice === "__asr__" ? "" : choice;
      for (const [otherMediaPath, assignedSubtitlePath] of Object.entries(next)) {
        if (otherMediaPath !== mediaPath && assignedSubtitlePath === subtitlePath && subtitlePath) {
          delete next[otherMediaPath];
        }
      }
      next[mediaPath] = subtitlePath;
      return next;
    });
  };

  const removePairedSubtitle = (subtitlePath: string) => {
    setPairedSubtitlePaths((current) => current.filter((path) => path !== subtitlePath));
    setManualSubtitlePairs((current) => Object.fromEntries(
      Object.entries(current).filter(([, assignedPath]) => assignedPath !== subtitlePath),
    ));
  };

  const clearSubtitlePairing = () => {
    setPairedSubtitlePaths([]);
    setManualSubtitlePairs({});
  };

  const revealTaskIssue = (targetId: string) => {
    requestAnimationFrame(() => {
      const target = document.getElementById(targetId);
      if (!target) return;
      const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      target.scrollIntoView({ behavior: reduceMotion ? "auto" : "smooth", block: "center" });
      target.focus({ preventScroll: true });
    });
  };

  const handleCreate = async () => {
    if (!selectedPath) {
      setError(missingFileHint || (taskType === "translate-only" ? t("home.prereqSub") : t("home.prereqMedia")));
      revealTaskIssue("task-source-input");
      return;
    }
    if (inputTypeMismatch) {
      setError(inputTypeMismatchHint);
      revealTaskIssue("task-source-input");
      return;
    }
    if (!canStartTask) {
      setError(modelPrerequisiteHint || pipelinePrerequisiteHint || t("home.prereqModel"));
      revealTaskIssue(modelPrerequisiteHint ? "task-recognition-core" : "task-delivery-options");
      return;
    }
    setCreating(true);
    setError("");
    try {
      const requests = selectedPaths.map((mediaPath, index) => {
        const resolvedOutputName = outputName.trim()
          ? outputName.trim().split("{index}").join(String(index + 1).padStart(2, "0"))
          : undefined;
        const pipelineEnabled = reviewRequired || enableDubbing || enableCompose || dubbingReview;
        return {
          task_type: taskType,
          media_path: mediaPath,
          provided_subtitle_path: taskType === "translate-only"
            ? undefined
            : subtitlePairing.pairedByMedia.get(mediaPath),
          engine_id: engineId,
          model_id: modelId,
          source_language: sourceLanguage,
          target_language: taskType === "generate-only" ? undefined : targetLanguage,
          translation_content_mode:
            taskType === "generate-only" ? undefined : translationContentMode,
          output_format: outputFormat,
          output_name: resolvedOutputName,
          strip_chinese_punctuation: stripChinesePunctuation,
          review_required: pipelineEnabled ? false : reviewRequired,
          max_subtitle_chars: maxSubtitleChars,
          pipeline: pipelineEnabled ? {
            enable_dubbing: enableDubbing,
            enable_compose: enableCompose,
            subtitle_review: reviewRequired,
            dubbing_review: enableDubbing && dubbingReview,
            dubbing: enableDubbing ? {
              engine: dubbingEngine,
              model_or_provider_id: dubbingTargetId,
              voice: dubbingVoice,
              global_speed: dubbingSpeed,
              local_concurrency: dubbingConcurrency,
            } : undefined,
            compose: enableCompose ? {
              soft_subtitle: composeSoftSubtitle,
              audio_mode: enableDubbing ? composeAudioMode : "keep" as const,
              encoder_mode: composeEncoderMode,
              style: composeSoftSubtitle ? undefined : composeStyle,
            } : undefined,
          } : undefined,
        };
      });
      await createTasks(requests);
      navigate("/tasks");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const handlePreview = async () => {
    if (taskType === "translate-only") {
      setError(t("home.previewOnlyMedia"));
      return;
    }
    if (inputTypeMismatch) {
      setError(inputTypeMismatchHint);
      return;
    }
    if (!selectedPath) {
      setError(t("home.selectMediaPrereq"));
      return;
    }
    if (selectedPaths.length !== 1) {
      setError(t("home.previewSingleOnly"));
      return;
    }
    setCreating(true);
    setError("");
    try {
      await createPreviewTask(selectedPath);
      navigate("/tasks");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const handleUpdate = async () => {
    if (!updateInfo) return;
    setUpdateError("");
    if (!updateInfo.install_supported) {
      try {
        await openPath(updateInfo.url);
      } catch (openError) {
        setUpdateError(openError instanceof Error ? openError.message : String(openError));
      }
      return;
    }

    setUpdateProgress({ phase: "downloading", downloaded_bytes: 0, total_bytes: null });
    try {
      await downloadAndInstallUpdate(updateInfo.latest_version, setUpdateProgress);
    } catch (installError) {
      setUpdateProgress(null);
      setUpdateError(installError instanceof Error ? installError.message : String(installError));
    }
  };

  const recipeLocalTts = readyTtsModels[0]
    ?? ttsModels.find((model) => !model.clone_only);
  const recipeCloudTts = ttsProviders.find((provider) => provider.text_upload_consent)
    ?? ttsProviders[0];
  const recipeDubbingEngine: "local" | "cloud" = recipeLocalTts ? "local" : "cloud";
  const recipeDubbingTarget = recipeLocalTts?.id ?? recipeCloudTts?.id ?? "tts-not-configured";
  const recipeDubbingVoice = recipeLocalTts?.default_voice_id ?? recipeCloudTts?.voice ?? "";

  const builtInRecipes: Array<{
    id: string;
    name: string;
    description: string;
    snapshot: TaskRecipeSnapshot;
  }> = [
    {
      id: "video-dub-final",
      name: t("home.recipeDubVideoName"),
      description: t("home.recipeDubVideoDesc"),
      snapshot: {
        task_type: "generate-and-translate",
        engine_id: "parakeet-mlx",
        model_id: "parakeet-tdt-0.6b-v2",
        source_language: "auto",
        target_language: "zh",
        translation_content_mode: "target-only",
        output_format: "srt",
        output_name: "",
        strip_chinese_punctuation: false,
        review_required: true,
        max_subtitle_chars: 0,
        pipeline: {
          enable_dubbing: true,
          enable_compose: true,
          subtitle_review: true,
          dubbing_review: false,
          dubbing_engine: recipeDubbingEngine,
          dubbing_model_or_provider_id: recipeDubbingTarget,
          dubbing_voice: recipeDubbingVoice,
          dubbing_speed: 1,
          local_concurrency: 1,
          compose_soft_subtitle: false,
          compose_audio_mode: "replace",
          compose_encoder_mode: "auto",
          compose_style: { ...DEFAULT_SUBTITLE_STYLE },
        },
      },
    },
    {
      id: "offline-fast",
      name: t("home.recipeOfflineName"),
      description: t("home.recipeOfflineDesc"),
      snapshot: {
        task_type: "generate-only",
        engine_id: "parakeet-mlx",
        model_id: "parakeet-tdt-0.6b-v2",
        source_language: "auto",
        target_language: "zh",
        translation_content_mode: "target-only",
        output_format: "srt",
        output_name: "",
        strip_chinese_punctuation: false,
        review_required: false,
        max_subtitle_chars: 0,
      },
    },
    {
      id: "bilingual-review",
      name: t("home.recipeBilingualName"),
      description: t("home.recipeBilingualDesc"),
      snapshot: {
        task_type: "generate-and-translate",
        engine_id: "parakeet-mlx",
        model_id: "parakeet-tdt-0.6b-v2",
        source_language: "auto",
        target_language: "zh",
        translation_content_mode: "source-and-target",
        output_format: "srt",
        output_name: "",
        strip_chinese_punctuation: false,
        review_required: true,
        max_subtitle_chars: 0,
      },
    },
    {
      id: "translate-review",
      name: t("home.recipeTranslateName"),
      description: t("home.recipeTranslateDesc"),
      snapshot: {
        task_type: "translate-only",
        engine_id: "subtitle-translation",
        model_id: "srt-input",
        source_language: "auto",
        target_language: "zh",
        translation_content_mode: "target-only",
        output_format: "srt",
        output_name: "",
        strip_chinese_punctuation: false,
        review_required: true,
        max_subtitle_chars: 0,
      },
    },
  ];

  const currentRecipeSnapshot = (): TaskRecipeSnapshot => ({
    task_type: taskType,
    engine_id: engineId,
    model_id: modelId,
    source_language: sourceLanguage,
    target_language: targetLanguage,
    translation_content_mode: translationContentMode,
    output_format: outputFormat,
    output_name: outputName,
    strip_chinese_punctuation: stripChinesePunctuation,
    review_required: reviewRequired,
    max_subtitle_chars: maxSubtitleChars,
    pipeline: (reviewRequired || enableDubbing || enableCompose || dubbingReview) ? {
      enable_dubbing: enableDubbing,
      enable_compose: enableCompose,
      subtitle_review: reviewRequired,
      dubbing_review: enableDubbing && dubbingReview,
      dubbing_engine: dubbingEngine,
      dubbing_model_or_provider_id: dubbingTargetId,
      dubbing_voice: dubbingVoice,
      dubbing_speed: dubbingSpeed,
      local_concurrency: dubbingConcurrency,
      compose_soft_subtitle: composeSoftSubtitle,
      compose_audio_mode: enableDubbing ? composeAudioMode : "keep",
      compose_encoder_mode: composeEncoderMode,
      compose_style: enableCompose && !composeSoftSubtitle ? composeStyle : undefined,
    } : undefined,
  });

  const applyRecipe = (snapshot: TaskRecipeSnapshot, name: string) => {
    const nextTaskType = ["generate-only", "generate-and-translate", "translate-only"].includes(
      snapshot.task_type,
    ) ? snapshot.task_type : "generate-and-translate";
    // A recipe changes processing parameters, not the imported source. Keep
    // the current selection and let the input guard explain any type mismatch.
    handleTaskTypeChange(nextTaskType);

    let usedFallback = false;
    if (nextTaskType !== "translate-only") {
      const isUsable = (model: AsrModelInfo) =>
        model.engine_id === "custom-command" || model.status === "downloaded";
      const exactModel = models.find(
        (model) => model.engine_id === snapshot.engine_id
          && model.id === snapshot.model_id
          && isUsable(model),
      );
      const fallbackModel = exactModel
        ?? models.find((model) => model.engine_id === snapshot.engine_id && isUsable(model))
        ?? models.find(isUsable)
        ?? models.find(
          (model) => model.engine_id === snapshot.engine_id && model.id === snapshot.model_id,
        )
        ?? models[0];
      if (fallbackModel) {
        setEngineId(fallbackModel.engine_id);
        setModelId(fallbackModel.id);
        usedFallback = !exactModel;
        const supportedLanguages = sourceLanguagesForEngine(fallbackModel.engine_id);
        setSourceLanguage(
          supportedLanguages.some(({ value }) => value === snapshot.source_language)
            ? snapshot.source_language
            : "auto",
        );
      }
    } else {
      setSourceLanguage(
        sourceLanguageOptions.some(({ value }) => value === snapshot.source_language)
          ? snapshot.source_language
          : "auto",
      );
    }

    setTargetLanguage(
      targetLanguageOptions.some(({ value }) => value === snapshot.target_language)
        ? snapshot.target_language
        : "zh",
    );
    setTranslationContentMode(snapshot.translation_content_mode);
    setOutputFormat(
      outputFormats.some(({ value }) => value === snapshot.output_format)
        ? snapshot.output_format
        : "srt",
    );
    setOutputName(snapshot.output_name);
    setStripChinesePunctuation(snapshot.strip_chinese_punctuation);
    const recipePipeline = snapshot.pipeline;
    const recipeAllowsMediaPipeline = nextTaskType !== "translate-only";
    setEnableDubbing(recipeAllowsMediaPipeline && Boolean(recipePipeline?.enable_dubbing));
    setEnableCompose(recipeAllowsMediaPipeline && Boolean(recipePipeline?.enable_compose));
    setDubbingReview(recipeAllowsMediaPipeline && Boolean(recipePipeline?.enable_dubbing && recipePipeline.dubbing_review));
    setReviewRequired(recipePipeline?.subtitle_review ?? snapshot.review_required);
    if (recipePipeline) {
      const exactLocal = ttsModels.find(
        (model) => model.id === recipePipeline.dubbing_model_or_provider_id
          && model.status === "ready"
          && !model.clone_only,
      );
      const exactCloud = ttsProviders.find(
        (provider) => provider.id === recipePipeline.dubbing_model_or_provider_id
          && provider.text_upload_consent,
      );
      const fallbackLocal = readyTtsModels[0];
      const fallbackCloud = ttsProviders.find((provider) => provider.text_upload_consent);
      if (recipePipeline.enable_dubbing) {
        if (recipePipeline.dubbing_engine === "local" && (exactLocal || fallbackLocal)) {
          const selected = exactLocal ?? fallbackLocal!;
          setDubbingEngine("local");
          setDubbingTargetId(selected.id);
          setDubbingVoice(exactLocal ? recipePipeline.dubbing_voice : selected.default_voice_id);
          usedFallback ||= !exactLocal;
        } else if (exactCloud || fallbackCloud) {
          const selected = exactCloud ?? fallbackCloud!;
          setDubbingEngine("cloud");
          setDubbingTargetId(selected.id);
          setDubbingVoice(exactCloud ? recipePipeline.dubbing_voice : selected.voice);
          usedFallback ||= !exactCloud || recipePipeline.dubbing_engine !== "cloud";
        } else {
          setDubbingEngine(recipePipeline.dubbing_engine);
          setDubbingTargetId(recipePipeline.dubbing_model_or_provider_id);
          setDubbingVoice(recipePipeline.dubbing_voice);
        }
      }
      setDubbingSpeed(recipePipeline.dubbing_speed);
      setDubbingConcurrency(recipePipeline.local_concurrency ?? 1);
      setComposeSoftSubtitle(recipePipeline.compose_soft_subtitle);
      if (recipePipeline.compose_audio_mode !== "keep") {
        setComposeAudioMode(recipePipeline.compose_audio_mode);
      }
      setComposeEncoderMode(recipePipeline.compose_encoder_mode);
      setComposeStyle(recipePipeline.compose_style ?? { ...DEFAULT_SUBTITLE_STYLE });
    }
    const recipeSubtitleChars = snapshot.max_subtitle_chars ?? 0;
    setMaxSubtitleChars(recipeSubtitleChars);
    if (recipeSubtitleChars > 0) {
      setCustomSubtitleChars(recipeSubtitleChars);
      setCustomSubtitleDraft(String(recipeSubtitleChars));
    }
    setError("");
    setRecipeNotice(
      usedFallback
        ? t("home.recipeModelFallback", { name })
        : t("home.recipeApplied", { name }),
    );
  };

  const openRecipeDialog = (
    mode: "create" | "rename" | "delete",
    recipe?: TaskRecipe,
  ) => {
    setRecipeName(recipe?.name ?? "");
    setRecipeDialog({ mode, recipe });
    setRecipeNotice("");
  };

  const handleRecipeDialogConfirm = async () => {
    if (!recipeDialog) return;
    setRecipeBusy(true);
    setRecipeNotice("");
    try {
      if (recipeDialog.mode === "delete" && recipeDialog.recipe) {
        await deleteTaskRecipe(recipeDialog.recipe.id);
        setRecipes((current) => current.filter((recipe) => recipe.id !== recipeDialog.recipe?.id));
        setRecipeNotice(t("home.recipeDeleted"));
      } else {
        const saved = await saveTaskRecipe({
          id: recipeDialog.recipe?.id,
          name: recipeName,
          snapshot: recipeDialog.recipe?.snapshot ?? currentRecipeSnapshot(),
        });
        setRecipes((current) => [
          saved,
          ...current.filter((recipe) => recipe.id !== saved.id),
        ]);
        setRecipeNotice(
          recipeDialog.mode === "rename"
            ? t("home.recipeRenamed")
            : t("home.recipeSaved"),
        );
      }
      setRecipeDialog(null);
    } catch (recipeError) {
      setRecipeNotice(recipeError instanceof Error ? recipeError.message : String(recipeError));
    } finally {
      setRecipeBusy(false);
    }
  };

  const activeTaskType = taskTypes.find((item) => item.value === taskType);
  const sourceLanguageLabel = sourceLanguageOptions.find((item) => item.value === sourceLanguage);
  const targetLanguageLabel = sourceLanguageOptions.find((item) => item.value === targetLanguage);
  const pipelineLanguage = taskType === "generate-only"
    ? (sourceLanguageLabel ? t(sourceLanguageLabel.labelKey) : sourceLanguage)
    : `${sourceLanguageLabel ? t(sourceLanguageLabel.labelKey) : sourceLanguage} → ${targetLanguageLabel ? t(targetLanguageLabel.labelKey) : targetLanguage}`;
  const taskReady = Boolean(selectedPath && canStartTask);
  const readinessHint = !selectedPath
    ? missingFileHint
    : (inputTypeMismatchHint || modelPrerequisiteHint || pipelinePrerequisiteHint || t("home.readyToStart"));
  const workspaceStatus = bootstrapState === "error" || ffmpegVersion === "unavailable"
    ? "error"
    : (bootstrapState === "loading" || ffmpegVersion === "detecting" ? "loading" : "ready");
  const workspaceStatusLabel = workspaceStatus === "ready"
    ? t("home.readyStatus")
    : (workspaceStatus === "loading" ? t("home.loadingWorkspace") : t("home.workspaceNeedsAttention"));
  const compactActionBar = (
    <div className="liquid-control flex items-center gap-3 rounded-[1rem] border border-border-default px-3 py-2.5 shadow-lg backdrop-blur-xl">
      <span className={`h-2.5 w-2.5 shrink-0 rounded-full ${taskReady ? "bg-success" : "bg-warning"}`} aria-hidden="true" />
      <span className={`min-w-0 flex-1 truncate text-xs font-semibold ${taskReady ? "text-success" : "text-text-secondary"}`} title={readinessHint}>
        {readinessHint}
      </span>
      <Button
        type="button"
        onClick={handleCreate}
        disabled={creating}
        variant="primary"
        size="sm"
        className="shrink-0"
      >
        <Play size={14} />
        {creating
          ? t("home.creating")
          : (selectedPaths.length > 1
            ? t("home.batchCreateTask", { count: selectedPaths.length })
            : t("home.createTask"))}
      </Button>
    </div>
  );

  return (
    <div className="page-shell space-y-5">
      <section className="flex flex-col gap-4 px-1 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0 max-w-3xl">
          <div className="flex items-center gap-2 text-[11px] font-bold uppercase tracking-[0.14em] text-brand">
            <Sparkles size={13} />
            {t("home.workspaceEyebrow")}
          </div>
          <h2 className="mt-2 font-display text-[clamp(1.85rem,3.2vw,2.7rem)] font-bold leading-[1.08] tracking-[-0.04em] text-text-primary">
            {t("home.newTask")}
          </h2>
          <p className="mt-1.5 max-w-2xl text-sm leading-6 text-text-secondary">
            {taskType === "translate-only" ? t("home.sourceHintSubtitle") : t("home.sourceHintMedia")}
          </p>
        </div>

        <div className="flex max-w-full flex-wrap items-center gap-2 sm:shrink-0 sm:justify-end">
          <span className="status-chip text-xs font-semibold">
            <span
              className={`status-dot ${workspaceStatus === "loading" ? "status-dot-pending" : ""} ${workspaceStatus === "error" ? "status-dot-error" : ""}`}
              aria-hidden="true"
            />
            {workspaceStatusLabel}
          </span>
          {bootstrapState === "error" && (
            <button
              type="button"
              onClick={() => void loadWorkspace()}
              className="status-chip text-xs font-semibold text-brand transition hover:border-brand/35 hover:text-brand"
            >
              {t("home.retrySetup")}
            </button>
          )}
        </div>
      </section>

      {updateInfo && (
        <div className="liquid-control rounded-[1.25rem] p-4">
          <div className="relative z-10 flex flex-col items-start justify-between gap-3 sm:flex-row sm:items-center">
            <div className="flex min-w-0 items-center gap-3">
              <AlertCircle className="shrink-0 text-info" size={18} />
              <div className="min-w-0 text-sm text-text-secondary">
                <span className="font-semibold text-text-primary">{t("home.newVersionAvailable")}{updateInfo.latest_version}</span>
                {updateInfo.body && (
                  <span className="ml-1 opacity-90">
                    {t("home.updateNotes")}: {updateInfo.body.slice(0, 100)}{updateInfo.body.length > 100 ? "…" : ""}
                  </span>
                )}
              </div>
            </div>
            <Button
              onClick={() => void handleUpdate()}
              variant="primary"
              size="sm"
              disabled={Boolean(updateProgress)}
            >
              <Download size={15} />
              {updateProgress
                ? t(`home.updatePhase.${updateProgress.phase}`)
                : (updateInfo.install_supported ? t("home.installUpdate") : t("home.goDownload"))}
            </Button>
          </div>
          {updateProgress?.phase === "downloading" && updateProgress.total_bytes && (
            <div
              className="relative z-10 mt-3 h-1.5 overflow-hidden rounded-full bg-border/45"
              role="progressbar"
              aria-label={t("home.updateDownloadProgress")}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.min(100, Math.round((updateProgress.downloaded_bytes / updateProgress.total_bytes) * 100))}
            >
              <span
                className="block h-full rounded-full bg-brand transition-[width] duration-150"
                style={{ width: `${Math.min(100, (updateProgress.downloaded_bytes / updateProgress.total_bytes) * 100)}%` }}
              />
            </div>
          )}
          {updateError && (
            <p className="relative z-10 mt-3 text-sm font-medium text-danger" role="alert">
              {updateError}
            </p>
          )}
        </div>
      )}

      <div className="grid items-start gap-5 min-[1100px]:grid-cols-[minmax(0,1fr)_19rem]">
        <div className="min-w-0 space-y-5">
          <Card className="p-5 sm:p-6">
            <fieldset className="mb-5">
              <legend className="mb-2.5 text-xs font-semibold text-text-tertiary">{t("home.taskType")}</legend>
              <div className="grid gap-2 sm:grid-cols-3" role="radiogroup">
                {taskTypes.map((item) => {
                  const Icon = item.icon;
                  const isActive = taskType === item.value;
                  return (
                    <button
                      key={item.value}
                      type="button"
                      role="radio"
                      aria-checked={isActive}
                      tabIndex={isActive ? 0 : -1}
                      onKeyDown={(event) => {
                        if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
                        event.preventDefault();
                        const direction = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
                        const currentIndex = taskTypes.findIndex(({ value }) => value === taskType);
                        const nextIndex = (currentIndex + direction + taskTypes.length) % taskTypes.length;
                        handleTaskTypeChange(taskTypes[nextIndex].value);
                        const radios = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>("[role='radio']");
                        requestAnimationFrame(() => radios?.[nextIndex]?.focus());
                      }}
                      onClick={() => handleTaskTypeChange(item.value)}
                      data-testid={`task-type-${item.value}`}
                      className={`flex min-h-14 items-center gap-2.5 rounded-xl border px-3 py-2.5 text-left text-sm transition ${
                        isActive
                          ? "border-brand/35 bg-brand/10 text-text-primary"
                          : "border-border-subtle bg-surface-overlay/30 text-text-secondary hover:border-border-strong hover:bg-surface-overlay"
                      }`}
                    >
                      <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-lg ${isActive ? "bg-brand text-white" : "bg-surface-overlay text-text-tertiary"}`}>
                        <Icon size={15} />
                      </span>
                      <span className="min-w-0">
                        <span className="block font-semibold text-text-primary">{t(item.labelKey)}</span>
                        <span className="mt-0.5 hidden truncate text-[11px] text-text-tertiary lg:block">{t(item.descKey)}</span>
                      </span>
                    </button>
                  );
                })}
              </div>
            </fieldset>

            <div className="min-w-0 space-y-4">
              <div className="min-w-0">
                <div className="mb-5">
                  <span className="step-label">{t("home.sourceStep")}</span>
                  <h3 className="mt-3 font-display text-[1.42rem] font-bold tracking-[-0.025em] text-text-primary">
                    {taskType === "translate-only" ? t("home.selectSubtitleFile") : t("home.selectMediaFile")}
                  </h3>
                  <p className="mt-1.5 text-sm leading-6 text-text-secondary">
                    {taskType === "translate-only" ? t("home.sourceHintSubtitle") : t("home.sourceHintMedia")}
                  </p>
                </div>

                <div
                  id="task-source-input"
                  tabIndex={-1}
                  onClick={(event) => {
                    const target = event.target as HTMLElement;
                    if (target.closest("button, input, select, a")) return;
                    void handleSelectMedia();
                  }}
                  className={`file-stage cursor-pointer rounded-[1.3rem] p-4 transition focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/45 sm:p-5 ${dragActive ? "ring-2 ring-brand/70 bg-brand/10" : ""}`}
                  data-testid="source-asset"
                >
                  <div className="flex flex-wrap items-center gap-4">
                    <span className="file-icon">
                      {displayedInputKind === "subtitle" ? <FileText size={24} /> : <FileVideo size={24} />}
                    </span>
                    <div className="min-w-[12rem] flex-[1_1_14rem]">
                      {selectedPath && (
                        <Badge variant="info" className="mb-1.5">
                          {t("home.selectedFile")} · {selectedFileKind}
                        </Badge>
                      )}
                      <p className={`${selectedPath ? "truncate" : "leading-6"} font-semibold text-text-primary`}>
                        {selectedPaths.length > 1
                          ? t("home.batchSelected", { count: selectedPaths.length })
                          : (selectedPath ? fileNameFromPath(selectedPath) : `${t("home.noFileSelected")} · ${selectedFileKind}`)}
                      </p>
                      <p className={`mt-1 font-mono text-xs leading-5 text-text-tertiary ${selectedPath ? "truncate" : ""}`} title={selectedPath || undefined}>
                        {selectedPath || t("home.sourcePathHint")}
                      </p>
                    </div>
                    <div className="ml-auto flex flex-wrap justify-end gap-2">
                      {selectedInputKind === "media" && selectedPath && (
                        <Button type="button" onClick={handleSelectPairedSubtitles} variant="secondary" size="sm">
                          <Link2 size={14} />
                          {t("home.pairSubtitles")}
                        </Button>
                      )}
                      <Button type="button" onClick={handleSelectMedia} variant="secondary" size="sm">
                        <FolderOpen size={14} />
                        {t("home.selectFile")}
                        <ChevronRight size={14} className="opacity-55" />
                      </Button>
                      <Button type="button" onClick={handleSelectFolder} variant="secondary" size="sm">
                        <FolderTree size={14} />
                        {t("home.selectFolder")}
                      </Button>
                    </div>
                  </div>

                  {inputTypeMismatchHint && (
                    <div className="mt-4 flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-xs leading-5 text-warning" role="status">
                      <AlertCircle className="mt-0.5 shrink-0" size={14} />
                      <div className="min-w-0 flex-1">
                        <p>{inputTypeMismatchHint}</p>
                        <button
                          type="button"
                          onClick={() => handleTaskTypeChange(selectedInputKind === "media" ? "generate-and-translate" : "translate-only")}
                          className="mt-1.5 inline-flex items-center gap-1 font-semibold text-warning underline decoration-warning/45 underline-offset-2 transition hover:text-warning/80"
                        >
                          {selectedInputKind === "media" ? t("home.switchToMediaWorkflow") : t("home.switchToSubtitleWorkflow")}
                          <ChevronRight size={13} />
                        </button>
                      </div>
                    </div>
                  )}

                  {selectedPaths.length > 1 && (
                    <div className="mt-4 border-t border-border-subtle pt-4">
                      <div className="flex flex-wrap gap-2">
                        {selectedPaths.slice(0, 6).map((path) => (
                          <span
                            key={path}
                            title={path}
                            className="max-w-full truncate rounded-lg border border-border-subtle bg-surface-overlay px-2.5 py-1.5 font-mono text-[11px] text-text-secondary"
                          >
                            {fileNameFromPath(path)}
                          </span>
                        ))}
                        {selectedPaths.length > 6 && (
                          <span className="rounded-lg bg-brand/10 px-2.5 py-1.5 text-[11px] font-semibold text-brand">
                            +{selectedPaths.length - 6}
                          </span>
                        )}
                      </div>
                      <button
                        type="button"
                        onClick={clearSelectedPaths}
                        className="mt-3 text-xs font-semibold text-text-tertiary underline-offset-4 hover:text-text-primary hover:underline"
                      >
                        {t("home.clearSelection")}
                      </button>
                    </div>
                  )}

                  {selectedInputKind === "media" && selectedPath && pairedSubtitlePaths.length > 0 && (
                    <section
                      className="mt-4 rounded-xl border border-border-subtle bg-surface-overlay/55 p-3.5"
                      aria-labelledby="subtitle-pairing-title"
                      data-testid="subtitle-pairing"
                    >
                      <div className="flex flex-wrap items-start justify-between gap-2">
                        <div className="min-w-0">
                          <h4 id="subtitle-pairing-title" className="flex items-center gap-2 text-sm font-semibold text-text-primary">
                            <Link2 size={14} className="text-brand" />
                            {t("home.pairingTitle")}
                          </h4>
                          <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("home.pairingHint")}</p>
                        </div>
                        <Badge variant={asrFallbackCount === 0 ? "success" : "info"}>
                          {t("home.pairingSummary", { paired: pairedSubtitleCount, asr: asrFallbackCount })}
                        </Badge>
                      </div>

                      <div className="mt-3 max-h-72 space-y-2 overflow-y-auto pr-1">
                        {visiblePairingMedia.map((mediaPath) => {
                          const hasManualChoice = Object.prototype.hasOwnProperty.call(manualSubtitlePairs, mediaPath);
                          const manualPath = manualSubtitlePairs[mediaPath];
                          const choice = hasManualChoice ? (manualPath || "__asr__") : "__auto__";
                          const effectiveSubtitle = subtitlePairing.pairedByMedia.get(mediaPath);
                          const subtitleOptions = manualPath && !visiblePairingSubtitles.includes(manualPath)
                            ? [manualPath, ...visiblePairingSubtitles]
                            : visiblePairingSubtitles;
                          return (
                            <div key={mediaPath} className="grid items-center gap-2 rounded-lg border border-border-subtle/80 bg-surface-raised/45 px-2.5 py-2 sm:grid-cols-[minmax(0,0.8fr)_minmax(12rem,1fr)]">
                              <div className="min-w-0">
                                <p className="truncate font-mono text-[11px] font-semibold text-text-secondary" title={mediaPath}>
                                  {fileNameFromPath(mediaPath)}
                                </p>
                                <p className={`mt-0.5 truncate text-[10px] ${effectiveSubtitle ? "text-success" : "text-text-tertiary"}`} title={effectiveSubtitle}>
                                  {effectiveSubtitle
                                    ? fileNameFromPath(effectiveSubtitle)
                                    : t("home.pairWillUseAsr")}
                                </p>
                              </div>
                              <Select
                                value={choice}
                                onChange={(event) => handlePairChoice(mediaPath, event.target.value)}
                                aria-label={t("home.pairSelectFor", { file: fileNameFromPath(mediaPath) })}
                                className="h-9 text-xs"
                              >
                                <option value="__auto__">{t("home.pairAuto")}</option>
                                <option value="__asr__">{t("home.pairUseAsr")}</option>
                                {subtitleOptions.map((subtitlePath) => (
                                  <option key={subtitlePath} value={subtitlePath}>{fileNameFromPath(subtitlePath)}</option>
                                ))}
                              </Select>
                            </div>
                          );
                        })}
                      </div>

                      {(selectedPaths.length > visiblePairingMedia.length || pairedSubtitlePaths.length > visiblePairingSubtitles.length) && (
                        <p className="mt-2 text-[11px] leading-5 text-text-tertiary">
                          {t("home.pairingMore", {
                            media: Math.max(0, selectedPaths.length - visiblePairingMedia.length),
                            subtitles: Math.max(0, pairedSubtitlePaths.length - visiblePairingSubtitles.length),
                          })}
                        </p>
                      )}

                      {subtitlePairing.unpairedSubtitles.length > 0 && (
                        <div className="mt-3 border-t border-border-subtle pt-3">
                          <p className="text-[11px] font-semibold text-text-tertiary">
                            {t("home.pairUnused", { count: subtitlePairing.unpairedSubtitles.length })}
                          </p>
                          <div className="mt-2 flex flex-wrap gap-1.5">
                            {subtitlePairing.unpairedSubtitles.slice(0, 12).map((subtitlePath) => (
                              <button
                                key={subtitlePath}
                                type="button"
                                onClick={() => removePairedSubtitle(subtitlePath)}
                                title={`${t("home.removePairedSubtitle")}: ${subtitlePath}`}
                                className="inline-flex max-w-full items-center gap-1 rounded-lg border border-border-subtle bg-surface-overlay px-2 py-1 font-mono text-[10px] text-text-secondary transition hover:border-danger/30 hover:text-danger"
                              >
                                <span className="truncate">{fileNameFromPath(subtitlePath)}</span>
                                <Trash2 size={11} className="shrink-0" />
                              </button>
                            ))}
                          </div>
                        </div>
                      )}

                      <div className="mt-3 flex flex-wrap gap-3 border-t border-border-subtle pt-3 text-xs font-semibold">
                        <button type="button" onClick={handleSelectPairedSubtitles} className="text-brand hover:text-brand-hover">
                          {t("home.addPairedSubtitles")}
                        </button>
                        <button type="button" onClick={clearSubtitlePairing} className="text-text-tertiary hover:text-danger">
                          {t("home.clearPairing")}
                        </button>
                      </div>
                    </section>
                  )}

                  {!selectedPath && (
                    <p className="mt-4 border-t border-dashed border-border-subtle pt-4 text-center text-xs text-text-tertiary">
                      {dragActive ? t("home.dropNow") : t("home.dragPasteHint")}
                    </p>
                  )}
                </div>
              </div>

              <details id="task-recognition-core" tabIndex={-1} className="core-picker group rounded-[1rem] border border-border-subtle bg-surface-overlay/35 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/45">
                <summary className="flex cursor-pointer list-none items-center gap-3 px-4 py-3.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/35 sm:px-5">
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-brand/12 text-brand">
                    {taskNeedsAsr
                      ? <Cpu size={17} />
                      : (allSelectedMediaPaired ? <Link2 size={17} /> : <Languages size={17} />)}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-semibold text-text-tertiary">{t("home.coreStep")}</p>
                    <p className="mt-0.5 truncate text-sm font-semibold text-text-primary">
                      {taskNeedsAsr
                        ? `${engineLabels[engineId] ?? engineId}${activeModel ? ` · ${activeModel.name}` : ""}`
                        : (allSelectedMediaPaired ? t("home.corePairedNotRequired") : t("home.coreNotRequired"))}
                    </p>
                  </div>
                  {taskNeedsAsr && activeModel && (
                    <span className={`hidden items-center gap-1 text-xs font-semibold sm:inline-flex ${activeModel.status === "downloaded" || engineId === "custom-command" ? "text-success" : "text-warning"}`}>
                      {activeModel.status === "downloaded" || engineId === "custom-command" ? <CheckCircle size={12} /> : <AlertCircle size={12} />}
                      {activeModel.status === "downloaded" || engineId === "custom-command" ? t("home.coreReady") : t("home.coreNeedsSetup")}
                    </span>
                  )}
                  <ChevronDown size={16} className="shrink-0 text-text-tertiary transition-transform duration-200 group-open:rotate-180" aria-hidden="true" />
                </summary>

                {taskNeedsAsr ? (
                  <div className="space-y-3 border-t border-border-subtle px-4 py-4 sm:px-5">
                    <p className="text-xs leading-5 text-text-secondary">
                      {pairedSubtitleCount > 0
                        ? t("home.corePartialPairing", { count: asrFallbackCount })
                        : t("home.coreHint")}
                    </p>
                    <div>
                      <label htmlFor="task-asr-engine" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.asrEngine")}</label>
                      <Select
                        id="task-asr-engine"
                        value={engineId}
                        onChange={(event) => {
                          setEngineId(event.target.value);
                          const first = models.find((model) => model.engine_id === event.target.value);
                          if (first) setModelId(first.id);
                        }}
                      >
                        {engines.map((engine) => (
                          <option key={engine} value={engine}>{engineLabels[engine] ?? engine}</option>
                        ))}
                      </Select>
                    </div>
                    <div>
                      <label htmlFor="task-asr-model" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.asrModel")}</label>
                      <Select id="task-asr-model" value={modelId} onChange={(event) => setModelId(event.target.value)}>
                        {engineModels.map((model) => <option key={model.id} value={model.id}>{model.name}</option>)}
                      </Select>
                    </div>
                    <div className="flex flex-wrap items-center gap-2 pt-0.5">
                      <Badge variant={engineId === "cloud-asr" ? "info" : "success"}>
                        {engineId === "cloud-asr" ? <Cloud size={12} /> : <Cpu size={12} />}
                        {engineId === "cloud-asr" ? t("home.coreCloud") : t("home.coreLocal")}
                      </Badge>
                      {activeModel && (
                        <span className={`inline-flex items-center gap-1 text-xs font-semibold ${activeModel.status === "downloaded" || engineId === "custom-command" ? "text-success" : "text-warning"}`}>
                          {activeModel.status === "downloaded" || engineId === "custom-command" ? <CheckCircle size={12} /> : <AlertCircle size={12} />}
                          {activeModel.status === "downloaded" || engineId === "custom-command" ? t("home.coreReady") : t("home.coreNeedsSetup")}
                        </span>
                      )}
                    </div>
                    {activeModel && activeModel.status !== "downloaded" && engineId !== "custom-command" && (
                      <button
                        type="button"
                        onClick={() => navigate("/models")}
                        className="inline-flex items-center gap-1 text-xs font-semibold text-brand transition hover:text-brand-hover"
                      >
                        {t("home.openModelManage")}
                        <ChevronRight size={13} />
                      </button>
                    )}
                  </div>
                ) : (
                  <div className="border-t border-border-subtle px-4 py-3 text-xs leading-5 text-text-secondary sm:px-5">
                    {allSelectedMediaPaired ? t("home.corePairedNotRequired") : t("home.coreNotRequired")}
                  </div>
                )}
              </details>

              {taskType !== "generate-only" && (
                <section
                  id="task-translation-options"
                  tabIndex={-1}
                  aria-labelledby="task-translation-title"
                  className="rounded-[1.15rem] border border-brand/20 bg-brand/[0.055] p-4 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/45 sm:p-5"
                  data-testid="translation-setup"
                >
                  <div className="flex flex-col gap-3 min-[900px]:flex-row min-[900px]:items-start min-[900px]:justify-between">
                    <div className="flex min-w-0 items-start gap-3">
                      <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-brand/12 text-brand">
                        <Languages size={18} />
                      </span>
                      <div className="min-w-0">
                        <p className="text-xs font-bold uppercase tracking-[0.1em] text-brand">{t("home.translationSetupStep")}</p>
                        <h4 id="task-translation-title" className="mt-1 text-base font-bold text-text-primary">{t("home.translationSetupTitle")}</h4>
                        <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("home.translationSetupHint")}</p>
                      </div>
                    </div>
                    <span className="shrink-0 rounded-full border border-brand/15 bg-surface-overlay px-3 py-1.5 text-xs font-semibold text-brand">
                      {pipelineLanguage}
                    </span>
                  </div>

                  <div className="mt-4 grid gap-4 min-[1400px]:grid-cols-[minmax(12rem,0.36fr)_minmax(0,1fr)]">
                    <div>
                      <label htmlFor="task-target-language" className="mb-2 block text-sm font-semibold text-text-primary">{t("home.targetLang")}</label>
                      <Select
                        id="task-target-language"
                        data-testid="task-target-language"
                        value={targetLanguage}
                        onChange={(event) => setTargetLanguage(event.target.value)}
                      >
                        {targetLanguageOptions.map(({ value, labelKey }) => (
                          <option key={value} value={value}>{t(labelKey)} ({value})</option>
                        ))}
                      </Select>
                    </div>

                    <fieldset>
                      <legend className="mb-2 text-sm font-semibold text-text-primary">{t("home.subtitleContent")}</legend>
                      <div className="grid grid-cols-[repeat(auto-fit,minmax(10.5rem,1fr))] gap-2.5" role="radiogroup">
                        {translationContentModes.map((mode) => {
                          const isActive = translationContentMode === mode.value;
                          return (
                            <button
                              key={mode.value}
                              type="button"
                              role="radio"
                              aria-checked={isActive}
                              tabIndex={isActive ? 0 : -1}
                              data-testid={`translation-content-${mode.value}`}
                              onKeyDown={(event) => {
                                if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
                                event.preventDefault();
                                const direction = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
                                const currentIndex = translationContentModes.findIndex(({ value }) => value === translationContentMode);
                                const nextIndex = (currentIndex + direction + translationContentModes.length) % translationContentModes.length;
                                setTranslationContentMode(translationContentModes[nextIndex].value);
                                const radios = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>("[role='radio']");
                                requestAnimationFrame(() => radios?.[nextIndex]?.focus());
                              }}
                              onClick={() => setTranslationContentMode(mode.value)}
                              className={`min-h-[4.75rem] rounded-[1rem] border px-3 py-2.5 text-left text-sm transition-all duration-200 ${
                                isActive
                                  ? "liquid-selected text-text-primary"
                                  : "border-border-default bg-surface-overlay/35 text-text-secondary hover:border-border-strong hover:bg-surface-overlay hover:text-text-primary"
                              }`}
                            >
                              <span className="block font-semibold">{t(mode.labelKey)}</span>
                              <span className="mt-1 block text-xs leading-5 text-text-tertiary">{t(mode.descKey)}</span>
                            </button>
                          );
                        })}
                      </div>
                    </fieldset>
                  </div>
                </section>
              )}
            </div>

            {mediaMetadata && selectedPaths.length === 1 && (
              <div className="mt-5 border-t border-border-subtle pt-4">
                <p className="mb-3 text-xs font-bold uppercase tracking-[0.12em] text-text-tertiary">{t("home.mediaInfo")}</p>
                <div className="grid gap-x-6 gap-y-2 text-xs text-text-secondary sm:grid-cols-2 lg:grid-cols-3">
                  <div>{t("home.miDuration")}: <span className="font-medium text-text-primary">{mediaMetadata.duration_string}</span></div>
                  {mediaMetadata.width > 0 && <div>{t("home.miResolution")}: <span className="font-medium text-text-primary">{mediaMetadata.width}×{mediaMetadata.height}</span></div>}
                  {mediaMetadata.fps > 0 && <div>{t("home.miFps")}: <span className="font-medium text-text-primary">{mediaMetadata.fps.toFixed(2)} fps</span></div>}
                  {mediaMetadata.codec !== "unknown" && <div>{t("home.miVideoCodec")}: <span className="font-medium text-text-primary">{mediaMetadata.codec}</span></div>}
                  {mediaMetadata.audio_codec && <div>{t("home.miAudioCodec")}: <span className="font-medium text-text-primary">{mediaMetadata.audio_codec}</span></div>}
                  {mediaMetadata.audio_sample_rate && (
                    <div className={mediaMetadata.audio_sample_rate !== 16000 ? "font-semibold text-warning" : ""}>
                      {t("home.miSampleRate")}: {mediaMetadata.audio_sample_rate} Hz
                    </div>
                  )}
                  {mediaMetadata.audio_channels && <div>{t("home.miChannels")}: <span className="font-medium text-text-primary">{mediaMetadata.audio_channels} ch</span></div>}
                  <div>{t("home.miAudioTracks")}: <span className="font-medium text-text-primary">{mediaMetadata.audio_tracks}</span></div>
                </div>
              </div>
            )}

            {error && (
              <div className="mt-4 flex items-start gap-2 rounded-xl border border-danger/20 bg-danger/10 px-3.5 py-3 text-sm text-danger" role="alert">
                <AlertCircle className="mt-0.5 shrink-0" size={16} />
                <span>{error}</span>
              </div>
            )}
          </Card>

          {createPortal(
            <div className="fixed inset-x-4 bottom-24 z-40 sm:hidden">{compactActionBar}</div>,
            document.body,
          )}
          <div className="sticky bottom-3 z-30 hidden sm:block min-[1100px]:!hidden">{compactActionBar}</div>

          <Card className="p-5 sm:p-6">
            <div className="mb-5">
              <span className="step-label">{t("home.workflowStep")}</span>
              <h3 className="mt-3 font-display text-[1.42rem] font-bold tracking-[-0.025em] text-text-primary">{t("home.taskConfig")}</h3>
              <p className="mt-1.5 text-sm leading-6 text-text-secondary">{t("home.workflowDesc")}</p>
            </div>

            <div className="space-y-5">
              <fieldset id="task-delivery-options" tabIndex={-1} className="rounded-[1.2rem] border border-brand/16 bg-brand/[0.045] p-4 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/45 sm:p-5" data-testid="delivery-targets">
                <legend className="sr-only">{t("home.deliveryTargets")}</legend>
                <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
                  <div>
                    <p className="text-xs font-bold uppercase tracking-[0.1em] text-brand">{t("home.deliveryTargets")}</p>
                    <h4 className="mt-1 text-base font-bold text-text-primary">{t("home.chooseDeliverables")}</h4>
                    <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("home.deliveryTargetsHint")}</p>
                  </div>
                  <span className="rounded-full border border-brand/15 bg-surface-overlay px-3 py-1.5 text-xs font-semibold text-brand">
                    {deliveryTargets}
                  </span>
                </div>

                <div className={`mt-4 grid gap-2.5 ${taskType === "translate-only" ? "" : "sm:grid-cols-3"}`}>
                  <div
                    className="flex min-h-20 items-start gap-3 rounded-[1rem] border border-border-subtle bg-surface-overlay/45 p-3.5 text-left"
                  >
                    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-success/10 text-success"><CheckCircle size={16} /></span>
                    <span><span className="block text-sm font-semibold text-text-primary">{t("home.targetSubtitle")}</span><span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("home.targetSubtitleDesc")}</span></span>
                  </div>
                  {taskType !== "translate-only" && <button
                    type="button"
                    aria-pressed={enableDubbing}
                    onClick={() => {
                      const next = !enableDubbing;
                      setEnableDubbing(next);
                      if (!next) setDubbingReview(false);
                      setError("");
                    }}
                    className={`flex min-h-20 items-start gap-3 rounded-[1rem] border p-3.5 text-left transition disabled:cursor-not-allowed disabled:opacity-45 ${enableDubbing ? "liquid-selected" : "border-border-default bg-surface-overlay/35 hover:border-border-strong"}`}
                  >
                    <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-xl ${enableDubbing ? "bg-brand/15 text-brand" : "bg-surface-overlay text-text-tertiary"}`}><Volume2 size={16} /></span>
                    <span><span className="block text-sm font-semibold text-text-primary">{t("home.targetDubbing")}</span><span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("home.targetDubbingDesc")}</span></span>
                  </button>}
                  {taskType !== "translate-only" && <button
                    type="button"
                    aria-pressed={enableCompose}
                    onClick={() => {
                      const next = !enableCompose;
                      setEnableCompose(next);
                      setError("");
                    }}
                    className={`flex min-h-20 items-start gap-3 rounded-[1rem] border p-3.5 text-left transition disabled:cursor-not-allowed disabled:opacity-45 ${enableCompose ? "liquid-selected" : "border-border-default bg-surface-overlay/35 hover:border-border-strong"}`}
                  >
                    <span className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-xl ${enableCompose ? "bg-brand/15 text-brand" : "bg-surface-overlay text-text-tertiary"}`}><Film size={16} /></span>
                    <span><span className="block text-sm font-semibold text-text-primary">{t("home.targetVideo")}</span><span className="mt-1 block text-xs leading-5 text-text-tertiary">{t("home.targetVideoDesc")}</span></span>
                  </button>}
                </div>

                {(enableDubbing || enableCompose) && (
                  <div className="mt-4 grid gap-4 border-t border-border-subtle pt-4 lg:grid-cols-2">
                    {enableDubbing && (
                      <div className="rounded-xl border border-border-subtle bg-surface-overlay/55 p-3.5">
                        <div className="flex items-center justify-between gap-2">
                          <p className="text-sm font-semibold text-text-primary">{t("home.dubbingConfig")}</p>
                          <Badge variant={dubbingReady ? "success" : "warning"}>{dubbingReady ? t("home.coreReady") : t("home.coreNeedsSetup")}</Badge>
                        </div>
                        <div className="mt-3 space-y-3">
                          <div>
                            <label htmlFor="pipeline-tts-target" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.dubbingEngine")}</label>
                            <Select
                              id="pipeline-tts-target"
                              value={dubbingTargetId ? `${dubbingEngine}:${dubbingTargetId}` : ""}
                              onChange={(event) => {
                                const separator = event.target.value.indexOf(":");
                                const kind = event.target.value.slice(0, separator) as "local" | "cloud";
                                const id = event.target.value.slice(separator + 1);
                                setDubbingEngine(kind);
                                setDubbingTargetId(id);
                                if (kind === "local") {
                                  const model = ttsModels.find((item) => item.id === id);
                                  setDubbingVoice(model?.default_voice_id ?? "");
                                } else {
                                  const provider = ttsProviders.find((item) => item.id === id);
                                  setDubbingVoice(provider?.voice ?? "");
                                }
                              }}
                            >
                              {!dubbingTargetId && <option value="">{t("home.dubbingNoEngine")}</option>}
                              <optgroup label={t("home.dubbingLocalModels")}>
                                {readyTtsModels.map((model) => <option key={model.id} value={`local:${model.id}`}>{model.name}</option>)}
                              </optgroup>
                              <optgroup label={t("home.dubbingCloudServices")}>
                                {ttsProviders.map((provider) => <option key={provider.id} value={`cloud:${provider.id}`}>{provider.name}{provider.text_upload_consent ? "" : ` · ${t("home.dubbingNeedsConsent")}`}</option>)}
                              </optgroup>
                            </Select>
                          </div>
                          <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_7rem]">
                            <div>
                              <label htmlFor="pipeline-tts-voice" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.dubbingVoice")}</label>
                              {dubbingEngine === "local" && selectedTtsModel?.voices.length ? (
                                <Select id="pipeline-tts-voice" value={dubbingVoice} onChange={(event) => setDubbingVoice(event.target.value)}>
                                  {selectedTtsModel.voices.map((voice) => <option key={voice.id} value={voice.id}>{voice.label}</option>)}
                                </Select>
                              ) : (
                                <Input id="pipeline-tts-voice" value={dubbingVoice} onChange={(event) => setDubbingVoice(event.target.value)} placeholder={t("home.dubbingVoicePlaceholder")} />
                              )}
                            </div>
                            <div>
                              <label htmlFor="pipeline-tts-speed" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.dubbingSpeed")}</label>
                              <Select id="pipeline-tts-speed" value={String(dubbingSpeed)} onChange={(event) => setDubbingSpeed(Number(event.target.value))}>
                                <option value="0.85">0.85×</option><option value="1">1.00×</option><option value="1.15">1.15×</option><option value="1.3">1.30×</option>
                              </Select>
                            </div>
                          </div>
                          {dubbingEngine === "local" && (
                            <div className="space-y-2">
                              <div className="flex items-end justify-between gap-3">
                                <label className="text-xs font-semibold text-text-secondary">{t("dubbing.localConcurrency")}</label>
                                <span className="text-[11px] text-text-tertiary">{t("dubbing.localConcurrencySummary", { count: dubbingConcurrency })}</span>
                              </div>
                              <div className="grid grid-cols-3 gap-1 rounded-xl border border-border-subtle bg-surface-card/70 p-1" role="group" aria-label={t("dubbing.localConcurrency")}>
                                {[1, 2, 3].map((count) => (
                                  <button
                                    key={count}
                                    type="button"
                                    aria-pressed={dubbingConcurrency === count}
                                    onClick={() => setDubbingConcurrency(count)}
                                    className={`min-h-9 rounded-lg px-1.5 text-[11px] font-semibold transition ${dubbingConcurrency === count ? "liquid-selected text-brand" : "text-text-secondary hover:bg-surface-overlay"}`}
                                  >
                                    {t(`dubbing.localConcurrency${count}` as "dubbing.localConcurrency1")}
                                  </button>
                                ))}
                              </div>
                              <p className="text-[11px] leading-5 text-text-tertiary">{t(`dubbing.localConcurrencyDesc${dubbingConcurrency}` as "dubbing.localConcurrencyDesc1")}</p>
                            </div>
                          )}
                          {!dubbingReady && (
                            <button type="button" onClick={() => navigate("/models")} className="text-xs font-semibold text-brand hover:text-brand-hover">
                              {t("home.openModelManage")} <ChevronRight size={12} className="inline" />
                            </button>
                          )}
                        </div>
                      </div>
                    )}

                    {enableCompose && (
                      <div className="rounded-xl border border-border-subtle bg-surface-overlay/55 p-3.5">
                        <p className="text-sm font-semibold text-text-primary">{t("home.composeConfig")}</p>
                        <div className="mt-3 space-y-3">
                          <div className="grid gap-3 sm:grid-cols-2">
                            <div><label htmlFor="pipeline-subtitle-mode" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.composeSubtitleMode")}</label><Select id="pipeline-subtitle-mode" value={composeSoftSubtitle ? "soft" : "hard"} onChange={(event) => setComposeSoftSubtitle(event.target.value === "soft")}><option value="hard">{t("home.composeHardSubtitle")}</option><option value="soft">{t("home.composeSoftSubtitle")}</option></Select></div>
                            <div><label htmlFor="pipeline-encoder-mode" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.composeEncoder")}</label><Select id="pipeline-encoder-mode" value={composeEncoderMode} onChange={(event) => setComposeEncoderMode(event.target.value as "auto" | "cpu" | "hardware")} disabled={composeSoftSubtitle}><option value="auto">{t("home.composeEncoderAuto")}</option><option value="cpu">CPU</option><option value="hardware">{t("home.composeEncoderHardware")}</option></Select></div>
                          </div>
                          {!composeSoftSubtitle ? (
                            <div>
                              <div className="flex items-end justify-between gap-3">
                                <label htmlFor="pipeline-subtitle-style" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.composeStyle")}</label>
                                <button type="button" onClick={() => navigate("/subtitle-merge")} className="mb-1.5 text-[11px] font-semibold text-brand hover:text-brand-hover">{t("home.manageComposeStyles")}</button>
                              </div>
                              <Select
                                id="pipeline-subtitle-style"
                                data-testid="pipeline-subtitle-style"
                                value={composeStylePresetValue}
                                onChange={(event) => handleComposeStyleChange(event.target.value)}
                              >
                                {!matchingBuiltInStyle && !matchingUserStyle && <option value="snapshot">{t("home.composeEmbeddedStyle")}</option>}
                                <optgroup label={t("home.composeBuiltInStyles")}>
                                  {BUILT_IN_SUBTITLE_STYLE_PRESETS.map((preset) => <option key={preset.id} value={preset.id}>{t(preset.nameKey)}</option>)}
                                </optgroup>
                                {subtitleStylePresets.length > 0 && (
                                  <optgroup label={t("home.composePersonalStyles")}>
                                    {subtitleStylePresets.map((preset) => <option key={preset.id} value={preset.id}>{preset.name}</option>)}
                                  </optgroup>
                                )}
                              </Select>
                              <p className="mt-1.5 text-[11px] leading-5 text-text-tertiary">{t("home.composeStyleSnapshotHint")}</p>
                            </div>
                          ) : (
                            <p className="rounded-lg bg-surface-overlay px-3 py-2 text-xs leading-5 text-text-tertiary">{t("home.composeSoftStyleHint")}</p>
                          )}
                          {enableDubbing ? (
                            <div><label htmlFor="pipeline-audio-mode" className="mb-1.5 block text-xs font-semibold text-text-secondary">{t("home.composeAudioMode")}</label><Select id="pipeline-audio-mode" value={composeAudioMode} onChange={(event) => setComposeAudioMode(event.target.value as "replace" | "mix" | "add-track")}><option value="replace">{t("home.composeReplaceAudio")}</option><option value="mix">{t("home.composeMixAudio")}</option><option value="add-track">{t("home.composeAddTrack")}</option></Select></div>
                          ) : (
                            <p className="rounded-lg bg-surface-overlay px-3 py-2 text-xs leading-5 text-text-tertiary">{t("home.composeKeepAudio")}</p>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                )}

              </fieldset>

              <details className="group rounded-[1rem] border border-border-subtle bg-surface-overlay/25">
                <summary className="flex cursor-pointer list-none items-center gap-3 px-4 py-3.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/35 sm:px-5">
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-surface-overlay text-text-secondary">
                    <Sparkles size={16} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block text-sm font-semibold text-text-primary">{t("home.moreSettings")}</span>
                    <span className="mt-0.5 block truncate text-xs text-text-tertiary">{t("home.moreSettingsHint")}</span>
                  </span>
                  <span className="hidden text-xs font-semibold text-text-tertiary md:block">
                    {outputFormat.toUpperCase()} · {maxSubtitleChars === 0 ? t("home.subtitleBreakSmart") : maxSubtitleChars === -1 ? t("home.subtitleBreakUnlimited") : maxSubtitleChars}
                  </span>
                  <ChevronDown size={16} className="shrink-0 text-text-tertiary transition-transform duration-200 group-open:rotate-180" aria-hidden="true" />
                </summary>
                <div className="space-y-5 border-t border-border-subtle px-4 py-5 sm:px-5">

              {taskNeedsAsr && (
                <div>
                  {engineId === "parakeet-mlx" && (
                    <div className="flex items-start gap-2 rounded-xl border border-info/15 bg-info/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                      <Cpu size={14} className="mt-0.5 shrink-0 text-info" />
                      <span>{t("home.parakeetNotice")}</span>
                    </div>
                  )}
                  {["paraformer", "qwen3-asr", "firered-asr"].includes(engineId) && (
                    <div className="flex items-start gap-2 rounded-xl border border-info/15 bg-info/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                      <Cpu size={14} className="mt-0.5 shrink-0 text-info" />
                      <span>{t("home.sherpaNativeNotice")}</span>
                    </div>
                  )}
                  {engineId === "cloud-asr" && (
                    <div className="flex items-start gap-2 rounded-xl border border-brand/15 bg-brand/8 px-3.5 py-3 text-xs leading-5 text-text-secondary">
                      <Cloud size={14} className="mt-0.5 shrink-0 text-brand" />
                      <span>{t("home.cloudAsrNotice")}</span>
                    </div>
                  )}
                </div>
              )}

              {taskType === "translate-only" && (
                <div className="rounded-xl border border-border-subtle bg-surface-overlay p-3.5 text-sm leading-6 text-text-secondary">
                  {t("home.transOnlyInfo")}
                </div>
              )}

              <div className="border-t border-border-subtle pt-5">
                <div>
                  <label htmlFor="task-source-language" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.sourceLang")}</label>
                  <Select id="task-source-language" value={sourceLanguage} onChange={(event) => setSourceLanguage(event.target.value)}>
                    {availableSourceLanguages.map(({ value, labelKey }) => (
                      <option key={value} value={value}>{t(labelKey)} ({value})</option>
                    ))}
                  </Select>
                </div>
              </div>

              <fieldset className="rounded-[1.05rem] border border-border-subtle bg-surface-overlay/35 p-4">
                <legend className="sr-only">{t("home.subtitleBreakLabel")}</legend>
                <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
                  <div className="min-w-0">
                    <p className="text-sm font-semibold text-text-primary">{t("home.subtitleBreakLabel")}</p>
                    <p className="mt-1 text-xs leading-5 text-text-tertiary">{t("home.subtitleBreakHint")}</p>
                  </div>
                  <div className="liquid-control grid shrink-0 grid-cols-3 rounded-xl p-1" role="radiogroup" aria-label={t("home.subtitleBreakLabel")}>
                    {([
                      [0, "home.subtitleBreakSmart"],
                      [-1, "home.subtitleBreakUnlimited"],
                      [40, "home.subtitleBreakCustom"],
                    ] as const).map(([value, labelKey]) => {
                      const isActive = value === 0
                        ? maxSubtitleChars === 0
                        : value === -1
                          ? maxSubtitleChars === -1
                          : maxSubtitleChars > 0;
                      return (
                        <button
                          key={value}
                          type="button"
                          role="radio"
                          aria-checked={isActive}
                          tabIndex={isActive ? 0 : -1}
                          onKeyDown={(event) => {
                            if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
                            event.preventDefault();
                            const modes = [0, -1, 40] as const;
                            const currentMode = maxSubtitleChars === 0 ? 0 : maxSubtitleChars === -1 ? -1 : 40;
                            const direction = event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 1;
                            const nextIndex = (modes.indexOf(currentMode) + direction + modes.length) % modes.length;
                            const nextMode = modes[nextIndex];
                            setMaxSubtitleChars(nextMode === 40 ? customSubtitleChars : nextMode);
                            const radios = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>("[role='radio']");
                            requestAnimationFrame(() => radios?.[nextIndex]?.focus());
                          }}
                          onClick={() => {
                            if (value === 40) {
                              setMaxSubtitleChars(customSubtitleChars);
                            } else {
                              setMaxSubtitleChars(value);
                            }
                          }}
                          className={`rounded-lg px-2.5 py-2 text-xs font-semibold transition ${isActive ? "theme-selected text-brand" : "text-text-tertiary hover:text-text-primary"}`}
                          data-testid={`subtitle-break-${value === 0 ? "smart" : value === -1 ? "unlimited" : "custom"}`}
                        >
                          {t(labelKey as "home.subtitleBreakSmart" | "home.subtitleBreakUnlimited" | "home.subtitleBreakCustom")}
                        </button>
                      );
                    })}
                  </div>
                </div>
                {maxSubtitleChars > 0 && (
                  <div className="mt-3 flex items-center gap-2 border-t border-border-subtle pt-3">
                    <label htmlFor="task-max-subtitle-chars" className="text-xs font-medium text-text-secondary">{t("home.subtitleBreakCustomValue")}</label>
                    <Input
                      id="task-max-subtitle-chars"
                      type="number"
                      min={8}
                      max={120}
                      value={customSubtitleDraft}
                      onChange={(event) => {
                        const rawValue = event.target.value;
                        if (!/^\d{0,3}$/.test(rawValue)) return;
                        setCustomSubtitleDraft(rawValue);
                        const parsed = Number(rawValue);
                        if (Number.isInteger(parsed) && parsed >= 8 && parsed <= 120) {
                          setCustomSubtitleChars(parsed);
                          setMaxSubtitleChars(parsed);
                        }
                      }}
                      onBlur={() => {
                        const parsed = Number(customSubtitleDraft);
                        const nextValue = Number.isFinite(parsed)
                          ? Math.min(120, Math.max(8, Math.trunc(parsed)))
                          : customSubtitleChars;
                        setCustomSubtitleDraft(String(nextValue));
                        setCustomSubtitleChars(nextValue);
                        setMaxSubtitleChars(nextValue);
                      }}
                      className="h-9 w-20 text-right"
                      data-testid="subtitle-break-custom-value"
                    />
                    <span className="text-xs text-text-tertiary">{t("home.subtitleBreakUnit")}</span>
                  </div>
                )}
              </fieldset>

              <div className="grid gap-4 border-t border-border-subtle pt-6 sm:grid-cols-2">
                <div>
                  <label htmlFor="task-output-format" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.outputFormat")}</label>
                  <Select id="task-output-format" value={outputFormat} onChange={(event) => setOutputFormat(event.target.value)}>
                    {outputFormats.map((format) => <option key={format.value} value={format.value}>{format.label}</option>)}
                  </Select>
                </div>
                <div>
                  <label htmlFor="task-output-name" className="mb-2 block text-sm font-medium text-text-secondary">{t("home.outputName")}</label>
                  <Input
                    id="task-output-name"
                    value={outputName}
                    maxLength={180}
                    onChange={(event) => setOutputName(event.target.value)}
                    placeholder={t("home.outputNamePlaceholder")}
                  />
                  <p className="mt-1.5 text-xs leading-5 text-text-tertiary">{t("home.outputNameHint")}</p>
                </div>
                <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-border-subtle bg-surface-overlay px-3.5 py-3 text-sm text-text-secondary sm:col-span-2">
                  <input
                    type="checkbox"
                    checked={stripChinesePunctuation}
                    onChange={(event) => setStripChinesePunctuation(event.target.checked)}
                    className="h-4 w-4 rounded border-border-strong accent-brand"
                  />
                  <span>
                    <span className="block font-semibold text-text-primary">{t("home.stripChinesePunctuation")}</span>
                    <span className="mt-0.5 block text-xs text-text-tertiary">{t("home.stripChinesePunctuationHint")}</span>
                  </span>
                </label>
              </div>

              <div className={`grid gap-2.5 border-t border-border-subtle pt-5 ${enableDubbing ? "sm:grid-cols-2" : ""}`}>
                <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-warning/15 bg-warning/5 px-3.5 py-3 text-sm text-text-secondary">
                  <input type="checkbox" checked={reviewRequired} onChange={(event) => setReviewRequired(event.target.checked)} className="h-4 w-4 rounded border-border-strong accent-brand" />
                  <ShieldCheck size={17} className="shrink-0 text-warning" />
                  <span><span className="block font-semibold text-text-primary">{t("home.subtitleReviewGate")}</span><span className="mt-0.5 block text-xs leading-5 text-text-tertiary">{t("home.subtitleReviewGateHint")}</span></span>
                </label>
                {enableDubbing && (
                  <label className="flex cursor-pointer items-center gap-3 rounded-xl border border-border-subtle bg-surface-overlay/45 px-3.5 py-3 text-sm text-text-secondary">
                    <input type="checkbox" checked={dubbingReview} onChange={(event) => setDubbingReview(event.target.checked)} className="h-4 w-4 rounded border-border-strong accent-brand" />
                    <Volume2 size={17} className="shrink-0 text-brand" />
                    <span><span className="block font-semibold text-text-primary">{t("home.dubbingReviewGate")}</span><span className="mt-0.5 block text-xs leading-5 text-text-tertiary">{t("home.dubbingReviewGateHint")}</span></span>
                  </label>
                )}
              </div>
                </div>
              </details>

              {modelPrerequisiteHint && (
                <div id="task-prerequisite-hint" className="flex flex-wrap items-center gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm text-warning">
                  <AlertCircle size={14} className="shrink-0" />
                  <span>{modelPrerequisiteHint}</span>
                  <button type="button" onClick={() => navigate("/models")} className="font-semibold underline underline-offset-2 hover:text-warning/80">
                    {t("home.openModelManage")}
                  </button>
                </div>
              )}

              {pipelinePrerequisiteHint && (
                <div className="flex flex-wrap items-center gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm text-warning" role="status">
                  <AlertCircle size={14} className="shrink-0" />
                  <span>{pipelinePrerequisiteHint}</span>
                  {enableDubbing && !dubbingReady && (
                    <button type="button" onClick={() => navigate("/models")} className="font-semibold underline underline-offset-2 hover:text-warning/80">
                      {t("home.openModelManage")}
                    </button>
                  )}
                </div>
              )}

              {taskNeedsAsr && mediaMetadata?.audio_sample_rate && mediaMetadata.audio_sample_rate !== 16000 && (
                <div className="flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/10 px-3.5 py-3 text-sm text-warning">
                  <AlertCircle className="mt-0.5 shrink-0" size={14} />
                  <span>{t("home.resampleHint", { rate: mediaMetadata.audio_sample_rate })}</span>
                </div>
              )}
            </div>
          </Card>
        </div>

        <aside className="hidden space-y-5 min-[1100px]:sticky min-[1100px]:top-0 min-[1100px]:block">
          <Card className="relative overflow-hidden p-5">
            <span className="pipeline-glow" />
            <div className="relative z-10">
              <span className="step-label">{t("home.summaryStep")}</span>
              <h3 className="mt-2.5 font-display text-h2 font-bold text-text-primary">{t("home.summaryTitle")}</h3>

              <dl className="mt-4">
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summaryTask")}</dt>
                  <dd className="text-right text-sm font-semibold text-text-primary">{activeTaskType ? t(activeTaskType.labelKey) : taskType}</dd>
                </div>
                {taskNeedsAsr && (
                  <div className="system-row">
                    <dt className="text-xs text-text-tertiary">{t("home.summaryEngine")}</dt>
                    <dd className="max-w-[11rem] truncate text-right font-mono text-xs font-semibold text-text-primary" title={engineId}>{engineId}</dd>
                  </div>
                )}
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summaryLanguage")}</dt>
                  <dd className="text-right text-sm font-semibold text-text-primary">{pipelineLanguage}</dd>
                </div>
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.deliveryTargets")}</dt>
                  <dd className="max-w-[12rem] text-right text-sm font-semibold text-text-primary">{deliveryTargets}</dd>
                </div>
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summaryFormat")}</dt>
                  <dd className="font-mono text-sm font-bold uppercase text-brand">{outputFormat}</dd>
                </div>
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summarySubtitleBreak")}</dt>
                  <dd className="text-right text-sm font-semibold text-text-primary">
                    {maxSubtitleChars === 0
                      ? t("home.subtitleBreakSmart")
                      : maxSubtitleChars === -1
                        ? t("home.subtitleBreakUnlimited")
                        : `${t("home.subtitleBreakCustom")} · ${maxSubtitleChars}`}
                  </dd>
                </div>
                <div className="system-row">
                  <dt className="text-xs text-text-tertiary">{t("home.summaryReview")}</dt>
                  <dd className={`text-right text-sm font-semibold ${reviewRequired ? "text-warning" : "text-text-primary"}`}>
                    {reviewRequired ? t("home.reviewGateOn") : t("home.reviewGateOff")}
                  </dd>
                </div>
              </dl>

              <div className={`mt-4 flex items-start gap-2.5 rounded-xl border px-3.5 py-3 text-xs leading-5 ${taskReady ? "border-success/18 bg-success/8 text-success" : "border-warning/18 bg-warning/8 text-warning"}`}>
                <span className={`mt-1.5 h-2 w-2 shrink-0 rounded-full ${taskReady ? "bg-success shadow-[0_0_10px_color-mix(in_srgb,var(--color-success)_55%,transparent)]" : "bg-warning"}`} />
                <span>{readinessHint}</span>
              </div>

              <div className="mt-4 grid gap-2.5">
                <Button
                  type="button"
                  onClick={handleCreate}
                  disabled={creating}
                  aria-describedby={modelPrerequisiteHint ? "task-prerequisite-hint" : undefined}
                  title={modelPrerequisiteHint || pipelinePrerequisiteHint || undefined}
                  variant="primary"
                  size="lg"
                  className="w-full"
                >
                  <Play size={15} />
                  {creating
                    ? t("home.creating")
                    : (selectedPaths.length > 1
                      ? t("home.batchCreateTask", { count: selectedPaths.length })
                      : t("home.createTask"))}
                </Button>
                {taskType !== "translate-only" && (
                  <Button type="button" onClick={handlePreview} disabled={creating} variant="secondary" className="w-full">
                    {t("home.createPreview")}
                  </Button>
                )}
              </div>
            </div>
          </Card>

        </aside>
      </div>

      <Card className="overflow-hidden p-5 sm:p-6">
        <details className="group">
          <summary className="flex cursor-pointer list-none items-center gap-4 rounded-xl focus:outline-none focus-visible:ring-2 focus-visible:ring-brand/35">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-surface-overlay text-brand"><Save size={17} /></span>
            <span className="min-w-0 flex-1">
              <span className="step-label">{t("home.recipeEyebrow")}</span>
              <span className="mt-1 block font-display text-[1.1rem] font-bold tracking-[-0.02em] text-text-primary">{t("home.recipeTitle")}</span>
              <span className="mt-0.5 block truncate text-xs text-text-tertiary">{t("home.recipeDesc")}</span>
            </span>
            <span className="hidden rounded-full border border-border-subtle bg-surface-overlay px-2.5 py-1 text-xs font-semibold text-text-tertiary sm:block">
              {builtInRecipes.length + recipes.length}
            </span>
            <ChevronDown size={17} className="shrink-0 text-text-tertiary transition-transform duration-200 group-open:rotate-180" aria-hidden="true" />
          </summary>

          <div className="mt-5 flex justify-end border-t border-border-subtle pt-5">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => openRecipeDialog("create")}
            >
              <Save size={14} />
              {t("home.saveCurrentRecipe")}
            </Button>
          </div>

        <div className="mt-5 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {builtInRecipes.map((recipe) => (
            <button
              key={recipe.id}
              type="button"
              onClick={() => applyRecipe(recipe.snapshot, recipe.name)}
              className="group rounded-[1.15rem] border border-border-subtle bg-surface-overlay/40 p-4 text-left transition hover:-translate-y-0.5 hover:border-brand/30 hover:bg-brand/5"
            >
              <div className="flex items-center justify-between gap-3">
                <span className="text-[10px] font-bold uppercase tracking-[0.12em] text-brand">
                  {t("home.builtinRecipe")}
                </span>
                {recipe.snapshot.review_required && (
                  <ShieldCheck size={15} className="text-warning" />
                )}
              </div>
              <p className="mt-2 font-semibold text-text-primary">{recipe.name}</p>
              <p className="mt-1 text-xs leading-5 text-text-tertiary">{recipe.description}</p>
              <span className="mt-3 inline-flex text-xs font-semibold text-brand opacity-80 transition group-hover:opacity-100">
                {t("home.applyRecipe")} →
              </span>
            </button>
          ))}

          {recipes.map((recipe) => (
            <div
              key={recipe.id}
              className="rounded-[1.15rem] border border-border-subtle bg-surface-overlay/40 p-4"
            >
              <div className="flex items-start justify-between gap-3">
                <button
                  type="button"
                  onClick={() => applyRecipe(recipe.snapshot, recipe.name)}
                  className="min-w-0 flex-1 text-left"
                >
                  <span className="text-[10px] font-bold uppercase tracking-[0.12em] text-text-tertiary">
                    {t("home.userRecipe")}
                  </span>
                  <p className="mt-2 truncate font-semibold text-text-primary" title={recipe.name}>{recipe.name}</p>
                  <p className="mt-1 truncate font-mono text-[11px] text-text-tertiary">
                    {recipe.snapshot.task_type} · {recipe.snapshot.output_format.toUpperCase()}
                  </p>
                </button>
                <div className="flex shrink-0 items-center gap-1">
                  <button
                    type="button"
                    onClick={() => openRecipeDialog("rename", recipe)}
                    title={t("home.renameRecipe")}
                    className="rounded-lg p-2 text-text-tertiary transition hover:bg-surface-overlay hover:text-text-primary"
                  >
                    <Pencil size={14} />
                  </button>
                  <button
                    type="button"
                    onClick={() => openRecipeDialog("delete", recipe)}
                    title={t("home.deleteRecipe")}
                    className="rounded-lg p-2 text-text-tertiary transition hover:bg-danger/10 hover:text-danger"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
              <button
                type="button"
                onClick={() => applyRecipe(recipe.snapshot, recipe.name)}
                className="mt-3 text-xs font-semibold text-brand hover:underline"
              >
                {t("home.applyRecipe")} →
              </button>
            </div>
          ))}
        </div>

        {recipeNotice && (
          <p className="mt-4 rounded-xl border border-brand/15 bg-brand/8 px-3.5 py-2.5 text-sm text-text-secondary" role="status">
            {recipeNotice}
          </p>
        )}
        </details>
      </Card>

      {recipeDialog && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4 backdrop-blur-sm"
          role="dialog"
          aria-modal="true"
          aria-labelledby="recipe-dialog-title"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && !recipeBusy) setRecipeDialog(null);
          }}
        >
          <Card className="w-full max-w-md border border-border-default bg-surface-overlay p-6 shadow-lg">
            <h3 id="recipe-dialog-title" className="font-display text-h2 font-bold text-text-primary">
              {recipeDialog.mode === "create"
                ? t("home.recipeSaveTitle")
                : recipeDialog.mode === "rename"
                  ? t("home.recipeRenameTitle")
                  : t("home.recipeDeleteTitle")}
            </h3>
            {recipeDialog.mode === "delete" ? (
              <p className="mt-3 text-sm leading-6 text-text-secondary">
                {t("home.recipeDeleteDesc", { name: recipeDialog.recipe?.name ?? "" })}
              </p>
            ) : (
              <div className="mt-5">
                <label htmlFor="recipe-name" className="mb-2 block text-sm font-medium text-text-secondary">
                  {t("home.recipeName")}
                </label>
                <Input
                  id="recipe-name"
                  value={recipeName}
                  maxLength={80}
                  autoFocus
                  onChange={(event) => setRecipeName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && recipeName.trim() && !recipeBusy) {
                      void handleRecipeDialogConfirm();
                    }
                  }}
                  placeholder={t("home.recipeNamePlaceholder")}
                />
              </div>
            )}
            <div className="mt-6 flex justify-end gap-2.5">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                disabled={recipeBusy}
                onClick={() => setRecipeDialog(null)}
              >
                {t("common.cancel")}
              </Button>
              <Button
                type="button"
                variant={recipeDialog.mode === "delete" ? "danger" : "primary"}
                size="sm"
                disabled={recipeBusy || (recipeDialog.mode !== "delete" && !recipeName.trim())}
                onClick={() => void handleRecipeDialogConfirm()}
              >
                {recipeBusy
                  ? t("home.recipeSaving")
                  : recipeDialog.mode === "delete"
                    ? t("common.delete")
                    : t("common.save")}
              </Button>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}
