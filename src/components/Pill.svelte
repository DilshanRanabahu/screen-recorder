<!-- Compact always-on-top bar while recording. Excluded from the capture. -->
<script lang="ts">
  import { api, dbToFill, formatClock, toDb, type RecorderState } from "../lib/api";
  import Icon from "./Icon.svelte";

  let {
    rec,
    elapsed,
    levels,
    hotkey,
    pauseHotkey,
    run,
  }: {
    rec: RecorderState;
    elapsed: number;
    levels: { system: number; microphone: number };
    hotkey: string;
    pauseHotkey: string;
    run: (action: () => Promise<void>) => void;
  } = $props();

  const recording = $derived(rec.status === "recording" ? rec : null);
</script>

<main data-tauri-drag-region class="flex h-screen items-center gap-3 border border-line-strong bg-panel pr-2 pl-4">
  {#if recording}
    <span class="pointer-events-none size-2.5 shrink-0 rounded-full {recording.paused ? 'bg-paused' : 'rec-pulse bg-record'}"></span>
    <span class="pointer-events-none font-mono text-[15px] font-semibold tabular-nums {recording.paused ? 'text-paused' : ''}">{formatClock(elapsed)}</span>

    <!-- mini meters -->
    <span class="pointer-events-none flex h-5 items-end gap-[3px]" aria-hidden="true">
      {#each [["system", levels.system], ["microphone", levels.microphone]] as [track, peak] (track)}
        {#if recording.tracks.includes(track as "system" | "microphone")}
          <span class="relative h-full w-[5px] overflow-hidden rounded-full bg-white/10">
            <span
              class="absolute bottom-0 w-full rounded-full {dbToFill(toDb(peak as number)) > 0.85 ? 'bg-[var(--meter-red)]' : dbToFill(toDb(peak as number)) > 0.67 ? 'bg-[var(--meter-yellow)]' : 'bg-[var(--meter-green)]'}"
              style="height: {recording.paused ? 0 : dbToFill(toDb(peak as number)) * 100}%"
            ></span>
          </span>
        {/if}
      {/each}
    </span>

    <div class="ml-auto flex items-center gap-1">
      <button
        class="grid size-9 place-items-center rounded-lg hover:bg-hover {recording.paused ? 'text-paused' : ''}"
        aria-label={recording.paused ? "Resume" : "Pause"}
        title="{recording.paused ? 'Resume' : 'Pause'} ({pauseHotkey})"
        onclick={() => run(api.togglePause)}
      >
        <Icon name={recording.paused ? "play" : "pause"} size={16} fill={recording.paused} />
      </button>
      <button
        class="flex h-9 items-center gap-2 rounded-lg bg-record px-3 text-[13px] font-semibold text-white hover:brightness-110"
        title="Stop ({hotkey})"
        onclick={() => run(api.stop)}
      >
        <span class="size-2.5 rounded-[2px] bg-white"></span>Stop
      </button>
      <button class="grid size-9 place-items-center rounded-lg text-muted hover:bg-hover hover:text-fg" aria-label="Open studio" title="Open studio" onclick={() => api.setView("studio")}>
        <Icon name="expand" size={15} />
      </button>
    </div>
  {:else}
    <span class="spin pointer-events-none size-3.5 rounded-full border-2 border-white/30 border-t-white"></span>
    <span class="pointer-events-none text-muted">{rec.status === "starting" ? "Starting…" : "Saving recording…"}</span>
  {/if}
</main>
