import { Channel, convertFileSrc, invoke as tauriInvoke } from "@tauri-apps/api/core";
import {
  listen as tauriListen,
  type EventCallback,
  type EventName,
  type Options as EventOptions,
  type UnlistenFn,
} from "@tauri-apps/api/event";
import {
  open as tauriOpenDialog,
  save as tauriSaveDialog,
  type OpenDialogOptions,
  type OpenDialogReturn,
  type SaveDialogOptions,
} from "@tauri-apps/plugin-dialog";
import {
  openPath as tauriOpenPath,
  openUrl as tauriOpenUrl,
  revealItemInDir as tauriRevealItemInDir,
} from "@tauri-apps/plugin-opener";
import {
  readTextFile as tauriReadTextFile,
  writeTextFile as tauriWriteTextFile,
} from "@tauri-apps/plugin-fs";
import { getCurrentWebview, type DragDropEvent } from "@tauri-apps/api/webview";

export const TASK_UPDATED_EVENT = "task-updated";
export const TASK_DELETED_EVENT = "task-deleted";

type InvokeArgs = Record<string, unknown> | undefined;

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  if (isTauriRuntime()) {
    return tauriInvoke<T>(command, args);
  }

  if (import.meta.env.DEV) {
    return mockInvoke<T>(command, args);
  }

  throw new Error(`Tauri runtime is unavailable for command: ${command}`);
}

async function mockInvoke<T>(command: string, args?: InvokeArgs): Promise<T> {
  return mockInvokeResult(command, args) as T;
}

export async function listen<T>(
  event: EventName,
  handler: EventCallback<T>,
  options?: EventOptions,
): Promise<UnlistenFn> {
  if (isTauriRuntime()) {
    return tauriListen<T>(event, handler, options);
  }

  if (import.meta.env.DEV) {
    void handler;
    void options;
    return () => undefined;
  }

  throw new Error(`Tauri event runtime is unavailable for event: ${event}`);
}

export async function listenDragDrop(
  handler: (event: DragDropEvent) => void,
): Promise<UnlistenFn> {
  if (isTauriRuntime()) {
    return getCurrentWebview().onDragDropEvent((event) => handler(event.payload));
  }
  return () => undefined;
}

export async function openDialog<T extends OpenDialogOptions>(
  options?: T,
): Promise<OpenDialogReturn<T>> {
  if (isTauriRuntime()) {
    return tauriOpenDialog(options);
  }

  if (import.meta.env.DEV) {
    const path = mockOpenDialogPath(options);
    return (options?.multiple ? [path] : path) as OpenDialogReturn<T>;
  }

  throw new Error("Tauri dialog runtime is unavailable for open dialog");
}

export async function saveDialog(options?: SaveDialogOptions): Promise<string | null> {
  if (isTauriRuntime()) {
    return tauriSaveDialog(options);
  }

  if (import.meta.env.DEV) {
    return options?.defaultPath ?? "/Users/example/Downloads/finalsub-output.srt";
  }

  throw new Error("Tauri dialog runtime is unavailable for save dialog");
}

export async function readTextFilePath(path: string): Promise<string> {
  if (isTauriRuntime()) {
    return tauriReadTextFile(path);
  }
  throw new Error(`Browser mock cannot read local file: ${path}`);
}

export async function writeTextFilePath(path: string, contents: string): Promise<void> {
  if (isTauriRuntime()) {
    return tauriWriteTextFile(path, contents);
  }
  if (import.meta.env.DEV) {
    console.info(`[dev browser mock] writeTextFilePath(${path}, ${contents.length} bytes)`);
    return;
  }
  throw new Error(`Tauri file runtime is unavailable for path: ${path}`);
}

export async function openPath(path: string, openWith?: string): Promise<void> {
  if (isTauriRuntime()) {
    return tauriOpenPath(path, openWith);
  }

  if (import.meta.env.DEV) {
    console.info(`[dev browser mock] openPath(${path})`);
    return;
  }

  throw new Error(`Tauri opener runtime is unavailable for path: ${path}`);
}

export async function openUrl(url: string): Promise<void> {
  if (isTauriRuntime()) {
    return tauriOpenUrl(url);
  }
  if (import.meta.env.DEV) {
    console.info(`[dev browser mock] openUrl(${url})`);
    return;
  }
  throw new Error(`Tauri opener runtime is unavailable for URL: ${url}`);
}

export function fileAssetUrl(path: string): string {
  return isTauriRuntime() ? convertFileSrc(path) : "";
}

export async function revealItemInDir(path: string | string[]): Promise<void> {
  if (isTauriRuntime()) {
    return tauriRevealItemInDir(path);
  }

  if (import.meta.env.DEV) {
    console.info(`[dev browser mock] revealItemInDir(${Array.isArray(path) ? path.join(", ") : path})`);
    return;
  }

  throw new Error("Tauri opener runtime is unavailable for revealItemInDir");
}

function mockOpenDialogPath(options?: OpenDialogOptions): string {
  if (options?.directory) {
    return "/Users/example/FinalSub";
  }

  const extensions = options?.filters?.flatMap((filter) => filter.extensions) ?? [];
  if (extensions.some((ext) => ["mp4", "mov", "mkv", "webm", "avi", "flv", "wmv"].includes(ext))) {
    return "/Users/example/Movies/demo.mp4";
  }
  if (extensions.some((ext) => ["srt", "vtt", "ass", "ssa", "lrc"].includes(ext))) {
    return "/Users/example/Subtitles/demo.srt";
  }
  if (extensions.some((ext) => ["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus"].includes(ext))) {
    return "/Users/example/Audio/demo-dub.wav";
  }
  if (extensions.includes("bin")) {
    return "/Users/example/Models/ggml-small.bin";
  }
  if (extensions.includes("onnx")) {
    return "/Users/example/Models/sensevoice.onnx";
  }
  if (extensions.includes("txt")) {
    return "/Users/example/Models/tokens.txt";
  }
  if (extensions.includes("json")) {
    return "/Users/example/Downloads/finalsub-config.json";
  }

  return "/Users/example/Downloads/finalsub-demo.file";
}

export interface AppInfo {
  version: string;
  name: string;
}

export interface AsrModelInfo {
  id: string;
  engine_id: string;
  name: string;
  description: string;
  languages: string[];
  best_for: string;
  size_mb: number | null;
  download_url: string | null;
  status: "available" | "downloading" | "downloaded" | "not-ready" | { error: string };
}

export type TtsModelFamily = "kokoro" | "vits" | "zipvoice";
export type TtsModelStatus = "ready" | "not-installed" | "incomplete";
export type TtsModelLocation = "managed" | "external";

export interface TtsVoice {
  id: string;
  sid: number;
  label: string;
  label_en: string;
  language: string;
  gender: string;
}

export interface TtsModelInfo {
  id: string;
  family: TtsModelFamily;
  name: string;
  description: string;
  languages: string[];
  size_mb: number;
  download_url: string;
  extra_download_urls: string[];
  sample_rate: number;
  default_voice_id: string;
  clone_only: boolean;
  voices: TtsVoice[];
  status: TtsModelStatus;
  path: string | null;
  location: TtsModelLocation | null;
  missing_files: string[];
}

export interface LocalTtsSynthesisRequest {
  model_id: string;
  text: string;
  voice_id?: string;
  speed?: number;
  output_path: string;
  reference_audio_path?: string;
  reference_text?: string;
  num_steps?: number;
}

export interface TtsSynthesisResult {
  output_path: string;
  sample_rate: number;
  duration_ms: number;
}

export type TtsProviderProtocol = "openai-compatible" | "azure-speech" | "elevenlabs";

export interface TtsProviderProfile {
  id: string;
  name: string;
  protocol: TtsProviderProtocol;
  endpoint: string;
  model: string;
  voice: string;
  region: string;
  text_upload_consent: boolean;
  timeout_seconds: number;
  request_concurrency: number;
}

export interface SaveTtsProviderRequest extends Omit<TtsProviderProfile, "id"> {
  id?: string;
}

export interface CloudTtsSynthesisRequest {
  provider_id: string;
  text: string;
  voice?: string;
  speed?: number;
  output_path: string;
}

export type DubbingCueStatus = "pending" | "synthesizing" | "ready" | "overlong" | "accepted" | "failed";
export type DubbingEngineSelection =
  | { kind: "local"; model_id: string }
  | { kind: "cloud"; provider_id: string };

export interface DubbingRunConfig {
  engine: DubbingEngineSelection;
  voice: string;
  global_speed: number;
  reference_audio_path: string | null;
  reference_text: string | null;
  num_steps: number | null;
}

export interface DubbingCue {
  index: number;
  start_ms: number;
  end_ms: number;
  text: string;
  status: DubbingCueStatus;
  overlap: boolean;
  voice_id: string | null;
  synthesized_ms: number | null;
  applied_speed: number | null;
  slot_ms: number;
  ratio: number | null;
  wav_path: string | null;
  error: string | null;
}

export interface DubbingSession {
  version: number;
  id: string;
  subtitle_path: string;
  subtitle_hash: string;
  video_path: string | null;
  cues: DubbingCue[];
  last_config: DubbingRunConfig | null;
  output_path: string | null;
  created_at: string;
  updated_at: string;
  source_changed: boolean;
}

export interface DubbingSynthesizeCueRequest {
  session_id: string;
  cue_index: number;
  engine: DubbingEngineSelection;
  voice: string;
  global_speed: number;
  reference_audio_path?: string;
  reference_text?: string;
  num_steps?: number;
}

export type TranslationContentMode =
  | "target-only"
  | "source-and-target"
  | "target-and-source";

export interface Task {
  id: string;
  task_type: string;
  status: string;
  media_path: string;
  media_name: string;
  engine_id: string;
  model_id: string;
  source_language: string | null;
  target_language: string | null;
  translation_content_mode: TranslationContentMode;
  output_format: string;
  output_name: string | null;
  strip_chinese_punctuation: boolean;
  review_required: boolean;
  reviewed_at: string | null;
  progress: number;
  status_message: string;
  output_path: string | null;
  error: string | null;
  created_at: string;
  updated_at: string;
}

export interface TaskDeletedPayload {
  task_id: string;
}

export interface AudioExtractPlan {
  ffmpeg_bin: string;
  args: string[];
  input: string;
  output: string;
}

export async function getAppInfo(): Promise<AppInfo> {
  return invoke("get_app_info");
}

export async function listAsrModels(): Promise<AsrModelInfo[]> {
  return invoke("list_asr_models");
}

export async function scanModels(): Promise<AsrModelInfo[]> {
  return invoke("scan_models");
}

export async function listTtsModels(): Promise<TtsModelInfo[]> {
  return invoke("list_tts_models");
}

export async function registerTtsModelPath(
  modelId: string,
  sourcePath: string,
): Promise<TtsModelInfo> {
  return invoke("register_tts_model_path", { modelId, sourcePath });
}

export async function forgetTtsModelPath(modelId: string): Promise<void> {
  return invoke("forget_tts_model_path", { modelId });
}

export async function setTtsModelsRoot(modelsRoot: string): Promise<TtsModelInfo[]> {
  return invoke("set_tts_models_root", { modelsRoot });
}

export async function synthesizeLocalTts(
  generationId: string,
  request: LocalTtsSynthesisRequest,
): Promise<TtsSynthesisResult> {
  return invoke("synthesize_local_tts", { generationId, request });
}

export async function cancelLocalTts(generationId: string): Promise<boolean> {
  return invoke("cancel_local_tts", { generationId });
}

export async function listTtsProviders(): Promise<TtsProviderProfile[]> {
  return invoke("list_tts_providers");
}

export async function saveTtsProvider(
  request: SaveTtsProviderRequest,
): Promise<TtsProviderProfile> {
  return invoke("save_tts_provider", { request });
}

export async function deleteTtsProvider(providerId: string): Promise<void> {
  return invoke("delete_tts_provider", { providerId });
}

export async function synthesizeCloudTts(
  generationId: string,
  request: CloudTtsSynthesisRequest,
): Promise<TtsSynthesisResult> {
  return invoke("synthesize_cloud_tts", { generationId, request });
}

export async function testTtsProvider(providerId: string): Promise<TtsSynthesisResult> {
  return invoke("test_tts_provider", { providerId });
}

export async function createDubbingSession(
  subtitlePath: string,
  videoPath?: string,
): Promise<DubbingSession> {
  return invoke("create_dubbing_session", { subtitlePath, videoPath });
}

export async function getDubbingSession(sessionId: string): Promise<DubbingSession> {
  return invoke("get_dubbing_session", { sessionId });
}

export async function synthesizeDubbingCue(
  generationId: string,
  request: DubbingSynthesizeCueRequest,
): Promise<DubbingSession> {
  return invoke("synthesize_dubbing_cue", { generationId, request });
}

export async function acceptDubbingOverflow(
  generationId: string,
  sessionId: string,
  cueIndex: number,
): Promise<DubbingSession> {
  return invoke("accept_dubbing_overflow", { generationId, sessionId, cueIndex });
}

export async function exportDubbingAudio(
  generationId: string,
  sessionId: string,
  outputPath: string,
): Promise<DubbingSession> {
  return invoke("export_dubbing_audio", { generationId, sessionId, outputPath });
}

export async function discoverBatchInputs(
  paths: string[],
  taskType: string,
  recursive = true,
): Promise<string[]> {
  return invoke("discover_batch_inputs", { paths, taskType, recursive });
}

export async function deleteModel(modelId: string): Promise<void> {
  return invoke("delete_model", { modelId });
}

export async function importLocalModel(
  modelId: string,
  sourcePath: string,
  expectedSha256?: string,
): Promise<void> {
  return invoke("import_local_model", { modelId, sourcePath, expectedSha256 });
}

export async function importSensevoiceModel(
  modelOnnxPath: string,
  tokensPath: string,
): Promise<void> {
  return invoke("import_sensevoice_model", { modelOnnxPath, tokensPath });
}

export interface EmbeddedSubtitleStream {
  sub_index: number;
  codec: string;
  language: string | null;
}

export async function listEmbeddedSubtitles(
  videoPath: string,
): Promise<EmbeddedSubtitleStream[]> {
  return invoke("list_embedded_subtitles", { videoPath });
}

export async function extractEmbeddedSubtitle(
  videoPath: string,
  subIndex: number,
  outputPath: string,
): Promise<string> {
  return invoke("extract_embedded_subtitle", { videoPath, subIndex, outputPath });
}

export async function getModelStatus(modelId: string): Promise<AsrModelInfo | null> {
  return invoke("get_model_status", { modelId });
}

export interface CreateTaskRequest {
  task_type: string;
  media_path: string;
  engine_id: string;
  model_id: string;
  source_language?: string;
  target_language?: string;
  translation_content_mode?: TranslationContentMode;
  output_format?: string;
  output_name?: string;
  strip_chinese_punctuation?: boolean;
  review_required?: boolean;
}

export interface TaskRecipeSnapshot {
  task_type: string;
  engine_id: string;
  model_id: string;
  source_language: string;
  target_language: string;
  translation_content_mode: TranslationContentMode;
  output_format: string;
  output_name: string;
  strip_chinese_punctuation: boolean;
  review_required: boolean;
}

export interface TaskRecipe {
  id: string;
  name: string;
  snapshot: TaskRecipeSnapshot;
  created_at: string;
  updated_at: string;
}

export interface SaveTaskRecipeRequest {
  id?: string;
  name: string;
  snapshot: TaskRecipeSnapshot;
}

export async function createTask(req: CreateTaskRequest): Promise<Task> {
  return invoke("create_task", { req });
}

export async function createTasks(requests: CreateTaskRequest[]): Promise<Task[]> {
  return invoke("create_tasks", { requests });
}

export async function createPreviewTask(mediaPath: string): Promise<Task> {
  return invoke("create_preview_task", { mediaPath });
}

export async function listTasks(): Promise<Task[]> {
  return invoke("list_tasks");
}

export async function listTaskRecipes(): Promise<TaskRecipe[]> {
  return invoke("list_task_recipes");
}

export async function saveTaskRecipe(request: SaveTaskRecipeRequest): Promise<TaskRecipe> {
  return invoke("save_task_recipe", { request });
}

export async function deleteTaskRecipe(recipeId: string): Promise<string> {
  return invoke("delete_task_recipe", { recipeId });
}

export async function approveTask(taskId: string): Promise<Task> {
  return invoke("approve_task", { taskId });
}

export async function approveTasks(taskIds: string[]): Promise<Task[]> {
  return invoke("approve_tasks", { taskIds });
}

export async function deleteTask(taskId: string): Promise<string> {
  return invoke("delete_task", { taskId });
}

export async function deleteTasks(taskIds: string[]): Promise<string[]> {
  return invoke("delete_tasks", { taskIds });
}

export async function cancelTask(taskId: string): Promise<Task> {
  return invoke("cancel_task", { taskId });
}

export async function pauseTask(taskId: string): Promise<Task> {
  return invoke("pause_task", { taskId });
}

export async function resumeTask(taskId: string): Promise<Task> {
  return invoke("resume_task", { taskId });
}

export async function retryTask(taskId: string): Promise<Task> {
  return invoke("retry_task", { taskId });
}

export async function getTaskLogs(taskId: string): Promise<string> {
  return invoke("get_task_logs", { taskId });
}

export async function normalizeSrt(srtContent: string): Promise<string> {
  return invoke("normalize_srt", { srtContent });
}

export async function extractAudioPlan(
  videoPath: string,
  outputPath: string
): Promise<AudioExtractPlan> {
  return invoke("extract_audio_plan", { videoPath, outputPath });
}

export async function extractAudio(videoPath: string, outputPath: string): Promise<string> {
  return invoke("extract_audio", { videoPath, outputPath });
}

export interface BurnSubtitleRequest {
  video_path: string;
  subtitle_path: string;
  output_path: string;
  font_name?: string;
  font_size?: number;
  font_color?: string;
  outline_color?: string;
  outline_width?: number;
  shadow?: number;
  background_color?: string;
  opaque_background?: boolean;
  alignment?: number;
  margin_v?: number;
  crf?: number;
  preset?: string;
  soft_subtitle?: boolean;
  audio_path?: string;
  audio_mode?: "keep" | "replace" | "mix" | "add-track";
  subtitle_language?: string;
  subtitle_title?: string;
  audio_language?: string;
  audio_title?: string;
}

export async function burnSubtitle(req: BurnSubtitleRequest): Promise<string> {
  return invoke("burn_subtitle", { req });
}

export async function cancelBurnSubtitle(burnId: string): Promise<void> {
  return invoke("cancel_burn_subtitle", { burnId });
}

export interface VideoMetadata {
  duration_seconds: number;
  duration_string: string;
  width: number;
  height: number;
  fps: number;
  codec: string;
  audio_codec?: string;
  audio_sample_rate?: number;
  audio_channels?: number;
  audio_tracks: number;
}

export async function getVideoMetadata(videoPath: string): Promise<VideoMetadata> {
  return invoke("get_video_metadata", { videoPath });
}

export async function convertSubtitleOpencc(srtContent: string, config: string): Promise<string> {
  return invoke("convert_subtitle_opencc", { srtContent, config });
}

export async function convertStringsOpencc(texts: string[], config: string): Promise<string[]> {
  return invoke("convert_strings_opencc", { texts, config });
}

export async function generateSubtitlePreview(req: BurnSubtitleRequest): Promise<string> {
  return invoke("generate_subtitle_preview", { req });
}

export async function getFfmpegVersion(): Promise<string> {
  return invoke("get_ffmpeg_version");
}

export interface TranscribeRequest {
  audio_path: string;
  output_path: string;
  model_id: string;
  language?: string;
}

export async function transcribeAudio(req: TranscribeRequest): Promise<string> {
  return invoke("transcribe_audio", { req });
}

export interface TranscribeParakeetRequest {
  audio_path: string;
  output_path: string;
  language?: string;
}

export async function transcribeParakeet(req: TranscribeParakeetRequest): Promise<string> {
  return invoke("transcribe_parakeet", { req });
}

export interface TranslationProvider {
  id: string;
  name: string;
  provider_type: string;
  is_ai: boolean;
  implemented: boolean;
  requires_api_key: boolean;
  requires_endpoint: boolean;
  requires_model: boolean;
  secret_fields: string[];
  default_endpoint: string;
}

export interface TranslateRequest {
  text: string;
  source_language: string;
  target_language: string;
  provider: string;
  api_key?: string;
  api_url?: string;
  model_name?: string;
  secret_fields?: Record<string, string>;
  system_prompt?: string;
  user_prompt?: string;
  proxy_url?: string;
  custom_headers?: Record<string, string>;
  custom_body?: Record<string, unknown>;
  structured_output?: TranslationStructuredOutputMode;
  response_json_schema?: Record<string, unknown>;
  glossary_prompt?: string;
}

export interface TranslateResponse {
  translated_text: string;
  provider: string;
  success: boolean;
  error?: string;
}

export async function listTranslationProviders(): Promise<TranslationProvider[]> {
  return invoke("list_translation_providers");
}

export async function listTranslationModels(
  providerId: string,
  endpoint: string,
  customHeaders?: Record<string, string>,
): Promise<string[]> {
  return invoke("list_translation_models", { providerId, endpoint, customHeaders });
}

export async function testTranslation(req: TranslateRequest): Promise<TranslateResponse> {
  return invoke("test_translation", { req });
}

export async function testTranslationProxy(proxyUrl: string, targetUrl: string): Promise<string> {
  return invoke("test_translation_proxy", { proxyUrl, targetUrl });
}

export type CloudAsrProtocol = "openai-compatible" | "elevenlabs" | "deepgram" | "gladia" | "volcengine" | "tencent" | "aliyun" | "xfyun";

export interface CloudAsrProfile {
  id: string;
  name: string;
  protocol: CloudAsrProtocol;
  endpoint: string;
  model: string;
  upload_consent: boolean;
  timeout_seconds: number;
  retry_times: number;
  request_concurrency: number;
  request_interval_ms: number;
}

export type TranslationStructuredOutputMode = "disabled" | "json_object" | "json_schema";

export interface TranslationGlossaryEntry {
  id: string;
  source: string;
  target: string;
  note: string;
}

export interface TranslationGlossary {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  order: number;
  entries: TranslationGlossaryEntry[];
}

export interface Settings {
  language: string;
  asr_engine: string;
  cloud_asr_protocol: CloudAsrProtocol;
  cloud_asr_endpoint: string;
  cloud_asr_model: string;
  cloud_asr_upload_consent: boolean;
  cloud_asr_timeout_seconds: number;
  cloud_asr_retry_times: number;
  cloud_asr_request_concurrency: number;
  cloud_asr_request_interval_ms: number;
  cloud_asr_active_profile_id: string;
  cloud_asr_profiles: CloudAsrProfile[];
  models_path: string;
  parakeet_models_path: string;
  max_concurrent_tasks: number;
  subtitle_output_format: string;
  source_language: string;
  target_language: string;
  translate_provider: string;
  translate_endpoints: Record<string, string>;
  translate_models: Record<string, string>;
  translate_retry_times: number;
  translate_system_prompts: Record<string, string>;
  translate_user_prompts: Record<string, string>;
  translate_custom_headers: Record<string, Record<string, string>>;
  translate_custom_body: Record<string, Record<string, unknown>>;
  translate_structured_output: Record<string, TranslationStructuredOutputMode>;
  translate_echo_anchoring: Record<string, boolean>;
  translation_glossaries: TranslationGlossary[];
  translate_batch_size: number;
  translate_concurrency: number;
  translate_request_interval_ms: number;
  proxy_enabled: boolean;
  proxy_url: string;
  use_vad: boolean;
  vad_threshold: number;
  vad_min_speech_duration_ms: number;
  vad_min_silence_duration_ms: number;
  vad_max_speech_duration_s: number;
  vad_speech_pad_ms: number;
  vad_samples_overlap: number;
  check_update_on_startup: boolean;
  use_custom_temp_dir: boolean;
  custom_temp_dir: string;
  whisper_command: string;
  max_context: number;
  enable_telemetry: boolean;
}

export async function getSettings(): Promise<Settings> {
  return invoke("get_settings");
}

export async function saveSettingsCmd(newSettings: Settings): Promise<Settings> {
  return invoke("save_settings_cmd", { newSettings });
}

export async function resetSettings(): Promise<Settings> {
  return invoke("reset_settings");
}

export async function exportConfig(): Promise<string> {
  return invoke("export_config");
}

export async function importConfig(json: string): Promise<Settings> {
  return invoke("import_config", { json });
}

export async function exportConfigToPath(outputPath: string): Promise<string> {
  return invoke("export_config_to_path", { outputPath });
}

export async function importConfigFromPath(inputPath: string): Promise<Settings> {
  return invoke("import_config_from_path", { inputPath });
}

export async function exportEncryptedConfigToPath(
  outputPath: string,
  passphrase: string,
): Promise<string> {
  return invoke("export_encrypted_config_to_path", { outputPath, passphrase });
}

export async function importEncryptedConfigFromPath(
  inputPath: string,
  passphrase: string,
): Promise<Settings> {
  return invoke("import_encrypted_config_from_path", { inputPath, passphrase });
}

export async function setProviderSecret(providerId: string, endpoint: string, field: string, value: string): Promise<void> {
  return invoke("set_provider_secret", { providerId, endpoint, field, value });
}

export async function hasProviderSecret(providerId: string, endpoint: string, field: string): Promise<boolean> {
  return invoke("has_provider_secret", { providerId, endpoint, field });
}

export async function deleteProviderSecret(providerId: string, endpoint: string, field: string): Promise<void> {
  return invoke("delete_provider_secret", { providerId, endpoint, field });
}

export async function loadProofreadTasks(): Promise<string> {
  return invoke("load_proofread_tasks");
}

export async function saveProofreadTasks(data: string): Promise<void> {
  return invoke("save_proofread_tasks", { data });
}

// 受控运行时 scope 授权：导入视频时把视频所在目录加入 plugin-fs 允许范围，
// 以便扫描同目录字幕。文件读写一律改用 @tauri-apps/plugin-fs（dialog 选中即授权）。
export async function authorizeSubtitleDirectory(dirPath: string): Promise<void> {
  return invoke("authorize_subtitle_directory", { dirPath });
}

export interface ModelDownloadProgress {
  model_id: string;
  bytes_downloaded: number;
  total_bytes: number;
  progress: number;
  status: "downloading" | "done" | "cancelled" | "error";
  phase?: "downloading" | "resuming" | "verifying" | "installing" | "paused" | "ready" | "error";
  bytes_per_second?: number | null;
  eta_seconds?: number | null;
  error: string | null;
}

export async function downloadModel(modelId: string): Promise<void> {
  return invoke("download_model", { modelId });
}

export async function cancelModelDownload(modelId: string): Promise<void> {
  return invoke("cancel_model_download", { modelId });
}

export interface UpdateInfo {
  latest_version: string;
  url: string;
  body: string | null;
  install_supported: boolean;
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
  return invoke("check_for_update");
}

export type AppUpdatePhase = "downloading" | "verifying" | "installing" | "restarting";

export interface AppUpdateEvent {
  phase: AppUpdatePhase;
  downloaded_bytes: number;
  total_bytes: number | null;
}

export async function downloadAndInstallUpdate(
  expectedVersion: string,
  onProgress: (event: AppUpdateEvent) => void,
): Promise<void> {
  if (!isTauriRuntime()) {
    if (import.meta.env.DEV) {
      onProgress({ phase: "downloading", downloaded_bytes: 50, total_bytes: 100 });
      onProgress({ phase: "verifying", downloaded_bytes: 0, total_bytes: null });
      onProgress({ phase: "installing", downloaded_bytes: 0, total_bytes: null });
      return;
    }
    throw new Error("Tauri update runtime is unavailable");
  }

  const onProgressChannel = new Channel<AppUpdateEvent>(onProgress);
  return invoke("download_and_install_update", {
    expectedVersion,
    onProgress: onProgressChannel,
  });
}

function createMockSettings(): Settings {
  return {
    language: "zh",
    asr_engine: "parakeet-mlx",
    cloud_asr_protocol: "openai-compatible",
    cloud_asr_endpoint: "https://api.openai.com/v1",
    cloud_asr_model: "gpt-4o-transcribe",
    cloud_asr_upload_consent: false,
    cloud_asr_timeout_seconds: 120,
    cloud_asr_retry_times: 1,
    cloud_asr_request_concurrency: 1,
    cloud_asr_request_interval_ms: 0,
    cloud_asr_active_profile_id: "",
    cloud_asr_profiles: [],
    models_path: "~/Tools/Local-LLM/whisper-models",
    parakeet_models_path: "~/Tools/Local-LLM/parakeet-models",
    max_concurrent_tasks: 2,
    subtitle_output_format: "srt",
    source_language: "auto",
    target_language: "zh",
    translate_provider: "ollama",
    translate_endpoints: {
      ollama: "http://localhost:11434",
      deeplx: "https://api.deeplx.org",
      "custom-openai": "https://api.openai.com/v1",
    },
    translate_models: {
      ollama: "qwen2.5:7b",
      "custom-openai": "gpt-4o-mini",
    },
    translate_retry_times: 2,
    translate_system_prompts: {},
    translate_user_prompts: {},
    translate_custom_headers: {},
    translate_custom_body: {},
    translate_structured_output: {},
    translate_echo_anchoring: {},
    translation_glossaries: [],
    translate_batch_size: 24,
    translate_concurrency: 1,
    translate_request_interval_ms: 0,
    proxy_enabled: false,
    proxy_url: "",
    use_vad: false,
    vad_threshold: 0.5,
    vad_min_speech_duration_ms: 250,
    vad_min_silence_duration_ms: 100,
    vad_max_speech_duration_s: 0,
    vad_speech_pad_ms: 30,
    vad_samples_overlap: 0.1,
    check_update_on_startup: false,
    use_custom_temp_dir: false,
    custom_temp_dir: "",
    whisper_command: "",
    max_context: -1,
    enable_telemetry: false,
  };
}

let mockSettingsState: Settings | null = null;
const mockProviderSecrets = new Set<string>();
let mockTaskRecipes: TaskRecipe[] = [];
let mockTtsModelsState: TtsModelInfo[] | null = null;
let mockTtsProvidersState: TtsProviderProfile[] = [
  {
    id: "00000000-0000-4000-8000-000000000301",
    name: "OpenAI TTS 1",
    protocol: "openai-compatible",
    endpoint: "https://api.openai.com/v1",
    model: "gpt-4o-mini-tts",
    voice: "alloy",
    region: "",
    text_upload_consent: false,
    timeout_seconds: 60,
    request_concurrency: 1,
  },
];
let mockDubbingSessionState: DubbingSession | null = null;

function createMockDubbingSession(subtitlePath = "/Users/example/Subtitles/demo.srt"): DubbingSession {
  const now = new Date().toISOString();
  const texts = [
    "欢迎来到 FinalSub 配音工作台。",
    "这一行故意更长，用来展示超过时间槽位后的人工确认流程。",
    "本地模型不会上传文本。",
    "在线服务只有授权后才会发送文本。",
    "重叠字幕会保留原时间轴并分轨混合。",
    "最后导出 WAV 或 MP3。",
  ];
  return {
    version: 1,
    id: "00000000-0000-4000-8000-000000000401",
    subtitle_path: subtitlePath,
    subtitle_hash: "dev-browser-mock",
    video_path: "/Users/example/Movies/demo.mp4",
    cues: texts.map((text, index) => ({
      index,
      start_ms: 1000 + index * 2600,
      end_ms: 3100 + index * 2600,
      text,
      status: "pending",
      overlap: index === 3,
      voice_id: null,
      synthesized_ms: null,
      applied_speed: null,
      slot_ms: 2600,
      ratio: null,
      wav_path: null,
      error: null,
    })),
    last_config: null,
    output_path: null,
    created_at: now,
    updated_at: now,
    source_changed: false,
  };
}

function mockSecretIdentity(args?: InvokeArgs): string {
  const providerId = String(args?.providerId ?? "").trim();
  const endpoint = String(args?.endpoint ?? "").trim().replace(/\/+$/, "");
  const field = String(args?.field ?? "").trim();
  return `${providerId}\u0000${field}\u0000${endpoint}`;
}

function currentMockSettings(): Settings {
  mockSettingsState ??= createMockSettings();
  return mockSettingsState;
}

function createMockTtsModels(): TtsModelInfo[] {
  return [
    {
      id: "kokoro-multi-lang-v1_1",
      family: "kokoro",
      name: "Kokoro 多语 v1.1",
      description: "中英双语、103 个内置音色，原生 sherpa-onnx 离线合成",
      languages: ["zh", "en"],
      size_mb: 217,
      download_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_1.tar.bz2",
      extra_download_urls: [],
      sample_rate: 24000,
      default_voice_id: "10",
      clone_only: false,
      voices: [
        { id: "10", sid: 10, label: "中文女声 08", label_en: "Chinese Female 08", language: "zh", gender: "female" },
      ],
      status: "ready",
      path: "/Users/example/Local-LLM/tts/kokoro-multi-lang-v1_1",
      location: "external",
      missing_files: [],
    },
    {
      id: "vits-zh-aishell3",
      family: "vits",
      name: "VITS 中文 AIShell3",
      description: "174 个中文说话人，原生 sherpa-onnx 离线合成",
      languages: ["zh"],
      size_mb: 227,
      download_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-icefall-zh-aishell3.tar.bz2",
      extra_download_urls: [],
      sample_rate: 8000,
      default_voice_id: "0",
      clone_only: false,
      voices: [],
      status: "not-installed",
      path: null,
      location: null,
      missing_files: ["model.onnx", "tokens.txt", "lexicon.txt"],
    },
    {
      id: "zipvoice-distill-zh-en",
      family: "zipvoice",
      name: "ZipVoice 中英声音克隆",
      description: "本地零样本声音克隆，无内置音色；参考音频不会离开设备",
      languages: ["zh", "en"],
      size_mb: 217,
      download_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/sherpa-onnx-zipvoice-distill-int8-zh-en-emilia.tar.bz2",
      extra_download_urls: ["https://github.com/k2-fsa/sherpa-onnx/releases/download/vocoder-models/vocos_24khz.onnx"],
      sample_rate: 24000,
      default_voice_id: "",
      clone_only: true,
      voices: [],
      status: "not-installed",
      path: null,
      location: null,
      missing_files: ["encoder.int8.onnx", "decoder.int8.onnx", "vocos_24khz.onnx"],
    },
  ];
}

function currentMockTtsModels(): TtsModelInfo[] {
  mockTtsModelsState ??= createMockTtsModels();
  return mockTtsModelsState;
}

function createMockModels(): AsrModelInfo[] {
  const settings = currentMockSettings();
  const cloudSecretProvider = settings.cloud_asr_protocol === "elevenlabs"
    ? "cloud-asr-elevenlabs"
    : settings.cloud_asr_protocol === "deepgram"
      ? "cloud-asr-deepgram"
      : settings.cloud_asr_protocol === "gladia"
        ? "cloud-asr-gladia"
        : settings.cloud_asr_protocol === "volcengine"
          ? "cloud-asr-volcengine"
          : settings.cloud_asr_protocol === "tencent"
            ? "cloud-asr-tencent"
            : settings.cloud_asr_protocol === "aliyun"
              ? "cloud-asr-aliyun"
              : settings.cloud_asr_protocol === "xfyun"
                ? "cloud-asr-xfyun"
      : "cloud-asr-openai-compatible";
  const cloudSecretFields = ["tencent", "aliyun", "xfyun"].includes(settings.cloud_asr_protocol)
    ? ["accountId", "apiKey", "apiSecret"]
    : ["apiKey"];
  const cloudSecret = cloudSecretFields.every((field) => mockProviderSecrets.has(
    mockSecretIdentity({
      providerId: cloudSecretProvider,
      endpoint: settings.cloud_asr_endpoint,
      field,
    }),
  ));
  return [
    {
      id: "large-v3-turbo",
      engine_id: "whisper-cpp",
      name: "Whisper Large V3 Turbo",
      description: "速度和精度平衡较好的通用多语言模型",
      languages: ["en", "zh", "ja", "ko", "auto"],
      best_for: "general-multilingual",
      size_mb: 1500,
      download_url:
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
      status: "available",
    },
    {
      id: "small",
      engine_id: "whisper-cpp",
      name: "Whisper Small",
      description: "速度较快，占用较低，精度低于大模型",
      languages: ["en", "auto"],
      best_for: "fast-low-memory",
      size_mb: 500,
      download_url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
      status: "downloaded",
    },
    {
      id: "parakeet-tdt-0.6b-v2",
      engine_id: "parakeet-mlx",
      name: "Parakeet TDT 0.6B V2",
      description: "英文识别优化，原生 ONNX 运行时，安装后可完全离线",
      languages: ["en"],
      best_for: "english-fast",
      size_mb: 460,
      download_url:
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2",
      status: "available",
    },
    {
      id: "sensevoice-small",
      engine_id: "sensevoice",
      name: "SenseVoice Small",
      description: "中英日韩粤多语言识别，原生 sherpa-onnx int8 运行时",
      languages: ["zh", "yue", "en", "ja", "ko"],
      best_for: "chinese-cantonese",
      size_mb: 158,
      download_url:
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2",
      status: "available",
    },
    {
      id: "paraformer-zh-int8",
      engine_id: "paraformer",
      name: "Paraformer Zh Int8",
      description: "中文与川渝方言识别优化，Silero VAD 长音频分段",
      languages: ["zh", "四川话", "重庆话"],
      best_for: "chinese-dialects-fast",
      size_mb: 218,
      download_url:
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-paraformer-zh-int8-2025-10-07.tar.bz2",
      status: "available",
    },
    {
      id: "qwen3-asr-0.6b-int8",
      engine_id: "qwen3-asr",
      name: "Qwen3-ASR 0.6B Int8",
      description: "30 种语言、多种中文方言与歌声识别，原生 sherpa-onnx 运行",
      languages: ["zh", "en", "yue", "ja", "ko", "30 languages"],
      best_for: "multilingual-dialects-music",
      size_mb: 838,
      download_url:
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2",
      status: "available",
    },
    {
      id: "firered-asr2-ctc-int8",
      engine_id: "firered-asr",
      name: "FireRedASR2 CTC Int8",
      description: "中英与 20 余种中文方言/口音优化，支持 VAD 长音频",
      languages: ["zh", "en", "yue", "20+ dialects"],
      best_for: "chinese-english-dialects",
      size_mb: 496,
      download_url:
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-fire-red-asr2-ctc-zh_en-int8-2026-02-25.tar.bz2",
      status: "available",
    },
    {
      id: "openai-compatible",
      engine_id: "cloud-asr",
      name: `Cloud ASR · ${settings.cloud_asr_protocol} · ${settings.cloud_asr_model}`,
      description: "Managed cloud speech-to-text with explicit media upload consent",
      languages: ["auto", "multilingual"],
      best_for: "managed-cloud-accuracy",
      size_mb: null,
      download_url: null,
      status: settings.cloud_asr_upload_consent && cloudSecret ? "downloaded" : "not-ready",
    },
  ];
}

function createMockProviders(): TranslationProvider[] {
  return [
    {
      id: "ollama",
      name: "Ollama",
      provider_type: "ai",
      is_ai: true,
      implemented: true,
      requires_api_key: false,
      requires_endpoint: true,
      requires_model: true,
      secret_fields: [],
      default_endpoint: "http://localhost:11434",
    },
    {
      id: "deepseek",
      name: "DeepSeek",
      provider_type: "ai",
      is_ai: true,
      implemented: true,
      requires_api_key: true,
      requires_endpoint: true,
      requires_model: true,
      secret_fields: ["apiKey"],
      default_endpoint: "https://api.deepseek.com/v1",
    },
    {
      id: "deeplx",
      name: "DeepLX",
      provider_type: "api",
      is_ai: false,
      implemented: true,
      requires_api_key: false,
      requires_endpoint: true,
      requires_model: false,
      secret_fields: [],
      default_endpoint: "https://api.deeplx.org",
    },
    {
      id: "custom-openai",
      name: "Custom OpenAI",
      provider_type: "ai",
      is_ai: true,
      implemented: true,
      requires_api_key: true,
      requires_endpoint: true,
      requires_model: true,
      secret_fields: ["apiKey"],
      default_endpoint: "https://api.openai.com/v1",
    },
  ];
}

function createMockTask(): Task {
  const now = new Date().toISOString();
  return {
    id: "00000000-0000-4000-8000-000000000001",
    task_type: "generate-and-translate",
    status: "done",
    media_path: "/Users/example/Movies/demo.mp4",
    media_name: "demo.mp4",
    engine_id: "parakeet-mlx",
    model_id: "parakeet-tdt-0.6b-v2",
    source_language: "auto",
    target_language: "zh",
    translation_content_mode: "target-only",
    output_format: "srt",
    output_name: null,
    strip_chinese_punctuation: false,
    review_required: false,
    reviewed_at: null,
    progress: 1,
    status_message: "已完成",
    output_path: "/Users/example/Movies/demo.finalsub.zh.srt",
    error: null,
    created_at: now,
    updated_at: now,
  };
}

function savedSettingsFromArgs(args: InvokeArgs): Settings {
  const candidate = args?.newSettings;
  if (candidate && typeof candidate === "object") {
    return candidate as Settings;
  }
  return createMockSettings();
}

function mockInvokeResult(command: string, args?: InvokeArgs): unknown {
  switch (command) {
    case "get_app_info":
      return { name: "FinalSub", version: "1.0.10" } satisfies AppInfo;
    case "get_settings":
      return currentMockSettings();
    case "reset_settings":
      mockSettingsState = createMockSettings();
      return mockSettingsState;
    case "save_settings_cmd":
      mockSettingsState = savedSettingsFromArgs(args);
      return mockSettingsState;
    case "list_asr_models":
    case "scan_models":
      return createMockModels();
    case "list_tts_models":
      return currentMockTtsModels();
    case "register_tts_model_path": {
      const modelId = String(args?.modelId ?? "");
      mockTtsModelsState = currentMockTtsModels().map((model) => (
        model.id === modelId
          ? {
              ...model,
              status: "ready" as const,
              path: String(args?.sourcePath ?? "/Users/example/Local-LLM/tts/model"),
              location: "external" as const,
              missing_files: [],
            }
          : model
      ));
      return mockTtsModelsState.find((model) => model.id === modelId);
    }
    case "forget_tts_model_path":
      mockTtsModelsState = currentMockTtsModels().map((model) => (
        model.id === args?.modelId
          ? { ...model, status: "not-installed" as const, path: null, location: null }
          : model
      ));
      return undefined;
    case "set_tts_models_root":
      return currentMockTtsModels();
    case "synthesize_local_tts":
      return {
        output_path: String((args?.request as LocalTtsSynthesisRequest | undefined)?.output_path ?? "/Users/example/FinalSub/preview.wav"),
        sample_rate: 24000,
        duration_ms: 1460,
      } satisfies TtsSynthesisResult;
    case "cancel_local_tts":
      return true;
    case "list_tts_providers":
      return mockTtsProvidersState;
    case "save_tts_provider": {
      const request = args?.request as SaveTtsProviderRequest | undefined;
      if (!request) throw new Error("TTS provider request is missing");
      const profile: TtsProviderProfile = {
        ...request,
        id: request.id ?? `00000000-0000-4000-8000-${String(mockTtsProvidersState.length + 301).padStart(12, "0")}`,
      };
      mockTtsProvidersState = [
        profile,
        ...mockTtsProvidersState.filter((item) => item.id !== profile.id),
      ];
      return profile;
    }
    case "delete_tts_provider":
      mockTtsProvidersState = mockTtsProvidersState.filter((item) => item.id !== args?.providerId);
      return undefined;
    case "synthesize_cloud_tts":
    case "test_tts_provider":
      return {
        output_path: "/Users/example/FinalSub/tts-provider-preview.wav",
        sample_rate: 24000,
        duration_ms: 1720,
      } satisfies TtsSynthesisResult;
    case "create_dubbing_session":
      mockDubbingSessionState = createMockDubbingSession(String(args?.subtitlePath ?? "/Users/example/Subtitles/demo.srt"));
      return mockDubbingSessionState;
    case "get_dubbing_session":
      mockDubbingSessionState ??= createMockDubbingSession();
      return mockDubbingSessionState;
    case "synthesize_dubbing_cue": {
      mockDubbingSessionState ??= createMockDubbingSession();
      const request = args?.request as DubbingSynthesizeCueRequest | undefined;
      const cueIndex = request?.cue_index ?? 0;
      mockDubbingSessionState = {
        ...mockDubbingSessionState,
        last_config: request ? {
          engine: request.engine,
          voice: request.voice,
          global_speed: request.global_speed,
          reference_audio_path: request.reference_audio_path ?? null,
          reference_text: request.reference_text ?? null,
          num_steps: request.num_steps ?? null,
        } : null,
        updated_at: new Date().toISOString(),
        cues: mockDubbingSessionState.cues.map((cue) => cue.index === cueIndex ? {
          ...cue,
          status: cueIndex === 1 ? "overlong" as const : "ready" as const,
          synthesized_ms: cueIndex === 1 ? 4420 : 2200,
          applied_speed: request?.global_speed ?? 1,
          ratio: cueIndex === 1 ? 1.7 : 0.85,
          wav_path: `/Users/example/FinalSub/dubbing/cue-${cueIndex + 1}.wav`,
          error: null,
        } : cue),
      };
      return mockDubbingSessionState;
    }
    case "accept_dubbing_overflow":
      mockDubbingSessionState ??= createMockDubbingSession();
      mockDubbingSessionState = {
        ...mockDubbingSessionState,
        cues: mockDubbingSessionState.cues.map((cue) => cue.index === args?.cueIndex ? {
          ...cue,
          status: "accepted" as const,
          synthesized_ms: cue.slot_ms,
          applied_speed: cue.ratio ?? 1,
        } : cue),
      };
      return mockDubbingSessionState;
    case "export_dubbing_audio":
      mockDubbingSessionState ??= createMockDubbingSession();
      mockDubbingSessionState = {
        ...mockDubbingSessionState,
        output_path: String(args?.outputPath ?? "/Users/example/Downloads/finalsub-dubbing.wav"),
      };
      return mockDubbingSessionState;
    case "get_model_status":
      return createMockModels().find((model) => model.id === args?.modelId) ?? null;
    case "discover_batch_inputs":
      return Array.isArray(args?.paths) ? args.paths : [];
    case "list_tasks":
      return [
        {
          ...createMockTask(),
          id: "00000000-0000-4000-8000-000000000002",
          media_name: "needs-review.mp4",
          status: "review",
          review_required: true,
          reviewed_at: null,
          status_message: "等待人工审核",
        },
        createMockTask(),
      ];
    case "list_task_recipes":
      return mockTaskRecipes;
    case "save_task_recipe": {
      const request = args?.request as SaveTaskRecipeRequest | undefined;
      if (!request) throw new Error("Task recipe request is missing");
      const now = new Date().toISOString();
      const existing = request.id
        ? mockTaskRecipes.find((recipe) => recipe.id === request.id)
        : undefined;
      const recipe: TaskRecipe = {
        id: existing?.id ?? `00000000-0000-4000-8000-${String(mockTaskRecipes.length + 101).padStart(12, "0")}`,
        name: request.name,
        snapshot: request.snapshot,
        created_at: existing?.created_at ?? now,
        updated_at: now,
      };
      mockTaskRecipes = [recipe, ...mockTaskRecipes.filter((item) => item.id !== recipe.id)];
      return recipe;
    }
    case "delete_task_recipe":
      mockTaskRecipes = mockTaskRecipes.filter((recipe) => recipe.id !== args?.recipeId);
      return String(args?.recipeId ?? "");
    case "get_task_logs":
      return "[dev browser mock] Task log stream is available inside the Tauri app.";
    case "list_translation_providers":
      return createMockProviders();
    case "list_translation_models":
      return ["gpt-4.1-mini", "gpt-4o-mini", "qwen2.5:7b"];
    case "has_provider_secret":
      return mockProviderSecrets.has(mockSecretIdentity(args));
    case "check_for_update":
      return null;
    case "download_and_install_update":
      return undefined;
    case "set_provider_secret":
      mockProviderSecrets.add(mockSecretIdentity(args));
      return undefined;
    case "delete_provider_secret":
      mockProviderSecrets.delete(mockSecretIdentity(args));
      return undefined;
    case "save_proofread_tasks":
    case "authorize_subtitle_directory":
    case "delete_model":
    case "download_model":
    case "cancel_model_download":
    case "import_local_model":
    case "import_sensevoice_model":
    case "cancel_burn_subtitle":
      return undefined;
    case "import_config":
    case "import_config_from_path":
      return createMockSettings();
    case "export_config_to_path":
      return String(args?.outputPath ?? "/Users/example/Downloads/finalsub-config.json");
    case "export_encrypted_config_to_path":
      return String(args?.outputPath ?? "/Users/example/Downloads/finalsub-config.encrypted.json");
    case "import_encrypted_config_from_path":
      return createMockSettings();
    case "test_translation":
      return {
        translated_text: "你好，你怎么样？",
        provider: "dev-browser-mock",
        success: true,
      } satisfies TranslateResponse;
    case "test_translation_proxy":
      return "HTTP 200";
    case "load_proofread_tasks":
      return "[]";
    case "get_ffmpeg_version":
      return "ffmpeg dev-browser mock";
    case "get_video_metadata":
      return {
        duration_seconds: 62.4,
        duration_string: "00:01:02.400",
        width: 1920,
        height: 1080,
        fps: 30,
        codec: "h264",
        audio_codec: "aac",
        audio_sample_rate: 48000,
        audio_channels: 2,
        audio_tracks: 1,
      } satisfies VideoMetadata;
    case "list_embedded_subtitles":
      return [];
    case "extract_audio_plan":
      return {
        ffmpeg_bin: "ffmpeg-sidecar",
        args: ["-i", String(args?.videoPath ?? ""), String(args?.outputPath ?? "")],
        input: String(args?.videoPath ?? ""),
        output: String(args?.outputPath ?? ""),
      } satisfies AudioExtractPlan;
    case "extract_audio":
    case "extract_embedded_subtitle":
    case "transcribe_audio":
    case "transcribe_parakeet":
      return String(args?.outputPath ?? "/Users/example/FinalSub/output.srt");
    case "burn_subtitle":
    case "generate_subtitle_preview":
      return String(args?.req && typeof args.req === "object" && "output_path" in args.req
        ? args.req.output_path
        : "/Users/example/Movies/demo-subtitled.mp4");
    case "export_config":
      return JSON.stringify(createMockSettings(), null, 2);
    case "normalize_srt":
    case "convert_subtitle_opencc":
      return String(args?.srtContent ?? "");
    case "convert_strings_opencc":
      return Array.isArray(args?.texts) ? args.texts : [];
    case "create_task":
    case "create_preview_task":
    case "cancel_task":
    case "pause_task":
    case "resume_task":
    case "retry_task":
      return createMockTask();
    case "approve_task":
      return {
        ...createMockTask(),
        id: String(args?.taskId ?? createMockTask().id),
        review_required: true,
        reviewed_at: new Date().toISOString(),
        status_message: "审核通过",
      };
    case "approve_tasks":
      return Array.isArray(args?.taskIds)
        ? args.taskIds.map((taskId) => ({
            ...createMockTask(),
            id: String(taskId),
            review_required: true,
            reviewed_at: new Date().toISOString(),
            status_message: "审核通过",
          }))
        : [];
    case "create_tasks":
      return Array.isArray(args?.requests)
        ? args.requests.map((request, index) => ({
            ...createMockTask(),
            id: `00000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`,
            media_path: typeof request === "object" && request && "media_path" in request
              ? String(request.media_path)
              : createMockTask().media_path,
          }))
        : [];
    case "delete_task":
      return String(args?.taskId ?? "");
    case "delete_tasks":
      return Array.isArray(args?.taskIds) ? args.taskIds : [];
    default:
      throw new Error(`Tauri command "${command}" is unavailable in browser preview.`);
  }
}
