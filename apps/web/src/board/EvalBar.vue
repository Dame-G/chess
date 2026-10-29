<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { Color } from "@lichess-org/chessground/types";
import type { Score, Side } from "../api/types";
import { formatScore, pawns, winChance } from "../format";

const props = defineProps<{
  /** White's score, as the engine gave it; `undefined` before it has spoken. */
  score?: Score;
  /** Whose move it is, for a mate already given. */
  turn?: Side;
  /** Which side is at the bottom of the board. */
  orientation?: Color;
}>();

/**
 * White's share of the bar, from its chance to win. Rounded to a quarter of a percent so depths that agree to within a
 * hair do not retarget the easing while the engine streams. It holds the last figure while a new position waits for
 * its first depth, so the bar eases on from where it was instead of snapping back to the middle first.
 */
const white = ref(0.5);
const held = ref("");
watch(
  () => [props.score, props.turn] as const,
  ([score, turn]) => {
    if (!score) return;
    white.value = Math.round(winChance(pawns(score, turn)) * 400) / 400;
    held.value = formatScore(score);
  },
  { immediate: true },
);
const flipped = computed(() => props.orientation === "black");
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
    <div class="evalbar-white" :style="{ '--share': white }"></div>
    <div class="evalbar-mid"></div>
  </div>
</template>

<style scoped>
/* A thin bar as tall as the board; the figure it stands for is a score chip in the strip, so nothing is printed on it. */
.evalbar {
  position: relative;
  box-sizing: border-box;
  width: 100%;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  border-radius: var(--r-1);
  background: var(--eval-black);
  box-shadow: 0 0 0 var(--e-stroke-hair) var(--c-line-strong);
}

/* White's share is a scale from the bottom (the top when the board is flipped): composited, never a layout. */
.evalbar-white {
  position: absolute;
  inset: 0;
  background: var(--eval-white);
  transform: scaleY(var(--share, 0.5));
  transform-origin: 50% 100%;
  transition: transform 0.6s cubic-bezier(0.22, 0.8, 0.2, 1);
  will-change: transform;
}

.evalbar--flipped .evalbar-white {
  transform-origin: 50% 0;
}

/* Level is the middle: a neutral hairline that shows on both halves. */
.evalbar-mid {
  position: absolute;
  inset-inline: 0;
  top: 50%;
  height: var(--e-stroke-hair);
  background: var(--c-ink-3);
}

@media (prefers-reduced-motion: reduce) {
  .evalbar-white {
    transition: none;
  }
}
</style>
