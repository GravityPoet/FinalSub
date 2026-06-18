import { invoke } from "@tauri-apps/api/core";

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
  language: string | null;
  progress: number;
  status_message: string;
  output_path: string | null;
  error: string | null;
  created_at: string;
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

export async function getModelStatus(modelId: string): Promise<AsrModelInfo | null> {
  return invoke("get_model_status", { modelId });
}

export async function createTask(
  mediaPath: string,
  mediaName: string,
  engineId: string,
  modelId: string,
  language?: string
): Promise<Task> {
  return invoke("create_task", { mediaPath, mediaName, engineId, modelId, language });
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
  ffmpegBin: string,
  videoPath: string,
  outputPath: string
): Promise<AudioExtractPlan> {
  return invoke("extract_audio_plan", { ffmpegBin, videoPath, outputPath });
}

export async function getFfmpegVersion(): Promise<string> {
  return invoke("get_ffmpeg_version");
}
