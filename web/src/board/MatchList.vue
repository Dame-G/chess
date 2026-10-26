<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import { ago } from "../format";
import { gameClock } from "../lichess";
import { matchTimeText, movesText, sideScore, youSide, type MatchSummary, type Side } from "./matchText";

defineProps<{
  matches: MatchSummary[];
  /** The match on the board. */
  current?: number;
}>();
const emit = defineEmits<{ open: [id: number] }>();

/** The time control, and how long it went; a match still being played has no count worth showing. */
const detail = (m: MatchSummary) => {
  const clock = m.lichess ? `Lichess · ${gameClock(m)}` : matchTimeText(m.time);
  return m.status === "running" ? clock : `${clock} · ${movesText(m.plies)}`;
};

/** The side of ours in a game with a person or a bot on the other side: you, or the build that plays on Lichess. */
const ours = (m: MatchSummary): Side | undefined => youSide(m) ?? m.lichess?.our_side;
/**
 * One rule for the result: a game that has a side of ours (yours, or our bot's on Lichess) reads `Won`, `Lost` or
 * `Draw`; a match of two engines reads as a score (`1–0`, `½–½`, `0–1`). A game that did not finish is `Stopped`.
 */
const shown = (m: MatchSummary) => {
  if (m.status === "aborted" || m.result === "*") return "Stopped";
  const mine = ours(m);
  if (!mine) return `${sideScore(m.result, "white")}–${sideScore(m.result, "black")}`;
  const score = sideScore(m.result, mine);
  return score === "1" ? "Won" : score === "0" ? "Lost" : "Draw";
};
/** Who a side is: our bot on Lichess is `Dameless (main)`, not just the build's ref. */
const nameOf = (m: MatchSummary, side: Side) =>
  m.lichess && m[side].build != null ? `Dameless (${m[side].name})` : m[side].name;
/** Which side won, for the row to set the winner in bold and the loser back. */
const won = (m: MatchSummary, side: "white" | "black") =>
  m.result === (side === "white" ? "1-0" : "0-1") ? "win" : m.result === "*" || m.result === "1/2-1/2" ? "" : "lose";
</script>

<template>
  <div class="matches" role="list" aria-label="Matches">
    <EButton
      v-for="m in matches"
      :key="m.id"
      role="listitem"
      variant="quiet"
      size="sm"
      block
      class="row"
      :aria-current="m.id === current ? 'true' : undefined"
      :aria-label="`${nameOf(m, 'white')} against ${nameOf(m, 'black')}${m.status === 'running' ? ', live' : m.status === 'aborted' ? ', stopped' : ''}`"
      @click="emit('open', m.id)"
    >
      <span class="what">
        <span class="names">
          <span :class="['n', won(m, 'white')]">{{ nameOf(m, "white") }}</span>
          <span class="vs">vs</span>
          <span :class="['n', won(m, 'black')]">{{ nameOf(m, "black") }}</span>
        </span>
        <span class="tc num">{{ detail(m) }}</span>
      </span>
      <span v-if="m.status === 'running'" class="res res--live num"
        ><span class="dot" aria-hidden="true"></span>Live</span
      >
      <span v-else class="res num">
        {{ shown(m) }}
        <span class="when">{{ ago(m.created_at) }}</span>
      </span>
    </EButton>
  </div>
</template>

<style scoped>
.matches {
  display: grid;
  align-content: start;
}

.row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: baseline;
  gap: var(--e-gap-snug);

  /* A row holds two lines, so it grows with them: the button's own fixed control height would let the text spill over the next row. */
  block-size: auto;
  min-block-size: 2.75rem;
  padding: var(--e-gap-snug) var(--ws-pad);
  border-radius: 0;
  text-align: start;
  text-decoration: none;
}

.row[aria-current] {
  background: var(--e-primary-soft);
  box-shadow: inset 3px 0 0 var(--e-primary);
  color: var(--e-ink);
  font-weight: 600;
}

.row:focus-visible {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: -2px;
}

.res {
  justify-self: end;
  text-align: end;
  line-height: 1.25rem;
  color: var(--e-muted);
  font-size: var(--e-text-sm);
  font-weight: 700;
  white-space: nowrap;
}

.when {
  display: block;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 400;
}

.res--live {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  color: var(--e-info, var(--e-primary));
}

.dot {
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
  background: currentcolor;
  animation: pulse 1.6s ease-in-out infinite;
}

@keyframes pulse {
  50% {
    opacity: 0.3;
  }
}

@media (prefers-reduced-motion: reduce) {
  .dot {
    animation: none;
  }
}

.what {
  display: grid;
  min-width: 0;
}

.names {
  display: flex;
  line-height: 1.25rem;
  gap: 0.375rem;
  align-items: baseline;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
}

.n {
  overflow: hidden;
  text-overflow: ellipsis;
}

.win {
  font-weight: 700;
}

.lose {
  color: var(--e-muted);
}

.tc {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.vs,
.tc {
  flex: none;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
