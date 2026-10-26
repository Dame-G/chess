<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { clockText, type Side } from "./matchText";

const props = defineProps<{
  side: Side;
  /** The build's ref, or a Lichess player's name. */
  name: string;
  /** A Lichess title such as GM or BOT. */
  title?: string | null;
  /** What follows the name: the start of the build's commit, a Lichess rating. */
  detail: string;
  /** What the clock holds, in milliseconds, as of `at`. `null` when the match has no clock. */
  ms: number | null;
  /** When `ms` was true, as a `performance.now()` reading. */
  at: number;
  /** Whether the clock is running down from `ms`. */
  ticking: boolean;
  /** Shown in the clock's place when there is no clock, such as the time of the last move. */
  idle?: string;
  /** The side to move in a game that is on. */
  active: boolean;
  /** Under this many milliseconds the clock shows its low-time state. */
  lowMs: number;
  /** The result for this side once the game is over: `1`, `0` or `½`. */
  score?: string;
}>();

const clock = ref<HTMLElement>();
let frame = 0;
let shown = "";
let wasLow = false;

/** The clock's text, written straight to the element: a clock redraws every frame and nothing else should. */
function paint() {
  frame = 0;
  const el = clock.value;
  if (!el) return;
  if (props.ms === null) {
    if (shown !== (props.idle ?? "")) el.textContent = shown = props.idle ?? "";
    return;
  }
  const left = props.ticking ? props.ms - (performance.now() - props.at) : props.ms;
  const text = clockText(left);
  if (text !== shown) el.textContent = shown = text;
  const low = left <= props.lowMs;
  if (low !== wasLow) {
    wasLow = low;
    el.toggleAttribute("data-low", low);
  }
  if (props.ticking) frame = requestAnimationFrame(paint);
}
function restart() {
  cancelAnimationFrame(frame);
  paint();
}
onMounted(restart);
watch(() => [props.ms, props.at, props.ticking, props.idle, props.lowMs], restart);
onBeforeUnmount(() => cancelAnimationFrame(frame));
</script>

<template>
  <div class="strip" :class="{ 'strip--active': active }" role="group" :aria-label="`${side}: ${name}`">
    <span :class="['chip', `chip--${side}`]" aria-hidden="true"></span>
    <span class="who">
      <span v-if="title" class="title">{{ title }}</span>
      <span class="name">{{ name }}</span>
      <span class="commit num">{{ detail }}</span>
    </span>
    <EStamp
      v-if="score"
      size="sm"
      class="score"
      :tone="score === '1' ? 'success' : 'neutral'"
      :label="score"
      :title="score === '1' ? 'Won' : score === '0' ? 'Lost' : 'Drew'"
      :aria-label="`Scored ${score}`"
    />
    <span
      ref="clock"
      class="clock num"
      :class="{ 'clock--idle': ms === null }"
      :title="ms === null ? 'Time of its last move' : undefined"
      :role="ms === null ? undefined : 'timer'"
    ></span>
  </div>
</template>

<style scoped>
.strip {
  display: flex;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
  height: 100%;
  padding-inline: var(--e-pad-control);
  border-radius: var(--e-radius-sm);
  background: color-mix(in oklab, var(--e-ink) 4%, transparent);
  box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--e-border) 60%, transparent);
  transition:
    background-color 0.2s,
    box-shadow 0.2s;
}

/* The side that is thinking: a cognac edge and a warmer ground, so it reads at a glance without moving anything. */
.strip--active {
  background: var(--e-primary-soft);
  box-shadow:
    inset 0 0 0 1px color-mix(in oklab, var(--e-primary) 45%, transparent),
    inset 3px 0 0 var(--e-primary);
}

.chip {
  flex: none;
  width: 0.75rem;
  height: 0.75rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
}

.chip--white {
  background: #fffaf3;
}

.chip--black {
  background: #2a2420;
}

.who {
  display: flex;
  flex: 1;
  align-items: baseline;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.name {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.title {
  flex: none;
  color: var(--e-primary);
  font: var(--e-type-small);
  font-weight: 700;
}

.commit {
  flex: none;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.score {
  flex: none;
}

/* Big, mono and tabular: the digits never shift as they change. */
.clock {
  flex: none;
  min-width: 5.5ch;
  margin-inline-start: var(--e-gap-snug);
  font: 600 var(--e-text-2xl) / 1 var(--e-font-mono);
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.02em;
  text-align: end;
}

.clock--idle {
  color: var(--e-muted);
  font-size: var(--e-text-sm);
  font-weight: 400;
}

.clock[data-low] {
  color: var(--e-danger);
}
</style>
