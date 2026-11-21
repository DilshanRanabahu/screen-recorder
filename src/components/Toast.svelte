<script lang="ts" module>
  export interface ToastData {
    id: number;
    kind: "success" | "error" | "warning";
    title: string;
    detail?: string;
    path?: string;
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";

  let {
    toast,
    dismiss,
    open,
  }: { toast: ToastData; dismiss: () => void; open: (path: string, reveal: boolean) => void } = $props();

  $effect(() => {
    const t = setTimeout(dismiss, toast.kind === "error" ? 12_000 : 7_000);
    return () => clearTimeout(t);
  });

  const tone = $derived(
    toast.kind === "success" ? "text-ok bg-ok/15" : toast.kind === "warning" ? "text-paused bg-paused/15" : "text-record bg-record-soft",
  );
</script>

<div class="toast-in pointer-events-auto flex w-[360px] gap-3 rounded-xl border border-line-strong bg-raised/95 p-3 shadow-2xl backdrop-blur" role={toast.kind === "error" ? "alert" : "status"}>
  <span class="grid size-8 shrink-0 place-items-center rounded-lg {tone}">
    <Icon name={toast.kind === "success" ? "check" : "alert"} size={16} />
  </span>
  <div class="min-w-0 flex-1">
    <p class="font-medium">{toast.title}</p>
    {#if toast.detail}<p class="mt-0.5 text-xs break-words text-muted">{toast.detail}</p>{/if}
    {#if toast.path}
      {@const path = toast.path}
      <div class="mt-2 flex gap-2">
        <button class="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-[#0b0d12] hover:brightness-110" onclick={() => open(path, false)}>Play</button>
        <button class="rounded-md border border-line-strong px-2.5 py-1 text-xs hover:bg-hover" onclick={() => open(path, true)}>Show in folder</button>
      </div>
    {/if}
  </div>
  <button class="grid size-6 shrink-0 place-items-center rounded text-faint hover:bg-hover hover:text-fg" aria-label="Dismiss" onclick={dismiss}>
    <Icon name="close" size={12} />
  </button>
</div>
