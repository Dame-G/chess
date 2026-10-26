<script setup lang="ts">
import { computed } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import type { components } from "../api/schema";
import { bestFirst, sanMoves, whitePawns, whiteScore, type Position } from "./position";

type Info = components["schemas"]["Info"];

const props = defineProps<{
  position: Position;
  lines: Info[];
  /** How many lines to expect, for the placeholders. */
  count: number;
  /** Plies shown from each line. */
  depth?: number;
}>();
const emit = defineEmits<{
  /** The first `n` moves of a line, to play on the board. */
  play: [moves: string[]];
}>();

const rows = computed(() => {
  const parts = props.position.fen.split(" ");
  const blackFirst = parts[1] === "b";
  const first = Number(parts[5] ?? 1);
  return bestFirst(props.lines.filter(Boolean)).map((l) => {
    const pv = l.pv.slice(0, props.depth ?? 10);
    const sans = sanMoves({ fen: props.position.fen, moves: pv });
    return {
      id: l.multipv,
      // What a line looks like, in one string: a report that repeats it leaves the line alone.
      sig: `${whiteScore(l.score, props.position.turn)}|${props.position.fen}|${pv.join(" ")}`,
      white: whitePawns(l.score, props.position.turn) >= 0,
      score: whiteScore(l.score, props.position.turn),
      moves: sans.map((san, i) => {
        const ply = i + (blackFirst ? 1 : 0);
        const number = first + Math.floor(ply / 2);
        return { san, label: ply % 2 === 0 ? `${number}.` : i === 0 ? `${number}…` : "", pv: pv.slice(0, i + 1) };
      }),
    };
  });
});
</script>

<template>
  <ol class="lines" aria-label="Engine lines">
    <li v-for="r in rows" :key="r.id" v-memo="[r.sig]" class="line">
      <span class="score num" :class="r.white ? 'score--white' : 'score--black'">{{ r.score }}</span>
      <span class="pv">
        <template v-for="(m, i) in r.moves" :key="i">
          <span v-if="m.label" class="no num">{{ m.label }}</span>
          <EButton size="sm" variant="quiet" @click="emit('play', m.pv)">{{ m.san }}</EButton>
        </template>
      </span>
    </li>
    <template v-if="!rows.length">
      <li v-for="i in count" :key="i" class="line line--wait" aria-hidden="true">
        <ESkeleton :height="22" />
        <ESkeleton :height="14" />
      </li>
    </template>
  </ol>
</template>

<style scoped>
.lines {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.line {
  display: grid;
  min-height: 2.25rem;
  grid-template-columns: 3.5rem minmax(0, 1fr);
  align-items: center;
  gap: var(--e-gap-base);
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-start: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.score {
  padding: 0.125rem 0.25rem;
  border-radius: var(--e-radius-sm);
  font: 600 var(--e-text-sm) / 1.3 var(--e-font-mono);
  text-align: center;
}

.score--white {
  background: #f4ede2;
  box-shadow: inset 0 0 0 1px rgb(0 0 0 / 12%);
  color: #2a2420;
}

.score--black {
  background: #2a2420;
  color: #f4ede2;
}

.pv {
  /* One row, cut with an ellipsis as Lichess does: the line is long, the panel is not. */
  display: block;
  min-width: 0;
  overflow: hidden;
  font-family: var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.no {
  flex: none;
  padding-inline-start: 0.375rem;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.pv :deep(.e-btn) {
  height: 1.625rem;
  min-height: 0;
  padding-block: 0;
  padding-inline: 0.3125rem;
  text-decoration: none;
}

.pv :deep(.e-btn:first-of-type) {
  color: var(--e-primary);
  font-weight: 600;
}
</style>
