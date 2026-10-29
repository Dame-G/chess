<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { Score, Side } from "../api/types";
import { clock, formatNumber } from "../format";
import type { PlayerRef } from "../ui/player";
import UiPlayer from "../ui/UiPlayer.vue";
import UiScore from "../ui/UiScore.vue";

/**
 * A seat beside the board, one line of fixed height: who sits there (the side they play as, their name and rating),
 * the material they are ahead by, and the clock. The side to move says so when no clock runs to say it for them, and
 * the engine's score rides at the end where the eval bar is too thin to carry one.
 */
const props = defineProps<{
  /** Who sits here: a name, and what to show beside it. A side without a game is just "White" or "Black". */
  player: PlayerRef;
  side: Side;
  /** The clock in ms: what it holds now (live) or after the move on the board. `null` for a game without a clock. */
  ms: number | null;
  /** The clock is running: it counts down from `ms`, set at `since` (`Date.now()`). */
  running?: boolean;
  since?: number;
  /** This side is to move, and the position is one a move is waiting in. */
  toMove?: boolean;
  /** How many pawns of material this side is ahead by (0 or less shows nothing). */
  material?: number;
  /** The engine's score on the board, shown at the end of the strip when asked for. */
  score?: { score: Score; turn: Side } | null;
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
  <div class="strip">
    <UiPlayer :seat="player" :colour="side" class="who" />
    <span
      v-if="material && material > 0"
      class="material num"
      :title="`${formatNumber(material)} pawns of material ahead`"
      >+{{ material }}</span
    >
    <span v-if="toMove && !running" class="turn">to move</span>
    <span class="end">
      <UiScore v-if="score" :score="score.score" :turn="score.turn" size="sm" />
      <span
        v-if="left !== null"
        class="clock num"
        :class="{ 'clock--run': running, 'clock--low': low && running }"
        role="timer"
        :aria-label="`${player.name}'s clock`"
        >{{ clock(left) }}</span
      >
    </span>
  </div>
</template>

<style scoped>
.strip {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  min-width: 0;
  height: 100%;
}

.who {
  flex: 0 1 auto;
}

.material {
  flex: none;
  color: var(--c-ink-2);
  font: var(--t-fig);
}

.turn {
  flex: none;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.end {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--sp-3);
  margin-inline-start: auto;
}

/* The clock: a quiet figure while it waits; on a wash with the action colour's bar under it while it runs. */
.clock {
  position: relative;
  min-inline-size: 5.5ch;
  padding: var(--sp-1) var(--sp-2);
  border-radius: var(--r-1);
  color: var(--c-ink-2);
  font: var(--t-clock);
  text-align: end;
}

.clock--run {
  background: var(--c-sunk);
  color: var(--c-ink);
}

.clock--run::after {
  content: "";
  position: absolute;
  inset: auto var(--sp-2) 0;
  height: var(--e-stroke-focus);
  background: var(--c-act);
}

.clock--low {
  background: var(--c-loss-wash);
  color: var(--c-loss);
}

.clock--low::after {
  background: var(--c-loss);
}
</style>
