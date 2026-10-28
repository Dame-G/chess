<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ETabs } from "@eos-ui/vue/components/ETabs";
import { ELink } from "@eos-ui/vue/components/ELink";
import type { Classification } from "../api/types";
import MoveList from "../board/MoveList.vue";
import TreeMoves from "../board/TreeMoves.vue";
import { useBoard } from "../board/board";
import { modeLabel, type Mode } from "../board/modes";
import { INITIAL_FEN } from "../board/position";
import { hint } from "../board/review";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import DrillPanel from "./DrillPanel.vue";
import ExplorePanel from "./ExplorePanel.vue";
import PlayPanel from "./PlayPanel.vue";
import ReviewPanel from "./ReviewPanel.vue";
import WatchPanel from "./WatchPanel.vue";

/** The panel beside the board: the mode's tabs (only those that apply), the mode's content, and the moves under it. */
const board = useBoard();
const games = useGames();
const policy = usePolicy();

const tabs = computed(() => board.modes.map((id) => ({ id, label: modeLabel[id] })));
const mode = computed({
  get: () => board.mode,
  set: (m: string) => board.setMode(m as Mode),
});

const sans = computed(() => board.plies.slice(1).map((p) => p.san ?? ""));
/** The review's marks on the moves, hidden while analysis is off. */
const marks = computed<(Classification | null)[] | undefined>(() =>
  policy.allowed && games.review ? games.review.plies.map((p) => p.classification) : undefined,
);
const hints = computed(() => (policy.allowed && games.review ? games.review.plies.map(hint) : undefined));
</script>

<template>
  <aside class="panel" :class="{ 'panel--drill': board.drilling }" aria-label="Panel">
    <ENotice v-if="board.missing !== null" tone="warning" title="That game isn't here" class="missing">
      It may have been deleted, or the link is wrong.
      <ELink :link-component="RouterLink" to="/">Go to the board</ELink>
    </ENotice>
    <ETabs v-model="mode" :items="tabs" label="Mode" class="tabs">
      <template #panel-play><PlayPanel /></template>
      <template #panel-watch><WatchPanel /></template>
      <template #panel-review><ReviewPanel /></template>
      <template #panel-explore><ExplorePanel /></template>
      <template #panel-drill><DrillPanel /></template>
    </ETabs>
    <div v-if="!board.drilling" class="moves">
      <TreeMoves
        v-if="board.exploring"
        :tree="board.tree"
        :node="board.node"
        :version="board.treeVersion"
        @select="board.selectNode"
      />
      <MoveList
        v-else
        :sans="sans"
        :cursor="board.ply"
        :start="board.game?.start_fen ?? INITIAL_FEN"
        :loading="games.openLoading && !board.game"
        :marks="marks"
        :hints="hints"
        @go="board.go"
      />
    </div>
  </aside>
</template>

<style scoped>
.panel {
  display: grid;
  grid-template-rows: auto minmax(12rem, 1fr);
  min-height: 0;
  height: 100%;
}

.panel:has(.missing) {
  grid-template-rows: auto auto minmax(12rem, 1fr);
}

.missing {
  margin: var(--ws-pad);
}

.panel--drill {
  grid-template-rows: auto;
  align-content: start;
}

.tabs {
  min-height: 0;
  max-height: 55dvh;
  overflow: hidden auto;
  border-block-end: var(--line);
}

/* The tabs sit on the panel's rule, flush: square corners, the row as wide as the panel, and the selected tab covers
   the rule under it. (eos rounds a tab's outer corners; flush tabs want none.) */
.tabs :deep([role="tablist"]) {
  box-sizing: border-box;
  max-width: 100%;
  margin: 0;
  padding: 0;
  border-block-end: var(--line);
}

.tabs :deep([role="tab"]) {
  border-radius: 0;
}

.tabs :deep([role="tab"][aria-selected="true"]) {
  margin-block-end: calc(-1 * var(--e-stroke-hair));
}

.tabs :deep([role="tabpanel"]) {
  min-width: 0;
  padding: var(--ws-pad);
  overflow-x: hidden;
}

.moves {
  min-height: 0;
  overflow: hidden;
}

@media (width < 75rem) {
  .tabs {
    max-height: none;
  }

  /* Stacked, the page scrolls: the list grows with the game instead of scrolling inside the page. */
  .moves {
    height: auto;
    overflow: visible;
  }

  .moves :deep(.moves),
  .moves :deep(.tree) {
    height: auto;
    overflow: visible;
  }
}
</style>
