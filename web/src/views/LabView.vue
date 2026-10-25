<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhPlus, PhStop } from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESearchBar } from "@eos-ui/vue/components/ESearchBar";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { api } from "../api/runner";
import { useExperiment, useExperiments, useGames, type Experiment } from "../api/queries";
import { verdict } from "../format";
import HowItWorks from "../components/HowItWorks.vue";
import NewExperiment from "../components/NewExperiment.vue";
import Workspace from "../components/Workspace.vue";
import ExperimentStage from "../components/lab/ExperimentStage.vue";
import GameList from "../components/lab/GameList.vue";
import { defaultStage, headline, outcome, shortAgo } from "../components/lab/lab";

const route = useRoute();
const router = useRouter();
const { state: list } = useExperiments();

const id = computed(() => Number(route.params.id) || 0);
const { state } = useExperiment(id);
const experiment = computed(() =>
  state.value.data?.experiment.id === id.value ? state.value.data.experiment : undefined,
);
const tests = computed(() => state.value.data?.tests ?? []);
const active = computed(() => !!experiment.value && ["queued", "running"].includes(experiment.value.status));

// ---- Library ----
const search = ref("");
const filter = ref<"all" | "running" | "passed" | "failed">("all");
const batch = ref("");
const all = computed(() => list.value.data ?? []);
const batches = computed(() => [...new Set(all.value.map((e) => e.batch).filter((b): b is string => !!b))]);

const matches = (e: Experiment) =>
  filter.value === "all" ||
  (filter.value === "running" && ["queued", "running"].includes(e.status)) ||
  e.status === filter.value;
const shown = computed(() => {
  const q = search.value.trim().toLowerCase();
  return all.value.filter(
    (e) =>
      matches(e) &&
      (!batch.value || e.batch === batch.value) &&
      (!q || [e.name, e.dev, e.base, e.hypothesis, e.batch ?? ""].some((t) => t.toLowerCase().includes(q))),
  );
});

const mark = (e: Experiment) => {
  const tone = verdict(e).tone;
  return tone === "info" ? "primary" : tone === "neutral" ? "border-strong" : tone;
};

// ---- Selection lives in the URL ----
watch(
  [all, id],
  ([rows, current]) => {
    if (current || !rows.length) return;
    const pick = rows.find((e) => e.status === "running") ?? rows[0]!;
    void router.replace({ path: `/lab/${pick.id}`, query: route.query });
  },
  { immediate: true },
);

const listEl = ref<HTMLElement>();
watch(id, async () => {
  await nextTick();
  listEl.value?.querySelector("[aria-current]")?.scrollIntoView({ block: "nearest" });
});

function step(delta: number, event: KeyboardEvent) {
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest("input, textarea, select, [role=radio], [role=combobox], [role=listbox], [role=dialog]")) return;
  const rows = shown.value;
  if (!rows.length) return;
  event.preventDefault();
  const at = rows.findIndex((e) => e.id === id.value);
  const next = rows[Math.min(rows.length - 1, Math.max(0, at < 0 ? 0 : at + delta))]!;
  void router.replace({ path: `/lab/${next.id}`, query: route.query });
}

// ---- New experiment: the command palette sends ?new=1 ----
const creating = ref(false);
watch(
  () => route.query.new,
  (flag) => {
    if (!flag) return;
    creating.value = true;
    const rest = { ...route.query };
    delete rest.new;
    void router.replace({ path: route.path, query: rest });
  },
  { immediate: true },
);
useShortcuts(
  { arrowdown: (e) => step(1, e), arrowup: (e) => step(-1, e) },
  { enabled: () => !creating.value, preventDefault: false },
);

// ---- The stage ----
const picked = ref<string | null>(null);
watch(id, () => (picked.value = null));
const stage = computed(() => picked.value ?? (experiment.value ? defaultStage(experiment.value) : ""));
const test = computed(() => tests.value.find((t) => t.stage === stage.value));

const { state: games } = useGames(() => test.value?.id ?? 0);
const record = computed(() => {
  const r = { W: 0, D: 0, L: 0 };
  for (const g of games.value.data ?? []) r[outcome(g)]++;
  return r;
});

const stopping = ref(false);
async function cancel() {
  stopping.value = true;
  try {
    await api.DELETE("/api/experiments/{id}", { params: { path: { id: id.value } } });
  } finally {
    stopping.value = false;
  }
}
</script>

<template>
  <Workspace library-label="Experiments" inspector-label="Games">
    <template #library-head>
      <div class="lib-head">
        <div class="lib-top">
          <h1 class="lib-title">Experiments</h1>
          <EButton size="sm" :icon="PhPlus" @click="creating = true">Test an idea</EButton>
        </div>
        <ESearchBar v-model="search" label="Search experiments" placeholder="Search" size="sm" hide-action />
        <ESegmented
          v-model="filter"
          label="Show"
          hide-label
          size="sm"
          :options="[
            { value: 'all', label: 'All' },
            { value: 'running', label: 'Running' },
            { value: 'passed', label: 'Passed' },
            { value: 'failed', label: 'Failed' },
          ]"
        />
        <ESelect
          v-if="batches.length"
          v-model="batch"
          label="Batch"
          hide-label
          size="sm"
          :options="batches"
          placeholder="All batches"
          clearable
        />
      </div>
    </template>

    <template #library>
      <ESkeleton v-if="list.status === 'pending'" :lines="5" label="Loading experiments" />
      <p v-else-if="!all.length" class="note">No experiments yet.</p>
      <p v-else-if="!shown.length" class="note">Nothing matches.</p>
      <ol v-else ref="listEl" class="rows">
        <li v-for="e in shown" :key="e.id">
          <RouterLink class="row" :to="`/lab/${e.id}`">
            <span class="row-mark" :style="{ background: `var(--e-${mark(e)})` }" aria-hidden="true"></span>
            <span class="row-top">
              <span class="row-name">{{ e.name }}</span>
              <span class="row-time num">{{ shortAgo(e.created_at) }}</span>
            </span>
            <span class="row-refs num">{{ e.dev }} vs {{ e.base }}</span>
            <span :class="['row-result', `row-result--${verdict(e).tone}`]">{{ headline(e) }}</span>
          </RouterLink>
        </li>
      </ol>
      <EDisclosure v-if="all.length" class="how" title="How it works">
        <HowItWorks />
      </EDisclosure>
    </template>

    <template v-if="experiment" #stage-head>
      <h2 class="title">{{ experiment.name }}</h2>
      <EStamp :tone="verdict(experiment).tone" :label="verdict(experiment).label" />
      <span class="grow"></span>
      <EButton v-if="active" variant="outline" size="sm" :icon="PhStop" :loading="stopping" @click="cancel">
        Stop
      </EButton>
    </template>

    <ExperimentStage
      v-if="experiment"
      :experiment="experiment"
      :tests="tests"
      :stage="stage"
      @update:stage="picked = $event"
    />
    <ENotice v-else-if="state.status === 'error'" class="pad" tone="danger" title="Could not load the experiment">
      {{ state.error?.message }}
    </ENotice>
    <ESkeleton v-else-if="id || list.status === 'pending'" class="pad" :lines="6" label="Loading the experiment" />
    <div v-else class="empty">
      <EEmptyState title="Nothing tested yet" icon="info">
        Commit a change to a branch of the Reckless code, then press Test an idea. Each experiment goes through these
        steps.
      </EEmptyState>
      <HowItWorks />
    </div>

    <template v-if="test" #inspector-head>
      <h2 class="ins-title">Games</h2>
      <span class="ins-count num">{{ (games.data?.length ?? test.games).toLocaleString() }}</span>
      <span class="grow"></span>
      <span class="record num" aria-label="The new version's wins, draws and losses">
        <span class="record-w">{{ record.W.toLocaleString() }}</span
        >–<span>{{ record.D.toLocaleString() }}</span
        >–<span class="record-l">{{ record.L.toLocaleString() }}</span>
      </span>
    </template>
    <template v-if="test" #inspector>
      <GameList :test="test.id" />
    </template>
  </Workspace>

  <NewExperiment v-model:open="creating" />
</template>

<style scoped>
.lib-head {
  display: grid;
  flex: 1;
  grid-template-columns: minmax(0, 1fr);
  min-width: 0;
  gap: var(--e-gap-snug);
  padding-block: var(--e-gap-tight);
}

.lib-top {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--e-gap-snug);
}

.lib-title,
.title,
.ins-title {
  margin: 0;
  font: var(--e-type-h5);
}

.title {
  overflow: hidden;
  font: var(--e-type-h4);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.grow {
  flex: 1;
}

.note {
  margin: 0;
  padding: var(--e-pad-control-sm);
  color: var(--e-muted);
  font: var(--e-type-small);
}

.rows {
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  position: relative;
  display: grid;
  gap: 0.125rem;
  padding: 0.5rem var(--e-pad-control-sm) 0.5rem calc(var(--e-pad-control-sm) + 0.25rem);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 35%, transparent);
  color: inherit;
  text-decoration: none;
}

.row:hover {
  background: var(--e-surface-tint);
}

.row[aria-current] {
  background: var(--e-primary-soft);
}

.row:focus-visible {
  outline: 2px solid var(--e-primary);
  outline-offset: -2px;
}

.row-mark {
  position: absolute;
  inset-block: 0;
  inset-inline-start: 0;
  width: 3px;
}

.row-top {
  display: flex;
  justify-content: space-between;
  gap: var(--e-gap-snug);
}

.row-name {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-time {
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.row-refs,
.row-result {
  overflow: hidden;
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-result {
  font-family: var(--e-font-mono);
}

.row-result--success {
  color: var(--e-success);
}

.row-result--danger {
  color: var(--e-danger);
}

.row-result--info {
  color: var(--e-primary);
}

.how {
  margin: var(--e-gap-snug) var(--e-pad-control-sm);
}

.pad {
  margin: var(--e-gap-loose) var(--e-pad-control-sm);
  align-self: start;
}

.empty {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
  max-width: 34rem;
  padding: var(--e-gap-loose) var(--e-pad-control-sm);
}

.ins-count {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.record {
  display: inline-flex;
  gap: 0.25rem;
  color: var(--e-muted);
  font: 600 var(--e-text-sm, 0.8125rem) / 1 var(--e-font-mono);
}

.record-w {
  color: var(--e-success);
}

.record-l {
  color: var(--e-danger);
}
</style>
