// Typed wrappers around the Rust commands and events in src-tauri.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Preset = "standard" | "high" | "maximum";
export type AudioTrack = "system" | "microphone";
export type View = "studio" | "pill";

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

export interface RecordingFile {
  name: string;
  path: string;
  bytes: number;
  modified_ms: number;
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
  /** Live preview in the studio (off saves CPU). */
  preview: boolean;
}

export interface UiInfo {
  hotkey: string;
  pause_hotkey: string;
  output_dir: string;
}

export const api = {
  uiInfo: () => invoke<UiInfo>("ui_info"),
  displays: () => invoke<Display[]>("list_displays"),
  microphones: () => invoke<AudioDevice[]>("list_microphones"),
  levels: () => invoke<[number, number]>("get_levels"),
  setMonitoring: (on: boolean) => invoke<void>("set_monitoring", { on }),
  recordings: (limit: number) => invoke<RecordingFile[]>("list_recordings", { limit }),
  storageFree: () => invoke<number | null>("storage_free"),
  onBattery: () => invoke<boolean>("on_battery"),
  state: () => invoke<RecorderState>("get_state"),
  selection: () => invoke<Selection>("get_selection"),
  setSelection: (selection: Selection) => invoke<void>("set_selection", { selection }),
  start: () => invoke<void>("start_recording"),
  stop: () => invoke<void>("stop_recording"),
  togglePause: () => invoke<void>("toggle_pause"),
  view: () => invoke<View>("get_view"),
  setView: (view: View) => invoke<void>("set_view", { view }),
  openPath: (path: string, reveal: boolean) => invoke<void>("open_path", { path, reveal }),
  openFolder: () => invoke<void>("open_recordings_folder"),
  hide: () => invoke<void>("hide_window"),
  minimize: () => invoke<void>("minimize_window"),
  toggleMaximize: () => invoke<void>("toggle_maximize"),

  /**
   * Streams preview frames of a display; call `previewAck` after drawing each one.
   * Resolves to the session id. Each message:
   * `[encoding u8: 0 = JPEG, 1 = RGBA][width u32 LE][height u32 LE][session id u32 LE][data…]`.
   */
  startPreview(display: number, maxWidth: number, fps: number, raw: boolean, onFrame: (msg: unknown) => void) {
    const channel = new Channel<unknown>();
    channel.onmessage = onFrame;
    return invoke<number>("start_preview", { display, maxWidth, fps, raw, channel });
  },
  /** Stops preview session `id`; a stop for an older session is ignored. */
  stopPreview: (id: number) => invoke<void>("stop_preview_session", { id }),
  previewAck: (id: number) => invoke<void>("preview_ack", { id }),
  setPreviewFps: (fps: number) => invoke<void>("set_preview_fps", { fps }),

  onState: (cb: (s: RecorderState) => void): Promise<UnlistenFn> =>
    listen<RecorderState>("recorder-state", (e) => cb(e.payload)),
  onView: (cb: (v: View) => void): Promise<UnlistenFn> => listen<View>("view", (e) => cb(e.payload)),
  /** Fired (debounced) when files change in the recordings folder, from inside or outside the app. */
  /** Fired when microphones are plugged in or out, or the Windows default changes. */
  onAudioDevicesChanged: (cb: () => void): Promise<UnlistenFn> => listen("audio-devices-changed", () => cb()),
  onRecordingsChanged: (cb: () => void): Promise<UnlistenFn> => listen("recordings-changed", () => cb()),
};

// Mirrors QualityPreset in crates/recorder-core/src/options.rs.
export const presets: Record<Preset, { label: string; fps: number; codec: string; bpp: number; hint: string }> = {
  standard: { label: "Standard", fps: 30, codec: "H.264", bpp: 0.16, hint: "Meetings · small files" },
  high: { label: "High", fps: 60, codec: "H.264", bpp: 0.13, hint: "Tutorials · demos" },
  maximum: { label: "Maximum", fps: 60, codec: "HEVC", bpp: 0.17, hint: "Archive · not crash-safe" },
};

/** Video bitrate in bits/s, same formula as the engine. */
export function bitrate(preset: Preset, width: number, height: number): number {
  const p = presets[preset];
  const bps = width * height * p.fps * p.bpp;
  return Math.min(80e6, Math.max(2e6, bps));
}

export function formatClock(ms: number, alwaysHours = false): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h || alwaysHours ? `${pad(h)}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1e12) return `${(bytes / 1e12).toFixed(1)} TB`;
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  return `${Math.round(bytes / 1e3)} KB`;
}

export function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** "Today 17:43", "Yesterday 09:12" or "20 Nov 17:43". */
export function formatWhen(ms: number): string {
  const d = new Date(ms);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const day = new Date(d).setHours(0, 0, 0, 0);
  const today = new Date().setHours(0, 0, 0, 0);
  if (day === today) return `Today ${time}`;
  if (day === today - 86_400_000) return `Yesterday ${time}`;
  return `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
}

/** Peak (0–1) → dBFS. */
export function toDb(peak: number): number {
  return peak <= 0.000_001 ? -Infinity : 20 * Math.log10(peak);
}

/** dBFS → meter position 0–1 on a -60…0 dB scale. */
export function dbToFill(db: number): number {
  return Number.isFinite(db) ? Math.min(1, Math.max(0, (db + 60) / 60)) : 0;
}
