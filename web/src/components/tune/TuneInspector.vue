<script setup lang="ts">
import { computed } from "vue";
import { EDescriptionList } from "@eos-ui/vue/components/EDescriptionList";
import { EEmptyState } from "@eos-ui/vue/components/EEmptyState";
import { ETable } from "@eos-ui/vue/components/ETable";
import type { Tune } from "../../api/queries";
import { signed } from "../../format";
import { controlText, gamesIn, pairs, type Iteration } from "./tune";

/** What a tune was set up with, and its latest iterations. */
const props = defineProps<{ tune: Tune; history: Iteration[] }>();

const settings = computed(() => {
  const t = props.tune;
  const per = t.pairs_per_iteration;
  return [
    { label: "Engine", text: t.ref },
    {
      label: "Games",
      text: `${per} pairs (${per * 2} games) each iteration, ${gamesIn(t.iterations, per).toLocaleString("en")} in all`,
    },
    { label: "Length", text: `${controlText(t)}, ${t.concurrency} games at once` },
    { label: "Decay", text: `alpha ${t.alpha}, gamma ${t.gamma}, A ${t.a_ratio} of the iterations` },
  ];
});
const recent = computed(() => props.history.slice(-40).reverse());
const lead = (r: number) => signed(r, 0);
</script>

<template>
  <EDescriptionList class="settings" :items="settings" />
  <EEmptyState v-if="!recent.length" bare size="sm" title="No iterations yet">
    Each one plays a few game pairs and takes a step.
  </EEmptyState>
  <template v-else>
    <ETable caption="Recent iterations" hide-caption dense>
      <thead>
        <tr>
          <th scope="col">Iteration</th>
          <th scope="col" class="e-num" title="Games the higher copy won, minus games it lost">Lead</th>
          <th scope="col" class="e-num" title="Game pairs the higher copy won, drew and lost">Pairs W / D / L</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="h in recent" :key="h.iteration">
          <th scope="row" class="num it">{{ h.iteration }}</th>
          <td class="e-num num">{{ lead(h.result) }}</td>
          <td class="e-num num">{{ pairs(h.penta).win }} / {{ pairs(h.penta).draw }} / {{ pairs(h.penta).loss }}</td>
        </tr>
      </tbody>
    </ETable>
    <p class="note">
      Lead is how far the higher copy of the constants was ahead; the tune steps that way. The last
      {{ recent.length }} are shown.
    </p>
  </template>
</template>

<style scoped>
.settings {
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control-sm));
  border-block-end: var(--e-stroke-hair) solid color-mix(in oklab, var(--e-border) 55%, transparent);
}

.it {
  font-weight: 400;
  text-align: start;
}

.note {
  margin: 0;
  padding: var(--e-gap-base) var(--ws-pad, var(--e-pad-control-sm));
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
