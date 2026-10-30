<script setup lang="ts">
import { ref, watch } from "vue";

/**
 * The dot that means something is live: cognac while a game or work runs, green when connected, red on error. It
 * pulses once each time `pulse` changes (a move was played), never continuously. Decorative unless it has a `label`.
 */
const props = withDefaults(
  defineProps<{
    tone?: "act" | "win" | "loss";
    /** Any value that changes when there is news (the ply count): each change plays the pulse once. */
    pulse?: number | string | boolean;
    /** Names the dot for a screen reader; without it the dot is hidden from them. */
    label?: string;
  }>(),
  { tone: "act" },
);

const ticks = ref(0);
watch(
  () => props.pulse,
  () => ticks.value++,
);
</script>

<template>
  <span
    :key="ticks"
    class="ui-live"
    :class="[`ui-live--${tone}`, { 'ui-live--pulse': ticks > 0 }]"
    :role="label ? 'img' : undefined"
    :aria-label="label"
    :aria-hidden="label ? undefined : 'true'"
  />
</template>

<style scoped>
.ui-live {
  display: inline-block;
  flex: none;
  width: var(--dot);
  height: var(--dot);
  border-radius: var(--r-pill);
  background: currentcolor;
  color: var(--c-act);
}

.ui-live--win {
  color: var(--c-win);
}

.ui-live--loss {
  color: var(--c-loss);
}

.ui-live--pulse {
  animation: ui-pulse-once 700ms var(--m-ease);
}

@media (prefers-reduced-motion: reduce) {
  .ui-live--pulse {
    animation: none;
  }
}
</style>
