<script setup lang="ts">
import { computed } from "vue";
import { speed } from "../../format";

const props = defineProps<{
  /** One bar each: a version and the speed of its newest build. */
  bars: readonly { name: string; nps: number }[];
  /** The version the page is about: drawn in the brand colour, the others in grey. */
  selected: string;
}>();

const shown = computed(() => props.bars.slice(0, 8));
const fastest = computed(() => Math.max(...shown.value.map((b) => b.nps), 1));
</script>

<template>
  <ul class="bars" aria-label="Search speed of each version">
    <li v-for="b in shown" :key="b.name" :class="['bar', { 'bar--on': b.name === selected }]">
      <span class="bar-name" :title="b.name">{{ b.name }}</span>
      <span class="bar-track" aria-hidden="true"
        ><span :style="{ inlineSize: `${(b.nps / fastest) * 100}%` }"></span
      ></span>
      <span class="bar-value num">{{ speed(b.nps) }}</span>
    </li>
  </ul>
</template>

<style scoped>
.bars {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.bar {
  display: grid;
  grid-template-columns: minmax(5rem, 9rem) minmax(0, 1fr) 8rem;
  align-items: center;
  gap: var(--e-gap-base);
  padding: var(--e-gap-snug) var(--e-gap-base);
  border-inline-start: 0.1875rem solid transparent;
  color: var(--e-muted);
  font: var(--e-type-small);
}

/* On a phone the bar would be a stub between two labels: name and speed share a line, the bar runs under them. */
@container (width < 30rem) {
  .bar {
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--e-gap-tight) var(--e-gap-base);
  }

  .bar-track {
    grid-row: 2;
    grid-column: 1 / -1;
  }
}

.bar--on {
  border-inline-start-color: var(--e-primary);
  background: var(--e-primary-soft);
  color: var(--e-ink);
}

.bar-name {
  overflow: hidden;
  font-family: var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bar-track {
  block-size: 0.625rem;
  background: var(--e-surface-tint);
}

.bar-track span {
  display: block;
  block-size: 100%;
  background: color-mix(in oklab, var(--e-muted) 55%, transparent);
}

.bar--on .bar-track span {
  background: var(--e-primary);
}

.bar-value {
  font-family: var(--e-font-mono);
  text-align: end;
}
</style>
