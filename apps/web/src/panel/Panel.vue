<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import type { Classification } from "../api/types";
import MoveList from "../board/MoveList.vue";
import TreeMoves from "../board/TreeMoves.vue";
import { useBoard } from "../board/board";
import { modeLabel, type Mode } from "../board/modes";
import { INITIAL_FEN } from "../board/position";
import { hint } from "../board/review";
import type { TreeNode } from "../board/tree";
import { useMedia } from "../media";
import { useGames } from "../stores/games";
import { usePolicy } from "../stores/policy";
import UiDialog from "../ui/UiDialog.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiLive from "../ui/UiLive.vue";
import UiPanelHead from "../ui/UiPanelHead.vue";
import UiSeg from "../ui/UiSeg.vue";
import DrillPanel from "./DrillPanel.vue";
import ExplorePanel from "./ExplorePanel.vue";
import MoveStrip from "./MoveStrip.vue";
import PlayPanel from "./PlayPanel.vue";
import ReviewPanel from "./ReviewPanel.vue";
import { stripMoves, type LineMove } from "./strip";
import WatchPanel from "./WatchPanel.vue";

/**
 * The panel beside the board, the same in every mode: the game card (who is playing whom), the mode's tabs when there
 * is more than one, the mode's content (never more than 40 % of the height) and the moves under it, filling the rest.
 * On a phone the moves are one row first, and the whole list is a sheet.
 */
const board = useBoard();
const games = useGames();
const policy = usePolicy();
const router = useRouter();

const modes = computed(() => board.modes.map((id) => ({ value: id, label: modeLabel[id] })));
const mode = computed({
  get: () => board.mode,
  set: (m: Mode) => board.setMode(m),
});

const start = computed(() => board.game?.start_fen ?? INITIAL_FEN);
const sans = computed(() => board.plies.slice(1).map((p) => p.san ?? ""));
/** The review's marks on the moves, hidden while analysis is off. */
const marks = computed<(Classification | null)[] | undefined>(() =>
  policy.allowed && games.review ? games.review.plies.map((p) => p.classification) : undefined,
);
const hints = computed(() => (policy.allowed && games.review ? games.review.plies.map(hint) : undefined));

// ---- The phone's move row, and the sheet with the whole list.
const phone = useMedia("(width < 48rem)");
const sheet = ref(false);

const line = computed<LineMove[]>(() => {
  if (board.exploring) {
    void board.treeVersion;
    const path: TreeNode[] = [];
    for (let n: TreeNode | null = board.node; n?.parent; n = n.parent) path.unshift(n);
    for (let n: TreeNode | undefined = board.node.children[0]; n; n = n.children[0]) path.push(n);
    return path.map((n) => ({ id: n.id, ply: n.ply, san: n.san }));
  }
  return sans.value.map((san, i) => ({ id: i + 1, ply: i + 1, san, mark: marks.value?.[i] ?? null }));
});
const strip = computed(() =>
  stripMoves(
    board.exploring ? board.tree.root.fen : start.value,
    line.value,
    board.exploring ? board.node.id : board.ply || null,
  ),
);
/** A move chosen in the sheet goes to the board and closes it. */
function pickNode(node: TreeNode) {
  board.selectNode(node);
  sheet.value = false;
}
function pickPly(ply: number) {
  board.go(ply);
  sheet.value = false;
}
function goTo(id: number) {
  if (!board.exploring) return board.go(id);
  const node = board.tree.find(id);
  if (node) board.selectNode(node);
}
</script>

<template>
  <aside class="panel" :class="{ 'panel--drill': board.drilling }" aria-label="Panel">
    <UiEmpty
      v-if="board.missing !== null"
      title="That game isn't here"
      body="It may have been deleted, or the link is wrong."
    >
      <template #actions>
        <EButton variant="outline" @click="router.push('/')">Go to the board</EButton>
      </template>
    </UiEmpty>

    <template v-else>
      <MoveStrip v-if="phone && !board.drilling" class="strip" :moves="strip" @go="goTo" @open="sheet = true" />

      <header v-if="board.summary && !board.drilling" class="head">
        <UiPanelHead :game="board.summary" :mode="board.mode">
          <template v-if="board.mode === 'watch'" #status>
            <UiLive :pulse="board.lastPly" label="Live" /> Live
          </template>
        </UiPanelHead>
      </header>

      <div v-if="modes.length > 1" class="tabs">
        <UiSeg v-model="mode" :options="modes" label="Mode" full />
      </div>

      <div class="body" :data-mode="board.mode">
        <PlayPanel v-if="board.mode === 'play'" />
        <WatchPanel v-else-if="board.mode === 'watch'" />
        <ReviewPanel v-else-if="board.mode === 'review'" />
        <ExplorePanel v-else-if="board.mode === 'explore'" />
        <DrillPanel v-else-if="board.mode === 'drill'" />
      </div>

      <div v-if="!board.drilling && !phone" class="moves">
        <TreeMoves
          v-if="board.exploring"
          :tree="board.tree"
          :node="board.node"
          :version="board.treeVersion"
          @select="board.selectNode"
          @step="board.step"
        />
        <MoveList
          v-else
          :sans="sans"
          :cursor="board.ply"
          :start="start"
          :loading="games.openLoading && !board.game"
          :marks="marks"
          :hints="hints"
          @go="board.go"
          @step="board.step"
        />
      </div>

      <UiDialog v-if="phone" v-model:open="sheet" title="Moves" size="sm" cancel-label="Close">
        <div class="sheet">
          <TreeMoves
            v-if="board.exploring"
            :tree="board.tree"
            :node="board.node"
            :version="board.treeVersion"
            @select="pickNode"
            @step="board.step"
          />
          <MoveList
            v-else
            :sans="sans"
            :cursor="board.ply"
            :start="start"
            :marks="marks"
            :hints="hints"
            @go="pickPly"
            @step="board.step"
          />
        </div>
      </UiDialog>
    </template>
  </aside>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  height: 100%;
  background: var(--c-pane);
}

.head {
  flex: none;
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.tabs {
  flex: none;
  padding: var(--sp-2) var(--sp-3) 0;
}

/* The mode's content is inset like the rest of the workspace and never takes more than 40 % of the panel. */
.body {
  flex: none;
  min-width: 0;
  padding: var(--sp-3);
  overflow: hidden auto;
}

.moves {
  flex: 1 1 0;
  min-height: 12rem;
  border-block-start: var(--e-stroke-hair) solid var(--c-line);
}

@media (width >= 48rem) {
  /* md */
  .body {
    max-height: 40dvh;
  }
}

/* The drill takes the whole panel: the question, the answer and the actions, with no list under them. */
.panel--drill .body {
  max-height: none;
}

/* A phone: the move row sits right under the board, then the game card and the mode's content. */
@media (width < 48rem) {
  /* md */
  .panel {
    height: auto;
  }

  .strip {
    flex: none;
  }

  .body {
    max-height: none;
  }
}

.sheet {
  max-height: 60dvh;
  margin: calc(-1 * var(--sp-4));
  overflow-y: auto;
}

@media (width < 48rem) {
  /* md: a phone stacks the panel under the board, the page scrolls and the list grows with the game. A tablet keeps two
     columns, so the list scrolls inside the panel as it does beside the board. */
  .moves {
    flex: none;
  }
}
</style>
