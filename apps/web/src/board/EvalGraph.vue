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
  height?: number;
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

function options(width: number): uPlot.Options {
  const h = props.height ?? 56;
  return {
    width,
    height: h,
    padding: [2, 0, 2, 0],
    legend: { show: false },
    cursor: { x: false, y: false, points: { show: false }, drag: { x: false, y: false } },
    select: { show: false, left: 0, top: 0, width: 0, height: 0 },
    scales: { x: { time: false }, y: { range: [-1, 1] } },
    axes: [{ show: false }, { show: false }],
    series: [
      {},
      {
        stroke: css("--e-muted", "#888"),
        width: 1.25,
        spanGaps: true,
        points: { show: false },
        fill: css("--graph-white", "#fffaf3"),
        fillTo: -1,
      },
    ],
    hooks: {
      drawClear: [
        (u) => {
          // Black's ground under the line, then White's area as the series' fill from the bottom.
          const ctx = u.ctx;
          ctx.save();
          ctx.fillStyle = css("--graph-black", "#2a2420");
          ctx.fillRect(u.bbox.left, u.bbox.top, u.bbox.width, u.bbox.height);
          ctx.restore();
        },
      ],
      draw: [
        (u) => {
          const ctx = u.ctx;
          const { left, top, width, height } = u.bbox;
          const mid = Math.round(u.valToPos(0, "y", true)) + 0.5;
          ctx.save();
          ctx.strokeStyle = css("--graph-mid", "rgba(200,120,60,0.8)");
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
          ctx.strokeStyle = css("--e-primary", "#b5652a");
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
    if (el.value?.clientWidth) plot?.setSize({ width: el.value.clientWidth, height: props.height ?? 56 });
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
.graph {
  --graph-white: #fffaf3;
  --graph-black: #2a2420;
  --graph-mid: rgb(200 120 60 / 80%);

  position: relative;
  margin: 0;
  min-width: 0;
  overflow: hidden;
  border-radius: var(--e-radius-sm);
  box-shadow: 0 0 0 1px var(--e-border-strong, var(--e-border));
}

/* The hover line follows the pointer; the readout hangs from the top edge beside it. */
.graph-line {
  position: absolute;
  inset-block: 0;
  width: 1px;
  background: var(--e-muted);
  opacity: 0.8;
  pointer-events: none;
}

.graph-tip {
  position: absolute;
  top: 0.125rem;
  padding: 0.125rem 0.375rem;
  border-radius: var(--e-radius-sm);
  background: var(--e-ink);
  color: var(--e-surface);
  font: 600 0.6875rem/1.2 var(--e-font-mono);
  margin-inline-start: 0.375rem;
  pointer-events: none;
  white-space: nowrap;
}

.graph-tip--left {
  margin-inline-start: 0;
  transform: translateX(calc(-100% - 0.375rem));
}

.graph-plot :deep(.u-over) {
  cursor: pointer;
  touch-action: none;
}
</style>
