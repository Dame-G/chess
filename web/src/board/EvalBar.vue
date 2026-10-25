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

/** White's share of the bar, from its chance to win. */
const white = computed(() => (props.pawns === undefined ? 0.5 : winChance(props.pawns * 100)));
const flipped = computed(() => props.orientation === "black");
const leader = computed(() => (white.value >= 0.5 ? "white" : "black"));
/** One decimal fits the bar: `+0.3`, `-1.2`; mates stay as given. */
const short = computed(() => {
  const t = props.text;
  if (!t || t.startsWith("#")) return t;
  const n = Number(t.replace("−", "-"));
  return Number.isFinite(n) ? `${n > 0 ? "+" : ""}${n.toFixed(1)}` : t;
});
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
    <div class="evalbar-white" :style="{ height: `${white * 100}%` }"></div>
    <div class="evalbar-mid" aria-hidden="true"></div>
    <span v-if="text" class="evalbar-text" :class="`evalbar-text--${leader}`" aria-hidden="true">{{ short }}</span>
  </div>
</template>

<style scoped>
.evalbar {
  position: relative;
  display: flex;
  flex-direction: column-reverse;
  width: var(--evalbar, 1.5rem);
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  background: #2a2420;
  box-shadow: 0 0 0 1px var(--e-border);
}

.evalbar--flipped {
  flex-direction: column;
}

.evalbar-white {
  background: linear-gradient(90deg, #f4ede2, #fffaf3);
  transition: height 0.45s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.evalbar-mid {
  position: absolute;
  inset-inline: 0;
  top: 50%;
  height: 1px;
  background: rgb(200 120 60 / 70%);
}

.evalbar-text {
  position: absolute;
  inset-inline: 0;
  font: 600 0.625rem/1 var(--e-font-mono);
  text-align: center;
  letter-spacing: -0.04em;
}

/* The figure sits at the leading side's end, in that side's ink. */
.evalbar-text--white {
  bottom: 0.3rem;
  color: #2a2420;
}

.evalbar-text--black {
  top: 0.3rem;
  color: #f4ede2;
}

.evalbar--flipped .evalbar-text--white {
  top: 0.3rem;
  bottom: auto;
}

.evalbar--flipped .evalbar-text--black {
  top: auto;
  bottom: 0.3rem;
}

@media (prefers-reduced-motion: reduce) {
  .evalbar-white {
    transition: none;
  }
}
</style>
