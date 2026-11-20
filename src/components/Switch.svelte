<script lang="ts">
  let {
    checked,
    label,
    disabled = false,
    onchange,
  }: { checked: boolean; label: string; disabled?: boolean; onchange: (v: boolean) => void } = $props();
</script>

<button
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  class="relative h-[18px] w-8 shrink-0 rounded-full transition-colors disabled:opacity-40 {checked
    ? 'bg-accent'
    : 'bg-line-strong'}"
  onclick={() => onchange(!checked)}
>
  <span
    class="absolute top-[3px] size-3 rounded-full bg-white shadow transition-[left] {checked ? 'left-[17px]' : 'left-[3px]'}"
  ></span>
</button>
