<script lang="ts">
  import { api, bitrate, formatBytes, formatClock, presets, type Display, type RecorderState, type Selection, type WindowInfo } from "../lib/api";
  import Icon from "./Icon.svelte";

  let {
    rec,
    elapsed,
    selection,
    display,
    window,
    freeBytes,
    hotkey,
    pauseHotkey,
    run,
  }: {
    rec: RecorderState;
    elapsed: number;
    selection: Selection;
    display: Display | undefined;
    /** Set in Window mode: the video size is the window size. */
    window?: WindowInfo;
    freeBytes: number | null;
    hotkey: string;
    pauseHotkey: string;
    run: (action: () => Promise<void>) => void;
  } = $props();

  const recording = $derived(rec.status === "recording" ? rec : null);
  const busy = $derived(rec.status === "starting" || rec.status === "stopping");
  const preset = $derived(presets[selection.preset]);
  const source = $derived(window ?? display);
  const w = $derived((source?.width ?? 1920) & ~1);
  const h = $derived((source?.height ?? 1080) & ~1);
  const videoBps = $derived(bitrate(selection.preset, w, h));
  const audioTracks = $derived((selection.system_audio ? 1 : 0) + (selection.microphone ? 1 : 0));
  const bytesPerHour = $derived(((videoBps + audioTracks * 192_000) / 8) * 3600);
  const hoursLeft = $derived(freeBytes ? freeBytes / bytesPerHour : null);
</script>

<footer class="flex h-[76px] shrink-0 items-center gap-5 border-t border-line bg-panel px-5">
  <!-- Record / stop -->
  <button
    class="group flex h-12 items-center gap-3 rounded-full pr-5 pl-1.5 font-display text-[14px] font-semibold transition-colors disabled:opacity-60 {recording
      ? 'bg-white/[0.06] hover:bg-white/10'
      : 'bg-record-soft text-fg hover:bg-record/25'}"
    disabled={busy}
    title="{recording ? 'Stop' : 'Start'} recording ({hotkey})"
    onclick={() => run(recording ? api.stop : api.start)}
  >
    <span class="grid size-9 place-items-center rounded-full bg-record shadow-[0_0_0_4px_rgb(255_77_94/0.18)]">
      {#if busy}
        <span class="spin size-4 rounded-full border-2 border-white/40 border-t-white"></span>
      {:else if recording}
        <span class="size-3 rounded-[3px] bg-white"></span>
      {:else}
        <span class="size-3.5 rounded-full bg-white"></span>
      {/if}
    </span>
    {rec.status === "starting" ? "Starting…" : rec.status === "stopping" ? "Saving…" : recording ? "Stop" : "Start recording"}
  </button>

  <!-- Pause -->
  <button
    class="grid size-11 place-items-center rounded-full border border-line-strong text-fg transition-colors hover:bg-hover disabled:opacity-30 {recording?.paused
      ? 'border-paused/60 text-paused'
      : ''}"
    disabled={!recording}
    aria-label={recording?.paused ? "Resume" : "Pause"}
    title="{recording?.paused ? 'Resume' : 'Pause'} ({pauseHotkey})"
    onclick={() => run(api.togglePause)}
  >
    <Icon name={recording?.paused ? "play" : "pause"} size={18} fill={!!recording?.paused} />
  </button>

  <!-- Timer -->
  <div class="flex flex-col">
    <span class="font-mono text-[26px] leading-none font-semibold tabular-nums {recording?.paused ? 'text-paused' : recording ? 'text-fg' : 'text-faint'}">
      {formatClock(elapsed, true)}
    </span>
    <span class="mt-1 text-[11px] text-faint">{recording ? (recording.paused ? "Paused" : "Recording") : `${hotkey} to start · ${pauseHotkey} to pause`}</span>
  </div>

  <!-- Stats -->
  <div class="ml-auto flex items-center gap-2">
    {#each [`${w}×${h}`, `${preset.fps} fps`, preset.codec, `${(videoBps / 1e6).toFixed(0)} Mbps`] as chip (chip)}
      <span class="rounded-md border border-line bg-raised px-2 py-1 font-mono text-[11px] text-muted">{chip}</span>
    {/each}
    <div class="ml-2 flex items-center gap-2 border-l border-line pl-4 text-muted">
      <Icon name="disk" size={16} />
      <div class="leading-tight">
        <p class="text-[12px] text-fg">{freeBytes != null ? `${formatBytes(freeBytes)} free` : "—"}</p>
        <p class="text-[11px] text-faint">
          ≈{formatBytes(bytesPerHour)}/h{hoursLeft != null ? ` · ${hoursLeft >= 100 ? "100+" : hoursLeft.toFixed(0)} h left` : ""}
        </p>
      </div>
    </div>
    {#if recording}
      <button class="ml-2 grid size-9 place-items-center rounded-lg text-muted hover:bg-hover hover:text-fg" title="Compact view" aria-label="Compact view" onclick={() => api.setView("pill")}>
        <Icon name="compact" size={16} />
      </button>
    {/if}
  </div>
</footer>
