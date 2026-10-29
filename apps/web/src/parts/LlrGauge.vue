<script setup lang="ts">
import { computed } from "vue";
import type { Llr } from "../api/types";
import { signed } from "../format";
import { gaugePosition } from "../sheets/idea";
import Term from "./Term.vue";

/**
 * The evidence in a trial between its two lines. The marker moves right as the change looks better and left as it
 * does not; reaching the right line keeps it, the left line drops it. From the zero tick to the marker the track is
 * filled in the outcome's colour, so how far it has gone reads without finding a 2 px bar. `compact` is the thin form
 * for a list row: the track alone, with the value beside it.
 */
const props = defineProps<{ llr: Llr; compact?: boolean }>();
const at = computed(() => gaugePosition(props.llr));
const zero = computed(() => gaugePosition({ ...props.llr, value: 0 }));
const fill = computed(() => ({
  insetInlineStart: `${Math.min(at.value, zero.value) * 100}%`,
  inlineSize: `${Math.abs(at.value - zero.value) * 100}%`,
}));
const tone = computed(() => (props.llr.value >= 0 ? "win" : "loss"));
const text = computed(
  () =>
    `Evidence ${props.llr.value.toFixed(2)}, between ${props.llr.lower.toFixed(2)} (dropped) and ${props.llr.upper.toFixed(2)} (kept)`,
);
</script>

<template>
  <div class="gauge" :class="{ 'gauge--compact': compact }">
    <div v-if="!compact" class="head">
      <span class="label"><Term k="llr" text="Evidence" /></span>
      <span class="now">{{ signed(llr.value) }}</span>
    </div>
    <div class="line">
      <div
        class="track"
        role="meter"
        :aria-valuemin="llr.lower"
        :aria-valuemax="llr.upper"
        :aria-valuenow="llr.value"
        :aria-valuetext="text"
        aria-label="Evidence so far"
      >
        <span class="fill" :class="`fill--${tone}`" :style="fill"></span>
        <span class="zero" :style="{ insetInlineStart: `${zero * 100}%` }"></span>
        <span class="marker" :style="{ insetInlineStart: `${at * 100}%` }"></span>
      </div>
      <span v-if="compact" class="now now--compact">{{ signed(llr.value) }}</span>
    </div>
    <div v-if="!compact" class="ends">
      <span
        ><span class="figure">{{ signed(llr.lower) }}</span> drops it</span
      >
      <span
        >keeps it <span class="figure">{{ signed(llr.upper) }}</span></span
      >
    </div>
  </div>
</template>

<style scoped>
.gauge {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}

.head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--sp-3);
}

.label {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.now {
  min-inline-size: 5ch;
  font: var(--t-fig-lg);
  text-align: end;
}

.line {
  display: flex;
  align-items: center;
  gap: var(--sp-3);
}

.track {
  position: relative;
  flex: 1 1 auto;
  block-size: var(--sp-3);
  margin-inline: var(--sp-2);
  border-radius: var(--r-pill);
  background: var(--c-sunk);
}

.gauge--compact .track {
  block-size: var(--sp-2);
}

.fill {
  position: absolute;
  inset-block: 0;
  opacity: 0.55;
  transition:
    inset-inline-start var(--m-move) var(--m-ease),
    inline-size var(--m-move) var(--m-ease);
}

.fill--win {
  background: var(--c-win);
}

.fill--loss {
  background: var(--c-loss);
}

.zero {
  position: absolute;
  inset-block: calc(-1 * var(--sp-1));
  inline-size: var(--e-stroke-hair);
  background: var(--c-ink-3);
}

/* A pin: a ring of the pane's colour around an ink dot taller than the track, so it is found at a glance. */
.marker {
  position: absolute;
  inset-block-start: 50%;
  inline-size: var(--sp-4);
  block-size: var(--sp-4);
  margin: calc(-0.5 * var(--sp-4)) 0 0 calc(-0.5 * var(--sp-4));
  border: var(--e-stroke-focus) solid var(--c-pane);
  border-radius: var(--r-pill);
  background: var(--c-ink);
  box-sizing: border-box;
  transition: inset-inline-start var(--m-move) var(--m-ease);
}

.gauge--compact .marker {
  inline-size: var(--sp-3);
  block-size: var(--sp-3);
  margin: calc(-0.5 * var(--sp-3)) 0 0 calc(-0.5 * var(--sp-3));
}

.now--compact {
  flex: none;
  font: var(--t-fig);
}

.ends {
  display: flex;
  justify-content: space-between;
  gap: var(--sp-2);
  padding-inline: var(--sp-2);
  color: var(--c-ink-2);
  font: var(--t-meta);
  white-space: nowrap;
}

.figure {
  color: var(--c-ink);
  font: var(--t-fig);
  font-size: inherit;
}

@media (prefers-reduced-motion: reduce) {
  .fill,
  .marker {
    transition: none;
  }
}
</style>
