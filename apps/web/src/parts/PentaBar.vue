<script setup lang="ts">
import { computed } from "vue";

/**
 * How the game pairs ended. Each opening is played twice with the colours swapped, so a pair scores 0, ½, 1, 1½ or 2:
 * the shape of the bar shows whether wins and losses balance, and a lopsided bar is the first sign of a bug.
 */
const props = defineProps<{ penta: readonly number[] }>();
const total = computed(() => props.penta.reduce((a, b) => a + b, 0));
const COLUMNS = [
  { label: "0", name: "Two losses", cls: "ll" },
  { label: "½", name: "A loss and a draw", cls: "ld" },
  { label: "1", name: "Even", cls: "dd" },
  { label: "1½", name: "A win and a draw", cls: "wd" },
  { label: "2", name: "Two wins", cls: "ww" },
];
const share = (n: number) => (total.value ? `${(100 * n) / total.value}%` : "0%");
const words = computed(() => COLUMNS.map((c, i) => `${c.name} ${props.penta[i] ?? 0}`).join(", "));
</script>

<template>
  <div class="penta">
    <div class="bar" role="img" :aria-label="`Pairs: ${words}`">
      <span
        v-for="(c, i) in COLUMNS"
        :key="c.cls"
        class="seg"
        :class="c.cls"
        :style="{ inlineSize: share(penta[i] ?? 0) }"
      ></span>
    </div>
    <ul class="plain cols" aria-hidden="true">
      <li v-for="(c, i) in COLUMNS" :key="c.cls" class="col">
        <span class="n mono">{{ penta[i] ?? 0 }}</span>
        <span class="l mono">{{ c.label }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.penta {
  display: grid;
  gap: 0.25rem;
}

.bar {
  display: flex;
  block-size: 0.75rem;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: var(--e-border);
}

.ll {
  background: var(--e-danger);
}

.ld {
  background: color-mix(in oklab, var(--e-danger) 45%, transparent);
}

.dd {
  background: color-mix(in oklab, var(--e-muted) 55%, transparent);
}

.wd {
  background: color-mix(in oklab, var(--e-success) 45%, transparent);
}

.ww {
  background: var(--e-success);
}

.cols {
  grid-template-columns: repeat(5, 1fr);
  text-align: center;
}

.col {
  display: grid;
}

.n {
  font-size: 0.8125rem;
}

.l {
  color: var(--e-muted);
  font-size: 0.75rem;
}
</style>
