<script lang="ts">
  import {
    api,
    formatBytes,
    formatWhen,
    presets,
    type CaptureMode,
    type Display,
    type Preset,
    type RecorderState,
    type RecordingFile,
    type Selection,
    type WindowInfo,
  } from "../lib/api";
  import type { IconName } from "../lib/icons";
  import Icon from "./Icon.svelte";

  let {
    selection,
    displays,
    windows,
    refreshWindows,
    recent,
    rec,
    update,
    open,
  }: {
    selection: Selection;
    displays: Display[];
    /** Open windows, refreshed while Window mode is shown. */
    windows: WindowInfo[];
    refreshWindows: () => void;
    recent: RecordingFile[];
    rec: RecorderState;
    update: (patch: Partial<Selection>) => void;
    /** Opens a recording (or shows it in Explorer); reports missing files. */
    open: (path: string, reveal: boolean) => void;
  } = $props();

  const locked = $derived(rec.status !== "idle");
  const modes: { name: string; mode: CaptureMode | null; icon: IconName }[] = [
    { name: "Screen", mode: "screen", icon: "monitor" },
    { name: "Window", mode: "window", icon: "window" },
    { name: "Region", mode: null, icon: "region" },
  ];
  // The chosen window, if it is still open.
  const chosenWindow = $derived(windows.find((w) => w.id === selection.window?.id));

  function pickWindow(id: number) {
    const w = windows.find((x) => x.id === id);
    update({ window: w ? { id: w.id, title: w.title, process: w.process } : null });
  }
</script>

{#snippet heading(text: string)}
  <h2 class="mb-2.5 text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">{text}</h2>
{/snippet}

<!-- The panel itself only scrolls as a fallback in very short windows. -->
<aside class="scroll-list flex min-h-0 w-[300px] shrink-0 flex-col gap-3 overflow-y-auto">
  <!-- Source -->
  <section class="rounded-xl border border-line bg-panel p-4">
    {@render heading("Capture")}
    <div class="grid grid-cols-3 gap-1.5" role="radiogroup" aria-label="Capture mode">
      {#each modes as m (m.name)}
        {@const active = m.mode === selection.capture_mode}
        <button
          role="radio"
          aria-checked={active}
          disabled={!m.mode || locked}
          title={m.mode ? m.name : `${m.name}: coming soon`}
          class="relative flex flex-col items-center gap-1.5 rounded-lg border py-2.5 text-xs transition-colors disabled:cursor-default {active
            ? 'border-accent/60 bg-accent-soft text-fg'
            : m.mode
              ? 'border-line text-muted hover:border-line-strong hover:bg-raised hover:text-fg'
              : 'border-line text-faint'} {locked && !active ? 'opacity-50' : ''}"
          onclick={() => m.mode && update({ capture_mode: m.mode })}
        >
          <Icon name={m.icon} size={18} />
          {m.name}
          {#if !m.mode}
            <span class="absolute -top-1.5 right-1 rounded bg-raised px-1 text-[9px] text-muted">SOON</span>
          {/if}
        </button>
      {/each}
    </div>

    {#if selection.capture_mode === "window"}
      <div class="mt-3">
        <div class="mb-1 flex items-center justify-between text-[11px] text-faint">
          <label for="window-picker">Window</label>
          <button class="text-accent hover:underline disabled:opacity-50" disabled={locked} onclick={refreshWindows}>Refresh</button>
        </div>
        <select
          id="window-picker"
          class="h-9 w-full cursor-pointer truncate rounded-lg border border-line bg-raised px-3 hover:border-line-strong disabled:cursor-default disabled:opacity-60"
          disabled={locked}
          value={chosenWindow?.id ?? ""}
          onfocus={refreshWindows}
          onchange={(e) => pickWindow(Number(e.currentTarget.value))}
        >
          {#if !chosenWindow}
            <option value="" disabled>{selection.window ? `${selection.window.title} (closed)` : "Choose a window…"}</option>
          {/if}
          {#each windows as w (w.id)}
            <option value={w.id}>{w.title} — {w.process.replace(/\.exe$/i, "")}{w.minimized ? " (minimized)" : ""}</option>
          {/each}
        </select>
        {#if chosenWindow?.minimized}
          <p class="mt-1 text-[11px] text-paused">Minimized: it records once you restore it.</p>
        {:else if chosenWindow}
          <p class="mt-1 text-[11px] text-faint">Records only this window, even when it's covered or moved.</p>
        {/if}
      </div>
    {:else}
      <label class="mt-3 block">
        <span class="mb-1 block text-[11px] text-faint">Display</span>
        <select
          class="h-9 w-full cursor-pointer rounded-lg border border-line bg-raised px-3 hover:border-line-strong disabled:cursor-default disabled:opacity-60"
          disabled={locked}
          value={selection.display}
          onchange={(e) => update({ display: Number(e.currentTarget.value) })}
        >
          {#each displays as d (d.index)}
            <option value={d.index}>Display {d.index}{d.name ? ` · ${d.name}` : ""} · {d.width}×{d.height}</option>
          {/each}
        </select>
      </label>
    {/if}
  </section>

  <!-- Quality -->
  <section class="rounded-xl border border-line bg-panel p-4">
    {@render heading("Quality")}
    <div class="flex flex-col gap-1.5" role="radiogroup" aria-label="Quality preset">
      {#each Object.entries(presets) as [key, p] (key)}
        {@const active = selection.preset === key}
        <button
          role="radio"
          aria-checked={active}
          disabled={locked}
          class="flex items-center gap-3 rounded-lg border px-3 py-2 text-left transition-colors disabled:cursor-default {active
            ? 'border-accent/60 bg-accent-soft'
            : 'border-line hover:border-line-strong hover:bg-raised'} {locked && !active ? 'opacity-50' : ''}"
          onclick={() => update({ preset: key as Preset })}
        >
          <span class="grid size-4 shrink-0 place-items-center rounded-full border {active ? 'border-accent' : 'border-faint'}">
            {#if active}<span class="size-2 rounded-full bg-accent"></span>{/if}
          </span>
          <span class="min-w-0 flex-1">
            <span class="block font-medium">{p.label}</span>
            <span class="block text-[11px] text-faint">{p.hint}</span>
          </span>
          <span class="font-mono text-[11px] text-muted">{p.fps}fps · {p.codec}</span>
        </button>
      {/each}
    </div>
  </section>

  <!-- Recent -->
  <!-- Fills the remaining height; only the list inside scrolls. -->
  <section class="flex min-h-[112px] flex-1 flex-col rounded-xl border border-line bg-panel pt-4 pb-2 pl-4 pr-2">
    <div class="flex items-center justify-between pr-2">
      <h2 class="mb-2.5 flex items-center gap-2 text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">
        Recent recordings
        {#if recent.length > 0}
          <span class="rounded-full bg-white/5 px-1.5 py-px text-[10px] font-medium tracking-normal text-faint tabular-nums">{recent.length}</span>
        {/if}
      </h2>
      <button class="-mt-2.5 text-[11px] text-accent hover:underline" onclick={() => api.openFolder()}>Open folder</button>
    </div>
    {#if recent.length === 0}
      <div class="flex flex-1 flex-col items-center justify-center gap-2 text-center text-faint">
        <Icon name="film" size={22} />
        <p class="text-xs">Your recordings will appear here</p>
      </div>
    {:else}
      <!-- Own scroll area: scrollbar only when the list overflows; scrolling
           stops at the ends instead of moving the rest of the panel. -->
      <ul class="scroll-list -ml-2 flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain pr-1" aria-label="Recent recordings">
        {#each recent as r (r.path)}
          <li class="group flex items-center gap-2.5 rounded-lg px-2 py-1.5 hover:bg-raised">
            <span class="grid size-8 shrink-0 place-items-center rounded-md bg-white/5 text-muted">
              <Icon name="film" size={15} />
            </span>
            <div class="min-w-0 flex-1 leading-tight">
              <p class="truncate text-[12px]" title={r.name}>{formatWhen(r.modified_ms)}</p>
              <p class="text-[11px] text-faint">{formatBytes(r.bytes)}</p>
            </div>
            <button class="grid size-7 place-items-center rounded-md text-muted opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-fg focus:opacity-100" title="Play" aria-label="Play {r.name}" onclick={() => open(r.path, false)}>
              <Icon name="play" size={13} fill />
            </button>
            <button class="grid size-7 place-items-center rounded-md text-muted opacity-0 group-hover:opacity-100 hover:bg-hover hover:text-fg focus:opacity-100" title="Show in folder" aria-label="Show {r.name} in folder" onclick={() => open(r.path, true)}>
              <Icon name="folder" size={14} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</aside>
