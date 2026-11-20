<!--
  Horizontal peak meter on a -60…0 dBFS scale.
  Zones: green below -20 dB, yellow -20…-9 dB, red above -9 dB.
  A peak-hold tick stays for ~1 s, then falls.
-->
<script lang="ts">
  import { dbToFill } from "../lib/api";

  let { db, active = true, ticks = false }: { db: number; active?: boolean; ticks?: boolean } = $props();

  const fill = $derived(active ? dbToFill(db) : 0);
  let hold = $state(0);
  let heldAt = 0;

  $effect(() => {
    const now = performance.now();
    if (fill >= hold) {
      hold = fill;
      heldAt = now;
    } else if (now - heldAt > 1000) {
      hold = Math.max(fill, hold - 0.02);
    }
  });

  const yellowAt = dbToFill(-20);
  const redAt = dbToFill(-9);
  const scale = [-60, -45, -30, -20, -9, -3, 0];
</script>

<div class="w-full">
  <div class="relative h-2 w-full overflow-hidden rounded-full bg-white/[0.06]">
    <!-- zone tints -->
    <div class="absolute inset-y-0 left-0 bg-[var(--meter-green)]/10" style="width: {yellowAt * 100}%"></div>
    <div class="absolute inset-y-0 bg-[var(--meter-yellow)]/10" style="left: {yellowAt * 100}%; width: {(redAt - yellowAt) * 100}%"></div>
    <div class="absolute inset-y-0 right-0 bg-[var(--meter-red)]/10" style="left: {redAt * 100}%"></div>
    <!-- level: the gradient spans the whole bar and is clipped to the level -->
    <div class="absolute inset-y-0 left-0 overflow-hidden" style="width: {fill * 100}%">
      <div
        class="h-full"
        style="width: {fill > 0 ? 100 / fill : 0}%; background: linear-gradient(90deg, var(--meter-green) 0%, var(--meter-green) {yellowAt * 100}%, var(--meter-yellow) {yellowAt * 100 + 4}%, var(--meter-yellow) {redAt * 100}%, var(--meter-red) {redAt * 100 + 3}%)"
      ></div>
    </div>
    {#if hold > 0.01}
      <div class="absolute inset-y-0 w-0.5 bg-white/80" style="left: calc({hold * 100}% - 1px)"></div>
    {/if}
  </div>
  {#if ticks}
    <div class="relative mt-1 h-3 font-mono text-[9px] text-faint">
      {#each scale as t (t)}
        <span class="absolute -translate-x-1/2" style="left: {dbToFill(t) * 100}%">{t}</span>
      {/each}
    </div>
  {/if}
</div>
