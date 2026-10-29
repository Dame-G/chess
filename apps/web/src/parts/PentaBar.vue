<script setup lang="ts">
import { computed } from "vue";
import { formatNumber } from "../format";
import Term from "./Term.vue";

/**
 * How the game pairs ended. Each opening is played twice with the colours swapped, so a pair scores 0, ½, 1, 1½ or 2
 * points: the shape of the bar shows whether wins and losses balance, and a lopsided bar is the first sign of a bug.
 * Each column says what its points mean, so the figures are never a row of bare numbers.
 */
const props = defineProps<{ penta: readonly number[] }>();
const total = computed(() => props.penta.reduce((a, b) => a + b, 0));
const COLUMNS = [
  { points: "0", name: "Lost both", cls: "ll" },
  { points: "½", name: "Lost, drew", cls: "ld" },
  { points: "1", name: "Even", cls: "dd" },
  { points: "1½", name: "Won, drew", cls: "wd" },
  { points: "2", name: "Won both", cls: "ww" },
];
const share = (n: number) => (total.value ? `${(100 * n) / total.value}%` : "0%");
const words = computed(() => COLUMNS.map((c, i) => `${c.name} ${props.penta[i] ?? 0}`).join(", "));
</script>

<template>
  <div class="penta">
    <p class="title">
      <Term k="pairs" text="Pairs" /> by points scored <span class="muted">(each opening is played twice)</span>
    </p>
    <div class="bar" role="img" :aria-label="`Pairs: ${words}`">
      <span
        v-for="(c, i) in COLUMNS"
        :key="c.cls"
        class="seg"
        :class="c.cls"
        :style="{ inlineSize: share(penta[i] ?? 0) }"
      ></span>
    </div>
    <ul class="cols" aria-hidden="true">
      <li v-for="(c, i) in COLUMNS" :key="c.cls" class="col" :title="`${c.points} points: ${c.name.toLowerCase()}`">
        <span class="n"><span class="swatch" :class="c.cls"></span>{{ formatNumber(penta[i] ?? 0) }}</span>
        <span class="p">{{ c.points }}</span>
        <span class="name">{{ c.name }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.penta {
  display: grid;
  gap: var(--sp-2);
  min-width: 0;
}

.title {
  margin: 0;
  font: var(--t-meta);
  color: var(--c-ink);
}

.muted {
  color: var(--c-ink-2);
}

.bar {
  display: flex;
  block-size: var(--sp-3);
  overflow: hidden;
  border-radius: var(--r-pill);
  background: var(--c-sunk);
}

.ll {
  background: var(--c-loss);
}

.ld {
  background: color-mix(in oklab, var(--c-loss) 45%, var(--c-pane));
}

.dd {
  background: color-mix(in oklab, var(--c-ink-2) 55%, var(--c-pane));
}

.wd {
  background: color-mix(in oklab, var(--c-win) 45%, var(--c-pane));
}

.ww {
  background: var(--c-win);
}

.cols {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: var(--sp-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.col {
  display: grid;
  gap: 0;
  justify-items: start;
  min-width: 0;
}

.n {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  font: var(--t-fig);
}

.swatch {
  flex: none;
  inline-size: var(--sp-2);
  block-size: var(--sp-2);
  border-radius: var(--r-pill);
}

.p {
  color: var(--c-ink);
  font: var(--t-meta);
  font-weight: 600;
}

.name {
  overflow: hidden;
  max-inline-size: 100%;
  color: var(--c-ink-2);
  font: var(--t-meta);
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (width < 30rem) {
  .name {
    display: none;
  }
}
</style>
