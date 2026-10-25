<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EInline } from "@eos-ui/vue/components/EInline";
import { ENotice } from "@eos-ui/vue/components/ENotice";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { EStepper } from "@eos-ui/vue/components/EStepper";
import { api } from "../api/runner";
import { useExperiment } from "../api/queries";
import { statusTone } from "../format";
import TestPanel from "../components/TestPanel.vue";

const route = useRoute();
const id = computed(() => Number(route.params.id));
const { state } = useExperiment(id);

const experiment = computed(() => state.value.data?.experiment);
const tests = computed(() => [...(state.value.data?.tests ?? [])].reverse());

const steps = computed(() => {
  const e = experiment.value;
  if (!e) return [];
  return e.funnel.map((stage) => {
    const name = stage.type === "match" ? stage.name : stage.type;
    const result = e.results.find((r) => r.stage === name);
    const state = result ? (result.passed ? "done" : "error") : e.current === name ? "current" : "upcoming";
    return { label: name, state } as const;
  });
});

const active = computed(() => experiment.value && ["queued", "running"].includes(experiment.value.status));

async function cancel() {
  await api.DELETE("/api/experiments/{id}", { params: { path: { id: id.value } } });
}
</script>

<template>
  <EStack v-if="experiment" gap="section">
    <EPageHeader
      :title="`${experiment.id}. ${experiment.name}`"
      :description="experiment.hypothesis || `${experiment.dev} against ${experiment.base}`"
    />
    <EInline gap="base" align="center">
      <EStamp :tone="statusTone[experiment.status]" :label="experiment.status" />
      <span>
        <code>{{ experiment.dev }}</code> vs <code>{{ experiment.base }}</code> · {{ experiment.kind }}
        <template v-if="experiment.batch"> · batch {{ experiment.batch }}</template>
      </span>
      <EButton v-if="active" variant="outline" size="sm" @click="cancel">Cancel</EButton>
    </EInline>

    <EStepper label="Funnel" :steps="steps" />

    <ENotice
      v-if="experiment.summary"
      :tone="experiment.status === 'passed' ? 'success' : experiment.status === 'failed' ? 'danger' : 'neutral'"
      :title="experiment.summary"
      :role="false"
    />

    <EDescriptionList
      v-if="experiment.results.length"
      :items="
        experiment.results.map((r) => ({
          label: r.stage,
          text: r.detail,
          stamp: r.passed ? 'Passed' : 'Failed',
          stampTone: r.passed ? 'success' : 'danger',
        }))
      "
    />

    <TestPanel v-for="t in tests" :id="t.id" :key="t.id" />
  </EStack>
  <ENotice v-else-if="state.status === 'error'" tone="danger" title="Could not load the experiment">
    {{ state.error?.message }}
  </ENotice>
</template>
