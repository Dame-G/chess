<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import {
  PhArrowsDownUp,
  PhBroadcast,
  PhCaretLeft,
  PhCaretRight,
  PhPause,
  PhPlay,
  PhSkipBack,
  PhSkipForward,
} from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { useBoard } from "./board";
import { isLive } from "./modes";

/** VLC's row under the board: step, play through, jump to the ends, flip; and back to the live move. */
const board = useBoard();

const playing = ref(false);
let timer: ReturnType<typeof setInterval> | undefined;
function stopPlaying() {
  playing.value = false;
  clearInterval(timer);
}
function toggle() {
  if (playing.value) return stopPlaying();
  if (board.atEnd) board.go(0);
  playing.value = true;
  timer = setInterval(() => {
    if (board.atEnd) stopPlaying();
    else board.go(board.cursor + 1);
  }, 900);
}
watch(() => board.gameId, stopPlaying);
onBeforeUnmount(stopPlaying);

const live = computed(() => !!board.summary && isLive(board.summary) && !board.exploring);
const total = computed(() => (board.exploring ? board.tree.end(board.node).ply : board.lastPly));
</script>

<template>
  <div class="transport">
    <EIconButton
      :icon="PhSkipBack"
      label="First move (Home)"
      tooltip
      size="sm"
      :disabled="board.cursor === 0"
      @click="board.go(0)"
    />
    <EIconButton
      :icon="PhCaretLeft"
      label="Back (Left arrow)"
      tooltip
      size="sm"
      :disabled="board.cursor === 0"
      @click="board.go(board.cursor - 1)"
    />
    <EIconButton
      :icon="playing ? PhPause : PhPlay"
      :label="playing ? 'Pause' : 'Play through the moves'"
      tooltip
      size="sm"
      :disabled="live || total === 0"
      @click="toggle"
    />
    <EIconButton
      :icon="PhCaretRight"
      label="Forward (Right arrow)"
      tooltip
      size="sm"
      :disabled="board.atEnd"
      @click="board.go(board.cursor + 1)"
    />
    <EIconButton
      :icon="PhSkipForward"
      label="Last move (End)"
      tooltip
      size="sm"
      :disabled="board.atEnd"
      @click="board.go(Infinity)"
    />
    <span class="counter num" aria-live="off">{{ board.cursor }} / {{ total }}</span>
    <EButton
      v-if="live && !board.following"
      size="sm"
      variant="outline"
      :icon="PhBroadcast"
      class="live"
      @click="board.go(Infinity)"
    >
      Live
    </EButton>
    <EIconButton
      :icon="PhArrowsDownUp"
      label="Flip the board (F)"
      tooltip
      size="sm"
      class="flip"
      @click="board.flip()"
    />
  </div>
</template>

<style scoped>
.transport {
  display: flex;
  align-items: center;
  gap: var(--e-gap-tight);
  min-width: 0;
}

/* Tabular digits in a fixed width: stepping from move 9 to 10 moves nothing. */
.counter {
  min-inline-size: 8ch;
  margin-inline-start: var(--e-gap-snug);
  color: var(--e-muted);
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.live {
  margin-inline-start: auto;
}

.flip {
  margin-inline-start: auto;
}

.live + .flip {
  margin-inline-start: 0;
}

@media (width < 40rem) {
  .transport {
    gap: 0;
    justify-content: space-between;
  }

  .counter {
    margin-inline-start: 0;
  }

  .flip {
    margin-inline-start: 0;
  }
}
</style>
