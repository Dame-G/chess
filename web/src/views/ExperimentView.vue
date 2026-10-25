<script setup lang="ts">
import { computed } from "vue";
import { RouterLink, useRoute } from "vue-router";
import { PhCheck, PhMinus, PhX } from "@phosphor-icons/vue";
import { EBreadcrumb } from "@eos-ui/vue/components/EBreadcrumb";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EHeading } from "@eos-ui/vue/components/EHeading";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { EStepper } from "@eos-ui/vue/components/EStepper";
import { api } from "../api/runner";
import { useExperiment } from "../api/queries";
import { ago, forecast, kinds, signed, stageLabel, stages, verdict } from "../format";
import TestPanel from "../components/TestPanel.vue";

const route = useRoute();
const id = computed(() => Number(route.params.id));
const { state } = useExperiment(id);

const experiment = computed(() => state.value.data?.experiment);
const tests = computed(() => [...(state.value.data?.tests ?? [])].reverse());
const v = computed(() => (experiment.value ? verdict(experiment.value) : undefined));

/** While it plays, the live numbers in a sentence. */
const sentence = computed(() => {
  const e = experiment.value;
  const t = tests.value[0];
  if (!e || !v.value) return "";
  if (e.status !== "running" || !t?.running || !t.estimate) return v.value.sentence;
  const s = t.estimate;
  const lead = `${stageLabel(t.stage)}: after ${t.games.toLocaleString()} games the new version scores ${(s.score * 100).toFixed(1)}%, about ${signed(s.elo, 1)} Elo (± ${s.elo_error.toFixed(1)}).`;
  const f = forecast(t);
  if (!f || f.progress < 0.15) return `${lead} Too early to call.`;
  return `${lead} Leaning towards ${f.toward === "accept" ? "passing" : "failing"}.`;
});

/** Each stage of the funnel with its result, in order. */
const funnel = computed(() => {
  const e = experiment.value;
  if (!e) return [];
  return e.funnel.map((stage) => {
    const name = stage.type === "match" ? stage.name : stage.type;
    const result = e.results.find((r) => r.stage === name);
    const state = result ? (result.passed ? "done" : "error") : e.current === name ? "current" : "upcoming";
    return { name, label: stageLabel(name), detail: stages[name]?.detail ?? "", result, state } as const;
  });
});

const active = computed(() => experiment.value && ["queued", "running"].includes(experiment.value.status));

async function cancel() {
  await api.DELETE("/api/experiments/{id}", { params: { path: { id: id.value } } });
}
</script>

<template>
  <EStack v-if="experiment && v" gap="section">
    <EPageHeader :title="experiment.name" :description="experiment.hypothesis || undefined">
      <template #breadcrumb>
        <EBreadcrumb
          :link-component="RouterLink"
          :items="[{ label: 'Experiments', to: '/experiments' }, { label: experiment.name }]"
        />
      </template>
      <template #status><EStamp :tone="v.tone" :label="v.label" /></template>
      <template #actions>
        <EButton v-if="active" variant="outline" @click="cancel">Stop experiment</EButton>
      </template>
    </EPageHeader>

    <section class="summary" :class="`summary--${v.tone}`">
      <div class="summary-icon" aria-hidden="true">
        <PhCheck v-if="v.tone === 'success'" weight="bold" />
        <PhX v-else-if="v.tone === 'danger'" weight="bold" />
        <PhMinus v-else weight="bold" />
      </div>
      <div class="summary-text">
        <p class="summary-kicker">{{ kinds[experiment.kind].label }}</p>
        <p class="summary-line">
          <code>{{ experiment.dev }}</code> against <code>{{ experiment.base }}</code>
        </p>
        <p class="summary-sentence">{{ sentence }}</p>
      </div>
    </section>

    <EStack gap="base">
      <EHeading :level="2">Stages</EHeading>
      <EStepper label="Stages" :steps="funnel.map((s) => ({ label: s.label, state: s.state }))" />
      <ol class="stages">
        <li v-for="s in funnel" :key="s.name" class="stage" :class="`stage--${s.state}`">
          <div class="stage-head">
            <strong>{{ s.label }}</strong>
            <EStamp
              v-if="s.result"
              size="sm"
              :tone="s.result.passed ? 'success' : 'danger'"
              :label="s.result.passed ? 'Passed' : 'Failed'"
            />
            <EStamp v-else-if="s.state === 'current'" size="sm" tone="info" label="Running" />
            <EStamp v-else size="sm" label="Not yet" />
          </div>
          <p class="stage-detail">{{ s.detail }}</p>
          <p v-if="s.result" class="stage-result">{{ s.result.detail }}</p>
        </li>
      </ol>
    </EStack>

    <EStack v-if="tests.length" gap="base">
      <EHeading :level="2">Games</EHeading>
      <TestPanel v-for="t in tests" :id="t.id" :key="t.id" />
    </EStack>

    <EDisclosure title="Details">
      <EDescriptionList
        :items="[
          { label: 'Kind', text: `${kinds[experiment.kind].label}: ${kinds[experiment.kind].detail}` },
          { label: 'New version', text: experiment.dev },
          { label: 'Compared with', text: experiment.base },
          experiment.batch ? { label: 'Batch', text: experiment.batch } : null,
          { label: 'Started', text: ago(experiment.created_at) },
          experiment.finished_at ? { label: 'Finished', text: ago(experiment.finished_at) } : null,
          { label: 'Number', text: String(experiment.id) },
        ]"
      />
    </EDisclosure>
  </EStack>
  <ENotice v-else-if="state.status === 'error'" tone="danger" title="Could not load the experiment">
    {{ state.error?.message }}
  </ENotice>
  <ESkeleton v-else :lines="6" label="Loading the experiment" />
</template>

<style scoped>
.summary {
  display: flex;
  gap: var(--e-gap-loose);
  align-items: center;
  padding: var(--e-gap-loose) var(--e-pad-card);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.summary-icon {
  display: grid;
  flex: none;
  place-items: center;
  width: 3rem;
  height: 3rem;
  border-radius: 50%;
  background: var(--e-surface-tint);
  color: var(--e-muted);
  font-size: 1.5rem;
}

.summary--success .summary-icon {
  background: var(--e-success-soft);
  color: var(--e-success);
}

.summary--danger .summary-icon {
  background: var(--e-danger-soft);
  color: var(--e-danger);
}

.summary--info .summary-icon {
  background: var(--e-primary-soft);
  color: var(--e-primary);
}

.summary-text {
  display: grid;
  gap: 0.25rem;
  min-width: 0;
}

.summary-text p {
  margin: 0;
}

.summary-kicker {
  color: var(--e-muted);
  font: var(--e-type-kicker);
  text-transform: uppercase;
}

.summary-sentence {
  font: var(--e-type-deck);
}

.stages {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
  gap: var(--e-gap-base);
  margin: 0;
  padding: 0;
  list-style: none;
}

.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-tight);
  padding: var(--e-gap-base);
  border-radius: var(--e-radius-md);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.stage--upcoming {
  opacity: 0.7;
}

.stage-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--e-gap-snug);
}

.stage-detail,
.stage-result {
  margin: 0;
  font: var(--e-type-small);
}

.stage-detail {
  color: var(--e-muted);
}

.stage-result {
  font-family: var(--e-font-mono);
}
</style>
