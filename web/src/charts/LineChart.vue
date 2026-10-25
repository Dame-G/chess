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
  /** Shades the area between the line and zero. */
  fill?: boolean;
}

const props = defineProps<{
  /** What the chart shows, for screen readers. */
  label: string;
  x: number[];
  series: Series[];
  /** Horizontal reference lines, such as SPRT bounds. */
  lines?: { value: number; label: string }[];
  height?: number;
  /** A fixed range for the y axis. */
  range?: [number, number];
  /** Shows the legend under the chart. */
  legend?: boolean;
  /** Draws a vertical marker at this index of `x`, such as the move on the board. */
  marker?: number;
  /** The x axis's name. */
  xLabel?: string;
  /** A slim strip with no axes, legend or grid: a timeline. The part after the marker is dimmed, as unplayed. */
  bare?: boolean;
  /** With `bare`: dragging across the strip keeps selecting, not only a click. */
  scrub?: boolean;
}>();
const emit = defineEmits<{ select: [index: number] }>();

const el = ref<HTMLElement>();
let plot: uPlot | undefined;
let observer: ResizeObserver | undefined;

/** uPlot draws on a canvas, so the eos colours are read from the page's tokens when the chart is built. */
function token(name: string, fallback: string) {
  return getComputedStyle(el.value!).getPropertyValue(`--e-${name}`).trim() || fallback;
}

/** A hex colour at an opacity, for fills: canvas has no colour-mix. */
function alpha(color: string, a: number) {
  return /^#[0-9a-f]{6}$/i.test(color)
    ? `${color}${Math.round(a * 255)
        .toString(16)
        .padStart(2, "0")}`
    : color;
}

function options(width: number): uPlot.Options {
  const ink = token("muted", "#888");
  const grid = alpha(token("border", "#888888"), 0.35);
  const font = `11px ${token("font-mono", "monospace")}`;
  const axis = { stroke: ink, font, grid: { stroke: grid, width: 1 }, ticks: { show: false } };
  const refLines = props.lines ?? [];
  const markerColor = token("primary", "#36c");
  const bare = props.bare === true;
  return {
    width,
    height: props.height ?? 220,
    legend: { show: bare ? false : (props.legend ?? true) },
    cursor: bare ? { show: false, y: false, drag: { x: false, y: false } } : { y: false, points: { size: 6 } },
    padding: bare ? [2, 0, 2, 0] : undefined,
    scales: { x: { time: false }, ...(props.range ? { y: { range: props.range } } : {}) },
    axes: bare
      ? [{ show: false }, { show: false }]
      : [
          { ...axis, label: props.xLabel, labelFont: font, labelSize: props.xLabel ? 18 : 0 },
          { ...axis, size: 44 },
        ],
    series: [
      { label: props.xLabel ?? "x" },
      ...props.series.map((s) => {
        const color = token(s.tone ?? "primary", "#36c");
        return {
          label: s.label,
          stroke: color,
          width: bare ? 1.5 : 2,
          fill: s.fill ? alpha(color, 0.18) : undefined,
          fillTo: 0,
          spanGaps: bare,
        };
      }),
      ...refLines.map((l) => ({ label: l.label, stroke: ink, width: 1, dash: [4, 4], points: { show: false } })),
    ],
    hooks: {
      ready: [
        (u) => {
          if (!bare) {
            u.over.addEventListener("click", () => {
              if (u.cursor.idx != null) emit("select", u.cursor.idx);
            });
            return;
          }
          const at = (e: PointerEvent) => {
            const left = e.clientX - u.over.getBoundingClientRect().left;
            return Math.max(0, Math.min(props.x.length - 1, u.posToIdx(left)));
          };
          let down = false;
          u.over.addEventListener("pointerdown", (e) => {
            down = true;
            u.over.setPointerCapture(e.pointerId);
            emit("select", at(e));
          });
          u.over.addEventListener("pointermove", (e) => {
            if (down && props.scrub) emit("select", at(e));
          });
          const up = () => (down = false);
          u.over.addEventListener("pointerup", up);
          u.over.addEventListener("pointercancel", up);
        },
      ],
      draw: [
        (u) => {
          const ctx = u.ctx;
          const { left, top, width: w, height: h } = u.bbox;
          if (bare) {
            // The even line, under everything.
            const mid = Math.round(u.valToPos(0, "y", true));
            ctx.save();
            ctx.strokeStyle = ink;
            ctx.globalAlpha = 0.35;
            ctx.lineWidth = devicePixelRatio;
            ctx.beginPath();
            ctx.moveTo(left, mid);
            ctx.lineTo(left + w, mid);
            ctx.stroke();
            ctx.restore();
          }
          if (props.marker == null || props.x[props.marker] == null) return;
          const x = Math.round(u.valToPos(props.x[props.marker]!, "x", true));
          ctx.save();
          if (bare) {
            // What is not played yet is dimmed.
            ctx.globalAlpha = 0.55;
            ctx.fillStyle = token("surface", "#fff");
            ctx.fillRect(x, top, left + w - x, h);
            ctx.globalAlpha = 1;
          }
          ctx.strokeStyle = markerColor;
          ctx.lineWidth = 2 * devicePixelRatio;
          ctx.beginPath();
          ctx.moveTo(x, top);
          ctx.lineTo(x, top + h);
          ctx.stroke();
          ctx.restore();
        },
      ],
    },
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
watch(
  () => props.marker,
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
