<script setup lang="ts">
import { computed, ref } from "vue";
import type { Metric, ProgressChart } from "../api/types";
import { shortDate } from "../format";
import { latest, progressData } from "../sheets/progress";
import type { ChartSeries } from "./chart";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSeg from "../ui/UiSeg.vue";
import ChartKey from "./ChartKey.vue";
import LineChart from "./LineChart.vue";

/**
 * Progress over time: Rating shows your Lichess rating; Accuracy shows yours and your drill hit rate.
 */
const props = defineProps<{ chart: ProgressChart }>();

const view = ref<"rating" | "accuracy">("rating");
const metrics = computed<Metric[]>(() => (view.value === "rating" ? ["rating"] : ["accuracy", "drill_hit_rate"]));
const data = computed(() => progressData(props.chart, metrics.value));
const hasAccuracy = computed(() => progressData(props.chart, ["accuracy", "drill_hit_rate"]).lines.length > 0);

const COLOR: Record<string, string> = {
  rating: "--c-info",
  accuracy: "--c-info",
  drill_hit_rate: "--c-ink-2",
};
const items = computed(() =>
  data.value.lines.map((l) => ({
    label: l.label,
    shape: "line" as const,
    color: COLOR[l.metric]!,
  })),
);
const series = computed<ChartSeries[]>(() =>
  data.value.lines.map((l) => ({
    label: l.label,
    values: l.values,
    color: COLOR[l.metric]!,
    kind: "line" as const,
    width: 1.5,
    dash: l.metric === "drill_hit_rate" ? [4, 3] : undefined,
  })),
);

const hover = ref<number | null>(null);
const readout = computed(() => {
  const i = hover.value;
  const parts = data.value.lines.map((l) => {
    const v = i == null ? latest(l) : l.values[i];
    if (v == null) return "";
    return `${l.label} ${Math.round(v)}`;
  });
  const when = i == null ? "" : shortDate(new Date(data.value.x[i]! * 1000).toISOString());
  return [when, ...parts].filter(Boolean).join(" · ");
});
const label = computed(() => `Your ${view.value === "rating" ? "rating" : "accuracy"} over time`);
</script>

<template>
  <div v-if="data.lines.length && data.x.length > 1" class="progress">
    <UiSeg
      v-if="hasAccuracy"
      v-model="view"
      label="Chart"
      size="sm"
      :options="[
        { value: 'rating', label: 'Rating' },
        { value: 'accuracy', label: 'Accuracy' },
      ]"
    />
    <ChartKey :items="items" :readout="hover == null ? undefined : readout" />
    <LineChart
      :label="label"
      :x="data.x"
      :series="series"
      time
      :height="170"
      :range="view === 'accuracy' ? [null, 100] : [null, null]"
      :format="(v) => String(Math.round(v))"
      @point="hover = $event"
    />
  </div>
  <UiEmpty v-else dense title="The lines start with your first rated or reviewed game." />
</template>

<style scoped>
.progress {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}
</style>
