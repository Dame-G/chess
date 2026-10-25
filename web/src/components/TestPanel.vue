<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { EButton } from "@eos-ui/vue/components/EButton";
import { EDisclosure } from "@eos-ui/vue/components/EDisclosure";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EMetric } from "@eos-ui/vue/components/EMetric";
import { EStamp } from "@eos-ui/vue/components/EStamp";
import { ETable } from "@eos-ui/vue/components/ETable";
import { ETag } from "@eos-ui/vue/components/ETag";
import LineChart from "../charts/LineChart.vue";
import DecisionGauge from "./DecisionGauge.vue";
import PairBar from "./PairBar.vue";
import Term from "./Term.vue";
import { useGames, useTest } from "../api/queries";
import { bounds, ending, signed, stageLabel, timeControl } from "../format";

const props = defineProps<{ id: number }>();

const { state } = useTest(() => props.id);
const { state: games } = useGames(() => props.id);

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

const shown = ref(40);
const recent = computed(() => (games.value.data ?? []).slice().reverse().slice(0, shown.value));
const total = computed(() => games.value.data?.length ?? 0);

/** A game's result for the new version. */
function outcome(g: { white: string; result: string }) {
  if (g.result === "1/2-1/2") return { label: "Draw", tone: "neutral" } as const;
  const devWhite = g.white === "dev";
  const whiteWon = g.result === "1-0";
  return devWhite === whiteWon
    ? ({ label: "Win", tone: "success" } as const)
    : ({ label: "Loss", tone: "danger" } as const);
}
</script>

<template>
  <section v-if="test" class="test">
    <header class="test-head">
      <div>
        <h3 class="test-title">{{ stageLabel(test.stage) }}</h3>
        <div class="test-tags">
          <ETag size="sm" :label="timeControl(test.spec.tc)" />
          <ETag size="sm" :label="`${test.spec.hash} MB hash`" />
          <ETag size="sm" :label="`${test.spec.threads} thread${test.spec.threads > 1 ? 's' : ''}`" />
          <ETag v-if="sprt" size="sm" :label="`bounds [${sprt.elo0}, ${sprt.elo1}]`">
            <Term of="bounds" inline>bounds [{{ sprt.elo0 }}, {{ sprt.elo1 }}]</Term>
          </ETag>
        </div>
      </div>
      <EStamp
        :tone="
          test.running
            ? 'info'
            : test.verdict === 'accepted'
              ? 'success'
              : test.verdict === 'rejected'
                ? 'danger'
                : 'neutral'
        "
        :label="
          test.running
            ? 'Playing'
            : test.verdict === 'accepted'
              ? 'Passed'
              : test.verdict === 'rejected'
                ? 'Failed'
                : 'Stopped'
        "
      />
    </header>

    <EGrid min="9rem" gap="base">
      <EMetric label="Games played" :value="test.games" />
      <EMetric
        label="Score"
        :value="test.estimate ? `${(test.estimate.score * 100).toFixed(1)}%` : '–'"
        help="for the new version"
      />
      <EMetric label="Elo difference" :value="test.estimate ? signed(test.estimate.elo, 1) : '–'">
        <template #help>
          <template v-if="test.estimate">± {{ test.estimate.elo_error.toFixed(1) }} · </template
          ><Term of="elo" inline>what is Elo?</Term>
        </template>
      </EMetric>
      <EMetric label="Chance stronger" :value="test.estimate ? `${Math.round(test.estimate.los * 100)}%` : '–'">
        <template #help><Term of="los" inline>how sure</Term></template>
      </EMetric>
    </EGrid>

    <DecisionGauge v-if="sprt" :test="test" />

    <PairBar :penta="test.penta" />

    <EDisclosure v-if="sprt && points.length > 1" title="Evidence over time">
      <LineChart
        label="Evidence (log-likelihood ratio) by games played"
        :x="points.map((p) => p.games)"
        :series="[{ label: 'evidence', values: points.map((p) => p.llr ?? null) }]"
        :lines="lines"
        x-label="games"
        :height="200"
      />
    </EDisclosure>

    <ETable v-if="recent.length" :caption="`Games (${total.toLocaleString()})`" dense max-height="22rem">
      <thead>
        <tr>
          <th scope="col">Game</th>
          <th scope="col">New version</th>
          <th scope="col">Result</th>
          <th scope="col">How it ended</th>
          <th scope="col" class="e-num">Moves</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="g in recent" :key="g.index">
          <th scope="row">
            <ELink :link-component="RouterLink" :to="`/tests/${id}/games/${g.index}`">#{{ g.index + 1 }}</ELink>
          </th>
          <td>
            <span :class="['side', g.white === 'dev' ? 'side--white' : 'side--black']" aria-hidden="true"></span>
            {{ g.white === "dev" ? "White" : "Black" }}
          </td>
          <td><EStamp size="sm" :tone="outcome(g).tone" :label="outcome(g).label" /></td>
          <td>{{ ending(g.termination) }}</td>
          <td class="e-num">{{ Math.ceil(g.plies / 2) }}</td>
        </tr>
      </tbody>
    </ETable>
    <EButton v-if="total > shown" variant="quiet" size="sm" @click="shown += 100">Show more games</EButton>
  </section>
</template>

<style scoped>
.test {
  display: grid;
  gap: var(--e-gap-loose);
  padding: var(--e-pad-card);
  border-radius: var(--e-radius-lg);
  background: var(--e-surface);
  box-shadow: var(--e-shadow-frame);
}

.test-head {
  display: flex;
  justify-content: space-between;
  align-items: start;
  gap: var(--e-gap-base);
}

.test-title {
  margin: 0 0 var(--e-gap-tight);
  font: var(--e-type-h3);
}

.test-tags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--e-gap-tight);
}

.side {
  display: inline-block;
  width: 0.625rem;
  height: 0.625rem;
  margin-inline-end: 0.25rem;
  border-radius: 50%;
  box-shadow: 0 0 0 1px var(--e-border-strong);
  vertical-align: baseline;
}

.side--white {
  background: #fffaf3;
}

.side--black {
  background: #2a2420;
}
</style>
