<script lang="ts">
  import { toDb, type AudioDevice, type RecorderState, type Selection } from "../lib/api";
  import Icon from "./Icon.svelte";
  import Meter from "./Meter.svelte";
  import Switch from "./Switch.svelte";

  let {
    selection,
    mics,
    levels,
    rec,
    update,
  }: {
    selection: Selection;
    mics: AudioDevice[];
    levels: { system: number; microphone: number };
    rec: RecorderState;
    update: (patch: Partial<Selection>) => void;
  } = $props();

  // Sources can't change mid-recording (they are tracks in the file).
  const locked = $derived(rec.status !== "idle");
  const systemDb = $derived(toDb(levels.system));
  const micDb = $derived(toDb(levels.microphone));
  const defaultMic = $derived(mics.find((m) => m.is_default));
  // A remembered microphone that is currently unplugged.
  const missingMic = $derived(!!selection.microphone_id && !mics.some((m) => m.id === selection.microphone_id));
  const fmt = (db: number) => (Number.isFinite(db) && db > -60 ? `${db.toFixed(0)} dB` : "-∞");
</script>

<section class="shrink-0 rounded-xl border border-line bg-panel">
  <header class="flex items-center justify-between px-4 pt-3 pb-1">
    <h2 class="text-[11px] font-semibold tracking-[0.08em] text-muted uppercase">Audio mixer</h2>
    <span class="text-[11px] text-faint">{locked ? "Tracks are fixed while recording" : "Each source is saved as its own track"}</span>
  </header>

  <div class="grid grid-cols-2 gap-x-6 px-4 pt-2 pb-3">
    <!-- System audio -->
    <div class="min-w-0">
      <div class="mb-2 flex h-7 items-center gap-2">
        <span class="grid size-7 shrink-0 place-items-center rounded-lg {selection.system_audio ? 'bg-accent-soft text-accent' : 'bg-white/5 text-faint'}">
          <Icon name="speaker" size={15} />
        </span>
        <div class="min-w-0 flex-1 leading-tight">
          <p class="font-medium">Desktop audio</p>
          <p class="truncate text-[11px] text-faint">Default output device</p>
        </div>
        <span class="w-14 text-right font-mono text-[11px] whitespace-nowrap text-muted tabular-nums">{selection.system_audio ? fmt(systemDb) : "off"}</span>
        <Switch checked={selection.system_audio} label="Desktop audio" disabled={locked} onchange={(v) => update({ system_audio: v })} />
      </div>
      <Meter db={systemDb} active={selection.system_audio} ticks />
    </div>

    <!-- Microphone -->
    <div class="min-w-0">
      <div class="mb-2 flex h-7 items-center gap-2">
        <span class="grid size-7 shrink-0 place-items-center rounded-lg {selection.microphone ? 'bg-accent-soft text-accent' : 'bg-white/5 text-faint'}">
          <Icon name="mic" size={15} />
        </span>
        <div class="min-w-0 flex-1 leading-tight">
          <p class="font-medium">Microphone</p>
          <select
            class="-ml-1 h-4 w-full min-w-0 cursor-pointer truncate rounded bg-transparent px-1 text-[11px] text-faint hover:text-muted disabled:cursor-default"
            aria-label="Microphone device"
            disabled={locked}
            value={selection.microphone_id ?? ""}
            onchange={(e) => update({ microphone_id: e.currentTarget.value || null })}
          >
            <option value="">Windows default{defaultMic ? ` · ${defaultMic.name}` : ""}</option>
            {#each mics as m (m.id)}
              <option value={m.id}>{m.name}{m.is_default ? " (default)" : ""}</option>
            {/each}
            {#if missingMic}
              <!-- The saved choice is unplugged: keep it selected so it comes back when reconnected. -->
              <option value={selection.microphone_id}>Saved microphone (not connected)</option>
            {/if}
          </select>
        </div>
        <span class="w-14 text-right font-mono text-[11px] whitespace-nowrap text-muted tabular-nums">{selection.microphone ? fmt(micDb) : "off"}</span>
        <Switch checked={selection.microphone} label="Microphone" disabled={locked} onchange={(v) => update({ microphone: v })} />
      </div>
      <Meter db={micDb} active={selection.microphone} ticks />
    </div>
  </div>
</section>
