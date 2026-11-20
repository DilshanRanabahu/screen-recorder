<script lang="ts">
  import { icons, type IconName } from "../lib/icons";

  let { name, size = 16, fill = false, class: cls = "" }: { name: IconName; size?: number; fill?: boolean; class?: string } =
    $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={fill ? "currentColor" : "none"}
  stroke="currentColor"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
  class={cls}
  aria-hidden="true"
>
  <path d={icons[name]} />
</svg>
