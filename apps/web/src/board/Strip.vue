<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { Seat } from "../api/types";
import { clock } from "../format";

/** A seat beside the board: who sits there, their rating, their clock, and the result once it is over. */
const props = defineProps<{
  seat: Seat;
  /** The seat's name as shown: "You", "Dameless v14", a Lichess name. */
  name: string;
  /** The clock in ms: what it holds now (live) or after the move on the board. `null` for a game without a clock. */
  ms: number | null;
  /** The clock is running: it counts down from `ms`, set at `since` (`Date.now()`). */
  running?: boolean;
  since?: number;
  /** Whether this side is to move. */
  active?: boolean;
  /** 1, ½ or 0 once the game is over. */
  result?: string | null;
  /** The engine's score, on the strip of the side it favours: shown only where the eval bar is too thin to carry it. */
  score?: string | null;
  /** Which side's colours the score wears. */
  scoreSide?: "white" | "black";
}>();

const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | undefined;
watch(
  () => props.running,
  (running) => {
    clearInterval(timer);
    if (running) timer = setInterval(() => (now.value = Date.now()), 100);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearInterval(timer));

const left = computed(() => {
  if (props.ms === null) return null;
  return props.running ? props.ms - (now.value - (props.since ?? now.value)) : props.ms;
});
const low = computed(() => left.value !== null && left.value < 20_000);
</script>

<template>
  <div class="strip" :class="{ 'strip--active': active }">
    <span class="who">
      <span v-if="seat.title" class="title">{{ seat.title }}</span>
      <span class="name">{{ name }}</span>
      <span v-if="seat.rating" class="rating num">{{ seat.rating }}</span>
    </span>
    <span
      v-if="score"
      class="score num"
      :class="`score--${scoreSide ?? 'white'}`"
      :title="`Engine score: ${score}`"
      :aria-label="`Engine score ${score}`"
      >{{ score }}</span
    >
    <span v-if="result" class="result num" :title="`Scored ${result}`" :aria-label="`Scored ${result}`">{{
      result
    }}</span>
    <span
      v-if="left !== null"
      class="clock num"
      :class="{ 'clock--run': running, 'clock--low': low }"
      role="timer"
      :aria-label="`${name}'s clock`"
      >{{ clock(left) }}</span
    >
  </div>
</template>

<style scoped>
.strip {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
  height: 100%;
}

.who {
  display: flex;
  flex: 1 1 auto;
  align-items: baseline;
  gap: var(--e-gap-tight);
  min-width: 0;
}

.name {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.title {
  color: var(--e-accent-ink, var(--e-primary));
  font: 700 0.75rem/1 var(--e-font-sans);
}

.rating {
  color: var(--e-muted);
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
}

.result {
  font: 700 0.9375rem/1 var(--e-font-mono);
}

/* The eval bar's figure, where the bar is a thin line (stacked): a small chip in the favoured side's colours. */
.score {
  display: none;
  flex: none;
  padding: 0.125rem 0.375rem;
  border-radius: var(--e-radius-sm);
  font: 700 0.75rem/1.1 var(--e-font-mono);
  font-variant-numeric: tabular-nums;
}

.score--white {
  background: #fffaf3;
  color: #2a2420;
  box-shadow: 0 0 0 1px var(--e-border-strong, var(--e-border));
}

.score--black {
  background: #2a2420;
  color: #fffaf3;
}

@media (width < 75rem) {
  .score {
    display: inline-block;
  }
}

/* The clock: a flat plate, lit in the place colour while it runs, red when time is short. */
.clock {
  min-inline-size: 5.5ch;
  padding: 0.25rem 0.5rem;
  border-radius: var(--e-radius-sm);
  background: color-mix(in oklab, var(--e-ink) 8%, transparent);
  font: 600 1.0625rem/1.1 var(--e-font-mono);
  text-align: end;
}

.clock--run {
  background: var(--e-primary);
  color: var(--e-on-primary);
}

.clock--low.clock--run {
  background: var(--e-danger);
  color: var(--e-on-danger, #fff);
}

.strip--active .name::before {
  display: inline-block;
  width: 0.5rem;
  height: 0.5rem;
  margin-inline-end: 0.375rem;
  border-radius: 50%;
  background: var(--e-primary);
  content: "";
}
</style>
