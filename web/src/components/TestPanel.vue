<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import { ELink } from "@eos-ui/vue/components/ELink";
import { EGrid } from "@eos-ui/vue/components/EGrid";
import { EHeading } from "@eos-ui/vue/components/EHeading";
import { EMetric } from "@eos-ui/vue/components/EMetric";
import { EStack } from "@eos-ui/vue/components/EStack";
import { ETable } from "@eos-ui/vue/components/ETable";
import LineChart from "../charts/LineChart.vue";
import { useGames, useTest } from "../api/queries";
import { signed, timeControl } from "../format";

const props = defineProps<{ id: number }>();

const { state } = useTest(() => props.id);
const { state: games } = useGames(() => props.id);

const test = computed(() => state.value.data?.test);
const points = computed(() => state.value.data?.points ?? []);
const sprt = computed(() => test.value?.spec.sprt);

/** The SPRT's decision lines, ln(beta/(1-alpha)) and ln((1-beta)/alpha). */
const lines = computed(() => {
  const s = sprt.value;
  if (!s) return [];
  return [
    { value: Math.log(s.beta / (1 - s.alpha)), label: "reject" },
    { value: Math.log((1 - s.beta) / s.alpha), label: "accept" },
  ];
});

const penta = ["0", "½", "1", "1½", "2"];
const recent = computed(() => (games.value.data ?? []).slice(-30).reverse());
</script>

<template>
  <EStack v-if="test" gap="base">
    <EHeading :level="3">
      {{ test.stage }} · {{ timeControl(test.spec.tc) }}, {{ test.spec.hash }} MiB, {{ test.spec.threads }}t
      <template v-if="sprt"> · SPRT [{{ sprt.elo0 }}, {{ sprt.elo1 }}]</template>
    </EHeading>
    <EGrid min="9rem" gap="base">
      <EMetric label="Games" :value="test.games" :help="test.running ? 'Running' : 'Finished'" />
      <EMetric
        label="nElo"
        :value="test.estimate ? signed(test.estimate.nelo) : '–'"
        :help="test.estimate ? `± ${test.estimate.nelo_error.toFixed(2)}` : undefined"
      />
      <EMetric
        label="Elo"
        :value="test.estimate ? signed(test.estimate.elo) : '–'"
        :help="test.estimate ? `± ${test.estimate.elo_error.toFixed(2)}` : undefined"
      />
      <EMetric
        v-if="test.llr != null"
        label="LLR"
        :value="test.llr.toFixed(2)"
        :help="lines.length ? `(${lines[0]!.value.toFixed(2)}, ${lines[1]!.value.toFixed(2)})` : undefined"
      />
    </EGrid>

    <LineChart
      v-if="sprt && points.length > 1"
      label="LLR by games played"
      :x="points.map((p) => p.games)"
      :series="[{ label: 'LLR', values: points.map((p) => p.llr ?? null) }]"
      :lines="lines"
    />

    <ETable caption="Game pairs by points scored by dev" dense>
      <thead>
        <tr>
          <th v-for="p in penta" :key="p" scope="col" class="e-num">{{ p }}</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td v-for="(n, i) in test.penta" :key="i" class="e-num">{{ n }}</td>
        </tr>
      </tbody>
    </ETable>

    <ETable v-if="recent.length" caption="Latest games" dense max-height="20rem">
      <thead>
        <tr>
          <th scope="col">Game</th>
          <th scope="col">White</th>
          <th scope="col">Black</th>
          <th scope="col">Result</th>
          <th scope="col">Ending</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="g in recent" :key="g.index">
          <th scope="row">
            <ELink :link-component="RouterLink" :to="`/tests/${id}/games/${g.index}`">{{ g.index + 1 }}</ELink>
          </th>
          <td>{{ g.white }}</td>
          <td>{{ g.black }}</td>
          <td>{{ g.result }}</td>
          <td>{{ g.termination }}, {{ g.plies }} plies</td>
        </tr>
      </tbody>
    </ETable>
  </EStack>
</template>
