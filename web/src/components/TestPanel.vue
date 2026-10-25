<script setup lang="ts">
import { computed } from "vue";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { EMetric } from "@eos-ui/vue/components/EMetric";
import { ESkeleton } from "@eos-ui/vue/components/ESkeleton";
import { ETag } from "@eos-ui/vue/components/ETag";
import LineChart from "../charts/LineChart.vue";
import DecisionGauge from "./DecisionGauge.vue";
import PairBar from "./PairBar.vue";
import Term from "./Term.vue";
import { useTest } from "../api/queries";
import { bounds, signed, timeControl } from "../format";

/** One match of an experiment: how far the evidence has come, and the numbers behind it. */
const props = defineProps<{ id: number }>();

const { state } = useTest(() => props.id);

const test = computed(() => state.value.data?.test);
const points = computed(() => state.value.data?.points ?? []);
const sprt = computed(() => test.value?.spec.sprt);
const lines = computed(() => {
  const b = test.value && bounds(test.value);
  return b
    ? [
        { value: b.reject, label: "fail line" },
        { value: b.accept, label: "pass line" },
      ]
    : [];
});
const est = computed(() => test.value?.estimate);
</script>

<template>
  <section v-if="test" class="test" :aria-label="`Test ${test.id}`">
    <DecisionGauge v-if="sprt" :test="test" />

    <EGrid min="8rem" gap="base" class="figures">
      <EMetric label="Games" :value="test.games.toLocaleString()" />
      <EMetric label="Score" :value="est ? `${(est.score * 100).toFixed(1)}%` : '–'" help="new version" />
      <EMetric label="Elo" :value="est ? signed(est.elo, 1) : '–'">
        <template #help>
          <template v-if="est">± {{ est.elo_error.toFixed(1) }} · </template><Term of="elo" inline>what is Elo?</Term>
        </template>
      </EMetric>
      <EMetric label="Chance stronger" :value="est ? `${Math.round(est.los * 100)}%` : '–'">
        <template #help><Term of="los" inline>how sure?</Term></template>
      </EMetric>
    </EGrid>

    <PairBar :penta="test.penta" />

    <figure v-if="sprt && points.length > 1" class="evidence">
      <figcaption><Term of="llr">Evidence over time</Term></figcaption>
      <LineChart
        label="Evidence (log-likelihood ratio) by games played"
        :x="points.map((p) => p.games)"
        :series="[{ label: 'evidence', values: points.map((p) => p.llr ?? null) }]"
        :lines="lines"
        x-label="games"
        :height="170"
      />
    </figure>

    <div class="settings" aria-label="Test settings">
      <ETag size="sm" :label="timeControl(test.spec.tc)" />
      <ETag size="sm" :label="`${test.spec.hash} MB hash`" />
      <ETag size="sm" :label="`${test.spec.threads} thread${test.spec.threads > 1 ? 's' : ''}`" />
      <ETag v-if="sprt" size="sm" :label="`bounds [${sprt.elo0}, ${sprt.elo1}]`" />
      <Term v-if="sprt" of="bounds" inline>What are bounds?</Term>
    </div>
  </section>
  <ESkeleton v-else-if="state.status === 'pending'" :lines="6" label="Loading the test" />
</template>

<style scoped>
.test {
  display: grid;
  gap: var(--e-gap-loose);
}

.evidence {
  display: grid;
  gap: var(--e-gap-snug);
  margin: 0;
}

.evidence figcaption {
  font: var(--e-type-small);
}

.settings {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-tight);
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
