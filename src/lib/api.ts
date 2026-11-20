// Typed wrappers around the Rust commands and events in src-tauri.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Preset = "standard" | "high" | "maximum";
export type AudioTrack = "system" | "microphone";

export interface Display {
  index: number;
  name: string;
  width: number;
  height: number;
  refresh_rate: number;
}

export interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
}

export interface Summary {
  path: string;
  duration: { secs: number; nanos: number };
  frames: number;
  dropped_frames: number;
  bytes: number;
  audio_tracks: AudioTrack[];
  warnings: string[];
}

export type RecorderState =
  | { status: "idle"; last: Summary | null; error: string | null }
  | {
      status: "recording";
      elapsed_ms: number;
      at_ms: number;
      paused: boolean;
      path: string;
      tracks: AudioTrack[];
      warnings: string[];
    }
  | { status: "starting" }
  | { status: "stopping" };

export interface Selection {
  display: number;
  preset: Preset;
  system_audio: boolean;
  microphone: boolean;
  microphone_id: string | null;
}

export interface UiInfo {
  mica: boolean;
  hotkey: string;
  pause_hotkey: string;
}

export const api = {
  uiInfo: () => invoke<UiInfo>("ui_info"),
  displays: () => invoke<Display[]>("list_displays"),
  microphones: () => invoke<AudioDevice[]>("list_microphones"),
  levels: () => invoke<[number, number]>("get_levels"),
  state: () => invoke<RecorderState>("get_state"),
  selection: () => invoke<Selection>("get_selection"),
  setSelection: (selection: Selection) => invoke<void>("set_selection", { selection }),
  start: () => invoke<void>("start_recording"),
  stop: () => invoke<void>("stop_recording"),
  togglePause: () => invoke<void>("toggle_pause"),
  openPath: (path: string, reveal: boolean) => invoke<void>("open_path", { path, reveal }),
  openFolder: () => invoke<void>("open_recordings_folder"),
  hide: () => invoke<void>("hide_window"),
  onState: (cb: (s: RecorderState) => void): Promise<UnlistenFn> =>
    listen<RecorderState>("recorder-state", (e) => cb(e.payload)),
};

export const presetLabels: Record<Preset, string> = {
  standard: "Standard · 30 fps",
  high: "High · 60 fps",
  maximum: "Maximum · 60 fps HEVC",
};

export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = String(m).padStart(h ? 2 : 1, "0");
  const ss = String(s).padStart(2, "0");
  return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${Math.round(bytes / 1e3)} KB`;
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/// Peak (0–1) → meter fill (0–1) on a -60…0 dB scale, which matches how loud things sound.
export function meterFill(peak: number): number {
  if (peak <= 0.001) return 0;
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db + 60) / 60));
}
