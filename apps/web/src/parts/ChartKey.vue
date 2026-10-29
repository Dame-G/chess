<script setup lang="ts">
/**
 * The line above a chart: what each mark is, and, while the pointer is on the chart, what the point under it says in
 * the same place, so the chart never moves. `readout` is empty at rest.
 */
defineProps<{
  items: { label: string; shape: "dot" | "line"; color: string }[];
  readout?: string;
}>();
</script>

<template>
  <div class="key">
    <p v-if="readout" class="readout num truncate" role="status">{{ readout }}</p>
    <ul v-else class="items" aria-hidden="true">
      <li v-for="i in items" :key="i.label" class="item">
        <span class="mark" :class="`mark--${i.shape}`" :style="{ background: `var(${i.color})` }"></span>
        {{ i.label }}
      </li>
    </ul>
  </div>
</template>

<style scoped>
.key {
  display: flex;
  align-items: center;
  min-height: var(--sp-5);
  min-width: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.items {
  display: flex;
  flex-wrap: wrap;
  gap: 0 var(--sp-4);
  margin: 0;
  padding: 0;
  list-style: none;
}

.item {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
}

.mark {
  display: inline-block;
  flex: none;
}

.mark--dot {
  inline-size: var(--sp-2);
  block-size: var(--sp-2);
  border-radius: var(--r-pill);
}

.mark--line {
  inline-size: var(--sp-4);
  block-size: 2px;
}

.readout {
  margin: 0;
  color: var(--c-ink);
  font: var(--t-fig);
}
</style>
