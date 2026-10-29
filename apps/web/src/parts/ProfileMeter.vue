<script setup lang="ts">
import { computed } from "vue";
import { formatPercent } from "../format";

/**
 * A small bar for a percentage in a table row: accuracy by phase, by speed. Ink on a quiet track; the figure is the
 * table's own column, so the bar only has to show the shape.
 */
const props = defineProps<{ value: number | null | undefined; label: string }>();
const width = computed(() => `${Math.max(0, Math.min(100, props.value ?? 0))}%`);
</script>

<template>
  <span
    class="meter"
    role="img"
    :aria-label="`${label} accuracy ${value == null ? 'unknown' : formatPercent(value)}`"
    :title="value == null ? undefined : formatPercent(value)"
  >
    <span class="fill" :style="{ inlineSize: width }"></span>
  </span>
</template>

<style scoped>
.meter {
  display: block;
  min-width: var(--sp-6);
  block-size: var(--sp-2);
  overflow: hidden;
  border-radius: var(--r-pill);
  background: var(--c-sunk);
}

.fill {
  display: block;
  block-size: 100%;
  border-radius: var(--r-pill);
  background: var(--c-ink-2);
}
</style>
