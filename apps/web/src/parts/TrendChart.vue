<script setup lang="ts">
import { computed, ref } from "vue";
import type { TrendPoint } from "../api/types";
import { percent, shortDate } from "../format";
import { trendSeries } from "../sheets/trend";
import type { ChartSeries } from "./chart";
import ChartKey from "./ChartKey.vue";
import LineChart from "./LineChart.vue";

/**
 * A player's accuracy and rating over their games: each reviewed game's accuracy as a dot, the average of the last
 * ten as a line, the rating on the right. Hovering reads out one game.
 */
const props = defineProps<{ points: TrendPoint[]; label: string }>();

const data = computed(() => trendSeries(props.points));
const hasAccuracy = computed(() => data.value.accuracy.some((a) => a != null));
const hasRating = computed(() => data.value.rating.some((r) => r != null));

const series = computed<ChartSeries[]>(() => [
  ...(hasAccuracy.value
    ? [
        { label: "Accuracy", values: data.value.accuracy, color: "--c-ink-2", kind: "points" as const },
        { label: "Average of 10 games", values: data.value.average, color: "--c-ink", width: 2 },
      ]
    : []),
  ...(hasRating.value
    ? [{ label: "Rating", values: data.value.rating, color: "--c-info", right: hasAccuracy.value, width: 1.5 }]
    : []),
]);

const items = computed(() => [
  ...(hasAccuracy.value
    ? [
        { label: "Accuracy per game", shape: "dot" as const, color: "--c-ink-2" },
        { label: "Average of 10", shape: "line" as const, color: "--c-ink" },
      ]
    : []),
  ...(hasRating.value ? [{ label: "Rating", shape: "line" as const, color: "--c-info" }] : []),
]);

const hover = ref<number | null>(null);
const readout = computed(() => {
  const i = hover.value ?? props.points.length - 1;
  const p = props.points[i];
  if (!p) return "";
  const result = p.points === 1 ? "won" : p.points === 0.5 ? "drawn" : p.points === 0 ? "lost" : "";
  return [
    shortDate(p.played_at),
    p.accuracy != null ? `accuracy ${percent(p.accuracy)}` : "not reviewed",
    p.rating != null ? `rating ${p.rating}` : "",
    result,
  ]
    .filter(Boolean)
    .join(" · ");
});
</script>

<template>
  <div class="trend">
    <ChartKey :items="items" :readout="hover == null ? undefined : readout" />
    <LineChart
      :label="label"
      :x="data.x"
      :series="series"
      time
      :height="170"
      :range="[null, hasAccuracy ? 100 : null]"
      :right-range="[null, null]"
      :format="(v) => String(Math.round(v))"
      @point="hover = $event"
    />
  </div>
</template>

<style scoped>
.trend {
  display: grid;
  gap: var(--sp-1);
  min-width: 0;
}
</style>
