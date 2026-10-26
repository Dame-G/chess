<script setup lang="ts">
import { computed, ref } from "vue";
import type { TrendPoint } from "../api/types";
import { percent, shortDate } from "../format";
import { trendSeries } from "../sheets/trend";
import type { ChartSeries } from "./chart";
import LineChart from "./LineChart.vue";

/**
 * A player's accuracy and rating over their games: each reviewed game's accuracy as a dot, the average of the last
 * ten as a line, the rating on the right. Hovering reads out one game.
 */
const props = defineProps<{ points: TrendPoint[]; label: string }>();

const data = computed(() => trendSeries(props.points));
const hasAccuracy = computed(() => data.value.accuracy.some((a) => a !== null));
const hasRating = computed(() => data.value.rating.some((r) => r !== null));

const series = computed<ChartSeries[]>(() => [
  ...(hasAccuracy.value
    ? [
        { label: "Accuracy", values: data.value.accuracy, color: "--e-muted", kind: "points" as const },
        { label: "Average of 10 games", values: data.value.average, color: "--e-ink", width: 2 },
      ]
    : []),
  ...(hasRating.value
    ? [{ label: "Rating", values: data.value.rating, color: "--e-primary", right: hasAccuracy.value, width: 1.5 }]
    : []),
]);

const hover = ref<number | null>(null);
const readout = computed(() => {
  const i = hover.value ?? props.points.length - 1;
  const p = props.points[i];
  if (!p) return "";
  const result = p.points === 1 ? "won" : p.points === 0.5 ? "drawn" : p.points === 0 ? "lost" : "";
  return [
    shortDate(p.played_at),
    p.accuracy !== null ? `accuracy ${percent(p.accuracy)}` : "not reviewed",
    p.rating !== null ? `rating ${p.rating}` : "",
    result,
  ]
    .filter(Boolean)
    .join(" · ");
});
</script>

<template>
  <div class="trend">
    <p class="key" aria-hidden="true">
      <span v-if="hasAccuracy" class="k k-dot">Accuracy per game</span>
      <span v-if="hasAccuracy" class="k k-ink">Average of 10</span>
      <span v-if="hasRating" class="k k-rate">Rating</span>
    </p>
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
    >
      <span class="mono readout">{{ readout }}</span>
    </LineChart>
  </div>
</template>

<style scoped>
.trend {
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
  margin-inline-end: 0.375rem;
  vertical-align: middle;
  content: "";
}

.k-dot::before {
  inline-size: 0.375rem;
  block-size: 0.375rem;
  border-radius: 50%;
  background: var(--e-muted);
}

.k-ink::before,
.k-rate::before {
  inline-size: 0.875rem;
  block-size: 2px;
}

.k-ink::before {
  background: var(--e-ink);
}

.k-rate::before {
  background: var(--e-primary);
}

.readout {
  font-size: 0.8125rem;
}
</style>
