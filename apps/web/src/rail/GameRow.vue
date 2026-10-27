<script setup lang="ts">
import { computed } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { GameSummary, Side } from "../api/types";
import { ago, resultText, seatName, speedName, timeControl, yourResult } from "../format";
import { isLive } from "../board/modes";
import { useVersions } from "../stores/versions";

/** One game in the rail: the players, the result, accuracy once reviewed, the time control and when. */
const props = defineProps<{ game: GameSummary; current: boolean; nested?: boolean }>();
const emit = defineEmits<{ open: [] }>();
const versions = useVersions();

const name = (side: Side) =>
  props.game.you === side ? "You" : seatName(props.game[side], versions.name(props.game[side].version));
const live = computed(() => isLive(props.game));
const mine = computed(() => yourResult(props.game));
const where = computed(() => {
  const s = props.game.source;
  if (s.type === "lichess" || s.type === "import") return `${speedName[s.speed]}${s.rated ? " · rated" : ""}`;
  return null;
});
/** Each side's accuracy to one decimal (the server sends f32s, which read as 95.80000305175781). */
const accuracy = computed(() => {
  const a = props.game.accuracy;
  return a ? { white: a.white.toFixed(1), black: a.black.toFixed(1) } : null;
});
</script>

<template>
  <EButton
    role="listitem"
    variant="quiet"
    size="sm"
    block
    class="row"
    :class="{ 'row--nested': nested }"
    :aria-current="current ? 'true' : undefined"
    @click="emit('open')"
  >
    <span class="line">
      <span v-if="live" class="pulse" aria-label="Live"></span>
      <span class="players">{{ name("white") }} <span class="vs">–</span> {{ name("black") }}</span>
      <span class="res num" :class="mine ? `res--${mine}` : undefined">{{
        live ? "live" : resultText(game.outcome)
      }}</span>
    </span>
    <span class="meta">
      <span v-if="accuracy" class="acc num" :title="`Accuracy: White ${accuracy.white}, Black ${accuracy.black}`"
        >{{ accuracy.white }} · {{ accuracy.black }}</span
      >
      <span class="num">{{ timeControl(game.time) }}</span>
      <span v-if="where">{{ where }}</span>
      <span class="when">{{ ago(game.started_at) }}</span>
    </span>
  </EButton>
</template>

<style scoped>
.row {
  height: auto;
  white-space: normal;
  display: grid;
  gap: 0.125rem;
  justify-items: stretch;
  place-content: start stretch;
  min-height: 0;
  padding: var(--e-gap-snug) var(--ws-pad, 0.75rem);
  border-radius: 0;
  text-align: start;
  text-decoration: none;
}

.row--nested {
  padding-inline-start: calc(var(--ws-pad, 0.75rem) + 0.75rem);
}

.row[aria-current] {
  background: var(--e-primary-soft);
  box-shadow: inset 3px 0 0 var(--e-primary);
  color: var(--e-ink);
}

.row:focus-visible {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: -2px;
}

.line {
  display: flex;
  align-items: center;
  gap: var(--e-gap-tight);
  min-width: 0;
}

.players {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.vs {
  color: var(--e-muted);
  font-weight: 400;
}

.res {
  flex: none;
  color: var(--e-muted);
  font: 600 0.8125rem/1 var(--e-font-mono);
}

.res--won {
  color: var(--e-success);
}

.res--lost {
  color: var(--e-danger);
}

.meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0 var(--e-gap-snug);
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 400;
}

.acc {
  color: var(--e-ink);
  font-family: var(--e-font-mono);
}

.when {
  margin-inline-start: auto;
}

/* A live game pulses, so the eye finds it at the top of the list. */
.pulse {
  flex: none;
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
  background: var(--e-danger);
  animation: pulse 1.6s ease-in-out infinite;
}

@keyframes pulse {
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .pulse {
    animation: none;
  }
}
</style>
