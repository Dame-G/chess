<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { seatName } from "../format";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import UiLive from "../ui/UiLive.vue";

/**
 * Dameless's game on Lichess, in the header while the bot plays one: who it plays, and a link that opens the game on
 * the board in watch mode. It stays out of the way of the game already on the board.
 */
const connections = useConnections();
const games = useGames();
const route = useRoute();

const game = computed(() => {
  const id = connections.bot.playing[0];
  return id == null ? undefined : games.list.find((g) => g.id === id);
});
const opponent = computed(() => {
  const g = game.value;
  if (!g) return "";
  return seatName(g.white.player === "dameless" ? g.black : g.white);
});
const onBoard = computed(() => route.path === "/" && route.query["game"] === String(game.value?.id));
</script>

<template>
  <RouterLink
    v-if="game && !onBoard"
    :to="{ path: '/', query: { game: String(game.id) } }"
    class="chip"
    :title="`Watch Dameless play ${opponent} on Lichess`"
  >
    <UiLive label="Live" :pulse="game.plies" />
    <span class="who truncate">Bot vs {{ opponent }}</span>
  </RouterLink>
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
  border-radius: var(--r-1);
  background: var(--c-sunk);
  color: var(--c-ink);
  font: var(--t-body-strong);
  text-decoration: none;
}

.chip:hover {
  background: var(--c-raise);
}

.chip:focus-visible {
  outline: var(--focus-ring);
  outline-offset: var(--e-stroke-focus);
}

.who {
  min-width: 0;
}

/* A phone has no room for it in the header; the Dameless sheet has the link. */
@media (width < 48rem) {
  .chip {
    display: none;
  }
}

/* Where there is little room for a name, the chip is its dot. */
@media (width < 64rem) {
  .who {
    display: none;
  }
}
</style>
