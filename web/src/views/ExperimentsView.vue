<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EForm } from "@eos-ui/vue/components/EForm";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { EPageHeader } from "@eos-ui/vue/components/EPageHeader";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import { ESelect } from "@eos-ui/vue/components/ESelect";
import { EStack } from "@eos-ui/vue/components/EStack";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ESwitch } from "@eos-ui/vue/components/ESwitch";
import { ETable } from "@eos-ui/vue/components/ETable";
import { ETextField } from "@eos-ui/vue/components/ETextField";
import { ETextarea } from "@eos-ui/vue/components/ETextarea";
import { api } from "../api/runner";
import { unwrap, useExperiments, type Experiment } from "../api/queries";
import { statusTone } from "../format";

const router = useRouter();
const { state } = useExperiments();

const dev = ref("");
const base = ref("main");
const kind = ref<"gainer" | "nonregression" | "nonfunctional">("gainer");
const hypothesis = ref("");
const ltc = ref(false);
const error = ref("");
const submitting = ref(false);

async function submit() {
  submitting.value = true;
  error.value = "";
  try {
    const e = await unwrap(
      api.POST("/api/experiments", {
        body: { dev: dev.value, base: base.value, kind: kind.value, hypothesis: hypothesis.value, ltc: ltc.value },
      }),
    );
    router.push(`/experiments/${e.id}`);
  } catch (e) {
    error.value = (e as Error).message;
  } finally {
    submitting.value = false;
  }
}

const batch = ref("");
const batches = computed(() => [
  ...new Set((state.value.data ?? []).map((e) => e.batch).filter((b): b is string => !!b)),
]);
const shown = computed(() => (state.value.data ?? []).filter((e) => !batch.value || e.batch === batch.value));

function outcome(e: Experiment) {
  if (e.status === "running") return e.current ? `Running ${e.current}` : "Building";
  return e.summary ?? "";
}
</script>

<template>
  <EStack gap="section">
    <EPageHeader
      title="Experiments"
      description="A change against a baseline, through a funnel: bench, a quick screen, then the confirmation. The first stage that fails stops it."
    />

    <EForm @submit="submit">
      <EGrid min="14rem" gap="base">
        <ETextField v-model="dev" label="Change (dev ref)" required :error="error" />
        <ETextField v-model="base" label="Baseline (base ref)" required />
      </EGrid>
      <ESegmented
        v-model="kind"
        label="Kind"
        :options="[
          { value: 'gainer', label: 'Gainer' },
          { value: 'nonregression', label: 'Non-regression' },
          { value: 'nonfunctional', label: 'Non-functional' },
        ]"
      />
      <ETextarea v-model="hypothesis" label="Hypothesis" :rows="2" optional />
      <ESwitch v-model="ltc" label="Confirm at LTC too" hint="Adds 40+0.4 after STC: hours on one machine." />
      <EButton type="submit" :loading="submitting">Run experiment</EButton>
    </EForm>

    <ESelect
      v-if="batches.length"
      v-model="batch"
      label="Batch"
      :options="batches"
      placeholder="All experiments"
      clearable
    />

    <ETable
      caption="Experiments"
      :loading="state.status === 'pending'"
      :empty="shown.length === 0"
      empty-title="No experiments yet"
      empty-text="Run one above, or submit a batch with the CLI."
    >
      <thead>
        <tr>
          <th scope="col">Experiment</th>
          <th scope="col">Change</th>
          <th scope="col">Status</th>
          <th scope="col">Outcome</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="e in shown" :key="e.id">
          <th scope="row">
            <ELink :link-component="RouterLink" :to="`/experiments/${e.id}`">{{ e.id }}. {{ e.name }}</ELink>
          </th>
          <td>
            <code>{{ e.dev }}</code> vs <code>{{ e.base }}</code>
          </td>
          <td><EStamp size="sm" :tone="statusTone[e.status]" :label="e.status" /></td>
          <td>{{ outcome(e) }}</td>
        </tr>
      </tbody>
    </ETable>
  </EStack>
</template>
