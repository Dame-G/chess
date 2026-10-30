<script setup lang="ts">
import { computed } from "vue";
import { PhArrowSquareOut } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { seatName } from "../format";
import { useBoard } from "../board/board";
import { sanLine } from "../board/position";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import UiLive from "../ui/UiLive.vue";
import UiScore from "../ui/UiScore.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import PanelLock from "./PanelLock.vue";
import PanelMenu from "./PanelMenu.vue";

/**
 * Watch: a live game You are not in. The thinking engine's line and score when there is one, and the way to the game
 * on Lichess. Who is playing and the clock are in the game card above; with nothing to say, nothing.
 */
const board = useBoard();
const games = useGames();
const policy = usePolicy();

const s = computed(() => board.summary);
const t = computed(() => (policy.allowed ? games.thinking : null));
const thinker = computed(() => {
  const x = s.value;
  return x && t.value ? seatName(x[t.value.side]) : "";
});
/** The line from the newest position, as the engine sees it. */
const line = computed(() => (t.value ? sanLine(board.plies[board.lastPly]?.fen ?? "", t.value.pv.slice(0, 10)) : ""));
const nps = computed(() => (t.value ? `${(t.value.nps / 1e6).toFixed(2)}M nodes/s` : ""));

const url = computed(() => s.value?.url ?? null);
</script>

<template>
  <div class="watch">
    <PanelLock v-if="!policy.allowed" />
    <section v-else-if="t" class="think" aria-label="The engine's line">
      <p class="who">
        <UiLive :pulse="board.lastPly" />
        <span class="who-name truncate">{{ thinker }} is thinking</span>
        <UiScore v-if="t.score" :score="t.score" :turn="t.side" class="who-score" />
      </p>
      <p class="pv num truncate" :title="line">{{ line }}</p>
      <p class="meta num">Depth {{ t.depth }} · {{ nps }}</p>
    </section>

    <UiToolbar align="between" label="Game">
      <EButton v-if="url" variant="outline" :href="url" external :icon-end="PhArrowSquareOut">
        Open on Lichess
      </EButton>
      <template #end><PanelMenu /></template>
    </UiToolbar>
  </div>
</template>

<style scoped>
.watch {
  display: grid;
  gap: var(--sp-3);
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.think {
  display: grid;
  gap: var(--sp-1);
}

.who {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  margin: 0;
  font: var(--t-body-strong);
}

.who-name {
  min-width: 0;
}

.who-score {
  margin-inline-start: auto;
}

.pv {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-fig);
}
</style>
