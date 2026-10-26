<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

/**
 * Every parameter's path over the iterations, one thin line each, all measured as a share of the parameter's own range
 * so that constants of any size share one chart. The chosen line is drawn thick on top; the line under the pointer is
 * reported, with its real value, to whoever listens to `hover`.
 */
const props = defineProps<{
  label: string;
  /** Iteration numbers: 0 is the start. */
  x: number[];
  /** One array per parameter, each the same length as `x`. */
  series: number[][];
  /** The chosen parameter's index. */
  selected?: number;
  height?: number;
}>();
const emit = defineEmits<{
  point: [point: { series: number; index: number } | null];
  select: [series: number];
}>();

const el = ref<HTMLElement>();
let plot: uPlot | undefined;
let observer: ResizeObserver | undefined;
let focused = -1;
let built = 0;

function token(name: string, fallback: string) {
  return getComputedStyle(el.value!).getPropertyValue(`--e-${name}`).trim() || fallback;
}

/** A hex colour at an opacity: canvas has no colour-mix. */
function alpha(color: string, a: number) {
  return /^#[0-9a-f]{6}$/i.test(color)
    ? `${color}${Math.round(a * 255)
        .toString(16)
        .padStart(2, "0")}`
    : color;
}

function options(width: number): uPlot.Options {
  const muted = token("muted", "#888");
  const font = `11px ${token("font-mono", "monospace")}`;
  const grid = alpha(token("border", "#888888"), 0.35);
  const axis = { stroke: muted, font, grid: { stroke: grid, width: 1 }, ticks: { show: false } };
  const line = alpha(token("ink", "#888888"), 0.4);
  const accent = token("primary", "#36c");
  return {
    width,
    height: props.height ?? 260,
    legend: { show: false },
    cursor: { y: false, points: { show: false }, focus: { prox: 24 } },
    focus: { alpha: 0.35 },
    scales: { x: { time: false } },
    axes: [
      { ...axis, label: "Iteration", labelFont: font, labelSize: 18 },
      { ...axis, size: 48, values: (_u, ticks) => ticks.map((v) => `${v > 0 ? "+" : ""}${v}%`) },
    ],
    series: [
      {},
      ...props.series.map((_, i) => ({ label: String(i), stroke: line, width: 1, points: { show: false } })),
    ],
    hooks: {
      ready: [
        (u) => {
          u.over.addEventListener("click", () => {
            if (focused > 0) emit("select", focused - 1);
          });
        },
      ],
      setSeries: [
        (_u, idx, opts) => {
          if (opts && "focus" in opts) {
            focused = opts.focus ? (idx ?? -1) : -1;
            report(_u);
          }
        },
      ],
      setCursor: [(u) => report(u)],
      draw: [
        (u) => {
          const i = props.selected;
          const ys = i == null ? undefined : props.series[i];
          if (!ys) return;
          const ctx = u.ctx;
          ctx.save();
          ctx.strokeStyle = accent;
          ctx.lineWidth = 2.5 * devicePixelRatio;
          ctx.lineJoin = "round";
          ctx.beginPath();
          props.x.forEach((xv, k) => {
            const px = u.valToPos(xv, "x", true);
            const py = u.valToPos(ys[k]!, "y", true);
            if (k) ctx.lineTo(px, py);
            else ctx.moveTo(px, py);
          });
          ctx.stroke();
          ctx.restore();
        },
      ],
    },
  };
}

function report(u: uPlot) {
  const index = u.cursor.idx;
  emit("point", index != null && focused > 0 ? { series: focused - 1, index } : null);
}

const data = (): uPlot.AlignedData => [props.x, ...props.series] as uPlot.AlignedData;

function build() {
  plot?.destroy();
  if (!el.value) return;
  built = props.series.length;
  plot = new uPlot(options(el.value.clientWidth), data(), el.value);
}

onMounted(() => {
  build();
  observer = new ResizeObserver(() => plot?.setSize({ width: el.value!.clientWidth, height: props.height ?? 260 }));
  observer.observe(el.value!);
});

watch(
  () => [props.x, props.series],
  () => {
    if (plot && built === props.series.length) plot.setData(data());
    else build();
  },
);
watch(
  () => props.selected,
  () => plot?.redraw(false),
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
  font: var(--e-type-small);
}

.chart :deep(.u-over) {
  cursor: crosshair;
  touch-action: none;
}
</style>
