<script setup lang="ts">
import { computed } from "vue";
import type { Score, Side } from "../api/types";
import { formatScore, scoreSide } from "../format";

/**
 * An engine score: `+0.6`, `−4.2`, `#3`. White's point of view by default, where the side ahead is shown as a small
 * pip (white or black) and the figure carries no colour, since White being ahead is not "good". From the mover's point
 * of view (a profile's own moves) positive is good and the figure takes the outcome colours, with no pip.
 */
const props = withDefaults(
  defineProps<{
    score: Score | number | null | undefined;
    perspective?: "white" | "mover";
    size?: "sm" | "md";
    /** Whose move it is, for a mate already given (`mate: 0` is against the side to move). */
    turn?: Side;
  }>(),
  { perspective: "white", size: "md", turn: "white" },
);

const text = computed(() => formatScore(props.score));
const side = computed(() => scoreSide(props.score, props.turn));

const tone = computed(() => {
  if (props.perspective !== "mover" || !side.value) return null;
  return side.value === "white" ? "win" : "loss";
});

const words = computed(() => {
  if (props.score == null) return "No score";
  const mate = typeof props.score === "object" && "mate" in props.score;
  const who = props.perspective === "mover" ? (side.value === "white" ? "You" : "Opponent") : side.value;
  if (!side.value) return "Level";
  const name = who === "white" ? "White" : who === "black" ? "Black" : who;
  return mate ? `${name} mates (${text.value})` : `${name} is ahead (${text.value})`;
});
</script>

<template>
  <span class="ui-score" :class="[`ui-score--${size}`, tone && `ui-score--${tone}`]" :title="words">
    <span
      v-if="perspective === 'white' && side"
      class="ui-score-pip"
      :class="`ui-score-pip--${side}`"
      aria-hidden="true"
    />
    <span class="ui-score-text num" aria-hidden="true">{{ text }}</span>
    <span class="sr-only">{{ words }}</span>
  </span>
</template>

<style scoped>
.ui-score {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
  color: var(--c-ink);
  font: var(--t-fig);
  white-space: nowrap;
}

.ui-score--sm {
  font-size: var(--e-text-xs);
}

.ui-score--win {
  color: var(--c-win);
}

.ui-score--loss {
  color: var(--c-loss);
}

.ui-score-pip {
  flex: none;
  box-sizing: border-box;
  width: var(--dot);
  height: var(--dot);
  border: var(--e-stroke-hair) solid var(--c-line-strong);
  border-radius: var(--r-0);
}

.ui-score-pip--white {
  background: var(--c-pip-white);
}

.ui-score-pip--black {
  background: var(--c-pip-black);
}
</style>
