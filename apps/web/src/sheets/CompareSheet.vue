<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { PhCaretDown, PhX } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { EVisuallyHidden } from "@eos-ui/vue/components/EVisuallyHidden";
import type { Profile, Speed } from "../api/types";
import { formatRating, speedName, windowName } from "../format";
import AddPlayer from "../parts/AddPlayer.vue";
import Term from "../parts/Term.vue";
import TopPlayers from "../parts/TopPlayers.vue";
import { useMedia, up } from "../media";
import { useEngines } from "../stores/engines";
import { usePlayers } from "../stores/players";
import UiEmpty from "../ui/UiEmpty.vue";
import UiPlayer from "../ui/UiPlayer.vue";
import UiSeg from "../ui/UiSeg.vue";
import UiSheetPage from "../ui/UiSheetPage.vue";
import UiTable from "../ui/UiTable.vue";
import UiToolbar from "../ui/UiToolbar.vue";
import { addressQuery, compareGroups, type CompareAddress, LIMITS, leads, parseAddress, SPEEDS } from "./compare";
import UiSelect from "../ui/UiSelect.vue";

/**
 * Compare: players as columns, rows in groups (results, accuracy, moves per game, time, openings), the leader of a row
 * in bold with a dot. The address holds everything (`?players=you,engine:stockfish,lichess:x&speeds=blitz&rated=true&limit=100`),
 * so a comparison is a link. Add someone by name or pasted profile link at the top, or from the leaderboard.
 */
const route = useRoute();
const router = useRouter();
const players = usePlayers();
const engines = useEngines();

const address = computed<CompareAddress>(() => parseAddress(route.query, engines.analysisId));
const failed = ref<string | null>(null);
const loading = ref(false);
const comparison = computed(() => players.comparison);
const MAX = 5;
/** Who a column is, as the rest of the app says it: you are "You", not your Lichess name. */
const named = (p: Profile): string => (p.player.key === "you" ? "You" : p.player.name);

async function load() {
  failed.value = null;
  loading.value = true;
  try {
    await players.loadComparison(address.value);
  } catch (e) {
    failed.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}
watch(() => JSON.stringify(address.value), load, { immediate: true });
onBeforeUnmount(() => players.unwatch());

function set(next: Partial<CompareAddress>) {
  void router.replace({ path: "/compare", query: addressQuery({ ...address.value, ...next }) });
}
const add = (key: string) => {
  if (!address.value.players.includes(key) && address.value.players.length < MAX)
    set({ players: [...address.value.players, key] });
};
const remove = (key: string) => set({ players: address.value.players.filter((k) => k !== key) });

const speeds = computed({
  get: () => address.value.speeds,
  // The address keeps the speeds in the order the app lists them, whatever order they were switched on in.
  set: (v: Speed[]) => set({ speeds: SPEEDS.filter((s) => v.includes(s)) }),
});
const rated = computed({
  get: () => (address.value.rated == null ? "both" : address.value.rated ? "rated" : "casual"),
  set: (v: string) => set({ rated: v === "rated" ? true : v === "casual" ? false : null }),
});
const limit = computed({
  get: () => String(address.value.limit),
  set: (v: string | undefined) => v && set({ limit: Number(v) }),
});

const speedOptions = SPEEDS.map((s) => ({ value: s, label: speedName[s] }));
const kinds = [
  { value: "both", label: "All" },
  { value: "rated", label: "Rated" },
  { value: "casual", label: "Casual" },
];
const limitOptions = LIMITS.map((n) => ({ value: String(n), label: windowName(n) }));

const profiles = computed(() => comparison.value?.profiles ?? []);
const groups = computed(() => compareGroups(profiles.value));
const stale = computed(
  () =>
    !!comparison.value && comparison.value.profiles.map((p) => p.player.key).join() !== address.value.players.join(),
);
const anyReviewedOnly = computed(() => tables.value.some((t) => t.reviewed));

/** Whether column `i` leads a row. A row where everyone ties has no leader worth marking. */
function leading(row: Parameters<typeof leads>[1], i: number): boolean {
  const c = comparison.value;
  if (!c || !leads(c, row, i)) return false;
  const lead = c.leaders.find((l) => l.row === row);
  return !lead || lead.players.length < c.profiles.length;
}

/** The head sticks while the table fits; a table that needs the width scrolls sideways instead, labels in view. */
const roomy = useMedia(up("md"));
const sticky = computed(() => roomy.value && profiles.value.length <= 3);

const columnsFor = (headFirst: string) => [
  { key: "label", label: headFirst },
  ...profiles.value.map((p, i) => ({ key: `p${i}`, label: named(p), align: "end" as const, fig: true })),
];
/** A row no one has a figure for is left out, and a group left with no rows with it: a column of dashes says nothing. */
const tables = computed(() =>
  groups.value
    .map((g) => ({ g, rows: g.rows.filter((r) => profiles.value.some((p) => r.get(p) != null)) }))
    .filter((x) => x.rows.length)
    .map(({ g, rows }, n) => ({
      id: g.id,
      n,
      title: g.title,
      reviewed: !!g.reviewed,
      columns: columnsFor(g.title),
      rows: rows.map((r) => ({ id: r.id, label: r.label, term: r.term, spec: r })),
    })),
);
const nothingReviewed = computed(() => groups.value.some((g) => g.reviewed) && !tables.value.some((t) => t.reviewed));
const openings = computed(() => ({
  columns: columnsFor("Openings most played"),
  rows: [{ id: "top3" }],
}));
const ref_ = (p: Profile) => ({ name: named(p), key: p.player.key, title: null, rating: null });

const cell = (v: number | null | undefined, text: (v: number) => string) => (v == null ? null : text(v));

const showTop = ref(false);
</script>

<template>
  <UiSheetPage title="Compare" wide :loading="!comparison && !failed">
    <div class="bar">
      <UiToolbar label="Which games to compare">
        <UiSeg v-model:values="speeds" label="Speeds" size="sm" multiple :options="speedOptions" />
        <UiSeg v-model="rated" label="Rated or casual" size="sm" :options="kinds" />
        <UiSelect v-model="limit" label="Games" dense :options="limitOptions" />
      </UiToolbar>
      <div class="add">
        <AddPlayer :taken="address.players" :full="address.players.length >= MAX" class="add-box" @add="add" />
        <EButton variant="quiet" size="sm" :aria-expanded="showTop" @click="showTop = !showTop">
          Top players
          <PhCaretDown class="caret" :class="{ 'is-open': showTop }" aria-hidden="true" />
        </EButton>
      </div>
    </div>

    <div v-if="showTop" class="top">
      <TopPlayers :taken="address.players" :full="address.players.length >= MAX" @add="add" />
    </div>

    <UiEmpty v-if="failed" tone="error" title="The comparison didn't load" :body="failed">
      <template #actions>
        <EButton variant="outline" @click="load">Try again</EButton>
      </template>
    </UiEmpty>
    <div v-else-if="comparison" class="cmp" :class="{ stale: stale || loading }" :style="{ '--n': profiles.length }">
      <UiTable
        v-for="t in tables"
        :key="t.id"
        :label="t.title"
        :columns="t.columns"
        :rows="t.rows"
        row-key="id"
        :sticky="sticky"
      >
        <template #head-p0>
          <template v-if="profiles[0]">
            <template v-if="t.n === 0">
              <span class="who">
                <UiPlayer :seat="ref_(profiles[0]!)" link />
                <EIconButton
                  v-if="profiles.length > 1"
                  :icon="PhX"
                  :label="`Remove ${named(profiles[0]!)}`"
                  tooltip
                  size="sm"
                  @click="remove(profiles[0]!.player.key)"
                />
              </span>
              <span class="sub"
                ><span class="num">{{ formatRating(profiles[0]!.rating) }}</span> · {{ profiles[0]!.reviewed }} of
                {{ profiles[0]!.games }} reviewed</span
              >
            </template>
            <span v-else class="name truncate">{{ named(profiles[0]!) }}</span>
          </template>
        </template>
        <template #head-p1>
          <template v-if="profiles[1]">
            <template v-if="t.n === 0">
              <span class="who">
                <UiPlayer :seat="ref_(profiles[1]!)" link />
                <EIconButton
                  v-if="profiles.length > 1"
                  :icon="PhX"
                  :label="`Remove ${named(profiles[1]!)}`"
                  tooltip
                  size="sm"
                  @click="remove(profiles[1]!.player.key)"
                />
              </span>
              <span class="sub"
                ><span class="num">{{ formatRating(profiles[1]!.rating) }}</span> · {{ profiles[1]!.reviewed }} of
                {{ profiles[1]!.games }} reviewed</span
              >
            </template>
            <span v-else class="name truncate">{{ named(profiles[1]!) }}</span>
          </template>
        </template>
        <template #head-p2>
          <template v-if="profiles[2]">
            <template v-if="t.n === 0">
              <span class="who">
                <UiPlayer :seat="ref_(profiles[2]!)" link />
                <EIconButton
                  v-if="profiles.length > 1"
                  :icon="PhX"
                  :label="`Remove ${named(profiles[2]!)}`"
                  tooltip
                  size="sm"
                  @click="remove(profiles[2]!.player.key)"
                />
              </span>
              <span class="sub"
                ><span class="num">{{ formatRating(profiles[2]!.rating) }}</span> · {{ profiles[2]!.reviewed }} of
                {{ profiles[2]!.games }} reviewed</span
              >
            </template>
            <span v-else class="name truncate">{{ named(profiles[2]!) }}</span>
          </template>
        </template>
        <template #head-p3>
          <template v-if="profiles[3]">
            <template v-if="t.n === 0">
              <span class="who">
                <UiPlayer :seat="ref_(profiles[3]!)" link />
                <EIconButton
                  v-if="profiles.length > 1"
                  :icon="PhX"
                  :label="`Remove ${named(profiles[3]!)}`"
                  tooltip
                  size="sm"
                  @click="remove(profiles[3]!.player.key)"
                />
              </span>
              <span class="sub"
                ><span class="num">{{ formatRating(profiles[3]!.rating) }}</span> · {{ profiles[3]!.reviewed }} of
                {{ profiles[3]!.games }} reviewed</span
              >
            </template>
            <span v-else class="name truncate">{{ named(profiles[3]!) }}</span>
          </template>
        </template>
        <template #head-p4>
          <template v-if="profiles[4]">
            <template v-if="t.n === 0">
              <span class="who">
                <UiPlayer :seat="ref_(profiles[4]!)" link />
                <EIconButton
                  v-if="profiles.length > 1"
                  :icon="PhX"
                  :label="`Remove ${named(profiles[4]!)}`"
                  tooltip
                  size="sm"
                  @click="remove(profiles[4]!.player.key)"
                />
              </span>
              <span class="sub"
                ><span class="num">{{ formatRating(profiles[4]!.rating) }}</span> · {{ profiles[4]!.reviewed }} of
                {{ profiles[4]!.games }} reviewed</span
              >
            </template>
            <span v-else class="name truncate">{{ named(profiles[4]!) }}</span>
          </template>
        </template>
        <template #cell-label="{ row }">
          <Term v-if="row.term" :k="row.term" :text="row.label" />
          <template v-else>{{ row.label }}</template>
          <span v-if="t.reviewed" class="mark" title="Counts reviewed games only" aria-hidden="true"> ◦</span>
        </template>
        <template #cell-p0="{ row }">
          <template v-if="profiles[0]">
            <span v-if="cell(row.spec.get(profiles[0]!), row.spec.text) == null" class="none">–</span>
            <span v-else class="val" :class="{ lead: leading(row.spec.leader, 0) }">
              {{ cell(row.spec.get(profiles[0]!), row.spec.text) }}
              <EVisuallyHidden v-if="leading(row.spec.leader, 0)"> (leads)</EVisuallyHidden>
            </span>
          </template>
        </template>
        <template #cell-p1="{ row }">
          <template v-if="profiles[1]">
            <span v-if="cell(row.spec.get(profiles[1]!), row.spec.text) == null" class="none">–</span>
            <span v-else class="val" :class="{ lead: leading(row.spec.leader, 1) }">
              {{ cell(row.spec.get(profiles[1]!), row.spec.text) }}
              <EVisuallyHidden v-if="leading(row.spec.leader, 1)"> (leads)</EVisuallyHidden>
            </span>
          </template>
        </template>
        <template #cell-p2="{ row }">
          <template v-if="profiles[2]">
            <span v-if="cell(row.spec.get(profiles[2]!), row.spec.text) == null" class="none">–</span>
            <span v-else class="val" :class="{ lead: leading(row.spec.leader, 2) }">
              {{ cell(row.spec.get(profiles[2]!), row.spec.text) }}
              <EVisuallyHidden v-if="leading(row.spec.leader, 2)"> (leads)</EVisuallyHidden>
            </span>
          </template>
        </template>
        <template #cell-p3="{ row }">
          <template v-if="profiles[3]">
            <span v-if="cell(row.spec.get(profiles[3]!), row.spec.text) == null" class="none">–</span>
            <span v-else class="val" :class="{ lead: leading(row.spec.leader, 3) }">
              {{ cell(row.spec.get(profiles[3]!), row.spec.text) }}
              <EVisuallyHidden v-if="leading(row.spec.leader, 3)"> (leads)</EVisuallyHidden>
            </span>
          </template>
        </template>
        <template #cell-p4="{ row }">
          <template v-if="profiles[4]">
            <span v-if="cell(row.spec.get(profiles[4]!), row.spec.text) == null" class="none">–</span>
            <span v-else class="val" :class="{ lead: leading(row.spec.leader, 4) }">
              {{ cell(row.spec.get(profiles[4]!), row.spec.text) }}
              <EVisuallyHidden v-if="leading(row.spec.leader, 4)"> (leads)</EVisuallyHidden>
            </span>
          </template>
        </template>
      </UiTable>

      <UiTable label="Openings most played" :columns="openings.columns" :rows="openings.rows" row-key="id">
        <template #cell-label>Top three</template>
        <template #head-p0>
          <span v-if="profiles[0]" class="name truncate">{{ named(profiles[0]!) }}</span>
        </template>
        <template #head-p1>
          <span v-if="profiles[1]" class="name truncate">{{ named(profiles[1]!) }}</span>
        </template>
        <template #head-p2>
          <span v-if="profiles[2]" class="name truncate">{{ named(profiles[2]!) }}</span>
        </template>
        <template #head-p3>
          <span v-if="profiles[3]" class="name truncate">{{ named(profiles[3]!) }}</span>
        </template>
        <template #head-p4>
          <span v-if="profiles[4]" class="name truncate">{{ named(profiles[4]!) }}</span>
        </template>
        <template #cell-p0>
          <template v-if="profiles[0]">
            <ol v-if="profiles[0]!.openings.length" class="opens">
              <li v-for="o in profiles[0]!.openings.slice(0, 3)" :key="o.name">
                <span class="open-name">{{ o.name }}</span> <span class="num muted">{{ o.results.games }}</span>
              </li>
            </ol>
            <span v-else class="none">–</span>
          </template>
        </template>
        <template #cell-p1>
          <template v-if="profiles[1]">
            <ol v-if="profiles[1]!.openings.length" class="opens">
              <li v-for="o in profiles[1]!.openings.slice(0, 3)" :key="o.name">
                <span class="open-name">{{ o.name }}</span> <span class="num muted">{{ o.results.games }}</span>
              </li>
            </ol>
            <span v-else class="none">–</span>
          </template>
        </template>
        <template #cell-p2>
          <template v-if="profiles[2]">
            <ol v-if="profiles[2]!.openings.length" class="opens">
              <li v-for="o in profiles[2]!.openings.slice(0, 3)" :key="o.name">
                <span class="open-name">{{ o.name }}</span> <span class="num muted">{{ o.results.games }}</span>
              </li>
            </ol>
            <span v-else class="none">–</span>
          </template>
        </template>
        <template #cell-p3>
          <template v-if="profiles[3]">
            <ol v-if="profiles[3]!.openings.length" class="opens">
              <li v-for="o in profiles[3]!.openings.slice(0, 3)" :key="o.name">
                <span class="open-name">{{ o.name }}</span> <span class="num muted">{{ o.results.games }}</span>
              </li>
            </ol>
            <span v-else class="none">–</span>
          </template>
        </template>
        <template #cell-p4>
          <template v-if="profiles[4]">
            <ol v-if="profiles[4]!.openings.length" class="opens">
              <li v-for="o in profiles[4]!.openings.slice(0, 3)" :key="o.name">
                <span class="open-name">{{ o.name }}</span> <span class="num muted">{{ o.results.games }}</span>
              </li>
            </ol>
            <span v-else class="none">–</span>
          </template>
        </template>
      </UiTable>

      <p v-if="nothingReviewed" class="foot">
        Accuracy and the judged moves come from reviewed games, and none of these has been reviewed yet.
      </p>
      <p v-if="anyReviewedOnly" class="foot">
        ◦ Counts reviewed games only. <Term k="accuracy" /> and <Term k="acpl" /> come from the same engine for
        everyone, so they compare fairly.
      </p>
    </div>
  </UiSheetPage>
</template>

<style scoped>
/* The toolbar and the add field stay in reach while the table scrolls under them. */
.bar {
  position: sticky;
  inset-block-start: 0;
  z-index: var(--e-z-sticky);
  display: grid;
  gap: var(--sp-1);
  margin: calc(-1 * var(--sp-3)) calc(-1 * var(--sp-3)) 0;
  padding: var(--sp-2) var(--sp-3);
  border-block-end: var(--e-stroke-hair) solid var(--c-line);
  background: var(--c-pane);
}

@media (width >= 56rem) {
  .bar {
    margin: calc(-1 * var(--sp-4)) calc(-1 * var(--sp-4)) 0;
    padding-inline: var(--sp-4);
  }
}

.add {
  display: flex;
  align-items: flex-start;
  gap: var(--sp-2);
}

.add-box {
  flex: 1 1 auto;
}

.caret {
  transition: transform var(--m-fast);
}

.caret.is-open {
  transform: rotate(180deg);
}

@media (prefers-reduced-motion: reduce) {
  .caret {
    transition: none;
  }
}

.top {
  max-height: 22rem;
  overflow: auto;
  padding: var(--sp-2) var(--sp-3);
  border: var(--e-stroke-hair) solid var(--c-line-strong);
  border-radius: var(--r-1);
}

.cmp {
  --label-w: 11rem;

  display: grid;
  gap: var(--sp-4);
  min-width: 0;
  transition: opacity var(--m-fast);
}

@media (prefers-reduced-motion: reduce) {
  .cmp {
    transition: none;
  }
}

@media (width < 30rem) {
  .cmp {
    --label-w: 7rem;
  }
}

.cmp.stale {
  opacity: 0.6;
}

/* Every group is its own table; fixed columns make them line up as one. */
.cmp :deep(table) {
  min-width: calc(var(--label-w) + var(--n, 2) * 6rem);
  table-layout: fixed;
}

.cmp :deep(th.is-fig),
.cmp :deep(td.is-fig) {
  width: auto;
}

.cmp :deep(th:first-child),
.cmp :deep(td:first-child) {
  position: sticky;
  inset-inline-start: 0;
  z-index: 1;
  width: var(--label-w);
  min-width: var(--label-w);
  overflow: hidden;
  text-overflow: ellipsis;
}

.cmp :deep(td:first-child) {
  background: var(--c-pane);
  font: var(--t-body);
  font-weight: 500;
  white-space: nowrap;
}

.cmp :deep(th) {
  overflow: hidden;
  text-overflow: ellipsis;
}

.cmp :deep(thead) {
  font-size: var(--e-text-xs);
}

.who {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  min-width: 0;
}

.sub {
  display: block;
  color: var(--c-ink-2);
  font: var(--t-meta);
  text-align: end;
}

.name {
  display: block;
  color: var(--c-ink-2);
}

.val {
  display: inline-flex;
  align-items: center;
  gap: var(--sp-1);
}

.val.lead {
  font-weight: 700;
}

.val.lead::before {
  content: "";
  flex: none;
  inline-size: var(--sp-1);
  block-size: var(--sp-1);
  border-radius: var(--r-pill);
  background: var(--c-ink);
}

.none {
  color: var(--c-ink-3);
}

.mark {
  color: var(--c-ink-3);
}

.opens {
  display: grid;
  gap: var(--sp-1);
  margin: 0;
  padding: 0;
  list-style: none;
  font: var(--t-body);
  text-align: end;
}

.open-name {
  overflow-wrap: anywhere;
}

.muted {
  color: var(--c-ink-2);
}

.foot {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
