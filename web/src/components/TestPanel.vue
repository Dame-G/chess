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
const b = computed(() => (test.value ? bounds(test.value) : undefined));
const lines = computed(() =>
  b.value
    ? [
        { value: b.value.reject, label: "reject" },
        { value: b.value.accept, label: "accept" },
      ]
    : [],
);
const est = computed(() => test.value?.estimate);
</script>

<template>
  <section v-if="test" class="test" :aria-label="`Test ${test.id}`">
    <DecisionGauge v-if="sprt" :test="test" />

    <EGrid min="9rem" gap="loose" class="figures">
      <EMetric label="Games" :value="test.games.toLocaleString()" />
      <EMetric label="Score" :value="est ? `${(est.score * 100).toFixed(1)}%` : '–'" help="new version" />
      <EMetric label="Chance stronger" :value="est ? `${Math.round(est.los * 100)}%` : '–'">
        <template #help><Term of="los" inline>how sure?</Term></template>
      </EMetric>
      <EMetric
        label="Time control"
        :value="timeControl(test.spec.tc)"
        :help="`${test.spec.hash} MB hash · ${test.spec.threads} thread${test.spec.threads > 1 ? 's' : ''}`"
      />
    </EGrid>

    <PairBar :penta="test.penta" />

    <figure v-if="sprt && b && points.length > 1" class="evidence">
      <figcaption class="cap">
        <Term of="llr">Evidence over time</Term>
      </figcaption>
      <LineChart
        label="Evidence (log-likelihood ratio) by games played, with the reject and accept lines"
        :x="points.map((p) => p.games)"
        :series="[{ label: 'evidence', values: points.map((p) => p.llr ?? null) }]"
        :lines="lines"
        x-label="games"
        :height="190"
      />
    </figure>

    <div class="settings" aria-label="Test settings">
      <ETag v-if="sprt" size="sm" :label="`bounds [${signed(sprt.elo0, 0)}, ${signed(sprt.elo1, 0)}]`" />
      <Term v-if="sprt" of="bounds" inline>What are bounds?</Term>
      <Term of="elo" inline>What is Elo?</Term>
    </div>
  </section>
  <ESkeleton v-else-if="state.status === 'pending'" :lines="6" label="Loading the test" />
</template>

<style scoped>
.test {
  display: grid;
  gap: var(--e-gap-section);
}

.figures {
  padding-block: var(--e-gap-loose);
  border-block: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
}

.evidence {
  display: grid;
  gap: var(--e-gap-base);
  margin: 0;
}

.cap {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: baseline;
  gap: var(--e-gap-tight) var(--e-gap-base);
  font: var(--e-type-small);
}

.settings {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--e-gap-snug);
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
