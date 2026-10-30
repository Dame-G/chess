<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { PhCaretRight } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import type { PrepLine } from "../api/types";
import { useBoard } from "../board/board";
import { INITIAL_FEN } from "../board/position";
import { plural } from "../format";
import { useStartDrills } from "../shell/drillFlow";
import { boardTo } from "../sheets/playOut";
import { byNeed, EARLY_PLY, earlyText, flagged, flags, recordText, scoreText } from "../sheets/prep";
import { lost } from "../sheets/themes";
import { useDrills } from "../stores/drills";
import { usePrep } from "../stores/prep";
import UiBadge from "../ui/UiBadge.vue";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSection from "../ui/UiSection.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiTable from "../ui/UiTable.vue";

/**
 * Prep for one colour: the opening lines you play, as Lichess names them, most played first with the ones that need
 * work on top: a line is flagged when you score badly in it or often make a mistake in its first twelve moves. A row
 * opens to the line, your record, the costliest early mistake, and two ways on: drill the mistakes you made in that line
 * (the positions as drills), or explore the line on the board. It is assistance, like your repertoire: it is hidden
 * while you play on Lichess, and the server refuses it.
 */
const prep = usePrep();
const drills = useDrills();
const board = useBoard();
const router = useRouter();
const startDrills = useStartDrills();

onMounted(() => void prep.load());

const lines = computed(() => byNeed(prep.prep?.lines ?? []));
const needing = computed(() => lines.value.filter(flagged).length);
const colour = computed(() => (prep.side === "white" ? "White" : "Black"));
const expanded = ref<string[]>([]);
// The line that needs most work is open to begin with.
watch(
  () => prep.prep,
  (now, before) => {
    const first = byNeed(now?.lines ?? []).find(flagged);
    if (now && !before && first) expanded.value = [first.name];
  },
  { immediate: true },
);
watch(
  () => prep.side,
  () => (expanded.value = []),
);
function toggle(row: { name: string }) {
  expanded.value = expanded.value.includes(row.name)
    ? expanded.value.filter((n) => n !== row.name)
    : [...expanded.value, row.name];
}

const columns = [
  { key: "name", label: "Line" },
  { key: "games", label: "Games", fig: true, sort: true },
  { key: "score", label: "Score", fig: true, sort: true },
  { key: "early", label: "Early mistakes", fig: true, hide: "sm" as const },
];
const rows = computed(() => lines.value.map((l) => ({ ...l, early: l.early_mistakes })));

/** Opens the line on the board, as played. */
function explore(line: PrepLine) {
  board.openFree(INITIAL_FEN, line.moves);
  void router.push({ path: "/", query: board.address });
}
const drill = (line: PrepLine) =>
  startDrills({ type: "line", name: line.name, games: line.drill_games, maxPly: EARLY_PLY });
const moveNo = (ply: number, san: string) => `${Math.ceil(ply / 2)}${ply % 2 ? "." : "…"} ${san}`;
</script>

<template>
  <UiSection v-if="prep.available" id="y-prep" title="Prep">
    <template #aside>
      <UiSeg
        v-model="prep.side"
        label="Colour"
        size="sm"
        :options="[
          { value: 'white', label: 'As White' },
          { value: 'black', label: 'As Black' },
        ]"
      />
    </template>
    <template #note>
      The lines you play as {{ colour }}, with your score and how often you go wrong in the first twelve moves.
    </template>

    <UiEmpty v-if="prep.failed" tone="error" title="Your prep did not load" :body="prep.failed">
      <template #actions>
        <EButton variant="outline" @click="prep.load()">Try again</EButton>
      </template>
    </UiEmpty>
    <div v-else-if="!prep.prep" class="skel table-skel" aria-hidden="true"></div>
    <UiEmpty
      v-else-if="!lines.length"
      dense
      :title="`No line you play as ${colour} has two games yet. Your prep appears after a few more.`"
    />
    <template v-else>
      <p class="count" role="status">
        <template v-if="needing">{{ plural(needing, "line") }} need work.</template>
        <template v-else>No line needs work.</template>
      </p>
      <UiTable
        :label="`Your lines as ${colour}`"
        :columns="columns"
        :rows="rows"
        row-key="name"
        :expanded="expanded"
        @select="toggle"
      >
        <template #cell-name="{ row }">
          <span class="line">
            <span class="name">
              <PhCaretRight class="chev" :class="{ 'is-open': expanded.includes(row.name) }" aria-hidden="true" />
              {{ row.name }}
            </span>
            <span v-if="flagged(row)" class="flags">
              <UiBadge v-for="f in flags(row)" :key="f" tone="warn">{{ f }}</UiBadge>
            </span>
          </span>
        </template>
        <template #cell-score="{ row }">{{ scoreText(row) }}</template>
        <template #cell-early="{ row }">{{ row.early_mistakes || "–" }}</template>
        <template #row-aside="{ row }">
          <div class="detail">
            <p class="pgn mono">{{ row.pgn }}</p>
            <p class="meta">
              {{ recordText(row) }}. Early mistakes in {{ earlyText(row)
              }}{{ row.early_swing ? `, ${lost(row.early_swing)} in all` : "" }}.
            </p>
            <p v-if="row.worst" class="meta">
              Costliest:
              <RouterLink class="ui-link" :to="boardTo(row.worst)">
                <span class="mono">{{ moveNo(row.worst.ply, row.worst.played) }}</span>
              </RouterLink>
              instead of <span class="mono">{{ row.worst.best }}</span
              >, {{ lost(row.worst.swing) }}.
            </p>
            <div class="actions">
              <EButton
                v-if="drills.available"
                variant="outline"
                size="sm"
                :disabled="!row.drill_games.length"
                :title="row.drill_games.length ? undefined : 'None of its games has an early mistake to drill'"
                @click="drill(row)"
              >
                Drill this line
              </EButton>
              <EButton variant="outline" size="sm" @click="explore(row)">Explore</EButton>
              <ELink :link-component="RouterLink" :to="{ path: '/', query: { game: String(row.example) } }">
                Latest game
              </ELink>
            </div>
          </div>
        </template>
      </UiTable>
    </template>
  </UiSection>
</template>

<style scoped>
.table-skel {
  block-size: calc(var(--sp-6) * 3);
}

.count {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.line {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-1) var(--sp-2);
}

.name {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
}

.flags {
  display: inline-flex;
  flex-wrap: wrap;
  gap: var(--sp-1);
}

.chev {
  flex: none;
  color: var(--c-ink-3);
  font-size: var(--i-sm);
  transition: transform var(--m-move) var(--m-ease);
}

.chev.is-open {
  transform: rotate(90deg);
}

@media (prefers-reduced-motion: reduce) {
  .chev {
    transition: none;
  }
}

.detail {
  display: grid;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-3) var(--sp-3) var(--sp-4);
}

.pgn {
  margin: 0;
  overflow-wrap: anywhere;
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--sp-2);
}
</style>
