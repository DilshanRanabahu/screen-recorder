<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import ControlBar from "./components/ControlBar.svelte";
  import Mixer from "./components/Mixer.svelte";
  import Pill from "./components/Pill.svelte";
  import Preview from "./components/Preview.svelte";
  import SidePanel from "./components/SidePanel.svelte";
  import TitleBar from "./components/TitleBar.svelte";
  import Toast, { type ToastData } from "./components/Toast.svelte";
  import {
    api,
    formatBytes,
    formatClock,
    type AudioDevice,
    type Display,
    type RecorderState,
    type RecordingFile,
    type Selection,
    type View,
  } from "./lib/api";

  let rec = $state<RecorderState>({ status: "idle", last: null, error: null });
  let view = $state<View>("studio");
  let displays = $state<Display[]>([]);
  let mics = $state<AudioDevice[]>([]);
  let recent = $state<RecordingFile[]>([]);
  let freeBytes = $state<number | null>(null);
  let selection = $state<Selection>({ display: 1, preset: "high", system_audio: true, microphone: false, microphone_id: null });
  let hotkey = $state("Ctrl+Alt+R");
  let pauseHotkey = $state("Ctrl+Alt+P");
  let now = $state(Date.now());
  let levels = $state({ system: 0, microphone: 0 });
  let toasts = $state<ToastData[]>([]);
  let minimized = $state(false);
  let hidden = $state(document.hidden);
  let nextToast = 1;

  const elapsed = $derived(
    rec.status === "recording" ? rec.elapsed_ms + (rec.paused ? 0 : Math.max(0, now - rec.at_ms)) : 0,
  );
  // False while minimized or hidden: preview and idle meters pause.
  const active = $derived(!minimized && !hidden && view === "studio");
  const display = $derived(displays.find((d) => d.index === selection.display) ?? displays[0]);

  function toast(t: Omit<ToastData, "id">) {
    toasts = [...toasts.slice(-2), { ...t, id: nextToast++ }];
  }

  async function refreshFiles() {
    [recent, freeBytes] = await Promise.all([api.recordings(6), api.storageFree()]);
  }

  /// Idle level meters run only while the studio is visible.
  function syncMonitoring() {
    hidden = document.hidden;
    api.setMonitoring(!minimized && !document.hidden && view === "studio");
  }

  function onState(s: RecorderState) {
    const was = rec.status;
    rec = s;
    if (s.status === "idle" && (was === "stopping" || was === "starting")) {
      if (s.last) {
        const l = s.last;
        const tracks = l.audio_tracks.length ? ` · ${l.audio_tracks.length} audio track${l.audio_tracks.length > 1 ? "s" : ""}` : "";
        toast({ kind: "success", title: "Recording saved", detail: `${formatClock(l.duration.secs * 1000)} · ${formatBytes(l.bytes)}${tracks}`, path: l.path });
        for (const w of l.warnings) toast({ kind: "warning", title: "Heads up", detail: w });
      }
      if (s.error) toast({ kind: "error", title: was === "starting" ? "Couldn't start recording" : "Recording failed", detail: s.error });
      refreshFiles();
    }
    if (s.status === "recording" && was === "starting") {
      for (const w of s.warnings) toast({ kind: "warning", title: "Heads up", detail: w });
    }
  }

  // The timer only needs ticking while recording.
  $effect(() => {
    if (rec.status !== "recording" || rec.paused) return;
    const clock = setInterval(() => (now = Date.now()), 250);
    return () => clearInterval(clock);
  });

  // Level meters: poll only when something can be shown.
  $effect(() => {
    const anySource = rec.status === "recording" ? rec.tracks.length > 0 && !rec.paused : selection.system_audio || selection.microphone;
    if (!active || !anySource) {
      levels = { system: 0, microphone: 0 };
      return;
    }
    const meter = setInterval(async () => {
      const [s, m] = await api.levels().catch(() => [0, 0] as [number, number]);
      // Fast attack, smooth release; skip the update when everything is silent.
      const next = { system: Math.max(s, levels.system * 0.75), microphone: Math.max(m, levels.microphone * 0.75) };
      if (next.system < 0.001 && next.microphone < 0.001 && levels.system === 0 && levels.microphone === 0) return;
      levels = {
        system: next.system < 0.001 ? 0 : next.system,
        microphone: next.microphone < 0.001 ? 0 : next.microphone,
      };
    }, 100);
    return () => clearInterval(meter);
  });

  onMount(() => {
    const unlisten: (() => void)[] = [];
    const storage = setInterval(() => api.storageFree().then((b) => (freeBytes = b)), 30_000);
    document.addEventListener("visibilitychange", syncMonitoring);

    // WebView2 doesn't always hide the page when minimized, so watch the window.
    const win = getCurrentWindow();
    win.onResized(async () => {
      minimized = await win.isMinimized().catch(() => false);
      syncMonitoring();
    }).then((u) => unlisten.push(u));

    (async () => {
      const info = await api.uiInfo();
      hotkey = info.hotkey;
      pauseHotkey = info.pause_hotkey;
      [displays, selection, rec, view] = await Promise.all([api.displays(), api.selection(), api.state(), api.view()]);
      unlisten.push(await api.onState(onState));
      unlisten.push(
        await api.onView((v) => {
          view = v;
          syncMonitoring();
        }),
      );
      syncMonitoring();
      mics = await api.microphones().catch(() => []);
      await refreshFiles();
    })().catch((e) => toast({ kind: "error", title: "Something went wrong", detail: String(e) }));

    return () => {
      clearInterval(storage);
      document.removeEventListener("visibilitychange", syncMonitoring);
      unlisten.forEach((u) => u());
      api.setMonitoring(false);
    };
  });

  async function update(patch: Partial<Selection>) {
    selection = { ...selection, ...patch };
    await api.setSelection(selection);
  }

  async function run(action: () => Promise<void>) {
    try {
      await action();
    } catch (e) {
      toast({ kind: "error", title: "Action failed", detail: String(e) });
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && rec.status === "idle" && view === "studio") api.hide();
  }
</script>

<svelte:window onkeydown={onKey} />

{#if view === "pill"}
  <Pill {rec} {elapsed} {levels} {hotkey} {pauseHotkey} {run} />
{:else}
  <div class="flex h-screen flex-col">
    <TitleBar {rec} {elapsed} />
    <div class="flex min-h-0 flex-1 gap-3 p-3">
      <section class="flex min-w-0 flex-1 flex-col gap-3">
        <Preview {display} {rec} {elapsed} {active} />
        <Mixer {selection} {mics} {levels} {rec} {update} />
      </section>
      <SidePanel {selection} {displays} {recent} {rec} {update} />
    </div>
    <ControlBar {rec} {elapsed} {selection} {display} {freeBytes} {hotkey} {pauseHotkey} {run} />
  </div>

  <div class="pointer-events-none fixed right-4 bottom-[92px] flex flex-col items-end gap-2">
    {#each toasts as t (t.id)}
      <Toast toast={t} dismiss={() => (toasts = toasts.filter((x) => x.id !== t.id))} />
    {/each}
  </div>
{/if}
