<script setup lang="ts">
import { computed, useId } from "vue";
import type { Experiment, Test } from "../../api/queries";
import { ago, kinds, stages, verdict } from "../../format";
import TestPanel from "../TestPanel.vue";
import Term from "../Term.vue";
import StagePipeline from "./StagePipeline.vue";
import { funnelOf, verdictLine } from "./lab";

/** One experiment: how it stands in a sentence, the funnel it goes through, and the evidence of the chosen stage. */
const props = defineProps<{ experiment: Experiment; tests: Test[]; stage: string }>();

const panel = `panel-${useId()}`;
const v = computed(() => verdict(props.experiment));
const funnel = computed(() => funnelOf(props.experiment, props.tests));
const picked = computed(() => funnel.value.find((s) => s.name === props.stage));
const test = computed(() => props.tests.find((t) => t.stage === props.stage));
</script>

<template>
  <div class="stage">
    <header class="intro">
      <p :class="['verdict', `verdict--${v.tone}`]" aria-live="polite">{{ verdictLine(experiment, tests) }}</p>
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
      <p v-if="experiment.hypothesis" class="idea">
        <span class="idea-label">The idea</span>
        {{ experiment.hypothesis }}
      </p>
    </header>

    <StagePipeline :stages="funnel" :stage="stage" :panel="panel" />

    <section v-if="picked" :id="panel" class="panel" role="tabpanel" :aria-labelledby="`${panel}-title`" tabindex="-1">
      <div class="panel-head">
        <h3 :id="`${panel}-title`" class="panel-title">{{ picked.label }}</h3>
        <p class="panel-detail">{{ stages[picked.name]?.detail }}</p>
      </div>
      <TestPanel v-if="test" :id="test.id" />
      <p v-else-if="picked.result" class="plain num">{{ picked.result.detail }}</p>
      <p v-else class="plain">
        {{
          picked.state === "current" ? "Running now. The result appears here as soon as it is in." : "Not reached yet."
        }}
      </p>
      <Term v-if="picked.name === 'bench'" of="bench" inline>What is a fingerprint?</Term>
    </section>
  </div>
</template>

<style scoped>
.stage {
  display: grid;
  align-content: start;
  gap: var(--e-gap-section);
  padding: var(--e-gap-section) var(--e-pad-card);
  min-width: 0;
}

.intro {
  display: grid;
  gap: var(--e-gap-base);
}

.verdict {
  margin: 0;
  padding-inline-start: var(--e-gap-base);
  border-inline-start: 3px solid var(--e-border-strong);
  font: var(--e-type-deck);
  text-wrap: balance;
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

.refs,
.idea {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.refs code {
  color: var(--e-ink);
  font-family: var(--e-font-mono);
}

.sep {
  margin-inline: var(--e-gap-tight);
}

.idea {
  display: grid;
  gap: var(--e-gap-tight);
  max-width: 60ch;
}

.idea-label {
  font: var(--e-type-meta);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.panel {
  display: grid;
  gap: var(--e-gap-loose);
  min-width: 0;
}

.panel:focus-visible {
  outline: none;
}

.panel-head {
  display: grid;
  gap: var(--e-gap-tight);
}

.panel-title {
  margin: 0;
  font: var(--e-type-h5);
}

.panel-detail,
.plain {
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.panel-detail {
  max-width: 60ch;
}

.plain {
  padding: var(--e-gap-base) var(--e-pad-card);
  border: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
  border-radius: var(--e-radius-sm);
  color: var(--e-ink);
}
</style>
