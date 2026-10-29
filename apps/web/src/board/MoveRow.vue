<script setup lang="ts">
import Glyph from "./Glyph.vue";
import type { Classification } from "../api/types";

/**
 * One line of a move list: the number and up to two moves, each a button the list reads by `data-id`. Its props are
 * plain values, so a step on the board only re-renders the rows whose state changed, not the whole game. The move on
 * the board is the one `aria-current` marks (the list scrolls to it and keeps focus on it).
 */
defineProps<{
  number: number;
  /** White's move and Black's in SAN; `null` for the gap before Black's first move of a game that starts on Black's turn. */
  a: string | null;
  b: string | null;
  /** What names each move to the list: its ply in a game, its node in a tree. */
  pa: number;
  pb: number;
  /** Whether each move is the one on the board, and whether it is still ahead of it. */
  curA: boolean;
  curB: boolean;
  futA: boolean;
  futB: boolean;
  /** The move that takes the tab stop: one per list, so Tab passes the list in one press. */
  tabA: boolean;
  tabB: boolean;
  /** What the review judged each move, and the line it has to say about it; nothing without a review. */
  ma?: Classification | null;
  mb?: Classification | null;
  ha?: string;
  hb?: string;
}>();
</script>

<template>
  <div class="mrow">
    <span class="mnum num">{{ number }}.</span>
    <button
      v-if="a"
      type="button"
      class="mv"
      :class="{ 'mv--future': futA }"
      :data-id="pa"
      :aria-current="curA ? 'true' : undefined"
      :tabindex="tabA ? 0 : -1"
      :title="ha || undefined"
    >
      <span class="mv-san">{{ a }}</span
      ><Glyph :of="ma" />
    </button>
    <span v-else class="mv mv--gap" aria-hidden="true">…</span>
    <button
      v-if="b"
      type="button"
      class="mv"
      :class="{ 'mv--future': futB }"
      :data-id="pb"
      :aria-current="curB ? 'true' : undefined"
      :tabindex="tabB ? 0 : -1"
      :title="hb || undefined"
    >
      <span class="mv-san">{{ b }}</span
      ><Glyph :of="mb" />
    </button>
    <span v-else class="mv" aria-hidden="true" />
  </div>
</template>

<style scoped>
.mrow {
  display: grid;
  grid-template-columns: 2.5rem minmax(0, 1fr) minmax(0, 1fr);
  min-height: var(--h-move);
}

/* The numbers sit in a quiet gutter that runs the length of the list, as in a chess client. */
.mnum {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding-inline: var(--sp-2);
  background: var(--c-raise);
  color: var(--c-ink-2);
  font: var(--t-fig);
  font-size: var(--e-text-xs);
}

.mv {
  display: flex;
  align-items: center;
  gap: 0;
  box-sizing: border-box;
  min-width: 0;
  min-height: var(--h-move);
  margin: 0;
  padding: 0 var(--sp-3);
  border: 0;
  border-radius: 0;
  background: transparent;
  color: var(--c-ink);
  font: var(--t-fig);
  text-align: start;
  cursor: pointer;
}

.mv-san {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

button.mv:hover {
  background: var(--c-raise);
}

/* A move still ahead of the board reads quieter, in ink rather than in transparency, so it stays legible. */
.mv--future {
  color: var(--c-ink-2);
}

.mv--gap {
  color: var(--c-ink-3);
  cursor: default;
}

.mv[aria-current] {
  background: var(--c-sunk);
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--c-act);
  color: var(--c-ink);
  font-weight: 700;
}

.mv:focus-visible {
  position: relative;
  z-index: var(--e-z-raised);
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}
</style>
