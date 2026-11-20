<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    fileName,
    formatBytes,
    formatClock,
    meterFill,
    presetLabels,
    type AudioDevice,
    type Display,
    type Preset,
    type RecorderState,
    type Selection,
  } from "./lib/api";

  let rec = $state<RecorderState>({ status: "idle", last: null, error: null });
  let displays = $state<Display[]>([]);
  let mics = $state<AudioDevice[]>([]);
  let selection = $state<Selection>({
    display: 1,
    preset: "high",
    system_audio: true,
    microphone: false,
    microphone_id: null,
  });
  let hotkey = $state("Ctrl+Alt+R");
  let pauseHotkey = $state("Ctrl+Alt+P");
  let now = $state(Date.now());
  let busy = $state(false);
  let actionError = $state<string | null>(null);
  let levels = $state({ system: 0, microphone: 0 });

  const elapsed = $derived(
    rec.status === "recording" ? rec.elapsed_ms + (rec.paused ? 0 : Math.max(0, now - rec.at_ms)) : 0,
  );
  const error = $derived(actionError ?? (rec.status === "idle" ? rec.error : null));
  const recording = $derived(rec.status === "recording" ? rec : null);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    const clock = setInterval(() => (now = Date.now()), 250);

    // Level meters: poll while recording, with a gentle fall-off.
    const meter = setInterval(async () => {
      if (rec.status !== "recording" || rec.paused) {
        levels = { system: 0, microphone: 0 };
        return;
      }
      const [s, m] = await api.levels();
      levels = {
        system: Math.max(meterFill(s), levels.system * 0.8),
        microphone: Math.max(meterFill(m), levels.microphone * 0.8),
      };
    }, 80);

    (async () => {
      const info = await api.uiInfo();
      document.documentElement.classList.toggle("mica", info.mica);
      hotkey = info.hotkey;
      pauseHotkey = info.pause_hotkey;
      [displays, selection, rec] = await Promise.all([api.displays(), api.selection(), api.state()]);
      mics = await api.microphones().catch(() => []);
      unlisten = await api.onState((s) => {
        rec = s;
        busy = false;
      });
    })().catch((e) => (actionError = String(e)));

    return () => {
      clearInterval(clock);
      clearInterval(meter);
      unlisten?.();
    };
  });

  async function updateSelection(patch: Partial<Selection>) {
    selection = { ...selection, ...patch };
    await api.setSelection(selection);
  }

  async function run(action: () => Promise<void>) {
    actionError = null;
    busy = true;
    try {
      await action();
    } catch (e) {
      actionError = String(e);
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && rec.status === "idle") api.hide();
  }
</script>

<svelte:window onkeydown={onKey} />

{#snippet toggle(checked: boolean, label: string, onchange: (v: boolean) => void)}
  <button
    role="switch"
    aria-checked={checked}
    aria-label={label}
    class="relative h-5 w-10 shrink-0 rounded-full border transition-colors {checked
      ? 'border-accent bg-accent'
      : 'border-muted/60 bg-transparent'}"
    onclick={() => onchange(!checked)}
  >
    <span
      class="absolute top-1/2 size-3 -translate-y-1/2 rounded-full transition-all {checked
        ? 'left-[22px] bg-on-accent'
        : 'left-[3px] bg-muted'}"
    ></span>
  </button>
{/snippet}

{#snippet meter(fill: number, label: string)}
  <span class="h-4 w-1 overflow-hidden rounded-full bg-fg/15" title={label} aria-hidden="true">
    <span class="block w-full rounded-full bg-fg/70" style="height: {fill * 100}%; margin-top: {(1 - fill) * 100}%"></span>
  </span>
{/snippet}

{#if rec.status === "idle"}
  <main class="flex h-screen flex-col">
    <!-- Title bar: drag to move -->
    <header data-tauri-drag-region class="flex h-11 shrink-0 items-center gap-2 pr-1.5 pl-4">
      <span class="pointer-events-none size-2.5 rounded-full bg-record"></span>
      <h1 class="pointer-events-none text-[13px] font-semibold">Screen Recorder</h1>
      <button
        class="ml-auto grid size-8 place-items-center rounded-md text-muted hover:bg-control-hover"
        aria-label="Hide to tray"
        title="Hide to tray (Esc)"
        onclick={() => api.hide()}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" />
        </svg>
      </button>
    </header>

    <div class="flex flex-1 flex-col gap-3 px-4 pb-4">
      <!-- Capture mode -->
      <div class="grid grid-cols-3 gap-1 rounded-lg border border-stroke bg-control p-1" role="radiogroup" aria-label="Capture mode">
        <button role="radio" aria-checked="true" class="h-8 rounded-md bg-accent text-[13px] font-medium text-on-accent">
          Screen
        </button>
        <button role="radio" aria-checked="false" disabled class="h-8 rounded-md text-[13px] text-muted opacity-60" title="Coming soon">
          Window
        </button>
        <button role="radio" aria-checked="false" disabled class="h-8 rounded-md text-[13px] text-muted opacity-60" title="Coming soon">
          Region
        </button>
      </div>

      <!-- Video -->
      <div class="flex flex-col divide-y divide-stroke rounded-lg border border-stroke bg-control">
        <label class="flex h-11 items-center gap-3 px-3">
          <span class="w-20 text-muted">Display</span>
          <select
            class="h-8 min-w-0 flex-1 cursor-pointer rounded-md bg-transparent px-1 hover:bg-control-hover"
            value={selection.display}
            onchange={(e) => updateSelection({ display: Number(e.currentTarget.value) })}
          >
            {#each displays as d (d.index)}
              <option value={d.index}>{d.index} · {d.name || "Display"} · {d.width}×{d.height}</option>
            {/each}
          </select>
        </label>
        <label class="flex h-11 items-center gap-3 px-3">
          <span class="w-20 text-muted">Quality</span>
          <select
            class="h-8 min-w-0 flex-1 cursor-pointer rounded-md bg-transparent px-1 hover:bg-control-hover"
            value={selection.preset}
            onchange={(e) => updateSelection({ preset: e.currentTarget.value as Preset })}
          >
            {#each Object.entries(presetLabels) as [value, label] (value)}
              <option {value}>{label}</option>
            {/each}
          </select>
        </label>
      </div>

      <!-- Audio -->
      <div class="flex flex-col divide-y divide-stroke rounded-lg border border-stroke bg-control">
        <div class="flex h-11 items-center gap-3 px-3">
          <span class="flex-1">System audio</span>
          {@render toggle(selection.system_audio, "System audio", (v) => updateSelection({ system_audio: v }))}
        </div>
        <div class="flex h-11 items-center gap-3 px-3">
          <span class="shrink-0">Microphone</span>
          {#if selection.microphone}
            <select
              class="h-8 min-w-0 flex-1 cursor-pointer truncate rounded-md bg-transparent px-1 text-[13px] text-muted hover:bg-control-hover"
              aria-label="Microphone device"
              value={selection.microphone_id ?? ""}
              onchange={(e) => updateSelection({ microphone_id: e.currentTarget.value || null })}
            >
              <option value="">Windows default</option>
              {#each mics as m (m.id)}
                <option value={m.id}>{m.name}</option>
              {/each}
            </select>
          {:else}
            <span class="flex-1"></span>
          {/if}
          {@render toggle(selection.microphone, "Microphone", (v) => updateSelection({ microphone: v }))}
        </div>
      </div>

      <!-- Last recording -->
      {#if rec.last}
        {@const last = rec.last}
        <div class="rounded-lg border border-stroke bg-control p-3">
          <p class="truncate text-[13px] font-medium" title={last.path}>{fileName(last.path)}</p>
          <p class="mt-0.5 text-xs text-muted">
            {formatClock(last.duration.secs * 1000)} · {formatBytes(last.bytes)}
            {#if last.audio_tracks.length}
              · {last.audio_tracks.length === 1 ? "1 audio track" : `${last.audio_tracks.length} audio tracks`}
            {/if}
          </p>
          {#each last.warnings as w (w)}
            <p class="mt-1 text-xs text-paused">{w}</p>
          {/each}
          <div class="mt-2 flex gap-2">
            <button class="h-8 rounded-md border border-stroke bg-raised px-3 text-[13px] hover:bg-control-hover" onclick={() => api.openPath(last.path, false)}>
              Open
            </button>
            <button class="h-8 rounded-md border border-stroke bg-raised px-3 text-[13px] hover:bg-control-hover" onclick={() => api.openPath(last.path, true)}>
              Show in folder
            </button>
          </div>
        </div>
      {/if}

      {#if error}
        <p class="rounded-md border border-record/40 px-3 py-2 text-[13px] text-record" role="alert">{error}</p>
      {/if}

      <!-- Start -->
      <div class="mt-auto flex flex-col items-center gap-2">
        <button
          class="flex h-11 w-full items-center justify-center gap-2 rounded-lg bg-record font-semibold text-white transition-[filter] hover:brightness-110 disabled:opacity-60"
          disabled={busy || displays.length === 0}
          onclick={() => run(api.start)}
        >
          <span class="size-2.5 rounded-full bg-white"></span>
          Start recording
        </button>
        <button class="text-xs text-muted hover:text-fg" onclick={() => api.openFolder()}>
          {hotkey} start/stop · {pauseHotkey} pause · Open folder
        </button>
      </div>
    </div>
  </main>
{:else}
  <!-- Compact control pill while recording. Excluded from the capture. -->
  <main data-tauri-drag-region class="flex h-screen items-center gap-2.5 pr-2 pl-4">
    {#if recording}
      <span
        class="pointer-events-none size-2.5 shrink-0 rounded-full {recording.paused ? 'bg-paused' : 'rec-dot bg-record'}"
      ></span>
      <span class="pointer-events-none font-semibold tabular-nums" aria-live="off">{formatClock(elapsed)}</span>
      {#if recording.paused}
        <span class="pointer-events-none text-xs text-paused">Paused</span>
      {:else}
        <span class="pointer-events-none flex items-end gap-0.5">
          {#if recording.tracks.includes("system")}{@render meter(levels.system, "System audio")}{/if}
          {#if recording.tracks.includes("microphone")}{@render meter(levels.microphone, "Microphone")}{/if}
        </span>
      {/if}
      <button
        class="ml-auto grid size-9 place-items-center rounded-md hover:bg-control-hover"
        aria-label={recording.paused ? "Resume" : "Pause"}
        title="{recording.paused ? 'Resume' : 'Pause'} ({pauseHotkey})"
        onclick={() => run(api.togglePause)}
      >
        {#if recording.paused}
          <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true"><path d="M3 1.5v11l9-5.5z" fill="currentColor" /></svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
            <rect x="2.5" y="1.5" width="3" height="11" rx="1" fill="currentColor" />
            <rect x="8.5" y="1.5" width="3" height="11" rx="1" fill="currentColor" />
          </svg>
        {/if}
      </button>
      <button
        class="flex h-9 items-center gap-2 rounded-md bg-record px-3 text-[13px] font-semibold text-white hover:brightness-110 disabled:opacity-60"
        disabled={busy}
        title="Stop ({hotkey})"
        onclick={() => run(api.stop)}
      >
        <span class="size-2.5 rounded-[2px] bg-white"></span>
        Stop
      </button>
    {:else}
      <span class="rec-dot pointer-events-none size-2.5 rounded-full bg-muted"></span>
      <span class="pointer-events-none text-[13px] text-muted">
        {rec.status === "starting" ? "Starting…" : "Saving recording…"}
      </span>
    {/if}
  </main>
{/if}
