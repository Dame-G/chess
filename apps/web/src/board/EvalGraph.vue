<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

/**
 * The game as a line: White's edge at each position (−1 Black winning, 1 White winning), the move on the board as a
 * marker, judged moves as dots. Click or drag along it to go to a move; it is the transport's timeline.
 */
const props = defineProps<{
  /** What it is, for screen readers. */
  label: string;
  /** White's edge at each position from the start, `null` where nothing is known. */
  values: (number | null)[];
  /** The position on the board, as an index into `values`. */
  cursor: number;
  /** Judged moves: the colour token of each position's mark (`--rv-blunder`), `null` for the rest. */
  marks?: (string | null)[];
  /** What to say about each position while the pointer is over it, such as `17. Rd8#  +8.60`. */
  tips?: string[];
}>();
const emit = defineEmits<{ select: [index: number] }>();

const el = ref<HTMLElement>();
let plot: uPlot | undefined;
let observer: ResizeObserver | undefined;
let dragging = false;
/** The position under the pointer and its x in the plot, for the hover line and its readout. */
const hover = ref<{ index: number; x: number } | null>(null);
const tip = computed(() => (hover.value ? (props.tips?.[hover.value.index] ?? "") : ""));
/** The readout sits on the side of the line with room: left of it past the middle. */
const tipLeft = computed(() => !!hover.value && !!el.value && hover.value.x > el.value.clientWidth / 2);

/** uPlot draws on a canvas, so colours are read from the page's tokens when it draws. */
function css(name: string, fallback: string) {
  return el.value ? getComputedStyle(el.value).getPropertyValue(name).trim() || fallback : fallback;
}

function data(): uPlot.AlignedData {
  const n = Math.max(2, props.values.length);
  const x = Array.from({ length: n }, (_, i) => i);
  const y = Array.from({ length: n }, (_, i) => props.values[i] ?? null);
  return [x, y];
}

/** The plot is as tall as the slot the stage holds for it. */
const slot = () => el.value?.clientHeight || 56;

function options(width: number): uPlot.Options {
  return {
    width,
    height: slot(),
    padding: [2, 0, 2, 0],
    legend: { show: false },
    cursor: { x: false, y: false, points: { show: false }, drag: { x: false, y: false } },
    select: { show: false, left: 0, top: 0, width: 0, height: 0 },
    scales: { x: { time: false }, y: { range: [-1, 1] } },
    axes: [{ show: false }, { show: false }],
    series: [
      {},
      {
        stroke: css("--graph-line", "#888"),
        width: 1.25,
        spanGaps: true,
        points: { show: false },
      },
    ],
    hooks: {
      draw: [
        (u) => {
          const ctx = u.ctx;
          const { left, top, width, height } = u.bbox;
          const mid = Math.round(u.valToPos(0, "y", true)) + 0.5;
          ctx.save();
          // The edge as an area between the line and the middle: White's above it, Black's below, one tint.
          const known = props.values.flatMap((v, i) =>
            v == null ? [] : [[u.valToPos(i, "x", true), u.valToPos(v, "y", true)] as const],
          );
          if (known.length > 1) {
            ctx.fillStyle = css("--graph-fill", "rgba(0,0,0,0.14)");
            ctx.beginPath();
            ctx.moveTo(known[0]![0], mid);
            for (const [x, y] of known) ctx.lineTo(x, y);
            ctx.lineTo(known[known.length - 1]![0], mid);
            ctx.closePath();
            ctx.fill();
          }
          ctx.strokeStyle = css("--graph-mid", "#999");
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(left, mid);
          ctx.lineTo(left + width, mid);
          ctx.stroke();
          for (const [i, tone] of (props.marks ?? []).entries()) {
            const v = props.values[i];
            if (!tone || v == null) continue;
            ctx.fillStyle = css(tone, "#c00");
            ctx.beginPath();
            ctx.arc(u.valToPos(i, "x", true), u.valToPos(v, "y", true), 3 * devicePixelRatio, 0, Math.PI * 2);
            ctx.fill();
          }
          const x = Math.round(u.valToPos(props.cursor, "x", true));
          ctx.strokeStyle = css("--graph-cursor", "#b5652a");
          ctx.lineWidth = 2 * devicePixelRatio;
          ctx.beginPath();
          ctx.moveTo(x, top);
          ctx.lineTo(x, top + height);
          ctx.stroke();
          ctx.restore();
        },
      ],
      ready: [
        (u) => {
          const at = (e: PointerEvent) => {
            const box = u.over.getBoundingClientRect();
            const i = Math.round(u.posToVal(e.clientX - box.left, "x"));
            return Math.max(0, Math.min(props.values.length - 1, i));
          };
          u.over.addEventListener("pointerdown", (e) => {
            dragging = true;
            u.over.setPointerCapture(e.pointerId);
            emit("select", at(e));
          });
          u.over.addEventListener("pointermove", (e) => {
            hover.value = { index: at(e), x: e.clientX - u.over.getBoundingClientRect().left };
            if (dragging) emit("select", at(e));
          });
          u.over.addEventListener("pointerleave", () => {
            if (!dragging) hover.value = null;
          });
          u.over.addEventListener("pointerup", (e) => {
            dragging = false;
            if (e.pointerType !== "mouse") hover.value = null;
          });
          u.over.addEventListener("pointercancel", () => {
            dragging = false;
            hover.value = null;
          });
        },
      ],
    },
  };
}

function build() {
  plot?.destroy();
  if (!el.value) return;
  plot = new uPlot(options(el.value.clientWidth || 300), data(), el.value);
}

onMounted(() => {
  build();
  observer = new ResizeObserver(() => {
    if (el.value?.clientWidth) plot?.setSize({ width: el.value.clientWidth, height: slot() });
  });
  observer.observe(el.value!);
});
watch(
  () => props.values,
  () => plot?.setData(data()),
  { deep: true },
);
watch(
  () => [props.cursor, props.marks],
  () => plot?.redraw(false),
);
// The scheme changes the tokens the canvas was painted with.
const schemeObserver = new MutationObserver(() => build());
onMounted(() =>
  schemeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-scheme"] }),
);
onBeforeUnmount(() => {
  schemeObserver.disconnect();
  observer?.disconnect();
  plot?.destroy();
});
</script>

<template>
  <figure class="graph" :aria-label="label">
    <div ref="el" class="graph-plot"></div>
    <template v-if="hover">
      <span class="graph-line" :style="{ left: `${hover.x}px` }" aria-hidden="true"></span>
      <span
        v-if="tip"
        class="graph-tip num"
        :class="{ 'graph-tip--left': tipLeft }"
        :style="{ left: `${hover.x}px` }"
        aria-hidden="true"
        >{{ tip }}</span
      >
    </template>
  </figure>
</template>

<style scoped>
/* The graph is neutral: a line and a tint in the ink, with no black and white halves. Its colours are tokens a canvas
   reads when it draws (see css() above). */
.graph {
  --graph-line: var(--c-ink-2);
  --graph-fill: color-mix(in oklab, var(--e-ink) 14%, transparent);
  --graph-mid: var(--e-border);
  --graph-cursor: var(--e-primary);

  position: relative;
  box-sizing: border-box;
  height: 100%;
  margin: 0;
  min-width: 0;
  overflow: hidden;
  border-radius: var(--r-1);
  background: var(--c-pane);
  box-shadow: 0 0 0 var(--e-stroke-hair) var(--c-line);
}

/* The hover line follows the pointer; the readout hangs from the top edge beside it. */
.graph-line {
  position: absolute;
  inset-block: 0;
  width: var(--e-stroke-hair);
  background: var(--c-ink-2);
  opacity: 0.8;
  pointer-events: none;
}

.graph-tip {
  position: absolute;
  top: var(--sp-1);
  margin-inline-start: var(--sp-2);
  padding: var(--sp-1) var(--sp-2);
  border-radius: var(--r-1);
  background: var(--c-ink);
  color: var(--c-pane);
  font: var(--t-meta);
  font-family: var(--e-font-mono);
  pointer-events: none;
  white-space: nowrap;
}

.graph-tip--left {
  margin-inline-start: 0;
  transform: translateX(calc(-100% - var(--sp-2)));
}

.graph-plot {
  height: 100%;
}

.graph-plot :deep(.u-over) {
  cursor: pointer;
  touch-action: none;
}
</style>
