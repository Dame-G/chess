<script setup lang="ts">
import { computed } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import type { components } from "../api/schema";
import { sanMoves, whitePawns, whiteScore, type Position } from "./position";

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
  return props.lines.filter(Boolean).map((l) => {
    const pv = l.pv.slice(0, props.depth ?? 10);
    const sans = sanMoves({ fen: props.position.fen, moves: pv });
    return {
      id: l.multipv,
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
    <li v-for="r in rows" :key="r.id" class="line">
      <span class="score num" :class="r.white ? 'score--white' : 'score--black'">{{ r.score }}</span>
      <span class="pv">
        <template v-for="(m, i) in r.moves" :key="i">
          <span v-if="m.label" class="no num">{{ m.label }}</span>
          <EButton size="sm" variant="quiet" @click="emit('play', m.pv)">{{ m.san }}</EButton>
        </template>
      </span>
    </li>
    <template v-if="!rows.length">
      <li v-for="i in count" :key="i" class="line"><ESkeleton text /></li>
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
  grid-template-columns: 3.5rem minmax(0, 1fr);
  align-items: start;
  gap: var(--e-gap-base);
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-start: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 40%, transparent);
}

.score {
  margin-block-start: 0.125rem;
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
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;

  /* Two rows of moves at most: the rest is one click away on the board. */
  max-height: 3.75rem;
  overflow: hidden;
  font-family: var(--e-font-mono);
}

.no {
  flex: none;
  padding-inline-start: 0.375rem;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.pv :deep(.e-btn) {
  flex: none;
  min-height: 1.75rem;
  padding-inline: 0.3125rem;
  text-decoration: none;
}

.pv :deep(.e-btn:first-of-type) {
  color: var(--e-primary);
  font-weight: 600;
}
</style>
