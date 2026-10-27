<script setup lang="ts">
import { computed } from "vue";
import type { Wdl } from "../api/types";

/** Wins, draws and losses as one bar with the counts beside it. Green and red are the result, not decoration. */
const props = defineProps<{ wdl: Wdl; label?: string }>();
const total = computed(() => props.wdl.wins + props.wdl.draws + props.wdl.losses);
const share = (n: number) => (total.value ? `${(100 * n) / total.value}%` : "0%");
const words = computed(
  () =>
    `${props.label ? `${props.label}: ` : ""}${props.wdl.wins} won, ${props.wdl.draws} drawn, ${props.wdl.losses} lost`,
);
</script>

<template>
  <div class="wdl">
    <span class="wdl-bar" role="img" :aria-label="words">
      <span v-if="total" class="wdl-win" :style="{ inlineSize: share(wdl.wins) }"></span>
      <span v-if="total" class="wdl-draw" :style="{ inlineSize: share(wdl.draws) }"></span>
      <span v-if="total" class="wdl-loss" :style="{ inlineSize: share(wdl.losses) }"></span>
    </span>
    <span class="wdl-text mono" aria-hidden="true">{{ wdl.wins }}–{{ wdl.draws }}–{{ wdl.losses }}</span>
  </div>
</template>

<style scoped>
.wdl {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.wdl-bar {
  display: flex;
  flex: 1 1 auto;
  min-inline-size: 3rem;
  block-size: 0.5rem;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: var(--e-border);
}

.wdl-win {
  background: var(--e-success);
}

.wdl-draw {
  background: color-mix(in oklab, var(--e-muted) 55%, transparent);
}

.wdl-loss {
  background: var(--e-danger);
}

.wdl-text {
  flex: none;
  min-inline-size: 5.5ch;
  color: var(--e-muted);
  font-size: 0.8125rem;
  text-align: end;
}
</style>
