<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { shortDate } from "../format";
import type { ChartSeries, ReferenceLine } from "./chart";

/**
 * A small line chart for the sheets: a profile's trend, Dameless's strength, an idea's evidence and a tune's
 * trajectory. One or two vertical scales, steps or lines or bare points, an optional band around a line, reference
 * lines, and a hover that reports which point the pointer is over. It draws on a canvas with the page's tokens and
 * draws again when the colour scheme changes.
 */
const props = withDefaults(
  defineProps<{
    /** What it shows, for screen readers. */
    label: string;
    x: number[];
    series: ChartSeries[];
    height?: number;
    /** The x values are Unix seconds. */
    time?: boolean;
    /** Fixed bounds for the left and right scales; `null` lets the data decide. */
    range?: [number | null, number | null];
    rightRange?: [number | null, number | null];
    /** How the vertical axes write a value. */
    format?: (v: number) => string;
    rightFormat?: (v: number) => string;
    /** How the horizontal axis writes a value, for charts that are not time. */
    xFormat?: (v: number) => string;
    lines?: ReferenceLine[];
  }>(),
  { height: 150 },
);
const emit = defineEmits<{ point: [index: number | null] }>();

const el = ref<HTMLElement>();
let plot: uPlot | undefined;
let observer: ResizeObserver | undefined;
let schemeObserver: MutationObserver | undefined;

/** The canvas cannot read `var(--x)`, so the tokens are read when it draws. */
function css(name: string, fallback = "#888"): string {
  const raw = el.value ? getComputedStyle(el.value).getPropertyValue(name).trim() : "";
  return raw || fallback;
}

/** The columns to draw. Times are made strictly increasing (games recorded at the same second a second apart), as
 * uPlot needs ascending x and a scale cannot be fitted to a single instant. */
function data(): uPlot.AlignedData {
  let x = props.x;
  if (props.time) {
    x = [];
    for (const t of props.x) x.push(x.length && t <= x[x.length - 1]! ? x[x.length - 1]! + 1 : t);
  }
  return [x, ...props.series.map((s) => s.values)] as uPlot.AlignedData;
}

/** A scale a little wider than the data; a flat line gets room around it (2 %, at least 1), not a range of ±1. */
function scaleRange(r: [number | null, number | null] | undefined): uPlot.Scale.Range {
  return (_u, min, max) => {
    const pad = (max - min) * 0.08 || Math.max(Math.abs(max) * 0.02, 1);
    return [r?.[0] ?? min - pad, r?.[1] ?? max + pad];
  };
}

/** The time axis spans at least a day, so games played at the same moment do not stretch it over years. */
const timeRange: uPlot.Scale.Range = (_u, min, max) => (max - min < 3600 ? [min - 43_200, max + 43_200] : [min, max]);

/** Time ticks as the app writes them: `14:30` within two days, else `12 Oct` (with the year when not this one). */
function timeValues(u: uPlot, ticks: number[]): string[] {
  const span = (u.scales["x"]?.max ?? 0) - (u.scales["x"]?.min ?? 0);
  return ticks.map((t) => {
    const d = new Date(t * 1000);
    return span < 2 * 86_400
      ? d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" })
      : shortDate(d.toISOString());
  });
}

/**
 * The width a vertical axis reserves: its widest label and the gap, measured when it draws, so a label like "+100%"
 * never reaches into the plot.
 */
function axisWidth(format: (v: number) => string): uPlot.Axis.Size {
  return (u, values) => {
    const pxRatio = devicePixelRatio || 1;
    const labels = values?.length ? values : [format(0)];
    u.ctx.save();
    u.ctx.font = `11px ${css("--e-font-mono", "monospace")}`;
    const widest = Math.max(...labels.map((v) => u.ctx.measureText(String(v)).width / pxRatio));
    u.ctx.restore();
    return Math.ceil(Math.max(widest + 4 + 6, 28));
  };
}

function options(width: number): uPlot.Options {
  const mono = css("--e-font-mono", "monospace");
  const font = `11px ${mono}`;
  const muted = css("--e-muted");
  const grid = { stroke: css("--e-border", "#ccc"), width: 1, dash: [] as number[] };
  const hasRight = props.series.some((s) => s.right);
  const fmt = props.format ?? ((v: number) => String(Math.round(v * 10) / 10));
  const rfmt = props.rightFormat ?? fmt;
  const axisBase = { font, stroke: muted, ticks: { show: false }, gap: 4, grid: { ...grid, show: true } };
  return {
    width,
    height: props.height,
    padding: [8, hasRight ? 4 : 16, 0, 0],
    legend: { show: false },
    cursor: { y: false, points: { size: 7, width: 1 }, drag: { x: false, y: false, setScale: false } },
    select: { show: false, left: 0, top: 0, width: 0, height: 0 },
    scales: {
      x: props.time ? { time: true, range: timeRange } : { time: false },
      y: { range: scaleRange(props.range) },
      y2: { range: scaleRange(props.rightRange) },
    },
    axes: [
      {
        ...axisBase,
        grid: { show: false },
        ...(props.time
          ? { values: timeValues }
          : props.xFormat
            ? { values: (_u: uPlot, ticks: number[]) => ticks.map((t) => props.xFormat!(t)) }
            : {}),
        size: 24,
      },
      { ...axisBase, size: axisWidth(fmt), values: (_u, ticks) => ticks.map((t) => fmt(t)) },
      ...(hasRight
        ? [
            {
              ...axisBase,
              scale: "y2",
              side: 1 as const,
              size: axisWidth(rfmt),
              grid: { show: false },
              values: (_u: uPlot, ticks: number[]) => ticks.map((t) => rfmt(t)),
            },
          ]
        : []),
    ],
    series: [
      {},
      ...props.series.map((s): uPlot.Series => ({
        label: s.label,
        scale: s.right ? "y2" : "y",
        stroke: css(s.color),
        width: s.kind === "points" ? 0 : (s.width ?? 1.5),
        dash: s.dash,
        spanGaps: true,
        paths: s.kind === "points" ? () => null : s.kind === "step" ? uPlot.paths.stepped!({ align: 1 }) : undefined,
        points: {
          show: s.kind === "points",
          size: 4,
          width: 0,
          fill: css(s.color),
          stroke: css(s.color),
        },
      })),
    ],
    hooks: {
      drawAxes: [
        (u) => {
          const { ctx } = u;
          ctx.save();
          ctx.beginPath();
          ctx.rect(u.bbox.left, u.bbox.top, u.bbox.width, u.bbox.height);
          ctx.clip();
          for (const [k, s] of props.series.entries()) {
            if (!s.band) continue;
            const scale = s.right ? "y2" : "y";
            const idx = props.x.map((_, i) => i).filter((i) => s.band!.low[i] != null && s.band!.high[i] != null);
            if (idx.length < 2) continue;
            ctx.beginPath();
            for (const [n, i] of idx.entries()) {
              const px = u.valToPos(props.x[i]!, "x", true);
              const py = u.valToPos(s.band.high[i]!, scale, true);
              if (n === 0) ctx.moveTo(px, py);
              else ctx.lineTo(px, py);
            }
            for (const i of [...idx].reverse())
              ctx.lineTo(u.valToPos(props.x[i]!, "x", true), u.valToPos(s.band.low[i]!, scale, true));
            ctx.closePath();
            ctx.fillStyle = css(s.color);
            ctx.globalAlpha = 0.16;
            ctx.fill();
            void k;
          }
          ctx.restore();
        },
      ],
      draw: [
        (u) => {
          const { ctx } = u;
          ctx.save();
          ctx.beginPath();
          ctx.rect(u.bbox.left, u.bbox.top, u.bbox.width, u.bbox.height);
          ctx.clip();
          for (const line of props.lines ?? []) {
            const y = Math.round(u.valToPos(line.value, "y", true)) + 0.5;
            ctx.strokeStyle = css(line.color);
            ctx.lineWidth = devicePixelRatio;
            ctx.setLineDash((line.dash ?? [4, 4]).map((d) => d * devicePixelRatio));
            ctx.beginPath();
            ctx.moveTo(u.bbox.left, y);
            ctx.lineTo(u.bbox.left + u.bbox.width, y);
            ctx.stroke();
          }
          ctx.restore();
        },
      ],
      setCursor: [(u) => emit("point", u.cursor.idx ?? null)],
    },
  };
}

function build() {
  plot?.destroy();
  plot = undefined;
  const target = el.value;
  if (!target || props.x.length === 0) return;
  plot = new uPlot(options(target.clientWidth || 320), data(), target);
}

onMounted(() => {
  build();
  observer = new ResizeObserver(() => {
    const w = el.value?.clientWidth;
    if (w && plot && Math.abs(plot.width - w) > 1) plot.setSize({ width: w, height: props.height });
  });
  observer.observe(el.value!);
  schemeObserver = new MutationObserver(build);
  schemeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-scheme"] });
});
watch(() => [props.x, props.series, props.lines, props.height], build, { deep: true });
onBeforeUnmount(() => {
  observer?.disconnect();
  schemeObserver?.disconnect();
  plot?.destroy();
});
</script>

<template>
  <figure class="chart" role="img" :aria-label="label">
    <div ref="el" class="chart-plot" :style="{ minHeight: `${height}px` }" @pointerleave="emit('point', null)"></div>
    <figcaption class="chart-caption"><slot /></figcaption>
  </figure>
</template>

<style scoped>
.chart {
  margin: 0;
  min-width: 0;
}

/* The canvas keeps the width it was drawn at until the observer redraws it; clipping lets the box shrink first, so a
   narrowing window never widens the sheet. */
.chart-plot {
  min-width: 0;
  overflow-x: clip;
}

.chart-plot :deep(.u-over) {
  touch-action: pan-y;
}

/* The numbers a screen reader would want are given by the caller in the caption; sighted readers have the hover. */
.chart-caption:empty {
  display: none;
}

.chart-caption {
  margin-block-start: 0.25rem;
  color: var(--e-muted);
  font: var(--e-type-small);
}
</style>
