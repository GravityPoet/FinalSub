import { invoke } from "@tauri-apps/api/core";

export const TASK_UPDATED_EVENT = "task-updated";

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
  output_format: string;
  progress: number;
  status_message: string;
  output_path: string | null;
  error: string | null;
  created_at: string;
  updated_at: string;
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

export async function deleteModel(modelId: string): Promise<void> {
  return invoke("delete_model", { modelId });
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
  output_format?: string;
}

export async function createTask(req: CreateTaskRequest): Promise<Task> {
  return invoke("create_task", { req });
}

export async function createPreviewTask(mediaPath: string): Promise<Task> {
  return invoke("create_preview_task", { mediaPath });
}

export async function listTasks(): Promise<Task[]> {
  return invoke("list_tasks");
}

export async function cancelTask(taskId: string): Promise<Task> {
  return invoke("cancel_task", { taskId });
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
  font_size?: number;
  font_color?: string;
  outline_color?: string;
  margin_v?: number;
}

export async function burnSubtitle(req: BurnSubtitleRequest): Promise<string> {
  return invoke("burn_subtitle", { req });
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

export async function testTranslation(req: TranslateRequest): Promise<TranslateResponse> {
  return invoke("test_translation", { req });
}

export interface Settings {
  language: string;
  asr_engine: string;
  models_path: string;
  max_concurrent_tasks: number;
  subtitle_output_format: string;
  source_language: string;
  target_language: string;
  translate_provider: string;
  translate_endpoints: Record<string, string>;
  translate_models: Record<string, string>;
  translate_retry_times: number;
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

export async function setProviderSecret(providerId: string, field: string, value: string): Promise<void> {
  return invoke("set_provider_secret", { providerId, field, value });
}

export async function getProviderSecret(providerId: string, field: string): Promise<string> {
  return invoke("get_provider_secret", { providerId, field });
}

export async function deleteProviderSecret(providerId: string, field: string): Promise<void> {
  return invoke("delete_provider_secret", { providerId, field });
}

export async function loadProofreadTasks(): Promise<string> {
  return invoke("load_proofread_tasks");
}

export async function saveProofreadTasks(data: string): Promise<void> {
  return invoke("save_proofread_tasks", { data });
}

export async function fsReadDir(dirPath: string): Promise<string[]> {
  return invoke("fs_read_dir", { dirPath });
}

export async function fsExists(filePath: string): Promise<boolean> {
  return invoke("fs_exists", { filePath });
}

export async function fsReadText(filePath: string): Promise<string> {
  return invoke("fs_read_text", { filePath });
}

export async function fsWriteText(filePath: string, content: string): Promise<void> {
  return invoke("fs_write_text", { filePath, content });
}
