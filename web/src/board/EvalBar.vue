<script setup lang="ts">
import { computed } from "vue";
import type { Color } from "@lichess-org/chessground/types";
import { winChance } from "../format";

const props = defineProps<{
  /** White's advantage in pawns, mates as ±100. `undefined` before the engine has spoken. */
  pawns?: number;
  /** The text to show, such as `+0.35` or `#3`. */
  text?: string;
  /** Which side is at the bottom of the board. */
  orientation?: Color;
}>();

/**
 * White's share of the bar, from its chance to win. Rounded to a quarter of a percent so depths that agree to within a
 * hair do not retarget the easing while the engine streams.
 */
const white = computed(() => (props.pawns === undefined ? 0.5 : Math.round(winChance(props.pawns * 100) * 400) / 400));
const flipped = computed(() => props.orientation === "black");
const leader = computed<Color>(() => (white.value >= 0.5 ? "white" : "black"));
/** One decimal fits: `+0.3`, `-1.2`; a mate reads `M3`. */
const short = computed(() => {
  const t = props.text;
  if (!t) return "";
  if (t.startsWith("#")) return `M${t.replace(/^#-?/, "")}`;
  const n = Number(t.replace("−", "-"));
  return Number.isFinite(n) ? `${n > 0 ? "+" : ""}${n.toFixed(1)}` : t;
});
/** The figure sits at the end of the bar on the leading side: under it when that side is at the bottom. */
const atBottom = computed(() => (leader.value === "white") !== flipped.value);
</script>

<template>
  <div
    class="evalbar"
    :class="{ 'evalbar--flipped': flipped }"
    role="meter"
    aria-label="Evaluation"
    aria-valuemin="0"
    aria-valuemax="100"
    :aria-valuenow="Math.round(white * 100)"
    :aria-valuetext="text ? `${text}, White ${Math.round(white * 100)}%` : 'No evaluation yet'"
  >
    <span class="evalbar-label num" aria-hidden="true">{{ short && !atBottom ? short : "" }}</span>
    <div class="evalbar-track">
      <div class="evalbar-white" :style="{ transform: `scaleY(${white})` }"></div>
      <div class="evalbar-mid"></div>
    </div>
    <span class="evalbar-label num" aria-hidden="true">{{ short && atBottom ? short : "" }}</span>
  </div>
</template>

<style scoped>
/* The bar sits between two label zones inside the board's height, so the board stays the governing size. */
.evalbar {
  display: grid;
  grid-template-rows: var(--evalbar-label, 1.125rem) minmax(0, 1fr) var(--evalbar-label, 1.125rem);
  justify-items: center;
  width: var(--evalbar-col, 2rem);
  min-height: 0;
}

/* Fixed-width zones with tabular figures: a new figure never moves anything. */
.evalbar-label {
  align-self: center;
  inline-size: 100%;
  contain: layout paint;
  color: var(--e-ink);
  font: 600 0.6875rem/1 var(--e-font-mono);
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.04em;
  text-align: center;
  white-space: nowrap;
}

.evalbar-track {
  position: relative;
  width: var(--evalbar, 0.875rem);
  height: 100%;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: #2a2420;
  box-shadow: 0 0 0 1px var(--e-border);
}

/* White's share is a scale from the bottom (the top when the board is flipped): composited, never a layout. */
.evalbar-white {
  position: absolute;
  inset: 0;
  background: #fffaf3;
  transform-origin: 50% 100%;
  transition: transform 0.6s cubic-bezier(0.22, 0.8, 0.2, 1);
  will-change: transform;
}

.evalbar--flipped .evalbar-white {
  transform-origin: 50% 0;
}

.evalbar-mid {
  position: absolute;
  inset-inline: 0;
  top: 50%;
  height: 1px;
  background: rgb(200 120 60 / 80%);
}

@media (prefers-reduced-motion: reduce) {
  .evalbar-white {
    transition: none;
  }
}
</style>
