<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { PhCheckCircle, PhChartLine, PhHammer, PhMinusCircle, PhWarning } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECodeBlock } from "@eos-ui/vue/components/ECodeBlock";
import { ECombobox } from "@eos-ui/vue/components/ECombobox";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EForm } from "@eos-ui/vue/components/EForm";
import { EIcon } from "@eos-ui/vue/components/EIcon";
import { ELink } from "@eos-ui/vue/components/ELink";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import Workspace from "../components/Workspace.vue";
import SpeedBars from "../components/engines/SpeedBars.vue";
import Term from "../components/Term.vue";
import { useRunnerEvent } from "../api/events";
import { api } from "../api/runner";
import { unwrap, useBranches, useBuilds, useJobs, type Build } from "../api/queries";
import { ago } from "../format";

const route = useRoute();
const router = useRouter();
const { state: builds } = useBuilds();
const { state: branches } = useBranches();
const { state: jobs } = useJobs();

// ---- Library: the build form and the builds

// Empty on purpose: the list suggests branches, and typing any branch, tag or commit works.
const branch = ref("");
const submitting = ref(false);
const error = ref("");
const combobox = ref<{ focus: () => void } | null>(null);

const built = (sha: string) => builds.value.data?.find((b) => b.commit === sha);
const options = computed(() =>
  (branches.value.data ?? []).map((b) => ({
    value: b.name,
    label: b.name,
    description: `${built(b.sha) ? "Built · " : ""}${b.subject} · ${ago(b.date)}`,
  })),
);
const already = computed(() => {
  const sha = branches.value.data?.find((b) => b.name === branch.value)?.sha;
  return sha ? built(sha) : undefined;
});

async function build() {
  if (!branch.value.trim()) return;
  submitting.value = true;
  error.value = "";
  try {
    await unwrap(api.POST("/api/builds", { body: { ref: branch.value.trim() } }));
    branch.value = "";
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    submitting.value = false;
  }
}

// The command palette sends `?build=1`: put the cursor in the form.
async function focusForm() {
  if (route.query.build !== "1") return;
  await nextTick();
  combobox.value?.focus();
  const query = { ...route.query };
  delete query.build;
  await router.replace({ query });
}
onMounted(focusForm);
watch(() => route.query.build, focusForm);

/** Builds that are compiling or waiting their turn. */
const pending = computed(() =>
  (jobs.value.data ?? [])
    .filter((j) => j.spec.kind === "build" && (j.status === "running" || j.status === "queued"))
    .reverse(),
);
const lastLine = ref("");
useRunnerEvent("job.log", (data) => {
  const { id, line } = data as { id: number; line: string };
  if (id === pending.value.find((j) => j.status === "running")?.id) lastLine.value = line.trim();
});

/** Whether the build's own bench agrees with the commit's `Bench:` line. */
type Check = "match" | "differs" | "unchecked";
const check = (b: Build): Check =>
  b.bench_expected == null ? "unchecked" : b.bench_expected === b.bench ? "match" : "differs";
const marks = {
  match: { icon: PhCheckCircle, label: "Fingerprint matches its commit", class: "mark--ok" },
  differs: { icon: PhWarning, label: "Fingerprint differs from its commit", class: "mark--bad" },
  unchecked: { icon: PhMinusCircle, label: "Commit has no Bench line to compare with", class: "mark--none" },
} as const;

const kilo = (nps: number) => `${Math.round(nps / 1000)}k`;

// ---- Stage: the chosen build

const id = computed(() => Number(route.params.id) || 0);
const current = computed(() => builds.value.data?.find((b) => b.id === id.value));

// `/engines` alone opens the newest build.
watch(
  [builds, id],
  () => {
    const newest = builds.value.data?.[0];
    if (!id.value && newest) void router.replace({ path: `/engines/${newest.id}`, query: route.query });
  },
  { immediate: true },
);

const verdict = computed(() => {
  const b = current.value;
  if (!b) return undefined;
  const c = check(b);
  if (c === "match")
    return {
      tone: "success",
      title: "Fingerprint matches",
      text: "Its fingerprint matches the commit, so this build plays exactly as intended.",
    } as const;
  if (c === "differs")
    return {
      tone: "warning",
      title: "Fingerprint differs from the commit",
      text: `It searched ${b.bench.toLocaleString("en")} positions where the commit's message says ${b.bench_expected?.toLocaleString("en")}. Either the message is out of date or the engine does not search the same way twice. Check before testing with it.`,
    } as const;
  return {
    tone: "info",
    title: "Nothing to compare with",
    text: "This commit's message has no Bench line, so there is no fingerprint to check it against. Experiments compare it with their baseline instead.",
  } as const;
});

const others = computed(() =>
  (builds.value.data ?? []).filter((b) => b.ref === current.value?.ref && b.id !== id.value),
);
const fastest = computed(() => Math.max(...(builds.value.data ?? []).map((b) => b.nps), 1));
const pace = computed(() => {
  const b = current.value;
  if (!b || (builds.value.data?.length ?? 0) < 2) return "";
  const gap = 1 - b.nps / fastest.value;
  return gap < 0.01 ? "The fastest build so far." : `${Math.round(gap * 100)}% slower than the fastest build.`;
});

const details = computed(() => {
  const b = current.value;
  if (!b) return [];
  return [
    { label: "Commit", text: b.commit },
    { label: "Network", text: b.network ?? "None embedded" },
    { label: "Fingerprint", text: `${b.bench.toLocaleString("en")} nodes` },
    { label: "Speed", text: `${Math.round(b.nps).toLocaleString("en")} nodes/s on this machine` },
    { label: "Tablebases", text: b.features === "syzygy" ? "Included" : "Not included" },
    others.value.length ? { label: "Same branch", text: others.value.map((o) => `#${o.id}`).join(", ") } : null,
  ];
});

// The compile log lives with the job that built it, loaded only when opened.
const logOpen = ref(false);
const log = ref("");
const logState = ref<"idle" | "loading" | "missing">("idle");
async function loadLog() {
  const b = current.value;
  const job = [...(jobs.value.data ?? [])]
    .reverse()
    .find((j) => j.spec.kind === "build" && (j.result as { build?: number } | null)?.build === b?.id);
  if (!job) {
    logState.value = "missing";
    return;
  }
  logState.value = "loading";
  const { data } = await api.GET("/api/jobs/{id}/log", { params: { path: { id: job.id } }, parseAs: "text" });
  log.value = data ?? "";
  logState.value = data ? "idle" : "missing";
}
watch([logOpen, id], () => {
  log.value = "";
  logState.value = "idle";
  if (logOpen.value) void loadLog();
});

const analyse = () => router.push({ path: "/board", query: { build: String(id.value) } });
</script>

<template>
  <Workspace library-label="Engines">
    <template #library-head>
      <EForm class="form" :submitting="submitting" @submit="build">
        <ECombobox
          ref="combobox"
          v-model="branch"
          label="Branch, tag or commit to build"
          hide-label
          size="sm"
          freeform
          placeholder="Branch, tag or commit"
          autocomplete="off"
          :options="options"
          :loading="branches.status === 'pending'"
          :error="error || undefined"
          required
        />
        <EButton v-if="already" size="sm" variant="outline" @click="router.push(`/engines/${already.id}`)">
          Already built: open
        </EButton>
        <EButton v-else type="submit" size="sm" :icon="PhHammer" :loading="submitting" :disabled="!branch.trim()">
          Build
        </EButton>
      </EForm>
    </template>

    <template #library>
      <ul v-if="pending.length" class="pending" aria-label="Being built">
        <li v-for="j in pending" :key="j.id" class="pending-item">
          <EProgress v-if="j.status === 'running'" :label="`Building ${j.spec.kind === 'build' ? j.spec.ref : ''}`" />
          <p v-if="j.status === 'running' && lastLine" class="pending-line">{{ lastLine }}</p>
          <p v-if="j.status === 'queued'" class="pending-line">
            Waiting to build <b>{{ j.spec.kind === "build" ? j.spec.ref : "" }}</b>
          </p>
        </li>
      </ul>

      <ESkeleton v-if="builds.status === 'pending'" :lines="5" label="Loading engines" />
      <EEmptyState v-else-if="!builds.data?.length" title="No engines yet" size="sm" icon="info">
        Type a branch, tag or commit above and press Build. <code>main</code> is the current Reckless.
      </EEmptyState>
      <ul v-else class="rows">
        <li v-for="b in builds.data" :key="b.id">
          <ELink :link-component="RouterLink" :to="`/engines/${b.id}`" class="row">
            <span class="row-ref">{{ b.ref }}</span>
            <span class="row-mark" :class="marks[check(b)].class" :title="marks[check(b)].label">
              <EIcon :name="marks[check(b)].icon" :label="marks[check(b)].label" />
            </span>
            <span class="row-sub num">{{ b.commit.slice(0, 7) }} · {{ ago(b.created_at) }}</span>
            <span class="row-speed num">{{ kilo(b.nps) }} n/s</span>
          </ELink>
        </li>
      </ul>
    </template>

    <template v-if="current" #stage-head>
      <h1 class="head">
        <span class="head-ref">{{ current.ref }}</span> at <span class="head-ref">{{ current.commit.slice(0, 7) }}</span
        >, built {{ ago(current.created_at) }}
      </h1>
      <EButton variant="outline" size="sm" :icon="PhChartLine" class="head-action" @click="analyse">
        Analyse with it
      </EButton>
    </template>

    <div class="stage">
      <ESkeleton v-if="builds.status === 'pending'" :lines="6" label="Loading the engine" />
      <EEmptyState v-else-if="!current" title="Pick an engine" icon="info">
        {{ builds.data?.length ? "Choose a build from the list." : "Build a branch to see it here." }}
      </EEmptyState>

      <template v-else-if="verdict">
        <h2 class="subject">{{ current.subject }}</h2>

        <div class="columns">
          <div class="column">
            <ENotice :tone="verdict.tone" :title="verdict.title" :role="false">
              {{ verdict.text }}
              <Term of="bench" inline>What is a fingerprint?</Term>
            </ENotice>

            <section class="section" aria-labelledby="facts-title">
              <h3 id="facts-title" class="section-title">Build</h3>
              <EDescriptionList :items="details" />
            </section>
          </div>

          <section class="section" aria-labelledby="speed-title">
            <h3 id="speed-title" class="section-title">Speed</h3>
            <p class="section-note">
              <b v-if="pace">{{ pace }}</b> Measured on this machine; runs of the same commit differ by a few percent.
            </p>
            <SpeedBars :builds="builds.data ?? []" :selected="current.id" />
          </section>
        </div>

        <EDisclosure v-model:open="logOpen" title="Compile log">
          <p v-if="logState === 'loading'" class="section-note">Loading…</p>
          <p v-else-if="logState === 'missing'" class="section-note">
            The log is no longer among the runner's recent jobs.
          </p>
          <div v-else-if="log" class="log"><ECodeBlock :code="log" lang="text" label="Compile log" wrap /></div>
        </EDisclosure>
      </template>
    </div>
  </Workspace>
</template>

<style scoped>
.form {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: start;
  inline-size: 100%;
  gap: var(--e-gap-tight);
}

.pending {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control));
  border-block-end: var(--e-stroke-hair) solid var(--e-border);
  background: var(--e-primary-soft);
  list-style: none;
}

.pending-item {
  display: grid;
  gap: var(--e-gap-tight);
}

.pending-line {
  overflow: hidden;
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rows {
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--e-gap-tight) var(--e-gap-base);
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control));
  padding-inline-start: calc(var(--ws-pad, var(--e-pad-control)) - 0.1875rem);
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
  border-inline-start: 0.1875rem solid transparent;
  color: var(--e-ink);
  text-decoration: none;
  transition: background-color 0.1s;
}

.row:hover {
  background: var(--e-surface-tint);
}

.row:focus-visible {
  outline-offset: calc(-1 * var(--e-stroke-focus));
}

.row[aria-current="page"] {
  border-inline-start-color: var(--e-primary);
  background: var(--e-primary-soft);
}

.row[aria-current="page"] .row-ref {
  color: var(--e-primary);
}

.row-ref {
  overflow: hidden;
  font: var(--e-type-body);
  font-family: var(--e-font-mono);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-mark {
  display: inline-flex;
  grid-row: 1;
  grid-column: 2;
  align-items: center;
}

.mark--ok {
  color: var(--e-success);
}

.mark--bad {
  color: var(--e-danger);
}

.mark--none {
  color: var(--e-muted);
}

.row-sub,
.row-speed {
  color: var(--e-muted);
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
}

.row-speed {
  text-align: end;
}

.head {
  flex: 1;
  min-inline-size: 0;
  margin: 0;
  font: var(--e-type-body);
}

.head-ref {
  font-family: var(--e-font-mono);
  font-weight: 600;
}

/* The stage uses its width: the fingerprint and details beside the speed chart, the log below. */
.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-section);
  container-type: inline-size;
  inline-size: 100%;
  max-inline-size: 90rem;
  padding: var(--e-pad-panel);
  box-sizing: border-box;
}

.subject {
  margin: 0;
  font: var(--e-type-h4);
}

.columns {
  display: grid;
  align-items: start;
  gap: var(--e-gap-section);
}

@container (width >= 44rem) {
  .columns {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  }
}

.column {
  display: grid;
  gap: var(--e-gap-section);
  min-inline-size: 0;
}

.section {
  display: grid;
  gap: var(--e-gap-base);
  min-inline-size: 0;
}

.log {
  max-block-size: 24rem;
  overflow: auto;
}

.section-title {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.section-note {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
