<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ETree, type TreeNode } from "@eos-ui/vue/components/ETree";
import type { Departure, Side } from "../api/types";
import { useBoard } from "../board/board";
import { INITIAL_FEN } from "../board/position";
import { formatPercent, plural } from "../format";
import { departures, mainLineIds, outline, share, type OutlineBranch, type OutlineRow } from "../sheets/repertoire";
import { useRepertoire } from "../stores/repertoire";
import { stored } from "../stored";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiTable from "../ui/UiTable.vue";

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
 * A move picked in the tree puts its line on the board and leaves the sheet open; only its chevron opens or closes it,
 * so the way on through the tree stays in reach (the board is beside the sheet on a wide screen).
 */
function pick(id: string) {
  const row = tree.value.rows.get(id);
  if (row) board.openFree(INITIAL_FEN, row.line);
}

const exits = [
  { key: "line", label: "Move" },
  { key: "games", label: "Games", fig: true },
  { key: "score", label: "Score", fig: true },
  { key: "example", label: "Example", hide: "sm" as const },
];
const departureKey = (d: Departure) => `${d.key}${d.uci}`;
const departureLine = (d: Departure) => `${Math.ceil(d.ply / 2)}${d.ply % 2 === 1 ? "." : "…"} ${d.san}`;
</script>

<template>
  <UiSection v-if="repertoire.available" id="y-rep" title="Your openings">
    <template #aside>
      <UiSeg
        v-model="side"
        label="Colour"
        size="sm"
        :options="[
          { value: 'white', label: 'As White' },
          { value: 'black', label: 'As Black' },
        ]"
      />
    </template>

    <UiEmpty v-if="repertoire.failed" tone="error" title="Your openings did not load" :body="repertoire.failed">
      <template #actions>
        <EButton variant="outline" @click="repertoire.load()">Try again</EButton>
      </template>
    </UiEmpty>
    <div v-else-if="!rep" class="skel tree-skel" aria-hidden="true"></div>
    <UiEmpty
      v-else-if="rep.games === 0"
      dense
      :title="`No games as ${side === 'white' ? 'White' : 'Black'} yet. Your lines appear after a few games.`"
    />
    <template v-else>
      <p class="note">
        {{ plural(rep.games, "game") }} as {{ side === "white" ? "White" : "Black" }}. A dot marks your moves; the
        others are what you met.
      </p>
      <div class="tree">
        <p class="legend" aria-hidden="true"><span>Move</span><span>Games · share · score</span></p>
        <ETree
          v-model:expanded="expanded"
          label="Your lines"
          :items="tree.items"
          :model-value="null"
          toggle-on="chevron"
          dense
          indent="sm"
          @select="pick"
        >
          <template #leading="{ item }">
            <span class="dot" :class="{ 'dot--you': tree.rows.get(item.id)?.yours }" aria-hidden="true"></span>
          </template>
          <template #item="{ item }">
            <span class="move mono"
              >{{ item.label }}<span v-if="tree.rows.get(item.id)?.leaves" class="out"> out of theory</span></span
            >
          </template>
          <template #trailing="{ item }">
            <span class="mono figures">{{ figures(item.id) }}</span>
          </template>
        </ETree>
      </div>

      <UiSection v-if="leaves.yours.length" :level="2" title="Where you leave theory">
        <UiTable label="Where you leave theory" :columns="exits" :rows="leaves.yours" :row-key="departureKey">
          <template #cell-line="{ row }">
            <EButton variant="quiet" size="sm" @click="show(row.fen, [row.uci])">
              <span class="mono">{{ departureLine(row) }}</span>
            </EButton>
          </template>
          <template #cell-score="{ row }">{{ formatPercent(row.score * 100, 0) }}</template>
          <template #cell-example="{ row }">
            <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(row.example) } }">
              Latest game
            </ELink>
          </template>
        </UiTable>
      </UiSection>
      <UiSection v-if="leaves.theirs.length" :level="2" title="Where your opponents leave">
        <UiTable label="Where your opponents leave" :columns="exits" :rows="leaves.theirs" :row-key="departureKey">
          <template #cell-line="{ row }">
            <EButton variant="quiet" size="sm" @click="show(row.fen, [row.uci])">
              <span class="mono">{{ departureLine(row) }}</span>
            </EButton>
          </template>
          <template #cell-score="{ row }">{{ formatPercent(row.score * 100, 0) }}</template>
          <template #cell-example="{ row }">
            <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(row.example) } }">
              Latest game
            </ELink>
          </template>
        </UiTable>
      </UiSection>
    </template>
  </UiSection>
</template>

<style scoped>
.tree-skel {
  block-size: calc(var(--sp-6) * 4);
}

.note {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.legend {
  display: flex;
  justify-content: space-between;
  margin: 0;
  padding: var(--sp-1) var(--sp-2);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.dot {
  inline-size: var(--sp-2);
  block-size: var(--sp-2);
}

.dot--you {
  border-radius: var(--r-pill);
  background: var(--c-ink);
}

.out {
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.move,
.figures {
  white-space: nowrap;
}
</style>
