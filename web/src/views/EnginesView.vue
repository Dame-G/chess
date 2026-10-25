<script setup lang="ts">
import { computed, ref } from "vue";
import { useRouter } from "vue-router";
import { PhChartLine, PhHammer } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ECodeBlock } from "@eos-ui/vue/components/ECodeBlock";
import { ECombobox } from "@eos-ui/vue/components/ECombobox";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { EForm } from "@eos-ui/vue/components/EForm";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { EProgress } from "@eos-ui/vue/components/EProgress";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { useRunnerEvent } from "../api/events";
import { api } from "../api/runner";
import { unwrap, useBranches, useBuilds, useJobs, type Build } from "../api/queries";
import { ago, speed } from "../format";
import Term from "../components/Term.vue";

const router = useRouter();
const { state: builds } = useBuilds();
const { state: branches } = useBranches();
const { state: jobs } = useJobs();

const branch = ref("main");
const submitting = ref(false);
const error = ref("");

const options = computed(() =>
  (branches.value.data ?? []).map((b) => ({
    value: b.name,
    label: b.name,
    description: `${built(b.sha) ? "Built · " : ""}${b.subject} · ${ago(b.date)}`,
  })),
);
const built = (sha: string) => builds.value.data?.find((b) => b.commit === sha);
const chosen = computed(() => branches.value.data?.find((b) => b.name === branch.value));
const already = computed(() => (chosen.value ? built(chosen.value.sha) : undefined));

async function build() {
  submitting.value = true;
  error.value = "";
  try {
    await unwrap(api.POST("/api/builds", { body: { ref: branch.value } }));
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    submitting.value = false;
  }
}

/** A build that is compiling now, with its last lines of output. */
const building = computed(() => jobs.value.data?.find((j) => j.status === "running" && j.spec.kind === "build"));
const log = ref<string[]>([]);
useRunnerEvent("job.log", (data) => {
  const { id, line } = data as { id: number; line: string };
  if (id !== building.value?.id) return;
  log.value = [...log.value.slice(-7), line];
});

const fastest = computed(() => Math.max(...(builds.value.data ?? []).map((b) => b.nps), 1));

function analyse(b: Build) {
  router.push({ path: "/analysis", query: { build: String(b.id) } });
}
</script>

<template>
  <EStack gap="section">
    <EPageHeader
      title="Engines"
      description="Each engine is Reckless compiled from one branch of its code. Build a branch to analyse with it or test it in an experiment."
    />

    <section class="build-card">
      <div class="build-intro">
        <span class="build-icon" aria-hidden="true"><PhHammer weight="duotone" /></span>
        <div>
          <h2 class="build-title">Build an engine</h2>
          <p class="quiet">Compiling takes about a minute. Each commit is built once and kept.</p>
        </div>
      </div>
      <EForm class="build-form" :submitting="submitting" @submit="build">
        <ECombobox
          v-model="branch"
          label="Branch, tag or commit"
          freeform
          :options="options"
          :loading="branches.status === 'pending'"
          :error="error || undefined"
          :hint="already ? `Already built as engine ${already.id}.` : 'Your branches first, then upstream Reckless.'"
          required
        />
        <EButton type="submit" :loading="submitting" :disabled="!!already">Build</EButton>
      </EForm>
      <div v-if="building" class="building">
        <EProgress :label="`Building ${building.spec.kind === 'build' ? building.spec.ref : ''}`" />
        <ECodeBlock v-if="log.length" :code="log.join('\n')" lang="text" label="Compiler output" wrap />
      </div>
    </section>

    <ESkeleton v-if="builds.status === 'pending'" :lines="4" label="Loading engines" />
    <EEmptyState v-else-if="!builds.data?.length" title="No engines yet" icon="info">
      Build <code>main</code> above to get the current Reckless.
    </EEmptyState>
    <ul v-else class="engines">
      <li v-for="b in builds.data" :key="b.id" class="engine">
        <header class="engine-head">
          <div>
            <p class="engine-name">{{ b.ref }}</p>
            <p class="quiet">
              <code>{{ b.commit.slice(0, 7) }}</code> · built {{ ago(b.created_at) }}
            </p>
          </div>
          <EStamp size="sm" :label="`#${b.id}`" />
        </header>
        <p class="engine-subject">{{ b.subject }}</p>
        <dl class="engine-facts">
          <div>
            <dt><Term of="nps" inline /></dt>
            <dd class="num">{{ speed(b.nps) }}</dd>
            <dd class="speedbar" aria-hidden="true"><span :style="{ width: `${(b.nps / fastest) * 100}%` }"></span></dd>
          </div>
          <div>
            <dt><Term of="bench" inline>Fingerprint</Term></dt>
            <dd class="num">
              {{ b.bench.toLocaleString() }}
              <EStamp
                v-if="b.bench_expected != null"
                size="sm"
                :tone="b.bench_expected === b.bench ? 'success' : 'danger'"
                :label="b.bench_expected === b.bench ? 'Matches commit' : 'Differs from commit'"
              />
            </dd>
          </div>
          <div>
            <dt>Network</dt>
            <dd class="mono">{{ b.network }}</dd>
          </div>
        </dl>
        <EButton variant="outline" size="sm" :icon="PhChartLine" @click="analyse(b)">Analyse with it</EButton>
      </li>
    </ul>

    <ENotice
      v-if="builds.data?.some((b) => b.bench_expected != null && b.bench_expected !== b.bench)"
      tone="warning"
      title="A fingerprint differs from its commit"
    >
      The commit message's Bench line does not match what the engine searched. The message may be out of date, or the
      engine does not search the same way twice. Check before testing with it.
    </ENotice>
  </EStack>
</template>

<style scoped>
.build-card,
.engine {
  display: grid;
  gap: var(--e-gap-base);
  padding: var(--e-pad-card);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.build-card {
  grid-template-columns: minmax(14rem, 1fr) minmax(16rem, 1.4fr);
  align-items: start;
}

.build-intro {
  display: flex;
  gap: var(--e-gap-base);
}

.build-icon {
  display: grid;
  flex: none;
  place-items: center;
  width: 2.75rem;
  height: 2.75rem;
  border-radius: var(--e-radius-md);
  background: var(--e-primary-soft);
  color: var(--e-primary);
  font-size: 1.5rem;
}

.build-title {
  margin: 0 0 0.25rem;
  font: var(--e-type-h4);
}

.build-form {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: start;
  gap: var(--e-gap-snug);
}

.build-form :deep(.e-btn) {
  margin-top: 1.6rem;
}

.building {
  display: grid;
  grid-column: 1 / -1;
  gap: var(--e-gap-snug);
}

.engines {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(19rem, 1fr));
  gap: var(--e-gap-base);
  margin: 0;
  padding: 0;
  list-style: none;
}

.engine {
  align-content: start;
}

.engine-head {
  display: flex;
  justify-content: space-between;
  align-items: start;
  gap: var(--e-gap-snug);
}

.engine-name {
  margin: 0;
  font: var(--e-type-h4);
  overflow-wrap: anywhere;
}

.engine-subject {
  margin: 0;
}

.engine-facts {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
}

.engine-facts div {
  display: grid;
  grid-template-columns: 7rem minmax(0, 1fr);
  align-items: center;
  gap: 0 var(--e-gap-snug);
}

.engine-facts dt {
  color: var(--e-muted);
  font: var(--e-type-small);
}

.engine-facts dd {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-tight);
  margin: 0;
}

.speedbar {
  grid-column: 2;
  height: 0.25rem;
  border-radius: 999px;
  background: var(--e-surface-tint);
}

.speedbar span {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--e-primary);
}

.mono {
  font: var(--e-type-small);
  font-family: var(--e-font-mono);
  overflow-wrap: anywhere;
}

.quiet {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

@media (width < 48rem) {
  .build-card {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
