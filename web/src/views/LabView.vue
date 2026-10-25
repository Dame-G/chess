<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { PhPlus } from "@phosphor-icons/vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { useExperiments, type Experiment } from "../api/queries";
import { ago, kinds, stageLabel, verdict } from "../format";
import HowItWorks from "../components/HowItWorks.vue";
import NewExperiment from "../components/NewExperiment.vue";

const { state } = useExperiments();
const creating = ref(false);

const filter = ref<"all" | "active" | "passed" | "failed">("all");
const batch = ref("");
const batches = computed(() => [
  ...new Set((state.value.data ?? []).map((e) => e.batch).filter((b): b is string => !!b)),
]);

const matches = (e: Experiment) =>
  filter.value === "all" ||
  (filter.value === "active" && ["queued", "running"].includes(e.status)) ||
  e.status === filter.value;
const shown = computed(() =>
  (state.value.data ?? []).filter((e) => matches(e) && (!batch.value || e.batch === batch.value)),
);
const count = (f: typeof filter.value) =>
  (state.value.data ?? []).filter(
    (e) => f === "all" || (f === "active" ? ["queued", "running"].includes(e.status) : e.status === f),
  ).length;

/** Each stage as a dot: passed, failed, running or to come. */
function dots(e: Experiment) {
  return e.funnel.map((stage) => {
    const name = stage.type === "match" ? stage.name : stage.type;
    const r = e.results.find((x) => x.stage === name);
    return { name, state: r ? (r.passed ? "done" : "error") : e.current === name ? "current" : "upcoming" };
  });
}
</script>

<template>
  <EStack gap="section">
    <EPageHeader
      title="Experiments"
      description="Each experiment asks one question: does this change make Reckless stronger?"
    >
      <template #actions>
        <EButton :icon="PhPlus" @click="creating = true">Test an idea</EButton>
      </template>
    </EPageHeader>

    <EDisclosure title="How an experiment works" :open="state.status === 'success' && state.data.length < 3">
      <HowItWorks />
    </EDisclosure>

    <div class="filters">
      <ESegmented
        v-model="filter"
        label="Show"
        hide-label
        size="sm"
        :options="[
          { value: 'all', label: `All ${count('all')}` },
          { value: 'active', label: `Running ${count('active')}` },
          { value: 'passed', label: `Passed ${count('passed')}` },
          { value: 'failed', label: `Failed ${count('failed')}` },
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

    <ESkeleton v-if="state.status === 'pending'" :lines="4" label="Loading experiments" />
    <EEmptyState v-else-if="!shown.length" title="No experiments here yet" icon="info">
      Commit a change to a branch of the Reckless code, then test it.
      <template #action><EButton :icon="PhPlus" @click="creating = true">Test an idea</EButton></template>
    </EEmptyState>

    <ul v-else class="list">
      <li v-for="e in shown" :key="e.id" class="row">
        <EStamp class="row-stamp" :tone="verdict(e).tone" :label="verdict(e).label" />
        <div class="row-main">
          <ELink class="row-title" :link-component="RouterLink" :to="`/experiments/${e.id}`">{{ e.name }}</ELink>
          <p class="row-sub">
            <code>{{ e.dev }}</code> vs <code>{{ e.base }}</code> · {{ kinds[e.kind].label }} · {{ ago(e.created_at) }}
          </p>
          <p v-if="e.summary" class="row-summary">{{ e.summary }}</p>
        </div>
        <ol class="dots" :aria-label="`Stages of ${e.name}`">
          <li
            v-for="d in dots(e)"
            :key="d.name"
            :class="['dot', `dot--${d.state}`]"
            :title="`${stageLabel(d.name)}: ${d.state}`"
          >
            <span class="dot-label">{{ stageLabel(d.name) }}</span>
          </li>
        </ol>
      </li>
    </ul>

    <NewExperiment v-model:open="creating" />
  </EStack>
</template>

<style scoped>
.filters {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: var(--e-gap-base);
}

.list {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: 8.5rem minmax(0, 1fr) auto;
  align-items: center;
  gap: var(--e-gap-loose);
  padding: var(--e-gap-base) var(--e-pad-card);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.row-stamp {
  justify-self: start;
}

.row-main {
  display: grid;
  gap: 0.15rem;
  min-width: 0;
}

.row-title {
  font: var(--e-type-h5);
}

.row-sub,
.row-summary {
  margin: 0;
  overflow: hidden;
  color: var(--e-muted);
  font: var(--e-type-small);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row-summary {
  color: var(--e-ink);
}

.dots {
  display: flex;
  gap: 0.3rem;
  margin: 0;
  padding: 0;
  list-style: none;
}

.dot {
  width: 0.75rem;
  height: 0.75rem;
  border-radius: 50%;
  background: var(--e-surface-tint);
  box-shadow: inset 0 0 0 1px var(--e-border);
}

.dot--done {
  background: var(--e-success);
  box-shadow: none;
}

.dot--error {
  background: var(--e-danger);
  box-shadow: none;
}

.dot--current {
  background: var(--e-primary);
  box-shadow: 0 0 0 3px var(--e-primary-soft);
}

.dot-label {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

@media (width < 40rem) {
  .row {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .row-stamp {
    grid-column: 1 / -1;
  }
}
</style>
