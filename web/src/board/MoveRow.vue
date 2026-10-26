<script setup lang="ts">
import { EButton } from "@eos-ui/vue/components/EButton";
import ReviewGlyph from "./ReviewGlyph.vue";
import type { Classification } from "./review";

/**
 * One line of the move list: the number and up to two moves. Its props are plain values, so a step on the board only
 * re-renders the two rows whose state changed, not the whole game.
 */
defineProps<{
  number: number;
  /** White's move and Black's, `null` for the gap before Black's first move in a game that starts on Black's turn. */
  a: string | null;
  b: string | null;
  /** The plies of `a` and `b`, as the board counts them. */
  pa: number;
  pb: number;
  /** Whether each move is the one on the board, and whether it is still ahead. */
  curA: boolean;
  curB: boolean;
  futA: boolean;
  futB: boolean;
  /** Whether each move is the list's one tab stop: the others are reached with the arrow keys, which step the board. */
  stopA: boolean;
  stopB: boolean;
  /** What the review judged each move, and the line it has to say about it; nothing without a review. */
  ma?: Classification | null;
  mb?: Classification | null;
  ha?: string;
  hb?: string;
}>();
const emit = defineEmits<{ go: [ply: number] }>();
</script>

<template>
  <span class="no num" role="presentation">{{ number }}</span>
  <EButton
    v-if="a"
    role="listitem"
    size="sm"
    variant="quiet"
    class="mv"
    :tabindex="stopA ? 0 : -1"
    :aria-current="curA ? 'step' : undefined"
    :class="{ future: futA }"
    :data-ply="pa"
    :title="ha || undefined"
    @click="emit('go', pa)"
  >
    {{ a }}<ReviewGlyph :of="ma" />
  </EButton>
  <span v-else class="gap">…</span>
  <EButton
    v-if="b"
    role="listitem"
    size="sm"
    variant="quiet"
    class="mv"
    :tabindex="stopB ? 0 : -1"
    :aria-current="curB ? 'step' : undefined"
    :class="{ future: futB }"
    :data-ply="pb"
    :title="hb || undefined"
    @click="emit('go', pb)"
  >
    {{ b }}<ReviewGlyph :of="mb" />
  </EButton>
  <span v-else class="gap" aria-hidden="true"></span>
</template>

<style scoped>
.no {
  display: grid;
  align-items: center;
  justify-content: end;
  padding-inline: var(--e-gap-base);
  background: color-mix(in oklab, var(--e-ink) 4%, transparent);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.mv {
  justify-content: flex-start;
  min-height: 2.25rem;
  padding-inline: var(--ws-pad, var(--e-pad-control));
  border-radius: 0;
  font-family: var(--e-font-mono);
  text-decoration: none;
  transition:
    background-color 0.12s,
    color 0.12s;
}

/* The move on the board: the place tint, as eos marks where you are. */
.mv[aria-current] {
  background: var(--e-primary);
  color: var(--e-on-primary);
  font-weight: 700;
}

/* On the current move's tint the glyph sits on a chip of the surface, so its colour still reads. */
.mv[aria-current] :deep(.gl) {
  padding-inline: 0.25rem;
  border-radius: var(--e-radius-sm);
  background: var(--e-surface);
}

.mv:focus-visible {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: -2px;
}

.future {
  opacity: 0.72;
}

.gap {
  display: grid;
  align-items: center;
  padding-inline: var(--ws-pad, var(--e-pad-control));
  color: var(--e-muted);
}
</style>
