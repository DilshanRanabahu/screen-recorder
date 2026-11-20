<!--
  Live preview of the selected display. The Rust side captures on the GPU,
  downscales with mipmaps and streams small JPEG frames over a Tauri channel
  (see crates/recorder-core/src/preview.rs). Each frame is decoded off the
  main thread with createImageBitmap and acknowledged, so the stream never
  outruns the UI. The app's own window is excluded from capture.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { api, formatClock, type Display, type RecorderState } from "../lib/api";

  let {
    display,
    rec,
    elapsed,
    active = true,
  }: { display: Display | undefined; rec: RecorderState; elapsed: number; active?: boolean } = $props();

  let canvas = $state<HTMLCanvasElement>();
  let hasFrame = $state(false);
  let failed = $state<string | null>(null);
  let fps = $state(0);

  const MAX_WIDTH = 960;
  // Raw RGBA frames (no JPEG encode/decode) vs JPEG; see docs/UI-DESIGN.md.
  const RAW_FRAMES = false;
  const recordingNow = $derived(rec.status === "recording");

  $effect(() => {
    const index = display?.index;
    const target = canvas;
    if (!index || !target || !active) return;
    // Initial rate only; changes are applied live by the effect below.
    const rate = untrack(() => targetFps);

    let stopped = false;
    let frames = 0;
    const ctx = target.getContext("2d", { alpha: false });
    const counter = setInterval(() => {
      fps = frames;
      frames = 0;
    }, 1000);

    api
      .startPreview(index, MAX_WIDTH, rate, RAW_FRAMES, async (msg) => {
        try {
          if (stopped || !ctx) return;
          const head = new DataView(msg, 0, 9);
          const raw = head.getUint8(0) === 1;
          const width = head.getUint32(1, true);
          const height = head.getUint32(5, true);
          if (target.width !== width || target.height !== height) {
            target.width = width;
            target.height = height;
          }
          if (raw) {
            ctx.putImageData(new ImageData(new Uint8ClampedArray(msg, 9, width * height * 4), width, height), 0, 0);
          } else {
            const bitmap = await createImageBitmap(new Blob([new Uint8Array(msg, 9)], { type: "image/jpeg" }));
            if (!stopped) ctx.drawImage(bitmap, 0, 0);
            bitmap.close();
          }
          hasFrame = true;
          frames++;
        } finally {
          api.previewAck();
        }
      })
      .then(() => (failed = null))
      .catch((e) => (failed = String(e)));

    return () => {
      stopped = true;
      clearInterval(counter);
      api.stopPreview();
    };
  });

  // Smooth whenever the studio is visible (focused or not: you may be working
  // in another window while watching the preview). A bit lower while
  // recording, to leave headroom for the encoder.
  const targetFps = $derived(recordingNow ? 20 : 30);
  $effect(() => {
    api.setPreviewFps(targetFps);
  });

  const aspect = $derived(display && display.height ? display.width / display.height : 16 / 9);
</script>

<div class="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden rounded-xl border border-line bg-black">
  <!-- Keep the display's aspect ratio inside whatever space is available. -->
  <div class="relative max-h-full max-w-full" style="aspect-ratio: {aspect}; height: 100%;">
    <canvas bind:this={canvas} class="h-full w-full object-contain" class:opacity-0={!hasFrame}></canvas>
    {#if rec.status === "recording"}
      <div class="pointer-events-none absolute inset-0 rounded-sm ring-2 {rec.paused ? 'ring-paused/70' : 'ring-record/70'} ring-inset"></div>
    {/if}
  </div>

  {#if !hasFrame}
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
  {#if display}
    <span class="pointer-events-none absolute top-3 right-3 rounded-md bg-black/60 px-2 py-1 text-[11px] text-white/80 backdrop-blur">
      Display {display.index} · {display.width}×{display.height} · {display.refresh_rate} Hz
    </span>
  {/if}
  {#if hasFrame}
    <!-- Frames only arrive when the screen changes, so a still screen reads 0 fps. -->
    <span class="pointer-events-none absolute right-3 bottom-3 rounded-md bg-black/60 px-2 py-1 font-mono text-[10px] text-white/60 backdrop-blur" title="Preview frames per second (the recording itself is always at the full frame rate)">
      preview {fps} fps
    </span>
  {/if}
</div>
