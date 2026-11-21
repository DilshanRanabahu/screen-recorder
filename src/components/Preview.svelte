<!--
  Live preview of the selected display. The Rust side captures on the GPU,
  downscales with mipmaps and streams small JPEG frames over a Tauri channel
  (see crates/recorder-core/src/preview.rs). Each frame is decoded off the
  main thread with createImageBitmap and acknowledged, so the stream never
  outruns the UI. The app's own window is excluded from capture.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { api, formatClock, type Display, type RecorderState, type WindowInfo } from "../lib/api";
  import Icon from "./Icon.svelte";

  let {
    display,
    window,
    windowMode = false,
    rec,
    elapsed,
    active = true,
    onBattery = false,
    enabled = true,
    setEnabled,
  }: {
    display: Display | undefined;
    /** The chosen window (Window mode), if it is still open. */
    window?: WindowInfo;
    windowMode?: boolean;
    rec: RecorderState;
    elapsed: number;
    /** False while minimized or hidden. */
    active?: boolean;
    /** Lower rate on battery. */
    onBattery?: boolean;
    /** The user's preview on/off switch. */
    enabled?: boolean;
    setEnabled: (on: boolean) => void;
  } = $props();

  let canvas = $state<HTMLCanvasElement>();
  let hasFrame = $state(false);
  let failed = $state<string | null>(null);
  let fps = $state(0);

  const MAX_WIDTH = 960;
  /** Frame header: encoding u8, width u32, height u32, session id u32. */
  const HEADER = 13;
  // Raw RGBA frames (no JPEG encode/decode) vs JPEG; see docs/UI-DESIGN.md.
  const RAW_FRAMES = false;
  const recordingNow = $derived(rec.status === "recording");

  // A plain number, so refreshing the window list (new objects every few
  // seconds) does not restart the preview; only choosing another window does.
  const previewWindowId = $derived(windowMode ? (window?.id ?? null) : null);

  $effect(() => {
    const index = display?.index;
    // In Window mode, preview the chosen window instead of the display.
    const windowId = previewWindowId;
    if (windowMode && windowId === null) {
      hasFrame = false;
      return;
    }
    const target = canvas;
    if (!index || !target || !active || !enabled) {
      hasFrame = false;
      return;
    }
    // Initial rate only; changes are applied live by the effect below.
    const rate = untrack(() => targetFps);

    let stopped = false;
    let frames = 0;
    const ctx = target.getContext("2d", { alpha: false });
    const counter = setInterval(() => {
      fps = frames;
      frames = 0;
    }, 1000);

    // Each start creates a new session with its own id. Cleanup stops exactly
    // that session, so when the display changes, a late "stop" for the old
    // session can never kill the new one (that race froze the preview).
    const session = api.startPreview(index, windowId, MAX_WIDTH, rate, RAW_FRAMES, async (incoming: unknown) => {
      let id: number | null = null;
      try {
        // Normally an ArrayBuffer. If the fast binary IPC is unavailable,
        // Tauri falls back to JSON and delivers a number array instead.
        const msg = toArrayBuffer(incoming);
        const head = new DataView(msg, 0, HEADER);
        id = head.getUint32(9, true);
        if (stopped || !ctx) return;
        const raw = head.getUint8(0) === 1;
        const width = head.getUint32(1, true);
        const height = head.getUint32(5, true);
        if (target.width !== width || target.height !== height) {
          target.width = width;
          target.height = height;
        }
        if (raw) {
          ctx.putImageData(new ImageData(new Uint8ClampedArray(msg, HEADER, width * height * 4), width, height), 0, 0);
        } else {
          const bitmap = await createImageBitmap(new Blob([new Uint8Array(msg, HEADER)], { type: "image/jpeg" }));
          if (!stopped) ctx.drawImage(bitmap, 0, 0);
          bitmap.close();
        }
        hasFrame = true;
        failed = null;
        frames++;
      } catch (e) {
        // Show the problem instead of staying on "Starting preview…".
        if (!hasFrame) failed = `could not draw frame (${String(e)})`;
        console.error("preview frame failed", e);
      } finally {
        if (id !== null) api.previewAck(id);
      }
    });
    session.then(() => (failed = null)).catch((e) => (failed = String(e)));

    return () => {
      stopped = true;
      clearInterval(counter);
      session.then((id) => api.stopPreview(id)).catch(() => {});
    };
  });

  // Smooth whenever the studio is visible (focused or not: you may be working
  // in another window while watching the preview). Lower on battery, and
  // while recording to leave headroom for the encoder.
  const targetFps = $derived(recordingNow ? 20 : onBattery ? 15 : 30);
  $effect(() => {
    api.setPreviewFps(targetFps);
  });

  /** Accepts the shapes Tauri may deliver binary channel data in. */
  function toArrayBuffer(data: unknown): ArrayBuffer {
    if (data instanceof ArrayBuffer) return data;
    if (ArrayBuffer.isView(data)) {
      return data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength) as ArrayBuffer;
    }
    if (Array.isArray(data)) return new Uint8Array(data).buffer;
    throw new Error(`unexpected preview data (${typeof data})`);
  }

  const source = $derived(windowMode ? window : display);
  const aspect = $derived(source && source.height > 0 ? source.width / source.height : 16 / 9);
</script>

<div class="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-xl border border-line bg-black">
  <!-- Keep the display's aspect ratio inside whatever space is available. -->
  <div class="relative max-h-full max-w-full" style="aspect-ratio: {aspect}; height: 100%;">
    <canvas bind:this={canvas} class="h-full w-full object-contain" class:opacity-0={!hasFrame}></canvas>
    {#if rec.status === "recording"}
      <div class="pointer-events-none absolute inset-0 rounded-sm ring-2 {rec.paused ? 'ring-paused/70' : 'ring-record/70'} ring-inset"></div>
    {/if}
  </div>

  {#if !enabled}
    <div class="absolute flex flex-col items-center gap-3 text-center">
      <span class="grid size-12 place-items-center rounded-full bg-white/5 text-muted"><Icon name="eyeOff" size={22} /></span>
      <div>
        <p class="font-medium">Preview is off</p>
        <p class="mt-0.5 text-xs text-faint">Saves CPU and battery. Recording works the same.</p>
      </div>
      <button class="rounded-lg border border-line-strong px-3 py-1.5 text-xs hover:bg-hover" onclick={() => setEnabled(true)}>Turn on preview</button>
    </div>
  {:else if windowMode && !window}
    <p class="absolute text-center text-muted">Choose a window to record<br /><span class="text-xs text-faint">in the Capture panel on the right</span></p>
  {:else if windowMode && window?.minimized}
    <p class="absolute text-center text-muted">“{window.title}” is minimized<br /><span class="text-xs text-faint">Restore it to see and record it</span></p>
  {:else if !hasFrame}
    <p class="absolute text-muted">{failed ? `Preview unavailable: ${failed}` : "Starting preview…"}</p>
  {/if}

  <!-- Overlays -->
  <div class="pointer-events-none absolute top-3 left-3 flex items-center gap-2">
    {#if rec.status === "recording"}
      <span class="flex items-center gap-1.5 rounded-md bg-black/70 px-2 py-1 font-mono text-xs font-semibold backdrop-blur {rec.paused ? 'text-paused' : 'text-record'}">
        <span class="size-1.5 rounded-full {rec.paused ? 'bg-paused' : 'rec-pulse bg-record'}"></span>
        {rec.paused ? "PAUSED" : "REC"} {formatClock(elapsed)}
      </span>
    {:else}
      <span class="rounded-md bg-black/60 px-2 py-1 text-[11px] font-medium tracking-wide text-white/80 backdrop-blur">PREVIEW</span>
    {/if}
  </div>
  {#if windowMode && window}
    <span class="pointer-events-none absolute top-3 right-3 max-w-[60%] truncate rounded-md bg-black/60 px-2 py-1 text-[11px] text-white/80 backdrop-blur">
      Window · {window.title} · {window.width}×{window.height}
    </span>
  {:else if !windowMode && display}
    <span class="pointer-events-none absolute top-3 right-3 rounded-md bg-black/60 px-2 py-1 text-[11px] text-white/80 backdrop-blur">
      Display {display.index} · {display.width}×{display.height} · {display.refresh_rate} Hz
    </span>
  {/if}
  {#if enabled}
    <button
      class="absolute bottom-3 left-3 flex items-center gap-1.5 rounded-md bg-black/60 px-2 py-1 text-[11px] text-white/70 backdrop-blur hover:bg-black/80 hover:text-white"
      title="Turn the live preview off to save CPU and battery (recording is unaffected)"
      onclick={() => setEnabled(false)}
    >
      <Icon name="eyeOff" size={13} />Hide preview
    </button>
  {/if}
  {#if hasFrame}
    <!-- Frames only arrive when the screen changes, so a still screen reads 0 fps. -->
    <span class="pointer-events-none absolute right-3 bottom-3 rounded-md bg-black/60 px-2 py-1 font-mono text-[10px] text-white/60 backdrop-blur" title="Preview frames per second (the recording itself is always at the full frame rate)">
      preview {fps} fps{onBattery ? " · battery" : ""}
    </span>
  {/if}
</div>
