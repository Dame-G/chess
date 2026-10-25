<script setup lang="ts">
import { computed } from "vue";
import { EStepper } from "@eos-ui/vue/components/EStepper";
import type { Experiment, Test } from "../../api/queries";
import { ago, kinds, stages, verdict } from "../../format";
import TestPanel from "../TestPanel.vue";
import Term from "../Term.vue";
import { funnelOf, verdictLine } from "./lab";

const props = defineProps<{ experiment: Experiment; tests: Test[] }>();
/** The stage shown below the stepper. */
const stage = defineModel<string>("stage", { required: true });

const v = computed(() => verdict(props.experiment));
const funnel = computed(() => funnelOf(props.experiment, props.tests));
const picked = computed(() => funnel.value.find((s) => s.name === stage.value));
const test = computed(() => props.tests.find((t) => t.stage === stage.value));
const steps = computed(() =>
  funnel.value.map((s) => ({
    label: s.note ? `${s.label} · ${s.note}` : s.label,
    state: s.state,
    disabled: s.state === "upcoming",
  })),
);
</script>

<template>
  <div class="stage">
    <section class="intro">
      <p class="refs">
        <code>{{ experiment.dev }}</code> vs <code>{{ experiment.base }}</code>
        <span class="sep" aria-hidden="true">·</span>
        {{ kinds[experiment.kind].label }}
        <span class="sep" aria-hidden="true">·</span>
        {{ ago(experiment.created_at) }}
        <template v-if="experiment.batch">
          <span class="sep" aria-hidden="true">·</span> batch {{ experiment.batch }}
        </template>
      </p>
      <p v-if="experiment.hypothesis" class="hypothesis">{{ experiment.hypothesis }}</p>
    </section>

    <p :class="['verdict', `verdict--${v.tone}`]" aria-live="polite">{{ verdictLine(experiment, tests) }}</p>

    <section :class="['funnel', 'sel-' + (funnel.findIndex((f) => f.name === stage) + 1)]">
      <EStepper label="Stages" interactive :steps="steps" @select="(i) => (stage = funnel[i]!.name)" />
    </section>

    <section class="panel">
      <TestPanel v-if="test" :id="test.id" />
      <div v-else-if="picked" class="plain">
        <p class="plain-line">
          <strong>{{ picked.label }}.</strong>
          {{ picked.result?.detail ?? (picked.state === "current" ? "Running now." : "Not reached yet.") }}
        </p>
        <p class="plain-detail">
          {{ stages[picked.name]?.detail }}
          <Term v-if="picked.name === 'bench'" of="bench" inline>What is a fingerprint?</Term>
        </p>
      </div>
    </section>
  </div>
</template>

<style scoped>
.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-loose);
  padding: var(--e-gap-loose) var(--e-pad-control-sm);
  min-width: 0;
}

.intro {
  display: grid;
  gap: var(--e-gap-tight);
}

.refs {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.refs code {
  color: var(--e-ink);
  font-family: var(--e-font-mono);
}

.sep {
  margin-inline: 0.25rem;
}

.hypothesis {
  margin: 0;
  font: var(--e-type-body);
}

.verdict {
  margin: 0;
  padding-inline-start: var(--e-gap-base);
  border-inline-start: 3px solid var(--e-border-strong);
  font: var(--e-type-deck);
}

.verdict--success {
  border-color: var(--e-success);
}

.verdict--danger {
  border-color: var(--e-danger);
}

.verdict--info {
  border-color: var(--e-primary);
}

.funnel {
  padding-block: var(--e-gap-snug);
  border-block: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
  overflow-x: auto;
}

.funnel :deep(.e-step-body) {
  padding-block-end: 0.375rem;
  border-block-end: 2px solid transparent;
}

.sel-1 :deep(.e-step:nth-child(1) .e-step-body),
.sel-2 :deep(.e-step:nth-child(2) .e-step-body),
.sel-3 :deep(.e-step:nth-child(3) .e-step-body),
.sel-4 :deep(.e-step:nth-child(4) .e-step-body),
.sel-5 :deep(.e-step:nth-child(5) .e-step-body),
.sel-6 :deep(.e-step:nth-child(6) .e-step-body) {
  border-block-end-color: var(--e-primary);
  font-weight: 700;
}

.plain {
  display: grid;
  gap: var(--e-gap-tight);
}

.plain p {
  margin: 0;
}

.plain-line {
  font-family: var(--e-font-mono);
  font-size: var(--e-text-sm);
}

.plain-detail {
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
