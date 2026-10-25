<script setup lang="ts">
import { computed } from "vue";
import type { Build } from "../../api/queries";

const props = defineProps<{
  builds: readonly Build[];
  /** The build the page is about: drawn in the brand colour, the others in grey. */
  selected: number;
}>();

const shown = computed(() => props.builds.slice(0, 8));
const fastest = computed(() => Math.max(...shown.value.map((b) => b.nps), 1));
const kilo = (nps: number) => `${Math.round(nps / 1000)}k`;
</script>

<template>
  <ul class="bars" aria-label="Search speed of each build, in thousands of nodes per second">
    <li v-for="b in shown" :key="b.id" :class="['bar', { 'bar--on': b.id === selected }]">
      <span class="bar-name"
        >{{ b.ref }} <span class="bar-id">#{{ b.id }}</span></span
      >
      <span class="bar-track" aria-hidden="true"
        ><span :style="{ inlineSize: `${(b.nps / fastest) * 100}%` }"></span
      ></span>
      <span class="bar-value num">{{ kilo(b.nps) }} n/s</span>
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
  grid-template-columns: minmax(5rem, 9rem) minmax(0, 1fr) 4.5rem;
  align-items: center;
  gap: var(--e-gap-base);
  padding: var(--e-gap-snug) var(--e-gap-base);
  border-inline-start: 0.1875rem solid transparent;
  color: var(--e-muted);
  font: var(--e-type-small);
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

.bar-id {
  color: var(--e-muted);
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
