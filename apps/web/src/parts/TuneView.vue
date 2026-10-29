<script setup lang="ts">
import { computed, ref } from "vue";
import type { TuneDetail } from "../api/types";
import { formatNumber, signed } from "../format";
import { tuneCost, tuneCostText } from "../sheets/tune";
import { usePolicy } from "../stores/policy";
import UiEmpty from "../ui/UiEmpty.vue";
import UiMeter from "../ui/UiMeter.vue";
import UiTable from "../ui/UiTable.vue";
import type { ChartSeries } from "./chart";
import LineChart from "./LineChart.vue";
import Term from "./Term.vue";

/**
 * A tune's trajectory: each constant's value through the iterations, as a share of its range so they share one scale,
 * and a table of where each started and where it is now. Pick a constant to draw its line in colour.
 */
const props = defineProps<{ tune: TuneDetail; running: boolean }>();

const policy = usePolicy();
const picked = ref<string | null>(null);

const steps = computed(() => props.tune.steps);
/**
 * Each constant with where it started, where it is now (the last step's value) and its range. The range and start
 * come from the tuned branch's build; when it is not there the start is the first step's value and the range unknown.
 */
const params = computed(() =>
  props.tune.params.map((p, i) => {
    const first = steps.value[0]?.values[i];
    const start = p.start ?? first ?? 0;
    const value = steps.value.at(-1)?.values[i] ?? start;
    return { name: p.name, start, value, min: p.min ?? null, max: p.max ?? null };
  }),
);
const x = computed(() => steps.value.map((s) => s.iteration));
const series = computed<ChartSeries[]>(() =>
  params.value.map((p, i) => ({
    label: p.name,
    values: steps.value.map((s) => {
      const v = s.values[i];
      const span = p.max != null && p.min != null ? p.max - p.min : Math.abs(p.start);
      return v === undefined ? null : (100 * (v - p.start)) / (span || 1);
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
const total = computed(() => props.tune.spec.iterations);
const cost = computed(() => tuneCost(total.value - done.value, props.tune.spec.pairs, policy.status?.cores ?? 8));
const fmt = (v: number) => (Number.isInteger(v) ? String(v) : v.toFixed(1));
const rows = computed(() =>
  params.value.map((p) => {
    const change = p.value - p.start;
    return {
      name: p.name,
      start: fmt(p.start),
      now: fmt(Math.round(p.value * 10) / 10),
      change,
      percent: p.start ? (100 * change) / p.start : 0,
      range: p.min != null && p.max != null ? `${p.min}–${p.max}` : null,
    };
  }),
);
const columns = [
  { key: "name", label: "Constant" },
  { key: "start", label: "Start", fig: true },
  { key: "now", label: "Now", fig: true },
  { key: "change", label: "Change", fig: true },
  { key: "range", label: "Range", fig: true, hide: "sm" as const },
];
function toggle(row: { name: string }) {
  picked.value = picked.value === row.name ? null : row.name;
}
</script>

<template>
  <div class="tune">
    <UiMeter
      label="Iterations"
      :value="done"
      :max="total"
      :text="`${formatNumber(done)} of ${formatNumber(total)}`"
      :tone="running ? 'info' : undefined"
    />
    <p v-if="running && total > done" class="meta">{{ tuneCostText(cost) }}</p>
    <UiEmpty
      v-if="!steps.length"
      dense
      title="No iteration has finished yet."
      body="The trajectory appears with the first."
    />
    <LineChart
      v-else
      label="How far each constant has moved from its start, as a share of its range"
      :x="x"
      :series="ordered"
      :height="150"
      :lines="[{ value: 0, color: '--e-border-strong', dash: [] }]"
      :x-format="(v) => String(Math.round(v))"
      :format="(v) => signed(v, 0) + '%'"
    />
    <UiTable
      label="The constants being tuned"
      :columns="columns"
      :rows="rows"
      row-key="name"
      :selected="picked ?? undefined"
      @select="toggle"
    >
      <template #cell-name="{ row }">
        <span class="mono">{{ row.name }}</span>
      </template>
      <template #cell-change="{ row }">
        {{ signed(row.change, 1) }} <span class="muted">{{ signed(row.percent, 0) }}%</span>
      </template>
    </UiTable>
    <p class="meta">
      <Term k="tune" /> moves each constant a little after every few games. The trajectory can wander on noise, so only
      the confirmation trial decides. Pick a constant to draw its line.
    </p>
  </div>
</template>

<style scoped>
.tune {
  display: grid;
  gap: var(--sp-3);
  min-width: 0;
}

.meta {
  margin: 0;
  color: var(--c-ink-2);
  font: var(--t-meta);
}

.muted {
  color: var(--c-ink-2);
  font: var(--t-meta);
}
</style>
