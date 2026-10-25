<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

interface Series {
  label: string;
  /** One value per x, `null` for a gap. */
  values: (number | null)[];
  /** A colour token name without `--e-`, such as `primary`. */
  tone?: string;
}

const props = defineProps<{
  /** What the chart shows, for screen readers. */
  label: string;
  x: number[];
  series: Series[];
  /** Horizontal reference lines, such as SPRT bounds. */
  lines?: { value: number; label: string }[];
  height?: number;
}>();

const el = ref<HTMLElement>();
let plot: uPlot | undefined;
let observer: ResizeObserver | undefined;

/** uPlot draws on a canvas, so the eos colours are read from the page's tokens when the chart is built. */
function token(name: string, fallback: string) {
  return getComputedStyle(el.value!).getPropertyValue(`--e-${name}`).trim() || fallback;
}

function options(width: number): uPlot.Options {
  const ink = token("muted", "#888");
  const grid = token("border", "rgba(128,128,128,0.2)");
  const axis = { stroke: ink, grid: { stroke: grid, width: 1 }, ticks: { stroke: grid, width: 1 } };
  const refLines = props.lines ?? [];
  return {
    width,
    height: props.height ?? 220,
    legend: { show: true },
    cursor: { y: false },
    scales: { x: { time: false } },
    axes: [axis, axis],
    series: [
      { label: "games" },
      ...props.series.map((s) => ({ label: s.label, stroke: token(s.tone ?? "primary", "#36c"), width: 2 })),
      ...refLines.map((l) => ({ label: l.label, stroke: ink, width: 1, dash: [4, 4], points: { show: false } })),
    ],
  };
}

function data(): uPlot.AlignedData {
  const refLines = (props.lines ?? []).map((l) => props.x.map(() => l.value));
  return [props.x, ...props.series.map((s) => s.values), ...refLines] as uPlot.AlignedData;
}

function build() {
  plot?.destroy();
  if (!el.value) return;
  plot = new uPlot(options(el.value.clientWidth), data(), el.value);
}

onMounted(() => {
  build();
  observer = new ResizeObserver(() => plot?.setSize({ width: el.value!.clientWidth, height: props.height ?? 220 }));
  observer.observe(el.value!);
});

// New points keep the chart; new series or lines rebuild it.
watch(
  () => [props.x, props.series.map((s) => s.values)],
  () => plot?.setData(data()),
);
watch(
  () => [props.series.length, props.lines?.map((l) => l.value).join()],
  () => build(),
);

onBeforeUnmount(() => {
  observer?.disconnect();
  plot?.destroy();
});
</script>

<template>
  <figure class="chart" :aria-label="label">
    <div ref="el"></div>
  </figure>
</template>

<style scoped>
.chart {
  margin: 0;
  min-width: 0;
  color: var(--e-muted);
}
</style>
