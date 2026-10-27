<script setup lang="ts">
import { computed, ref } from "vue";
import { PhCaretDown, PhCaretRight } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import type { GameId, TrialId } from "../api/types";
import { clockOf } from "../sheets/idea";
import { useBoard } from "../board/board";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { useVersions } from "../stores/versions";
import { stored } from "../stored";
import GameRow from "./GameRow.vue";
import { FILTERS, railRows, stageLabel, type RailFilter, type RailRow } from "./rail";

/** The Games rail: every game in one list, live ones pinned, a trial's games folded into one row that opens. */
const games = useGames();
const ideas = useIdeas();
const versions = useVersions();
const board = useBoard();

const filter = stored<RailFilter>("rail.filter", "all");
const rows = computed(() => railRows(games.list, filter.value));
const opened = ref(new Set<TrialId>());

function toggle(id: TrialId) {
  const next = new Set(opened.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  opened.value = next;
}

/** "deeper-lmr vs v14 · short games · 412 pairs", from the idea the trial belongs to. */
function trialTitle(row: Extract<RailRow, { type: "trial" }>) {
  const found = ideas.ofTrial(row.trial);
  if (!found) return { title: `Trial ${row.trial}`, detail: `${row.games.length} games` };
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

const open = (id: GameId) => void board.openGame(id);
</script>

<template>
  <nav class="rail" aria-label="Games">
    <div class="rail-head">
      <ESegmented v-model="filter" label="Show games" hide-label size="sm" :options="FILTERS" class="filter" />
    </div>
    <div class="rail-list" role="list" aria-label="Games list">
      <ESkeleton v-if="!games.loaded" :lines="10" label="Loading games" class="pad" />
      <EEmptyState v-else-if="!rows.length" class="pad" title="No games here yet" size="sm" bare>
        Games you play, Dameless's games on Lichess and trial games all land here.
      </EEmptyState>
      <template v-for="row in rows" :key="row.key">
        <GameRow
          v-if="row.type === 'game'"
          :game="row.game"
          :current="!board.drilling && board.gameId === row.game.id"
          @open="open(row.game.id)"
        />
        <template v-else>
          <div role="listitem">
            <EButton
              variant="quiet"
              size="sm"
              block
              class="trial"
              :icon="opened.has(row.trial) ? PhCaretDown : PhCaretRight"
              :aria-expanded="opened.has(row.trial)"
              @click="toggle(row.trial)"
            >
              <span class="trial-text">
                <span class="trial-title"
                  ><span v-if="row.live" class="pulse" aria-label="Live"></span>{{ trialTitle(row).title }}</span
                >
                <span class="trial-detail">{{ trialTitle(row).detail }} · {{ row.games.length }} games here</span>
              </span>
            </EButton>
          </div>
          <template v-if="opened.has(row.trial)">
            <GameRow
              v-for="g in row.games"
              :key="g.id"
              :game="g"
              nested
              :current="!board.drilling && board.gameId === g.id"
              @open="open(g.id)"
            />
          </template>
        </template>
      </template>
    </div>
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

.filter {
  inline-size: 100%;
}

.filter :deep([role="radiogroup"]) {
  inline-size: 100%;
}

.filter :deep([role="radio"]) {
  flex: 1 1 0;
  min-width: 0;
  padding-inline: 0.25rem;
}

.rail-list {
  overflow-y: auto;
  overscroll-behavior: contain;
}

.pad {
  padding: var(--ws-pad);
}

.trial {
  height: auto;
  white-space: normal;
  justify-content: flex-start;
  min-height: 0;
  padding: var(--e-gap-snug) var(--ws-pad);
  border-radius: 0;
  border-block-start: var(--line);
  text-align: start;
  text-decoration: none;
}

.trial-text {
  flex: 1 1 auto;
  display: grid;
  gap: 0.125rem;
  min-width: 0;
}

.trial-title {
  display: flex;
  align-items: center;
  gap: var(--e-gap-tight);
  font-weight: 600;
}

.trial-detail {
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 400;
}

.pulse {
  width: 0.5rem;
  height: 0.5rem;
  border-radius: 50%;
  background: var(--e-danger);
}
</style>
