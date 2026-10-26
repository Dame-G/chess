<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhList, PhPlus, PhStop, PhArrowClockwise, PhCheck } from "@phosphor-icons/vue";
import { useShortcuts } from "@eos-ui/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EDialog } from "@eos-ui/vue/components/EDialog";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ETable } from "@eos-ui/vue/components/ETable";
import { api } from "../api/runner";
import { unwrap, useJobs, useTune, useTunes } from "../api/queries";
import { shortAgo } from "../components/lab/lab";
import ApplyTune from "../components/tune/ApplyTune.vue";
import NewTune from "../components/tune/NewTune.vue";
import TuneExplainer from "../components/tune/TuneExplainer.vue";
import TuneStage from "../components/tune/TuneStage.vue";
import { controlText, eta, gamesIn, live, markTone, pairs, resumable, statusWord } from "../components/tune/tune";
import { signed } from "../format";
import Workspace from "../components/Workspace.vue";

const route = useRoute();
const router = useRouter();
const { state: list } = useTunes();
const { state: jobs } = useJobs();

const id = computed(() => Number(route.params.id) || 0);
const { state } = useTune(id);
const detail = computed(() => (state.value.data?.tune.id === id.value ? state.value.data : undefined));
const tune = computed(() => detail.value?.tune);
const history = computed(() => detail.value?.history ?? []);
const staged = computed(() => id.value > 0 || list.value.status === "pending" || !!tune.value);

// ---- Library: the tune running now first, then the newest
const all = computed(() => {
  const rows = list.value.data ?? [];
  return [...rows.filter((t) => live(t)), ...rows.filter((t) => !live(t))];
});

watch(
  [all, id],
  ([rows, current]) => {
    if (current || !rows.length) return;
    void router.replace({ path: `/tune/${rows[0]!.id}`, query: route.query });
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
  const rows = all.value;
  if (!rows.length) return;
  event.preventDefault();
  const at = rows.findIndex((t) => t.id === id.value);
  const next = rows[Math.min(rows.length - 1, Math.max(0, at < 0 ? 0 : at + delta))]!;
  void router.replace({ path: `/tune/${next.id}`, query: route.query });
}

function toLibrary() {
  const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
  document
    .querySelector("aside[aria-label='Tunes']")
    ?.scrollIntoView({ behavior: calm ? "auto" : "smooth", block: "start" });
}

// ---- New tune: the command palette sends ?new=1
const creating = ref(false);
const applying = ref(false);
const stopping = ref(false);
const busy = ref(false);
const error = ref("");
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
const dialogOpen = computed(() => creating.value || applying.value || stopping.value);
useShortcuts(
  { arrowdown: (e) => step(1, e), arrowup: (e) => step(-1, e) },
  { enabled: () => !dialogOpen.value, preventDefault: false },
);

// ---- A time estimate, from how fast this page has watched iterations arrive
const samples = ref<{ iteration: number; at: number }[]>([]);
watch(
  () => [tune.value?.id, tune.value?.status, tune.value?.iteration] as const,
  ([tid, status, iteration], old) => {
    const before = old?.[0];
    if (tid !== before || status !== "running" || iteration == null) samples.value = [];
    if (status === "running" && iteration != null)
      samples.value = [...samples.value, { iteration, at: Date.now() }].slice(-30);
  },
  { immediate: true },
);
const remaining = computed(() =>
  tune.value?.status === "running" ? eta(samples.value, tune.value.iterations) : undefined,
);

// ---- Actions
async function act(call: () => Promise<unknown>) {
  busy.value = true;
  error.value = "";
  try {
    await call();
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    busy.value = false;
    stopping.value = false;
  }
}
const stop = () => act(() => unwrap(api.DELETE("/api/tunes/{id}", { params: { path: { id: id.value } } })));
const resume = () => act(() => unwrap(api.POST("/api/tunes/{id}/resume", { params: { path: { id: id.value } } })));

/** The newest job that applied this tune. */
const applyJob = computed(() =>
  (jobs.value.data ?? []).find((j) => j.spec.kind === "apply_tune" && j.spec.id === id.value),
);

// ---- Inspector
const settings = computed(() => {
  const t = tune.value;
  if (!t) return [];
  const per = t.pairs_per_iteration;
  return [
    { label: "Engine", text: t.ref },
    {
      label: "Games",
      text: `${per} pairs (${per * 2} games) each iteration, ${gamesIn(t.iterations, per).toLocaleString("en")} in all`,
    },
    { label: "Length", text: `${controlText(t)}, ${t.concurrency} games at once` },
    { label: "Decay", text: `alpha ${t.alpha}, gamma ${t.gamma}, A ${t.a_ratio} of the iterations` },
  ];
});
const recent = computed(() => history.value.slice(-40).reverse());
const lead = (r: number) => signed(r, 0);
</script>

<template>
  <Workspace library-label="Tunes" inspector-label="Iterations">
    <template #library-head>
      <div class="lib-head">
        <h1 class="lib-title">Tunes</h1>
        <EButton size="sm" :icon="PhPlus" @click="creating = true">New tune</EButton>
      </div>
    </template>

    <template #library>
      <ESkeleton v-if="list.status === 'pending'" :lines="4" label="Loading tunes" />
      <EEmptyState v-else-if="!all.length" bare size="sm" title="No tunes yet">
        Press New tune to let the engine tune its own constants.
      </EEmptyState>
      <ol v-else ref="listEl" class="rows">
        <li v-for="t in all" :key="t.id">
          <RouterLink class="row" :to="`/tune/${t.id}`" :title="`${t.name}: ${t.ref}`">
            <span class="row-mark" :style="{ background: `var(--e-${markTone(t)})` }" aria-hidden="true"></span>
            <span class="row-top">
              <span class="row-name">{{ t.name }}</span>
              <span class="row-time num">{{ shortAgo(t.created_at) }}</span>
            </span>
            <EProgress
              v-if="live(t)"
              class="row-bar"
              label="Iteration"
              :value="t.iteration"
              :max="t.iterations"
              :value-text="`${t.iteration}/${t.iterations}`"
            />
            <span v-if="t.status !== 'running'" :class="['row-result', `row-result--${t.status}`]">
              <strong class="row-status">{{ statusWord(t) }}</strong>
              <span class="num">{{ t.params.length ? `${t.params.length} parameters` : t.ref }}</span>
            </span>
          </RouterLink>
        </li>
      </ol>
      <EDisclosure v-if="all.length" class="how" title="What is a tune?">
        <TuneExplainer />
      </EDisclosure>
    </template>

    <template v-if="staged" #stage-head>
      <h2 class="title">{{ tune?.name ?? "" }}</h2>
      <span class="grow"></span>
      <EButton class="to-library" variant="quiet" size="sm" :icon="PhList" @click="toLibrary">Tunes</EButton>
      <template v-if="tune">
        <EButton v-if="live(tune)" variant="outline" size="sm" :icon="PhStop" @click="stopping = true">Stop</EButton>
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

    <template v-if="tune">
      <ENotice v-if="error" class="pad" tone="danger" title="That did not work" closable @close="error = ''">
        {{ error }}
      </ENotice>
      <TuneStage :tune="tune" :history="history" :remaining="remaining" :applying="applyJob" />
    </template>
    <ENotice v-else-if="state.status === 'error'" class="pad" tone="danger" title="Could not load the tune">
      {{ state.error?.message }}
    </ENotice>
    <ESkeleton v-else-if="id || list.status === 'pending'" class="pad" :lines="6" label="Loading the tune" />
    <div v-else class="empty">
      <EEmptyState title="Nothing tuned yet" icon="info">
        A tune lets the engine find better values for its own search constants by playing against itself.
      </EEmptyState>
      <TuneExplainer />
    </div>

    <template v-if="staged" #inspector-head>
      <h2 class="ins-title">Iterations</h2>
      <span v-if="tune" class="ins-count num">{{ tune.iteration.toLocaleString("en") }}</span>
      <span class="grow"></span>
    </template>
    <template v-if="staged" #inspector>
      <ESkeleton v-if="!tune" class="pad" :lines="6" label="Loading iterations" />
      <template v-else>
        <EDescriptionList class="settings" :items="settings" />
        <EEmptyState v-if="!recent.length" bare size="sm" title="No iterations yet">
          Each one plays a few game pairs and takes a step.
        </EEmptyState>
        <template v-else>
          <ETable caption="Recent iterations" hide-caption dense>
            <thead>
              <tr>
                <th scope="col">Iteration</th>
                <th scope="col" class="e-num" title="Games the higher copy won, minus games it lost">Lead</th>
                <th scope="col" class="e-num" title="Game pairs the higher copy won, drew and lost">Pairs W / D / L</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="h in recent" :key="h.iteration">
                <th scope="row" class="num it">{{ h.iteration }}</th>
                <td class="e-num num">{{ lead(h.result) }}</td>
                <td class="e-num num">
                  {{ pairs(h.penta).win }} / {{ pairs(h.penta).draw }} / {{ pairs(h.penta).loss }}
                </td>
              </tr>
            </tbody>
          </ETable>
          <p class="ins-note">
            Lead is how far the higher copy of the constants was ahead; the tune steps that way. The last
            {{ recent.length }} are shown.
          </p>
        </template>
      </template>
    </template>
  </Workspace>

  <NewTune v-model:open="creating" />
  <ApplyTune v-if="tune" v-model:open="applying" :tune="tune" />
  <EDialog v-model:open="stopping" title="Stop this tune?" size="sm">
    Its iterations so far are kept, and it can resume later.
    <template #actions>
      <EButton :loading="busy" @click="stop">Stop tune</EButton>
      <EButton variant="quiet" @click="stopping = false">Keep going</EButton>
    </template>
  </EDialog>
</template>

<style scoped>
.lib-head {
  display: flex;
  flex: 1;
  justify-content: space-between;
  align-items: center;
  gap: var(--e-gap-snug);
  min-width: 0;
  padding-block: var(--e-gap-snug);
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

.row-time,
.row-result {
  color: var(--e-muted);
  font-size: var(--e-text-xs, 0.75rem);
}

.row-result {
  display: flex;
  gap: var(--e-gap-snug);
  align-items: baseline;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-status {
  flex: none;
  font-weight: 600;
}

.row-result--running {
  color: var(--e-primary);
}

.row-result--done {
  color: var(--e-success);
}

.row-result--failed {
  color: var(--e-danger);
}

.to-library {
  display: none;
}

@media (width < 64rem) {
  .to-library {
    display: inline-flex;
  }
}

.how {
  margin: var(--e-gap-base) var(--e-pad-control-sm);
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
  padding: var(--e-gap-section) var(--e-pad-card);
}

.ins-count {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.settings {
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control-sm));
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
}

.it {
  font-weight: 400;
  text-align: start;
}

.ins-note {
  margin: 0;
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control-sm));
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
