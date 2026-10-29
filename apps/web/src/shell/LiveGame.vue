<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { clock, seatName } from "../format";
import { isLive } from "../board/modes";
import { useBoard } from "../board/board";
import { useGames } from "../stores/games";
import UiLive from "../ui/UiLive.vue";

/**
 * Your live game, kept in the header while a sheet covers the panel and the board's clocks: who you are playing and
 * your own clock when the game is the one on the board. Pressing it goes back to the board.
 */
const games = useGames();
const board = useBoard();
const route = useRoute();
const router = useRouter();

const mine = computed(() => games.list.find((g) => g.you && isLive(g)));
const shown = computed(() => route.meta["sheet"] === true && !!mine.value);

const opponent = computed(() => {
  const g = mine.value;
  if (!g || !g.you) return "";
  return seatName(g.you === "white" ? g.black : g.white);
});

const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | undefined;

/** Your clock in ms, when the game on the board is yours and the clocks are known. */
const ms = computed(() => {
  const g = mine.value;
  const open = games.open;
  if (!g?.you || !open || open.summary.id !== g.id || !open.clocks) return null;
  const base = open.clocks[g.you];
  const turn = board.plies.at(-1)?.turn;
  const running = turn === g.you && open.summary.status === "live" && open.moves.length >= 2;
  return running ? Math.max(0, base - (now.value - games.clocksAt)) : base;
});
const running = computed(() => {
  const g = mine.value;
  return !!g?.you && board.plies.at(-1)?.turn === g.you && ms.value !== null;
});
watch(
  running,
  (on) => {
    clearInterval(timer);
    if (on) timer = setInterval(() => (now.value = Date.now()), 250);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearInterval(timer));

function back() {
  if (mine.value) void router.push({ path: "/", query: { game: String(mine.value.id) } });
}
</script>

<template>
  <button v-if="shown" type="button" class="chip" :title="`Back to your game against ${opponent}`" @click="back">
    <UiLive label="Live" :pulse="mine?.plies" />
    <span class="vs truncate">{{ opponent }}</span>
    <span v-if="ms !== null" class="time num" :class="{ 'time--run': running }">{{ clock(ms) }}</span>
  </button>
</template>

<style scoped>
.chip {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-2);
  box-sizing: border-box;
  min-height: var(--h-control);
  max-width: 14rem;
  padding-inline: var(--sp-2);
  border: 0;
  border-radius: var(--r-1);
  background: var(--c-sunk);
  color: var(--c-ink);
  font: var(--t-body-strong);
  cursor: pointer;
}

.chip:hover {
  background: var(--c-raise);
}

.chip:focus-visible {
  outline: var(--focus-ring);
  outline-offset: var(--e-stroke-focus);
}

.vs {
  min-width: 0;
}

.time {
  flex: none;
  color: var(--c-ink-2);
  font: var(--t-fig);
}

.time--run {
  color: var(--c-ink);
}

@media (width < 30rem) {
  .vs {
    display: none;
  }
}
</style>
