<script setup lang="ts">
import { computed, ref } from "vue";
import { ESegmented } from "@eos-ui/vue/components/ESegmented";
import type { Metric, ProgressChart } from "../api/types";
import { shortDate, signed } from "../format";
import { latest, progressData } from "../sheets/progress";
import type { ChartSeries } from "./chart";
import LineChart from "./LineChart.vue";

/**
 * Progress: the two players on one time axis, side by side. Rating shows your Lichess rating and Dameless's Elo (on
 * the same scale once the levels anchor it, else as Elo over v1 on the right); Accuracy shows yours and your drill hit
 * rate. `compact` is the small version for the Dameless sheet: rating only, no switch.
 */
const props = defineProps<{ chart: ProgressChart; compact?: boolean }>();

const view = ref<"rating" | "accuracy">("rating");
const metrics = computed<Metric[]>(() =>
  props.compact || view.value === "rating" ? ["rating", "elo"] : ["accuracy", "drill_hit_rate"],
);
const data = computed(() => progressData(props.chart, metrics.value));
const hasAccuracy = computed(() => progressData(props.chart, ["accuracy", "drill_hit_rate"]).lines.length > 0);

const COLOR: Record<string, string> = {
  rating: "--e-primary",
  accuracy: "--e-primary",
  drill_hit_rate: "--e-muted",
  elo: "--e-ink",
};
const series = computed<ChartSeries[]>(() =>
  data.value.lines.map((l) => ({
    label: l.label,
    values: l.values,
    color: COLOR[l.metric]!,
    right: l.scale === "elo",
    kind: "line" as const,
    width: l.metric === "elo" ? 2 : 1.5,
    dash: l.metric === "drill_hit_rate" ? [4, 3] : undefined,
  })),
);
const hasRight = computed(() => data.value.lines.some((l) => l.scale === "elo"));

const hover = ref<number | null>(null);
const readout = computed(() => {
  const i = hover.value;
  const parts = data.value.lines.map((l) => {
    const v = i == null ? latest(l) : l.values[i];
    if (v == null) return "";
    return `${l.label} ${l.metric === "elo" && l.scale === "elo" ? `${signed(v, 0)} over v1` : Math.round(v)}`;
  });
  const when = i == null ? "" : shortDate(new Date(data.value.x[i]! * 1000).toISOString());
  return [when, ...parts].filter(Boolean).join(" · ");
});
const label = computed(
  () => `${props.compact || view.value === "rating" ? "Ratings" : "Accuracy"} of you and Dameless over time`,
);
</script>

<template>
  <div v-if="data.lines.length && data.x.length > 1" class="progress">
    <ESegmented
      v-if="!compact && hasAccuracy"
      v-model="view"
      label="Chart"
      hide-label
      size="sm"
      :options="[
        { value: 'rating', label: 'Rating' },
        { value: 'accuracy', label: 'Accuracy' },
      ]"
    />
    <p class="key" aria-hidden="true">
      <span v-for="l in data.lines" :key="l.metric" class="k" :class="`k-${l.metric}`">{{ l.label }}</span>
    </p>
    <LineChart
      :label="label"
      :x="data.x"
      :series="series"
      time
      :height="compact ? 130 : 170"
      :range="view === 'accuracy' && !compact ? [null, 100] : [null, null]"
      :format="(v) => String(Math.round(v))"
      :right-format="hasRight ? (v) => signed(v, 0) : undefined"
      @point="hover = $event"
    >
      <span class="mono readout">{{ readout }}</span>
    </LineChart>
  </div>
  <p v-else class="sec-note">The lines start with your first rated or reviewed game, and Dameless's second champion.</p>
</template>

<style scoped>
.progress {
  display: grid;
  gap: var(--e-gap-tight);
  min-width: 0;
}

.key {
  display: flex;
  flex-wrap: wrap;
  gap: 0 var(--e-gap-base);
  margin: 0;
  color: var(--e-muted);
  font: var(--e-type-small);
}

.k::before {
  display: inline-block;
  inline-size: 0.875rem;
  block-size: 2px;
  margin-inline-end: 0.375rem;
  vertical-align: middle;
  background: var(--e-ink);
  content: "";
}

.k-rating::before,
.k-accuracy::before {
  background: var(--e-primary);
}

.k-drill_hit_rate::before {
  background: var(--e-muted);
}

.readout {
  font-size: 0.8125rem;
}
</style>
