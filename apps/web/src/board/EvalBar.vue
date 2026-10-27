<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { Color } from "@lichess-org/chessground/types";
import { signed, winChance } from "../format";

const props = defineProps<{
  /** White's advantage in pawns, mates as ±100. `undefined` before the engine has spoken. */
  pawns?: number;
  /** The text to show, such as `+0.35` or `M3`. */
  text?: string;
  /** Which side is at the bottom of the board. */
  orientation?: Color;
}>();

/**
 * White's share of the bar, from its chance to win. Rounded to a quarter of a percent so depths that agree to within a
 * hair do not retarget the easing while the engine streams.
 */
const white = ref(0.5);
/**
 * The figure last shown. Both hold while a new position waits for its first depth, so the bar eases on from where it
 * was instead of snapping back to the middle first.
 */
const held = ref("");
watch(
  () => [props.pawns, props.text] as const,
  ([pawns, text]) => {
    if (pawns === undefined) return;
    white.value = Math.round(winChance(pawns) * 400) / 400;
    held.value = text ?? "";
  },
  { immediate: true },
);
const flipped = computed(() => props.orientation === "black");
const leader = computed<Color>(() => (white.value >= 0.5 ? "white" : "black"));
/** One decimal fits: `+0.3`, `-1.2`; a mate reads `M3`. */
const short = computed(() => {
  const t = held.value;
  if (!t) return "";
  if (t.includes("M")) return `${/^[-−]/.test(t) ? "−" : ""}M${t.match(/\d+/)?.[0] ?? ""}`;
  const n = Number(t.replace("−", "-"));
  return Number.isFinite(n) ? signed(n, 1) : t;
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
    :aria-valuetext="held ? `${held}, White ${Math.round(white * 100)}%` : 'No evaluation yet'"
  >
    <div class="evalbar-white" :style="{ transform: `scaleY(${white})` }"></div>
    <div class="evalbar-mid"></div>
    <span
      v-if="short"
      class="evalbar-label num"
      :class="[atBottom ? 'evalbar-label--bottom' : 'evalbar-label--top', `evalbar-label--${leader}`]"
      aria-hidden="true"
      >{{ short }}</span
    >
  </div>
</template>

<style scoped>
/* The bar is exactly the board's height and its figure sits inside it, at the leading side's end, on that side's
   colour, so it always reads and nothing hangs above or below the board. */
.evalbar {
  position: relative;
  width: var(--col, 1.75rem);
  min-height: 0;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: #2a2420;
  box-shadow: 0 0 0 1px var(--e-border-strong, var(--e-border));
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

/* Tabular digits: a new figure never moves anything. */
.evalbar-label {
  position: absolute;
  inset-inline: 0;
  padding-block: 0.25rem;
  font: 700 0.625rem/1 var(--e-font-mono);
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.08em;
  text-align: center;
  white-space: nowrap;
}

.evalbar-label--top {
  top: 0;
}

.evalbar-label--bottom {
  bottom: 0;
}

.evalbar-label--white {
  color: #2a2420;
}

.evalbar-label--black {
  color: #fffaf3;
}

@media (prefers-reduced-motion: reduce) {
  .evalbar-white {
    transition: none;
  }
}
</style>
