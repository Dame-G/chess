<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhX } from "@phosphor-icons/vue";
import { ESheet } from "@eos-ui/vue/components/ESheet";
import { ECheckboxGroup } from "@eos-ui/vue/components/ECheckboxGroup";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EIconButton } from "@eos-ui/vue/components/EIconButton";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ETable } from "@eos-ui/vue/components/ETable";
import { EVisuallyHidden } from "@eos-ui/vue/components/EVisuallyHidden";
import type { Speed } from "../api/types";
import { speedName } from "../format";
import AddPlayer from "../parts/AddPlayer.vue";
import Term from "../parts/Term.vue";
import TopPlayers from "../parts/TopPlayers.vue";
import { usePlayers } from "../stores/players";
import { addressQuery, compareGroups, type CompareAddress, LIMITS, leads, parseAddress, SPEEDS } from "./compare";
import SheetBack from "./SheetBack.vue";
import { useSheet } from "./useSheet";

/**
 * Compare: players as columns, rows grouped (results, accuracy, moves per game, time, openings), the leader of a row
 * tinted. The address holds everything (`?players=you,dameless,lichess:x&speeds=blitz&rated=true&limit=100`), so a
 * comparison is a link. Add someone by name or pasted profile link, or from the leaderboard.
 */
const sheet = useSheet();
const route = useRoute();
const router = useRouter();
const players = usePlayers();

const address = computed<CompareAddress>(() => parseAddress(route.query));
const failed = ref<string | null>(null);
const loading = ref(false);
const comparison = computed(() => players.comparison);
const MAX = 5;

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
  set: (v: Speed[]) => set({ speeds: v }),
});
const rated = computed({
  get: () => (address.value.rated == null ? "both" : address.value.rated ? "rated" : "casual"),
  set: (v: string | undefined) => set({ rated: v === "rated" ? true : v === "casual" ? false : null }),
});
const limit = computed({
  get: () => String(address.value.limit),
  set: (v: string | undefined) => v && set({ limit: Number(v) }),
});

const speedOptions = SPEEDS.map((s) => ({ value: s, label: speedName[s] }));
const limitOptions = LIMITS.map((n) => ({ value: String(n), label: `${n} games` }));

const profiles = computed(() => comparison.value?.profiles ?? []);
const groups = computed(() => compareGroups(profiles.value));
const to = (key: string) => (key === "you" ? "/you" : key === "dameless" ? "/dameless" : `/players/${key}`);
const cell = (v: number | null | undefined, text: (v: number) => string) => (v == null ? "—" : text(v));
/** Whether column `i` leads a row. A row where everyone ties has no leader worth marking. */
function leading(row: Parameters<typeof leads>[1], i: number): boolean {
  const c = comparison.value;
  if (!c || !leads(c, row, i)) return false;
  const lead = c.leaders.find((l) => l.row === row);
  return !lead || lead.players.length < c.profiles.length;
}
const stale = computed(
  () =>
    !!comparison.value && comparison.value.profiles.map((p) => p.player.key).join() !== address.value.players.join(),
);
</script>

<template>
  <ESheet
    v-bind="sheet.bind"
    title="Compare"
    description="Players side by side over the games they have here, reviewed by the same engine."
  >
    <template #leading><SheetBack :show="sheet.canGoBack" @back="sheet.back" /></template>
    <ENotice v-if="failed" tone="danger" title="The comparison did not load" class="gap">{{ failed }}</ENotice>

    <section class="sec" aria-label="Filters">
      <div class="filters">
        <ECheckboxGroup v-model="speeds" label="Speeds" orientation="horizontal" :options="speedOptions" />
        <ESegmented
          v-model="rated"
          label="Games"
          size="sm"
          :options="[
            { value: 'both', label: 'Rated and casual' },
            { value: 'rated', label: 'Rated' },
            { value: 'casual', label: 'Casual' },
          ]"
        />
        <ESelect v-model="limit" label="Newest games" size="sm" :options="limitOptions" />
      </div>
    </section>

    <section class="sec" aria-label="The players">
      <div v-if="!comparison && !failed" aria-busy="true"><ESkeleton :lines="8" label="Loading the comparison" /></div>
      <div
        v-else-if="comparison"
        class="scroll"
        :class="{ stale: stale || loading }"
        :style="{ '--cols': profiles.length }"
      >
        <ETable caption="Players compared" hide-caption dense class="table">
          <thead>
            <tr>
              <th scope="col" class="label-col"><EVisuallyHidden>Measure</EVisuallyHidden></th>
              <th v-for="p in profiles" :key="p.player.key" scope="col" class="col-head">
                <span class="who">
                  <ELink :link-component="RouterLink" :to="to(p.player.key)" class="who-name">{{
                    p.player.name
                  }}</ELink>
                  <EIconButton
                    v-if="profiles.length > 1"
                    :icon="PhX"
                    :label="`Remove ${p.player.name}`"
                    tooltip
                    size="sm"
                    @click="remove(p.player.key)"
                  />
                </span>
                <span class="who-sub">{{ p.rating ?? "—" }} rating</span>
                <span class="who-sub">{{ p.reviewed }} of {{ p.games }} reviewed</span>
              </th>
            </tr>
          </thead>
          <tbody v-for="g in groups" :key="g.id">
            <tr class="group">
              <th :colspan="profiles.length + 1" scope="colgroup">{{ g.title }}</th>
            </tr>
            <tr v-for="r in g.rows" :key="r.id">
              <th scope="row" class="label-col">
                <Term v-if="r.term" :k="r.term" :text="r.label" />
                <template v-else>{{ r.label }}</template>
              </th>
              <td v-for="(p, i) in profiles" :key="p.player.key" class="e-num" :class="{ lead: leading(r.leader, i) }">
                {{ cell(r.get(p), r.text) }}
                <EVisuallyHidden v-if="leading(r.leader, i)"> (leads)</EVisuallyHidden>
              </td>
            </tr>
          </tbody>
          <tbody>
            <tr class="group">
              <th :colspan="profiles.length + 1" scope="colgroup">Openings most played</th>
            </tr>
            <tr>
              <th scope="row" class="label-col">Top three</th>
              <td v-for="p in profiles" :key="p.player.key" class="opens">
                <ol v-if="p.openings.length" class="plain">
                  <li v-for="o in p.openings.slice(0, 3)" :key="o.name">
                    {{ o.name }} <span class="mono muted">{{ o.results.games }}</span>
                  </li>
                </ol>
                <template v-else>—</template>
              </td>
            </tr>
          </tbody>
        </ETable>
      </div>
      <p v-if="comparison && profiles.some((p) => p.pending_review > 0)" class="sec-note">
        Rows that need the engine count reviewed games only. Open a player to review the rest.
      </p>
    </section>

    <section class="sec" aria-label="Add a player">
      <AddPlayer :taken="address.players" :full="address.players.length >= MAX" @add="add" />
      <EDisclosure title="Top players now">
        <TopPlayers :taken="address.players" :full="address.players.length >= MAX" @add="add" />
      </EDisclosure>
      <p class="sec-note">
        <Term k="accuracy" /> and <Term k="acpl" /> come from the same engine for everyone, so they compare fairly.
      </p>
    </section>
  </ESheet>
</template>

<style scoped>
.gap {
  margin-block-end: var(--e-gap-base);
}

.filters {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--e-gap-snug) var(--e-gap-base);
}

.scroll {
  --label-w: 9.5rem;

  position: relative;
  min-width: 0;
  overflow: clip;
  transition: opacity var(--e-duration-fade, 0.15s);
}

@media (prefers-reduced-motion: reduce) {
  .scroll {
    transition: none;
  }
}

.scroll.stale {
  opacity: 0.6;
}

/* The table is wider than a phone: it scrolls sideways inside eos's region while the labels stay in view. */
.table :deep(table) {
  min-inline-size: calc(var(--label-w) + var(--cols, 2) * 8.5rem);
  table-layout: fixed;
}

.table :deep(td),
.table :deep(th) {
  vertical-align: middle;
}

.table :deep(thead th) {
  vertical-align: top;
}

.table :deep(thead th.label-col) {
  background: var(--e-surface-tint);
}

.label-col {
  position: sticky;
  inset-inline-start: 0;
  z-index: var(--e-z-raised);
  inline-size: var(--label-w);
  min-inline-size: var(--label-w);
  background: var(--e-surface);
  font-family: var(--e-font-sans);
  font-weight: 500;
  letter-spacing: normal;
  text-transform: none;
  white-space: normal;
  text-align: start;
}

.col-head {
  min-inline-size: 7.5rem;
  font: var(--e-type-body);
  font-weight: 600;
  letter-spacing: normal;
  text-transform: none;
  vertical-align: top;
  white-space: normal;
}

.who {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--e-gap-tight);
}

.who-name {
  min-width: 0;
  font-weight: 600;
  overflow-wrap: anywhere;
}

.who-sub {
  display: block;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-variant-numeric: tabular-nums;
}

.group th {
  padding-block: var(--e-gap-snug) var(--e-gap-tight);
  background: var(--e-surface);
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  text-align: start;
}

/* The leader of a row. */
.lead {
  background: var(--e-success-soft);
  font-weight: 600;
}

.opens {
  vertical-align: top;
  text-align: start;
}

/* The checkboxes share the switches' height, so the three labels and controls sit on one line. */
.filters :deep(.e-checkgroup-items) {
  align-items: center;
  gap: 0 var(--e-gap-base);
  min-block-size: 2.125rem;
}

.filters :deep(.e-choice) {
  align-items: center;
  min-block-size: 2.125rem;
  padding-block: 0;
}

@media (width < 36rem) {
  .scroll {
    --label-w: 7rem;
  }
}

.muted {
  color: var(--e-muted);
}
</style>
