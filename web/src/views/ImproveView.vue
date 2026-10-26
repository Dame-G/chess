<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { PhArrowClockwise, PhCheck, PhList, PhPlus, PhStop } from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { ESearchBar } from "@eos-ui/vue/components/ESearchBar";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { api } from "../api/runner";
import { unwrap, useExperiment, useExperiments, useGames, useJobs, useTest, useTune, useTunes } from "../api/queries";
import { stageLabel } from "../format";
import Workspace from "../components/Workspace.vue";
import ExperimentStage from "../components/lab/ExperimentStage.vue";
import GameList from "../components/lab/GameList.vue";
import { defaultStage } from "../components/lab/lab";
import HowItWorks from "../components/improve/HowItWorks.vue";
import ImproveRows from "../components/improve/ImproveRows.vue";
import LibraryHead from "../components/improve/LibraryHead.vue";
import { stepTarget, toLibrary } from "../components/improve/libraryNav";
import NewWork from "../components/improve/NewWork.vue";
import {
  applyJobOf,
  itemsOf,
  keyOf,
  pathOf,
  testOfTune,
  testStatus,
  tuneOfTest,
  tuneStatus,
  type Item,
} from "../components/improve/improve";
import ApplyTune from "../components/tune/ApplyTune.vue";
import TuneInspector from "../components/tune/TuneInspector.vue";
import TuneStage from "../components/tune/TuneStage.vue";
import { eta, live as tuneLive, resumable } from "../components/tune/tune";

const route = useRoute();
const router = useRouter();
const { state: testList } = useExperiments();
const { state: tuneList } = useTunes();
const { state: jobs } = useJobs();

const kind = computed<Item["kind"] | "">(() =>
  route.name === "improve-test" ? "test" : route.name === "improve-tune" ? "tune" : "",
);
const id = computed(() => Number(route.params.id) || 0);
const current = computed(() => (kind.value && id.value ? `${kind.value}:${id.value}` : ""));
const loading = computed(() => testList.value.status === "pending" || tuneList.value.status === "pending");

// ---- Library: tests and tunes together
const search = ref("");
const show = ref<"all" | "test" | "tune">("all");
const batch = ref("");
const everything = computed(() => itemsOf(testList.value.data ?? [], tuneList.value.data ?? []));
const batches = computed(() => [
  ...new Set((testList.value.data ?? []).map((e) => e.batch).filter((b): b is string => !!b)),
]);

const shown = computed(() => {
  const q = search.value.trim().toLowerCase();
  return everything.value.filter((i) => {
    if (show.value !== "all" && i.kind !== show.value) return false;
    if (batch.value && (i.kind !== "test" || i.test.batch !== batch.value)) return false;
    if (!q) return true;
    const words =
      i.kind === "test"
        ? [i.test.name, i.test.dev, i.test.base, i.test.hypothesis, i.test.batch ?? ""]
        : [i.tune.name, i.tune.ref, "tune"];
    return words.some((t) => t.toLowerCase().includes(q));
  });
});
const rows = computed(() =>
  shown.value.map((item) => {
    if (item.kind === "test") return { item, status: testStatus(item.test) };
    const test = testOfTune(item.tune, jobs.value.data ?? [], testList.value.data ?? []);
    const applied = applyJobOf(item.tune, jobs.value.data ?? [])?.status === "done";
    return { item, status: tuneStatus(item.tune, test, applied) };
  }),
);

/** The query to carry between items: the chosen stage belongs to one test only. */
function keep() {
  const rest = { ...route.query };
  delete rest.stage;
  return rest;
}

// Selection lives in the URL; with none, the first item (a live one, else the newest) opens.
watch(
  [everything, current],
  ([all, now]) => {
    if (now || !all.length) return;
    const pick = all[0]!;
    void router.replace({ path: pathOf(pick.kind, pick.id), query: keep() });
  },
  { immediate: true },
);

const listEl = ref<HTMLElement>();
watch(current, async () => {
  await nextTick();
  listEl.value?.querySelector("[aria-current]")?.scrollIntoView({ block: "nearest" });
});

function clear() {
  search.value = "";
  show.value = "all";
  batch.value = "";
}

// ---- A test on the stage
const testId = computed(() => (kind.value === "test" ? id.value : 0));
const { state: testState } = useExperiment(testId);
const experiment = computed(() =>
  testState.value.data?.experiment.id === testId.value ? testState.value.data.experiment : undefined,
);
const rawTests = computed(() => testState.value.data?.tests ?? []);
// A running match's progress arrives as its own events, so its live figures replace the experiment's older copy.
const liveId = computed(() => rawTests.value.find((t) => t.running)?.id ?? 0);
const { state: liveState } = useTest(liveId);
const tests = computed(() => {
  const fresh = liveState.value.data?.test;
  return fresh && fresh.id === liveId.value
    ? rawTests.value.map((t) => (t.id === fresh.id ? fresh : t))
    : rawTests.value;
});
const stage = computed(() => {
  const e = experiment.value;
  if (!e) return "";
  const asked = String(route.query.stage ?? "");
  return e.funnel.some((s) => (s.type === "match" ? s.name : s.type) === asked) ? asked : defaultStage(e, tests.value);
});
const test = computed(() => tests.value.find((t) => t.stage === stage.value));
const { state: games } = useGames(() => test.value?.id ?? 0);
const madeBy = computed(() =>
  experiment.value ? tuneOfTest(experiment.value, jobs.value.data ?? [], tuneList.value.data ?? []) : undefined,
);
const testActive = computed(() => !!experiment.value && ["queued", "running"].includes(experiment.value.status));

// ---- A tune on the stage
const tuneId = computed(() => (kind.value === "tune" ? id.value : 0));
const { state: tuneState } = useTune(tuneId);
const detail = computed(() => (tuneState.value.data?.tune.id === tuneId.value ? tuneState.value.data : undefined));
const tune = computed(() => detail.value?.tune);
const history = computed(() => detail.value?.history ?? []);
const applyJob = computed(() => (tune.value ? applyJobOf(tune.value, jobs.value.data ?? []) : undefined));
const tuned = computed(() =>
  tune.value ? testOfTune(tune.value, jobs.value.data ?? [], testList.value.data ?? []) : undefined,
);

// A time estimate, from how fast this page has watched iterations arrive.
const samples = ref<{ iteration: number; at: number }[]>([]);
watch(
  () => [tune.value?.id, tune.value?.status, tune.value?.iteration] as const,
  ([tid, status, iteration], old) => {
    if (tid !== old?.[0] || status !== "running" || iteration == null) samples.value = [];
    if (status === "running" && iteration != null)
      samples.value = [...samples.value, { iteration, at: Date.now() }].slice(-30);
  },
  { immediate: true },
);
const remaining = computed(() =>
  tune.value?.status === "running" ? eta(samples.value, tune.value.iterations) : undefined,
);

/** Whether something is chosen or on its way, so the panes around the stage keep their size while it loads. */
const staged = computed(() => !!current.value || loading.value || !!experiment.value || !!tune.value);

// ---- Actions
const stoppingTune = ref(false);
const applying = ref(false);
const busy = ref(false);
const error = ref("");
async function act(call: () => Promise<unknown>) {
  busy.value = true;
  error.value = "";
  try {
    await call();
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
    stoppingTune.value = false;
  }
}
const stopTest = () => act(() => api.DELETE("/api/experiments/{id}", { params: { path: { id: testId.value } } }));
const stopTune = () => act(() => unwrap(api.DELETE("/api/tunes/{id}", { params: { path: { id: tuneId.value } } })));
const resume = () => act(() => unwrap(api.POST("/api/tunes/{id}/resume", { params: { path: { id: tuneId.value } } })));

// ---- New: the command palette sends ?new=1 (a test), ?new=test or ?new=tune
const creating = ref(false);
const mode = ref<"test" | "tune">("test");
/** A branch to start from, sent by the Versions page: ?new=test&dev=<branch> or ?new=tune&branch=<branch>. */
const prefill = ref("");
watch(
  () => route.query.new,
  (flag) => {
    if (!flag) return;
    mode.value = flag === "tune" ? "tune" : "test";
    const named = flag === "tune" ? route.query.branch : route.query.dev;
    prefill.value = typeof named === "string" ? named : "";
    creating.value = true;
    const rest = { ...route.query };
    delete rest.new;
    delete rest.dev;
    delete rest.branch;
    void router.replace({ path: route.path, query: rest });
  },
  { immediate: true },
);

function step(delta: number, event: KeyboardEvent) {
  const at = shown.value.findIndex((i) => keyOf(i) === current.value);
  const next = stepTarget(shown.value, at, delta, event);
  if (next) void router.replace({ path: pathOf(next.kind, next.id), query: keep() });
}
useShortcuts(
  { arrowdown: (e) => step(1, e), arrowup: (e) => step(-1, e) },
  { enabled: () => !creating.value && !applying.value && !stoppingTune.value, preventDefault: false },
);

const title = computed(() => experiment.value?.name ?? tune.value?.name ?? "");
</script>

<template>
  <Workspace library-label="Library" :inspector-label="kind === 'tune' ? 'Iterations' : 'Games'">
    <template #library-head>
      <LibraryHead title="Improve">
        <template #action>
          <EButton size="sm" :icon="PhPlus" @click="creating = true">Test a change</EButton>
        </template>
        <ESearchBar v-model="search" label="Search" placeholder="Search" size="sm" hide-action />
        <ESegmented
          v-model="show"
          label="Show"
          hide-label
          size="sm"
          :options="[
            { value: 'all', label: 'All' },
            { value: 'test', label: 'Tests' },
            { value: 'tune', label: 'Tunes' },
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
      </LibraryHead>
    </template>

    <template #library>
      <ESkeleton v-if="loading && !everything.length" :lines="5" label="Loading" />
      <EEmptyState v-else-if="!everything.length" bare size="sm" title="Nothing tried yet">
        Test a change to start. Tuning the engine's numbers is in the same dialog.
      </EEmptyState>
      <EEmptyState v-else-if="!rows.length" bare size="sm" title="Nothing matches">
        Try another word, or a different filter.
        <template #action>
          <EButton variant="quiet" size="sm" @click="clear">Clear filters</EButton>
        </template>
      </EEmptyState>
      <div v-else ref="listEl">
        <ImproveRows :rows="rows" :current="current" :query="search" />
      </div>
      <EDisclosure v-if="everything.length" class="how" title="How it works">
        <HowItWorks detail />
      </EDisclosure>
    </template>

    <template v-if="staged" #stage-head>
      <h2 class="title">{{ title }}</h2>
      <span class="grow"></span>
      <EButton class="to-library" variant="quiet" size="sm" :icon="PhList" @click="toLibrary('Library')">
        Library
      </EButton>
      <template v-if="experiment">
        <EButton v-if="testActive" variant="outline" size="sm" :icon="PhStop" :loading="busy" @click="stopTest">
          Stop
        </EButton>
      </template>
      <template v-else-if="tune">
        <EButton v-if="tuneLive(tune)" variant="outline" size="sm" :icon="PhStop" @click="stoppingTune = true">
          Stop
        </EButton>
        <EButton
          v-if="resumable(tune)"
          variant="outline"
          size="sm"
          :icon="PhArrowClockwise"
          :loading="busy"
          @click="resume"
        >
          Resume
        </EButton>
        <EButton v-if="tune.iteration > 0" variant="outline" size="sm" :icon="PhCheck" @click="applying = true">
          Apply
        </EButton>
      </template>
    </template>

    <ExperimentStage v-if="experiment" :experiment="experiment" :tests="tests" :stage="stage" :tune="madeBy" />
    <template v-else-if="tune">
      <ENotice v-if="error" class="pad" tone="danger" title="That did not work" closable @close="error = ''">
        {{ error }}
      </ENotice>
      <TuneStage :tune="tune" :history="history" :remaining="remaining" :applying="applyJob" :test="tuned" />
    </template>
    <ENotice
      v-else-if="testState.status === 'error' || tuneState.status === 'error'"
      class="pad"
      tone="danger"
      title="Could not load it"
    >
      {{ testState.error?.message ?? tuneState.error?.message }}
    </ENotice>
    <ESkeleton v-else-if="current || loading" class="pad" :lines="6" label="Loading" />
    <div v-else class="empty">
      <EEmptyState title="Nothing tried yet" icon="info">
        Improving the engine is a loop: change it, test it, keep what wins.
        <template #action>
          <EButton :icon="PhPlus" @click="creating = true">Test a change</EButton>
        </template>
      </EEmptyState>
      <HowItWorks />
    </div>

    <!-- Kept while something is chosen, so the stage keeps its width when a step has no games to list. -->
    <template v-if="staged" #inspector-head>
      <h2 class="ins-title">{{ kind === "tune" ? "Iterations" : "Games" }}</h2>
      <span v-if="kind === 'tune' && tune" class="ins-count num">{{ tune.iteration.toLocaleString("en") }}</span>
      <span v-else-if="test" class="ins-count num">{{ (games.data?.length ?? test.games).toLocaleString() }}</span>
      <span class="grow"></span>
    </template>
    <template v-if="staged" #inspector>
      <template v-if="kind === 'tune'">
        <TuneInspector v-if="tune" :tune="tune" :history="history" />
        <ESkeleton v-else class="pad" :lines="6" label="Loading iterations" />
      </template>
      <template v-else>
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
    </template>
  </Workspace>

  <NewWork v-model:open="creating" v-model:mode="mode" :branch="prefill" />
  <ApplyTune v-if="tune" v-model:open="applying" :tune="tune" />
  <EDialog v-model:open="stoppingTune" title="Stop this tune?" size="sm">
    Its iterations so far are kept, and it can resume later.
    <template #actions>
      <EButton :loading="busy" @click="stopTune">Stop tune</EButton>
      <EButton variant="quiet" @click="stoppingTune = false">Keep going</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
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

.to-library {
  display: none;
}

@media (width < 75rem) {
  .to-library {
    display: inline-flex;
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
