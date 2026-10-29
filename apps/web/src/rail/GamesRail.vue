<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { PhCaretDown, PhCaretRight } from "@phosphor-icons/vue";
import { useToast } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EList } from "@eos-ui/vue/components/EList";
import { EListRow } from "@eos-ui/vue/components/EListRow";
import { EVirtualList } from "@eos-ui/vue/components/EVirtualList";
import type { GameId, GameSummary, Idea, TrialSummary } from "../api/types";
import { clockOf } from "../sheets/idea";
import { useBoard } from "../board/board";
import { useMedia } from "../media";
import { useConnections } from "../stores/connections";
import { useGames } from "../stores/games";
import { useIdeas } from "../stores/ideas";
import { usePlayers } from "../stores/players";
import { useVersions } from "../stores/versions";
import { stored } from "../stored";
import UiEmpty from "../ui/UiEmpty.vue";
import UiLive from "../ui/UiLive.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import { useUi } from "../shell/ui";
import { useRailFeed, type FeedKey } from "./feed";
import GameRow from "./GameRow.vue";
import MoreRow from "./MoreRow.vue";
import { emptyNote, FILTERS, monthOf, readFilter, railRows, stageLabel, type RailFilter } from "./rail";

/**
 * The Games rail: the filter you pick is asked of the server and paged as you scroll, in one windowed list. Live
 * games are pinned. A trial is one row that opens onto its games, which come a page at a time as the row is read.
 */
const games = useGames();
const ideas = useIdeas();
const versions = useVersions();
const board = useBoard();
const feed = useRailFeed();
const ui = useUi();
const router = useRouter();
const toast = useToast();
const connections = useConnections();
const players = usePlayers();

const saved = stored<string>("rail.filter", "all");
const filter = computed<RailFilter>({ get: () => readFilter(saved.value), set: (v) => (saved.value = v) });
/** The filter whose games the server pages; `null` for Trials, whose rows are the ideas' trials. */
const gameFilter = computed(() => (filter.value === "trials" ? null : filter.value));
watch(
  gameFilter,
  (f) => {
    if (f) void feed.start(f);
  },
  { immediate: true },
);

const options = FILTERS.map((f) => ({ value: f.value, label: f.label }));

/** A trial in the rail, from the idea it belongs to. */
interface TrialItem {
  id: number;
  title: string;
  detail: string;
  live: boolean;
}
function trialItem(idea: Idea, trial: TrialSummary): TrialItem {
  const dev = versions.name(idea.version) ?? (idea.branch || idea.title);
  const base = versions.name(idea.base) ?? "the champion";
  const detail = [stageLabel[trial.stage], clockOf(trial, ideas.trials[trial.id]), `${trial.pairs} pairs`]
    .filter(Boolean)
    .join(" · ");
  return { id: trial.id, title: `${dev} vs ${base}`, detail, live: trial.status === "running" };
}
/** Every trial that has played, newest idea first, an idea's latest stage first. */
const trialList = computed(() =>
  ideas.list.flatMap((idea) =>
    [...idea.trials]
      .reverse()
      .filter((t) => t.pairs > 0 || t.status === "running")
      .map((t) => trialItem(idea, t)),
  ),
);

/** The games held for each trial that has any, newest first. */
const byTrial = computed(() => {
  const map = new Map<number, GameSummary[]>();
  for (const g of games.list) {
    if (g.source.type !== "trial") continue;
    const list = map.get(g.source.trial);
    if (list) list.push(g);
    else map.set(g.source.trial, [g]);
  }
  return map;
});

const opened = ref(new Set<number>());
function toggle(id: number) {
  const next = new Set(opened.value);
  if (next.has(id)) next.delete(id);
  else {
    next.add(id);
    void feed.start(`trial:${id}`);
  }
  opened.value = next;
}

/** What the list shows, top to bottom: a game, a trial's row, a request for the next page, or a trial's game inside it. */
type Item =
  | { key: string; kind: "game"; game: GameSummary; nested: boolean }
  | { key: string; kind: "trial"; trial: TrialItem; open: boolean }
  | { key: string; kind: "more"; feed: FeedKey; nested: boolean }
  | { key: string; kind: "month"; label: string };

function trialItems(trial: TrialItem): Item[] {
  const open = opened.value.has(trial.id);
  const out: Item[] = [{ key: `trial-${trial.id}`, kind: "trial", trial, open }];
  if (!open) return out;
  const key: FeedKey = `trial:${trial.id}`;
  const f = feed.of(key);
  for (const g of byTrial.value.get(trial.id) ?? []) {
    if (f.floor === null || g.started_at >= f.floor) {
      out.push({ key: `trial-${trial.id}-${g.id}`, kind: "game", game: g, nested: true });
    }
  }
  if (!f.done) out.push({ key: `more-${key}-${f.cursor}`, kind: "more", feed: key, nested: true });
  return out;
}

const items = computed<Item[]>(() => {
  const f = gameFilter.value;
  if (!f) return trialList.value.flatMap(trialItems);
  const out: Item[] = [];
  // Games older than two weeks sit under the month they were played in.
  let month: string | null = null;
  for (const row of railRows(games.list, f, feed.of(f).floor)) {
    if (row.type === "game") {
      const label = monthOf(row.game.started_at);
      if (label && label !== month) out.push({ key: `month-${label}`, kind: "month", label });
      month = label;
      out.push({ key: row.key, kind: "game", game: row.game, nested: false });
    } else out.push(...trialItems(trialItemOfRow(row.trial)));
  }
  const state = feed.of(f);
  if (!state.done && state.started)
    out.push({ key: `more-${f}-${state.cursor}`, kind: "more", feed: f, nested: false });
  return out;
});

/** A trial that is being played, for the All list (its idea may not be known yet). */
function trialItemOfRow(id: number): TrialItem {
  return trialList.value.find((t) => t.id === id) ?? { id, title: `Trial ${id}`, detail: "", live: true };
}

/** Whether the list has nothing to show and the server is not about to: what an empty state is for. */
const state = computed(() => {
  const f = gameFilter.value;
  if (!f) return { loading: !ideas.loaded, error: null };
  const s = feed.of(f);
  return { loading: !s.started || (s.loading && !items.value.length), error: s.error };
});
const rows = computed(() => items.value.filter((i) => i.kind === "game" || i.kind === "trial"));

/** The game on the board is the list's current row; picking a row does not make it current until the board shows it. */
const current = computed({
  get: () => (board.drilling ? null : board.gameId),
  set: () => {},
});

function retry() {
  const f = gameFilter.value;
  if (f) void feed.retry(f);
}

/** A trial's row opens or closes; a game opens on the board (and on a phone the board is what you came for). */
function pick(key: GameId | string) {
  if (typeof key === "string" && key.startsWith("trial:")) toggle(Number(key.slice(6)));
  else if (typeof key === "string" && key.startsWith("more:")) {
    const asked = items.value.find((i) => i.kind === "more" && `more:${i.key}` === key);
    if (asked?.kind === "more") void feed.retry(asked.feed);
  } else if (typeof key === "number") {
    ui.railOpen = false;
    void board.openGame(key as GameId);
  }
}

/** Brings your Lichess games in: what the Lichess filter's empty state offers when the account is linked. */
async function importMine() {
  try {
    await players.importHistory("you");
    toast.show({ title: "Importing your Lichess games", message: "They appear here as they arrive.", tone: "info" });
  } catch (e) {
    toast.show({
      title: "That did not go through",
      message: e instanceof Error ? e.message : String(e),
      tone: "warning",
    });
  }
}

const coarse = useMedia("(pointer: coarse)");
</script>

<template>
  <nav class="rail" aria-label="Games">
    <UiToolbar class="rail-head" label="Games" :wrap="false">
      <UiSeg v-model="filter" label="Show games" size="sm" dense :options="options" />
    </UiToolbar>
    <div v-if="state.loading && !rows.length" class="skeleton" aria-busy="true" aria-label="Loading games">
      <div v-for="n in 7" :key="n" class="skeleton-row">
        <span class="skel skeleton-title" :style="{ width: `${52 + ((n * 17) % 34)}%` }" />
        <span class="skel skeleton-meta" :style="{ width: `${34 + ((n * 11) % 24)}%` }" />
      </div>
    </div>
    <UiEmpty
      v-else-if="state.error && !rows.length"
      tone="error"
      title="Games did not load"
      body="The app did not answer."
    >
      <template #actions>
        <EButton variant="outline" size="sm" @click="retry">Try again</EButton>
      </template>
    </UiEmpty>
    <UiEmpty v-else-if="!rows.length" :title="emptyNote[filter].title" :body="emptyNote[filter].body">
      <template #actions>
        <template v-if="filter === 'trials'">
          <EButton size="sm" @click="router.push({ path: '/dameless', query: { new: 'idea' } })">New idea</EButton>
        </template>
        <template v-else-if="filter === 'lichess'">
          <EButton v-if="connections.you.configured" size="sm" @click="importMine"> Import my Lichess games </EButton>
          <EButton v-else size="sm" variant="outline" @click="router.push('/settings')"> Open Settings </EButton>
        </template>
        <template v-else>
          <EButton size="sm" @click="ui.startNewGame()">New game</EButton>
          <EButton v-if="filter === 'all'" size="sm" variant="quiet" @click="router.push('/settings')">
            Open Settings
          </EButton>
        </template>
      </template>
    </UiEmpty>
    <EList v-else v-model="current" label="Games" class="rail-list" flush @select="pick">
      <EVirtualList v-slot="{ item }" :items="items" :row-height="coarse ? 56 : 48" measure item-key="key">
        <GameRow v-if="item.kind === 'game'" :game="item.game" :nested="item.nested" />
        <EListRow
          v-else-if="item.kind === 'trial'"
          :value="`trial:${item.trial.id}`"
          :expanded="item.open"
          trailing-stack
          class="trial-row"
        >
          <template #title>{{ item.trial.title }}</template>
          <template #subtitle>
            <span class="truncate">{{ item.trial.detail }}</span>
          </template>
          <template #trailing>
            <UiLive v-if="item.trial.live" label="Running" class="dot" />
            <component :is="item.open ? PhCaretDown : PhCaretRight" class="caret" aria-hidden="true" />
          </template>
        </EListRow>
        <EListRow v-else-if="item.kind === 'month'" :value="item.key" disabled class="month">
          <template #title>{{ item.label }}</template>
        </EListRow>
        <MoreRow
          v-else
          :value="`more:${item.key}`"
          :loading="feed.of(item.feed).loading"
          :error="feed.of(item.feed).error"
          :nested="item.nested"
          @more="feed.more(item.feed)"
        />
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
  padding: var(--sp-1) var(--sp-2);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.rail-list {
  grid-template-rows: minmax(0, 1fr);
  min-height: 0;
}

/* Rows: two lines, a hairline between, no box. The current game is a wash with the action colour's bar at its edge. */
.rail :deep(.e-listrow) {
  position: relative;
  min-height: var(--h-rail-row);
  padding-block: var(--sp-1);
  padding-inline: var(--sp-3);
  border: 0;
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  border-radius: var(--r-0);
}

.rail :deep(.e-listrow-title) {
  gap: var(--sp-2);
  font: var(--t-body-strong);
}

.rail :deep(.e-listrow-sub) {
  gap: var(--sp-2);
  font: var(--t-meta);
}

.rail :deep(.e-listrow-trail) {
  gap: 0;
  color: var(--c-ink);
  font: var(--t-fig);
}

.rail :deep(.e-listrow:is([aria-current="true"], [aria-selected="true"])) {
  border-color: transparent;
  background-color: var(--c-sunk);
  box-shadow: inset var(--e-stroke-focus) 0 0 var(--c-act);
}

.rail :deep(.e-listrow:is([aria-current="true"], [aria-selected="true"]) .e-listrow-title) {
  color: var(--c-ink);
}

.rail :deep(.e-listrow:not([aria-current="true"], [aria-selected="true"], [aria-disabled="true"]):hover) {
  border-color: transparent;
  background-color: var(--c-raise);
}

.rail :deep(.e-listrow:focus-visible) {
  outline: var(--focus-ring);
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

/* A trial's games sit one step in under its row. */
.rail :deep(.nested > .e-listrow) {
  padding-inline-start: var(--sp-5);
}

.rail :deep(.month .e-listrow) {
  min-height: var(--h-row);
  background: var(--c-raise);
}

.rail :deep(.month .e-listrow-title) {
  color: var(--c-ink-2);
  font: var(--t-meta);
  font-weight: 600;
}

.trial-row .caret {
  color: var(--c-ink-2);
  font-size: var(--i-sm);
}

.trial-row .dot {
  margin-block: var(--sp-1);
}

.skeleton {
  display: grid;
  align-content: start;
}

.skeleton-row {
  display: grid;
  align-content: center;
  gap: var(--sp-2);
  height: var(--h-rail-row);
  padding-inline: var(--sp-3);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
}

.skeleton-title {
  height: var(--sp-3);
}

.skeleton-meta {
  height: var(--sp-2);
}
</style>
