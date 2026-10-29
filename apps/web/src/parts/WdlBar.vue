<script setup lang="ts">
import { computed } from "vue";
import type { Wdl } from "../api/types";

/**
 * Wins, draws and losses as one bar. Green and red are the result, not decoration. The record is the bar's tooltip and
 * its description for a screen reader; `counts` writes it beside the bar too, for a row with room.
 */
const props = defineProps<{ wdl: Wdl; label?: string; counts?: boolean }>();
const total = computed(() => props.wdl.wins + props.wdl.draws + props.wdl.losses);
const share = (n: number) => (total.value ? `${(100 * n) / total.value}%` : "0%");
const record = computed(() => `${props.wdl.wins} won, ${props.wdl.draws} drawn, ${props.wdl.losses} lost`);
const words = computed(() => `${props.label ? `${props.label}: ` : ""}${record.value}`);
</script>

<template>
  <div class="wdl">
    <span class="wdl-bar" role="img" :aria-label="words" :title="record">
      <span v-if="total" class="wdl-win" :style="{ inlineSize: share(wdl.wins) }"></span>
      <span v-if="total" class="wdl-draw" :style="{ inlineSize: share(wdl.draws) }"></span>
      <span v-if="total" class="wdl-loss" :style="{ inlineSize: share(wdl.losses) }"></span>
    </span>
    <span v-if="counts" class="wdl-text" aria-hidden="true">{{ wdl.wins }}–{{ wdl.draws }}–{{ wdl.losses }}</span>
  </div>
</template>

<style scoped>
.wdl {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  min-width: 0;
}

.wdl-bar {
  display: flex;
  flex: 1 1 auto;
  min-width: var(--sp-6);
  block-size: var(--sp-2);
  overflow: hidden;
  border-radius: var(--r-pill);
  background: var(--c-sunk);
}

.wdl-win {
  background: var(--c-win);
}

.wdl-draw {
  background: var(--c-ink-3);
}

.wdl-loss {
  background: var(--c-loss);
}

.wdl-text {
  flex: none;
  color: var(--c-ink-2);
  font: var(--t-fig);
  white-space: nowrap;
}
</style>
