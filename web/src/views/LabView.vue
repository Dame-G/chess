<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhList, PhPlus, PhStop } from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESearchBar } from "@eos-ui/vue/components/ESearchBar";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EHighlight } from "@eos-ui/vue/components/EHighlight";
import { api } from "../api/runner";
import { useExperiment, useExperiments, useGames, useTest, type Experiment } from "../api/queries";
import { stageLabel, verdict } from "../format";
import HowItWorks from "../components/HowItWorks.vue";
import NewExperiment from "../components/NewExperiment.vue";
import Workspace from "../components/Workspace.vue";
import ExperimentStage from "../components/lab/ExperimentStage.vue";
import GameList from "../components/lab/GameList.vue";
import { defaultStage, headline, highlight, shortAgo } from "../components/lab/lab";

const route = useRoute();
const router = useRouter();
const { state: list } = useExperiments();

const id = computed(() => Number(route.params.id) || 0);
const { state } = useExperiment(id);
const experiment = computed(() =>
  state.value.data?.experiment.id === id.value ? state.value.data.experiment : undefined,
);
const rawTests = computed(() => state.value.data?.tests ?? []);
// A running test's progress arrives as its own events, so its live figures replace the experiment's older copy.
const liveId = computed(() => rawTests.value.find((t) => t.running)?.id ?? 0);
const { state: live } = useTest(liveId);
const tests = computed(() => {
  const fresh = live.value.data?.test;
  return fresh && fresh.id === liveId.value
    ? rawTests.value.map((t) => (t.id === fresh.id ? fresh : t))
    : rawTests.value;
});
/** Whether an experiment is chosen or on its way, so the panes around the stage keep their size while it loads. */
const staged = computed(() => id.value > 0 || list.value.status === "pending" || !!experiment.value);
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

/** The query to carry between experiments: the chosen stage belongs to one experiment only. */
function keep() {
  const rest = { ...route.query };
  delete rest.stage;
  return rest;
}

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
    void router.replace({ path: `/lab/${pick.id}`, query: keep() });
  },
  { immediate: true },
);

/** Below 64rem the library sits under the stage: this brings it into view. */
function toLibrary() {
  const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
  document
    .querySelector("aside[aria-label='Experiments']")
    ?.scrollIntoView({ behavior: calm ? "auto" : "smooth", block: "start" });
}

function clear() {
  search.value = "";
  filter.value = "all";
  batch.value = "";
}

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
  void router.replace({ path: `/lab/${next.id}`, query: keep() });
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
// The chosen stage lives in the URL (?stage=), and falls back to the one running or last run.
const stage = computed(() => {
  const e = experiment.value;
  if (!e) return "";
  const asked = String(route.query.stage ?? "");
  return e.funnel.some((s) => (s.type === "match" ? s.name : s.type) === asked) ? asked : defaultStage(e, tests.value);
});
const test = computed(() => tests.value.find((t) => t.stage === stage.value));

const { state: games } = useGames(() => test.value?.id ?? 0);

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
      <EEmptyState v-else-if="!all.length" bare size="sm" title="No experiments yet">
        Press Test an idea to try a change.
      </EEmptyState>
      <EEmptyState v-else-if="!shown.length" bare size="sm" title="Nothing matches">
        Try another word, or a different filter.
        <template #action>
          <EButton variant="quiet" size="sm" @click="clear">Clear filters</EButton>
        </template>
      </EEmptyState>
      <ol v-else ref="listEl" class="rows">
        <li v-for="e in shown" :key="e.id">
          <RouterLink class="row" :to="`/lab/${e.id}`" :title="`${e.name}: ${e.dev} vs ${e.base}`">
            <span class="row-mark" :style="{ background: `var(--e-${mark(e)})` }" aria-hidden="true"></span>
            <span class="row-top">
              <span class="row-name">
                <template v-for="(p, i) in highlight(e.name, search)" :key="i">
                  <EHighlight v-if="p.hit">{{ p.text }}</EHighlight>
                  <template v-else>{{ p.text }}</template>
                </template>
              </span>
              <span class="row-time num">{{ shortAgo(e.created_at) }}</span>
            </span>
            <span class="row-refs num">{{ e.dev }} vs {{ e.base }}</span>
            <span :class="['row-result', `row-result--${verdict(e).tone}`]">
              <strong class="row-verdict">{{ headline(e).label }}</strong>
              <span v-if="headline(e).figure" class="num">{{ headline(e).figure }}</span>
            </span>
          </RouterLink>
        </li>
      </ol>
      <EDisclosure v-if="all.length" class="how" title="How it works">
        <HowItWorks />
      </EDisclosure>
    </template>

    <template v-if="staged" #stage-head>
      <h2 class="title">{{ experiment?.name ?? "" }}</h2>
      <span class="grow"></span>
      <EButton class="to-library" variant="quiet" size="sm" :icon="PhList" @click="toLibrary">Experiments</EButton>
      <EButton v-if="active" variant="outline" size="sm" :icon="PhStop" :loading="stopping" @click="cancel">
        Stop
      </EButton>
    </template>

    <ExperimentStage v-if="experiment" :experiment="experiment" :tests="tests" :stage="stage" />
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

    <!-- Kept while an experiment is chosen, so the stage keeps its width when a stage has no games to list. -->
    <template v-if="staged" #inspector-head>
      <h2 class="ins-title">Games</h2>
      <span v-if="test" class="ins-count num">{{ (games.data?.length ?? test.games).toLocaleString() }}</span>
      <span class="grow"></span>
    </template>
    <template v-if="staged" #inspector>
      <GameList v-if="test" :test="test.id" />
      <ESkeleton v-else-if="!experiment" class="pad" :lines="6" label="Loading games" />
      <EEmptyState v-else bare size="sm" title="No games here">
        {{
          ["bench", "speed"].includes(stage)
            ? `The ${stageLabel(stage).toLowerCase()} check plays no games.`
            : "This step has not played any games yet."
        }}
      </EEmptyState>
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
  gap: var(--e-gap-base);
  padding-block: var(--e-gap-snug);
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
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  font: var(--e-type-h4);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.grow {
  flex: 1;
}

.rows {
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  position: relative;
  display: grid;
  gap: var(--e-gap-tight);
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control-sm));
  padding-inline-start: calc(var(--ws-pad, var(--e-pad-control-sm)) + 0.25rem);
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

.row[aria-current] .row-mark {
  width: 4px;
}

.row[aria-current] .row-name {
  color: var(--e-ink);
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
  display: flex;
  gap: var(--e-gap-snug);
  align-items: baseline;
}

.row-verdict {
  flex: none;
  font-weight: 600;
}

.row-result .num {
  overflow: hidden;
  text-overflow: ellipsis;
}

.to-library {
  display: none;
}

@media (width < 64rem) {
  .to-library {
    display: inline-flex;
  }
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

.row-result--info::before {
  content: "";
  display: inline-block;
  width: 0.375rem;
  height: 0.375rem;
  align-self: center;
  border-radius: 50%;
  background: currentcolor;
  animation: pulse 1.4s ease-in-out infinite;
}

@keyframes pulse {
  50% {
    opacity: 0.25;
  }
}

@media (prefers-reduced-motion: reduce) {
  .row-result--info::before {
    animation: none;
  }
}

.how {
  margin: var(--e-gap-base) var(--ws-pad);
}

.pad {
  margin: var(--e-gap-loose) var(--ws-pad);
  align-self: start;
}

.empty {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
  max-width: 34rem;
  padding: var(--e-gap-section) var(--e-pad-card);
}

.ins-count {
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
