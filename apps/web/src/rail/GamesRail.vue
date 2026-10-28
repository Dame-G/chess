<script setup lang="ts">
import { computed, ref } from "vue";
import { PhCaretDown, PhCaretRight } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EList } from "@eos-ui/vue/components/EList";
import { EListRow } from "@eos-ui/vue/components/EListRow";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EVirtualList } from "@eos-ui/vue/components/EVirtualList";
import type { GameId, GameSummary, TrialId } from "../api/types";
import { plural } from "../format";
import { clockOf } from "../sheets/idea";
import { useBoard } from "../board/board";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import { stored } from "../stored";
import GameRow from "./GameRow.vue";
import { emptyNote, FILTERS, railRows, stageLabel, type RailFilter, type RailRow } from "./rail";

/**
 * The Games rail: every game in one windowed list, live ones pinned, a trial's games folded into one row that opens
 * (its games follow it, indented, while it is open).
 */
const games = useGames();
const ideas = useIdeas();
const versions = useVersions();
const board = useBoard();

const filter = stored<RailFilter>("rail.filter", "all");
const rows = computed(() => railRows(games.list, filter.value));
const opened = ref(new Set<TrialId>());
const TRIAL = "trial:";

function toggle(id: TrialId) {
  const next = new Set(opened.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  opened.value = next;
}

/** "deeper-lmr vs v14 · short games · 412 pairs", from the idea the trial belongs to. */
function trialTitle(row: Extract<RailRow, { type: "trial" }>) {
  const found = ideas.ofTrial(row.trial);
  if (!found) return { title: `Trial ${row.trial}`, detail: plural(row.games.length, "game") };
  const { idea, trial } = found;
  const dev = versions.name(idea.version) ?? idea.branch;
  const base = versions.name(idea.base) ?? "the champion";
  return {
    title: `${dev} vs ${base}`,
    detail: [stageLabel[trial.stage], clockOf(trial, ideas.trials[trial.id]), `${trial.pairs} pairs`]
      .filter(Boolean)
      .join(" · "),
  };
}

/** What the list shows, top to bottom: a game, a trial's row, or a trial's game while it is open. */
type Item =
  { key: string; game: GameSummary; nested: boolean } | { key: string; trial: Extract<RailRow, { type: "trial" }> };
const items = computed<Item[]>(() =>
  rows.value.flatMap((row): Item[] =>
    row.type === "game"
      ? [{ key: row.key, game: row.game, nested: false }]
      : [
          { key: row.key, trial: row },
          ...(opened.value.has(row.trial)
            ? row.games.map((g) => ({ key: `${row.key}-${g.id}`, game: g, nested: true }))
            : []),
        ],
  ),
);

/** The game on the board is the list's current row; picking a row does not make it current until the board shows it. */
const current = computed({
  get: () => (board.drilling ? null : board.gameId),
  set: () => {},
});

function retry() {
  games.loaded = false;
  void games.load();
}

/** A trial's row opens or closes; a game opens on the board. */
function pick(key: GameId | string) {
  if (typeof key === "string" && key.startsWith(TRIAL)) toggle(Number(key.slice(TRIAL.length)) as TrialId);
  else void board.openGame(key as GameId);
}
</script>

<template>
  <nav class="rail" aria-label="Games">
    <div class="rail-head">
      <ESegmented v-model="filter" label="Show games" hide-label size="sm" stretch :options="FILTERS" />
    </div>
    <ESkeleton v-if="!games.loaded" :lines="10" label="Loading games" class="pad" />
    <ENotice
      v-else-if="games.error && !games.list.length"
      class="pad"
      tone="danger"
      title="Games did not load"
      retry
      @retry="retry"
    >
      {{ games.error }}
    </ENotice>
    <EEmptyState v-else-if="!rows.length" class="pad" title="No games here yet" size="sm" align="start" bare>
      {{ emptyNote[filter] }}
      <template v-if="filter !== 'all'" #action>
        <EButton variant="outline" size="sm" @click="filter = 'all'">Show all games</EButton>
      </template>
    </EEmptyState>
    <EList v-else v-model="current" label="Games" class="rail-list" flush @select="pick">
      <EVirtualList v-slot="{ item }" :items="items" :row-height="56" measure item-key="key">
        <GameRow v-if="'game' in item" :game="item.game" :nested="item.nested" />
        <EListRow
          v-else
          :value="`${TRIAL}${item.trial.trial}`"
          :title="trialTitle(item.trial).title"
          :subtitle="trialTitle(item.trial).detail"
          :expanded="opened.has(item.trial.trial)"
        >
          <template #leading>
            <span class="gutter" aria-hidden="true">
              <component :is="opened.has(item.trial.trial) ? PhCaretDown : PhCaretRight" />
            </span>
          </template>
          <template #trailing>
            <span v-if="item.trial.live" class="live">live · </span>{{ item.trial.games.length
            }}<span class="e-sr"> games here</span>
          </template>
        </EListRow>
      </EVirtualList>
    </EList>
  </nav>
</template>

<style scoped>
.rail {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  min-height: 0;
  height: 100%;
}

.rail-head {
  padding: var(--e-gap-snug) var(--ws-pad);
  border-block-end: var(--line);
}

.rail-list {
  grid-template-rows: minmax(0, 1fr);
  min-height: 0;
}

/* The caret sits in the width of a live dot, so every title in the rail starts at one edge. */
.gutter {
  display: grid;
  place-items: center;
  flex: none;
  inline-size: 0.5em;
  color: var(--e-muted);
}

.live {
  color: var(--e-danger-ink);
}

/* The empty and error states are inset like the rail's other parts. */
.pad {
  margin: var(--ws-pad);
}
</style>
