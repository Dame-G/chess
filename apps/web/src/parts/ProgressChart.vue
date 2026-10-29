<script setup lang="ts">
import { computed, ref } from "vue";
import type { Metric, ProgressChart } from "../api/types";
import { shortDate, signed } from "../format";
import { latest, progressData } from "../sheets/progress";
import type { ChartSeries } from "./chart";
import UiEmpty from "../ui/UiEmpty.vue";
import UiSeg from "../ui/UiSeg.vue";
import ChartKey from "./ChartKey.vue";
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
  rating: "--c-info",
  accuracy: "--c-info",
  drill_hit_rate: "--c-ink-2",
  elo: "--c-ink",
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
    <UiSeg
      v-if="!compact && hasAccuracy"
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
      :height="compact ? 130 : 170"
      :range="view === 'accuracy' && !compact ? [null, 100] : [null, null]"
      :format="(v) => String(Math.round(v))"
      :right-format="hasRight ? (v) => signed(v, 0) : undefined"
      @point="hover = $event"
    />
  </div>
  <UiEmpty
    v-else
    dense
    title="The lines start with your first rated or reviewed game, and Dameless's second champion."
  />
</template>

<style scoped>
.progress {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}
</style>
