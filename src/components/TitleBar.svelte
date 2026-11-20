<script lang="ts">
  import { api, formatClock, type RecorderState } from "../lib/api";
  import Icon from "./Icon.svelte";

  let { rec, elapsed }: { rec: RecorderState; elapsed: number } = $props();
</script>

<!-- Double-click to maximize, like a native title bar; the buttons cover keyboard use. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<header
  data-tauri-drag-region
  class="flex h-10 shrink-0 items-center gap-3 border-b border-line bg-panel pl-4"
  ondblclick={() => api.toggleMaximize()}
>
  <div class="pointer-events-none flex items-center gap-2">
    <span class="grid size-5 place-items-center rounded-md bg-gradient-to-br from-[#ff6b7a] to-[#d62f45]">
      <span class="size-1.5 rounded-full bg-white"></span>
    </span>
    <span class="font-display text-[13px] font-semibold tracking-tight">Screen Recorder</span>
  </div>

  <!-- Status -->
  <div class="pointer-events-none mx-auto">
    {#if rec.status === "recording" && !rec.paused}
      <span class="flex items-center gap-2 rounded-full bg-record-soft px-3 py-1 text-xs font-semibold text-record">
        <span class="rec-pulse size-2 rounded-full bg-record"></span>REC {formatClock(elapsed)}
      </span>
    {:else if rec.status === "recording"}
      <span class="flex items-center gap-2 rounded-full bg-paused/15 px-3 py-1 text-xs font-semibold text-paused">
        <span class="size-2 rounded-full bg-paused"></span>Paused {formatClock(elapsed)}
      </span>
    {:else if rec.status === "starting" || rec.status === "stopping"}
      <span class="rounded-full bg-white/5 px-3 py-1 text-xs text-muted">
        {rec.status === "starting" ? "Starting…" : "Saving…"}
      </span>
    {:else}
      <span class="flex items-center gap-2 rounded-full bg-white/5 px-3 py-1 text-xs text-muted">
        <span class="size-2 rounded-full bg-ok"></span>Ready
      </span>
    {/if}
  </div>

  <div class="flex h-full items-stretch">
    <button class="grid w-11 place-items-center text-muted hover:bg-hover hover:text-fg" title="Open recordings folder" onclick={() => api.openFolder()}>
      <Icon name="folder" size={15} />
    </button>
    <button class="grid w-11 place-items-center text-muted hover:bg-hover hover:text-fg" aria-label="Minimize" onclick={() => api.minimize()}>
      <Icon name="minimize" size={15} />
    </button>
    <button class="grid w-11 place-items-center text-muted hover:bg-hover hover:text-fg" aria-label="Maximize" onclick={() => api.toggleMaximize()}>
      <Icon name="maximize" size={13} />
    </button>
    <button class="grid w-11 place-items-center text-muted hover:bg-[#c42b1c] hover:text-white" aria-label="Hide to tray" title="Hide to tray" onclick={() => api.hide()}>
      <Icon name="close" size={15} />
    </button>
  </div>
</header>
