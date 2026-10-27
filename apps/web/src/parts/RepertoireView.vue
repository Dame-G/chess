<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ETree, type TreeNode } from "@eos-ui/vue/components/ETree";
import type { Departure, Side } from "../api/types";
import { useBoard } from "../board/board";
import { INITIAL_FEN } from "../board/position";
import { plural } from "../format";
import { departures, mainLineIds, outline, share, type OutlineBranch, type OutlineRow } from "../sheets/repertoire";
import { useRepertoire } from "../stores/repertoire";
import { stored } from "../stored";

/**
 * Your openings, one colour at a time: the lines you play as a compact tree (move, games, share, score), where you and
 * your opponents leave theory, and a way to open any line in Explore. Built from your games. Hidden while the policy
 * forbids the explorer.
 */
const repertoire = useRepertoire();
const board = useBoard();
const router = useRouter();

const side = stored<Side>("repertoire.side", "white");
onMounted(() => void repertoire.load());

const rep = computed(() => repertoire.sides[side.value]);
/** The open moves, by id; the main line is open to begin with. */
const expanded = ref<string[]>([]);
// Opened on the main line when the lines first arrive; a refresh of them keeps what you opened.
watch(
  rep,
  (r, before) => {
    if (r && !before) expanded.value = [...mainLineIds(r)];
  },
  { immediate: true },
);
watch(side, () => {
  if (rep.value) expanded.value = [...mainLineIds(rep.value)];
});

/** The tree for the list, and each move's figures by its id. */
const tree = computed(() => {
  const rows = new Map<string, OutlineRow>();
  const node = (b: OutlineBranch): TreeNode => {
    rows.set(b.row.id, b.row);
    return {
      id: b.row.id,
      label: `${b.row.number} ${b.row.san}`,
      ...(b.children.length ? { children: b.children.map(node) } : {}),
    };
  };
  const items = rep.value ? outline(rep.value).map(node) : [];
  return { items, rows };
});
const leaves = computed(() => (rep.value ? departures(rep.value) : { yours: [], theirs: [] }));

/** Opens a line in Explore, on the board. */
function show(fen: string, moves: string[]) {
  board.openFree(fen, moves);
  void router.push({ path: "/", query: board.address });
}

/** A move's games, share and score, as the tree's last column. */
function figures(id: string): string {
  const r = tree.value.rows.get(id);
  return r ? `${r.games} · ${share(r.share)} · ${share(r.score)}` : "";
}

/**
 * A move picked in the tree puts its line on the board and leaves the sheet open: picking also opens or closes the
 * move, so the way on through the tree stays in reach (the board is beside the sheet on a wide screen).
 */
function pick(id: string) {
  const row = tree.value.rows.get(id);
  if (row) board.openFree(INITIAL_FEN, row.line);
}

const departureLine = (d: Departure) => `${Math.ceil(d.ply / 2)}${d.ply % 2 === 1 ? "." : "…"} ${d.san}`;
</script>

<template>
  <section v-if="repertoire.available" class="sec" aria-labelledby="y-rep">
    <div class="sec-head">
      <h2 id="y-rep" class="part-title">Your openings</h2>
      <ESegmented
        v-model="side"
        label="Colour"
        hide-label
        size="sm"
        :options="[
          { value: 'white', label: 'As White' },
          { value: 'black', label: 'As Black' },
        ]"
      />
    </div>

    <ENotice v-if="repertoire.failed" tone="danger" title="Your openings did not load">{{ repertoire.failed }}</ENotice>
    <ESkeleton v-else-if="!rep" :lines="4" label="Loading your openings" />
    <p v-else-if="rep.games === 0" class="sec-note">
      No games as {{ side === "white" ? "White" : "Black" }} yet. Your lines appear after a few games.
    </p>
    <template v-else>
      <p class="sec-note">
        {{ plural(rep.games, "game") }} as {{ side === "white" ? "White" : "Black" }}. Your moves are shown with a dot;
        the others are what you met.
      </p>
      <div class="tree">
        <p class="legend" aria-hidden="true"><span>Move</span><span>Games · share · score</span></p>
        <ETree v-model:expanded="expanded" label="Your lines" :items="tree.items" :model-value="null" @select="pick">
          <template #leading="{ item }">
            <span class="dot" :class="{ 'dot--you': tree.rows.get(item.id)?.yours }" aria-hidden="true"></span>
          </template>
          <template #item="{ item }">
            <span class="move mono"
              >{{ item.label }}<span v-if="tree.rows.get(item.id)?.leaves" class="out"> out of theory</span></span
            >
          </template>
          <template #trailing="{ item }">
            <span class="mono">{{ figures(item.id) }}</span>
          </template>
        </ETree>
      </div>

      <div v-if="leaves.yours.length || leaves.theirs.length" class="leave">
        <div v-if="leaves.yours.length">
          <h3 class="part-title">Where you leave theory</h3>
          <ul class="plain">
            <li v-for="d in leaves.yours" :key="`${d.key}${d.uci}`" class="dep">
              <EButton variant="quiet" size="sm" @click="show(d.fen, [d.uci])">
                <span class="mono">{{ departureLine(d) }}</span>
              </EButton>
              <span class="sec-note">
                {{ plural(d.games, "game") }}, {{ share(d.score) }} score ·
                <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(d.example) } }"
                  >latest game</ELink
                >
              </span>
            </li>
          </ul>
        </div>
        <div v-if="leaves.theirs.length">
          <h3 class="part-title">Where your opponents leave</h3>
          <ul class="plain">
            <li v-for="d in leaves.theirs" :key="`${d.key}${d.uci}`" class="dep">
              <EButton variant="quiet" size="sm" @click="show(d.fen, [d.uci])">
                <span class="mono">{{ departureLine(d) }}</span>
              </EButton>
              <span class="sec-note">
                {{ plural(d.games, "game") }}, {{ share(d.score) }} score ·
                <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(d.example) } }"
                  >latest game</ELink
                >
              </span>
            </li>
          </ul>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.legend {
  display: flex;
  justify-content: space-between;
  margin: 0;
  padding: 0 0.5rem var(--e-gap-tight);
  border-block-end: var(--line);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.dot {
  inline-size: 0.375rem;
  block-size: 0.375rem;
}

.dot--you {
  border-radius: 50%;
  background: var(--e-primary);
}

.out {
  color: var(--e-muted);
  font: var(--e-type-small);
}

/* A denser tree than eos's default row: a line of play is a few characters, and there can be many. */
.tree :deep(.e-tree-row) {
  min-block-size: 2rem;
  padding-block: 0.125rem;
}

.move,
.tree :deep(.e-listrow-trail) {
  white-space: nowrap;
}

/* A phone has no width for eos's indent per move: a line of play goes ten moves deep. */
@media (width < 36rem) {
  .tree :deep(.e-tree-row) {
    padding-inline-start: calc(0.5rem + (var(--_level) - 1) * 0.875rem);
  }

  .tree :deep(.e-listrow-trail) {
    font-size: 0.8125rem;
  }
}

.leave {
  display: grid;
  gap: var(--e-gap-base);
  margin-block-start: var(--e-gap-snug);
  padding-block-start: var(--e-gap-base);
  border-block-start: var(--line);
}

.leave .part-title {
  margin-block-end: var(--e-gap-tight);
}

.dep {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0 var(--e-gap-snug);
  padding-block: 0.125rem;
  border-block-end: var(--line);
}

.dep:last-child {
  border-block-end: 0;
}
</style>
