<script setup lang="ts">
import { computed } from "vue";
import Term from "./Term.vue";

/** Game pairs by the new version's points: lost both, lost one and drew one, even, won one and drew one, won both. */
const props = defineProps<{ penta: number[] }>();

const names = ["Lost both", "Lost one, drew one", "Even", "Won one, drew one", "Won both"];
const total = computed(() => props.penta.reduce((a, b) => a + b, 0));
const parts = computed(() =>
  props.penta.map((n, i) => ({ n, name: names[i]!, share: total.value ? n / total.value : 0, cls: `pair--${i}` })),
);
</script>

<template>
  <figure class="pairs">
    <figcaption class="pairs-head">
      <Term of="pairs">Game pairs</Term>
      <span class="num">{{ total.toLocaleString() }}</span>
    </figcaption>
    <div class="pairs-bar" aria-hidden="true">
      <span
        v-for="p in parts"
        :key="p.name"
        :class="['pairs-seg', p.cls]"
        :style="{ flexGrow: p.n }"
        :title="`${p.name}: ${p.n}`"
      ></span>
    </div>
    <ul class="pairs-legend">
      <li v-for="p in parts" :key="p.name" :class="{ none: !p.n }">
        <span :class="['pairs-swatch', p.cls]" aria-hidden="true"></span>
        {{ p.name }} <strong class="num">{{ p.n.toLocaleString() }}</strong>
        <span class="pairs-share num">{{ Math.round(p.share * 100) }}%</span>
      </li>
    </ul>
  </figure>
</template>

<style scoped>
.pairs {
  display: grid;
  gap: var(--e-gap-base);
  margin: 0;
}

.pairs-head {
  display: flex;
  gap: var(--e-gap-base);
  justify-content: space-between;
  align-items: center;
  font: var(--e-type-small);
}

.pairs-bar {
  display: flex;
  gap: 2px;
  height: 0.75rem;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: var(--e-surface-tint);
}

.pairs-seg {
  min-width: 2px;
}

.pair--0 {
  background: var(--e-danger);
}

.pair--1 {
  background: color-mix(in oklab, var(--e-danger) 45%, var(--e-surface));
}

.pair--2 {
  background: color-mix(in oklab, var(--e-border) 60%, var(--e-surface));
}

.pair--3 {
  background: color-mix(in oklab, var(--e-success) 45%, var(--e-surface));
}

.pair--4 {
  background: var(--e-success);
}

.pairs-legend {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-snug) var(--e-gap-loose);
  margin: 0;
  padding: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  list-style: none;
}

.pairs-legend li {
  display: inline-flex;
  align-items: center;
  gap: var(--e-gap-snug);
}

.pairs-legend li.none {
  opacity: 0.55;
}

.pairs-legend strong {
  color: var(--e-ink);
  font-weight: 600;
}

.pairs-swatch {
  width: 0.625rem;
  height: 0.625rem;
  border-radius: 2px;
}

.pairs-share {
  opacity: 0.9;
}
</style>
