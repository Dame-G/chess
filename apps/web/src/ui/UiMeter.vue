<script setup lang="ts">
import { computed } from "vue";

/**
 * A plain bar for an amount of a whole: data used of what the folder may hold, games played of the daily limit. A
 * label above it at the start and the figure at the end, the bar under them. No stripes and no animation, so it never
 * reads as something loading. `tone` turns the fill into a judgement (a full disk is `warn`); without it the fill is ink.
 */
const props = defineProps<{
  label: string;
  value: number;
  max: number;
  /** The figure at the end: "1.0 of 10 GB". Without it the bar is the whole meter. */
  text?: string;
  tone?: "win" | "loss" | "warn" | "info";
}>();

const share = computed(() => (props.max > 0 ? Math.min(100, Math.max(0, (props.value / props.max) * 100)) : 0));
</script>

<template>
  <div class="ui-meter">
    <div class="ui-meter-head">
      <span class="ui-meter-label">{{ label }}</span>
      <span v-if="text" class="ui-meter-text">{{ text }}</span>
    </div>
    <div
      class="ui-meter-track"
      role="meter"
      :aria-label="label"
      :aria-valuemin="0"
      :aria-valuemax="max"
      :aria-valuenow="Math.min(max, value)"
      :aria-valuetext="text"
    >
      <span
        class="ui-meter-fill"
        :class="tone ? `ui-meter-fill--${tone}` : undefined"
        :style="{ inlineSize: `${share}%` }"
      />
    </div>
  </div>
</template>

<style scoped>
.ui-meter {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.ui-meter-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--sp-3);
}

.ui-meter-label {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.ui-meter-text {
  font: var(--t-fig);
}

.ui-meter-track {
  block-size: var(--sp-2);
  overflow: hidden;
  border-radius: var(--r-pill);
  background: var(--c-sunk);
}

.ui-meter-fill {
  display: block;
  block-size: 100%;
  border-radius: var(--r-pill);
  background: var(--c-ink-2);
}

.ui-meter-fill--win {
  background: var(--c-win);
}

.ui-meter-fill--loss {
  background: var(--c-loss);
}

.ui-meter-fill--warn {
  background: var(--c-warn);
}

.ui-meter-fill--info {
  background: var(--c-info);
}
</style>
