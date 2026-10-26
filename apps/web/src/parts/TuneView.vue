<script setup lang="ts">
import { computed, ref } from "vue";
import { EButton } from "@eos-ui/vue/components/EButton";
import { ETable } from "@eos-ui/vue/components/ETable";
import type { TuneState } from "../api/types";
import { signed } from "../format";
import { tuneCost, tuneCostText } from "../sheets/tune";
import { usePolicy } from "../stores/policy";
import type { ChartSeries } from "./chart";
import LineChart from "./LineChart.vue";
import Term from "./Term.vue";

/**
 * A tune's trajectory: each constant's value through the iterations, as a share of its range so they share one scale,
 * and a table of where each started and where it is now. Pick a constant to draw its line in colour.
 */
const props = defineProps<{ tune: TuneState; running: boolean }>();

const policy = usePolicy();
const picked = ref<string | null>(null);

const steps = computed(() => props.tune.steps);
const x = computed(() => steps.value.map((s) => s.iteration));
const series = computed<ChartSeries[]>(() =>
  props.tune.params.map((p, i) => ({
    label: p.name,
    values: steps.value.map((s) => {
      const v = s.values[i];
      return v === undefined ? null : (100 * (v - p.start)) / (p.max - p.min || 1);
    }),
    color: picked.value === p.name ? "--e-primary" : "--e-muted",
    width: picked.value === p.name ? 2.5 : 1,
  })),
);
// The picked line is drawn last so it sits on top.
const ordered = computed(() =>
  [...series.value].sort((a, b) => Number(a.color === "--e-primary") - Number(b.color === "--e-primary")),
);

const done = computed(() => steps.value.at(-1)?.iteration ?? 0);
const cost = computed(() =>
  tuneCost(props.tune.spec.iterations - done.value, props.tune.spec.pairs, policy.status?.cores ?? 8),
);
const rows = computed(() =>
  props.tune.params.map((p) => {
    const change = p.value - p.start;
    return { ...p, change, percent: p.start ? (100 * change) / p.start : 0 };
  }),
);
const fmt = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(1));
</script>

<template>
  <div class="tune">
    <p class="detail">
      Iteration {{ done.toLocaleString("en-GB") }} of {{ tune.spec.iterations.toLocaleString("en-GB") }}
      <template v-if="running && tune.spec.iterations > done"> · {{ tuneCostText(cost) }}</template>
    </p>
    <LineChart
      label="How far each constant has moved from its start, as a share of its range"
      :x="x"
      :series="ordered"
      :height="150"
      :lines="[{ value: 0, color: '--e-border-strong', dash: [] }]"
      :x-format="(v) => String(Math.round(v))"
      :format="(v) => signed(v, 0) + '%'"
    />
    <ETable caption="The constants being tuned" hide-caption dense>
      <thead>
        <tr>
          <th scope="col">Constant</th>
          <th scope="col" class="e-num">Start</th>
          <th scope="col" class="e-num">Now</th>
          <th scope="col" class="e-num">Change</th>
          <th scope="col" class="e-num">Range</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in rows" :key="r.name" :class="{ picked: picked === r.name }">
          <th scope="row" class="name-col">
            <EButton
              variant="quiet"
              size="sm"
              :aria-pressed="picked === r.name"
              @click="picked = picked === r.name ? null : r.name"
            >
              <span class="mono">{{ r.name }}</span>
            </EButton>
          </th>
          <td class="e-num">{{ fmt(r.start) }}</td>
          <td class="e-num">{{ fmt(Math.round(r.value * 10) / 10) }}</td>
          <td class="e-num">
            {{ signed(r.change, 1) }} <small class="muted">{{ signed(r.percent, 0) }}%</small>
          </td>
          <td class="e-num muted">{{ r.min }}–{{ r.max }}</td>
        </tr>
      </tbody>
    </ETable>
    <p class="sec-note">
      <Term k="tune" /> moves each constant a little after every batch of games. The trajectory can wander on noise, so
      only the confirmation test decides.
    </p>
  </div>
</template>

<style scoped>
.tune {
  display: grid;
  gap: var(--e-gap-snug);
  min-width: 0;
}

.detail {
  margin: 0;
  font-weight: 600;
}

.name-col {
  --e-control-height: 1.75rem;
  --e-tap: 1.75rem;
}

.picked {
  background: var(--e-primary-soft);
}

.muted {
  color: var(--e-muted);
}
</style>
