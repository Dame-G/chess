<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { PhArrowSquareOut } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { seatName } from "../format";
import { clockOf } from "../sheets/idea";
import { useBoard } from "../board/board";
import { sanLine } from "../board/position";
import { stageLabel } from "../rail/rail";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { usePolicy } from "../stores/policy";
import UiLive from "../ui/UiLive.vue";
import UiScore from "../ui/UiScore.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import PanelLock from "./PanelLock.vue";
import PanelMenu from "./PanelMenu.vue";

/**
 * Watch: a live game You are not in. The thinking engine's line and score when there is one, a trial game's idea, and
 * the way to the game on Lichess. Who is playing and the clock are in the game card above; with nothing to say, nothing.
 */
const board = useBoard();
const games = useGames();
const ideas = useIdeas();
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

const trial = computed(() => {
  const x = s.value;
  if (x?.source.type !== "trial") return null;
  const found = ideas.ofTrial(x.source.trial);
  if (!found) return null;
  const clock = clockOf(found.trial, ideas.trials[found.trial.id]);
  const tc = clock ? ` (${clock})` : "";
  return {
    idea: found.idea,
    text: `Game ${x.source.index + 1} of ${found.idea.title}, ${stageLabel[found.trial.stage]}${tc}`,
  };
});
const url = computed(() => s.value?.url ?? null);
</script>

<template>
  <div class="watch">
    <p v-if="trial" class="about">
      <RouterLink class="ui-link" :to="`/dameless/ideas/${trial.idea.id}`">{{ trial.text }}</RouterLink>
    </p>

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

.about,
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
