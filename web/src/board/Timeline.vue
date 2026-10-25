<script setup lang="ts">
import { computed, ref, useId } from "vue";

const props = defineProps<{
  /** What the strip is for, for screen readers. */
  label: string;
  /** White's edge at each position from -1 (Black winning) to 1 (White winning); `null` where nothing is known. */
  values: (number | null)[];
  /** The move that leads to each position, such as `24. Nf3`. */
  moves: string[];
  /** The evaluation at each position, such as `+0.45`. */
  texts: (string | null)[];
  /** The position on the board, as an index into `values`. */
  cursor: number;
}>();
const emit = defineEmits<{ select: [index: number] }>();

/** Ticks mark every ten moves. */
const TICK = 20;

const id = useId();
const last = computed(() => Math.max(1, props.values.length - 1));
const pct = (i: number) => `${(Math.min(i, last.value) / last.value) * 100}%`;
const y = (v: number) => 50 - v * 50;

/** Runs of known values, so a gap in the evaluations breaks the line instead of bridging it. */
const runs = computed(() => {
  const out: [number, number][][] = [];
  let run: [number, number][] = [];
  props.values.forEach((v, i) => {
    if (v === null) {
      if (run.length) out.push(run);
      run = [];
    } else run.push([i, y(v)]);
  });
  if (run.length) out.push(run);
  return out;
});
const outline = computed(() =>
  runs.value
    .filter((r) => r.length > 1)
    .map((r) => r.map(([x, v], i) => `${i ? "L" : "M"}${x} ${v.toFixed(1)}`).join(""))
    .join(""),
);
/** The area between the line and the even line, split into White's and Black's halves by clipping. */
const area = computed(() =>
  runs.value
    .filter((r) => r.length > 1)
    .map((r) => `M${r[0]![0]} 50${r.map(([x, v]) => `L${x} ${v.toFixed(1)}`).join("")}L${r.at(-1)![0]} 50Z`)
    .join(""),
);
const ticks = computed(() => {
  let d = "";
  for (let i = TICK; i < props.values.length; i += TICK) d += `M${i} 0V100`;
  return d;
});

const here = computed(() => Math.max(0, Math.min(last.value, props.cursor)));
const dot = computed(() => props.values[here.value] ?? null);

const hover = ref<number | null>(null);
const dragging = ref(false);
const root = ref<HTMLElement>();

const describe = (i: number) => {
  const text = props.texts[i];
  return props.moves[i] ? (text ? `${props.moves[i]}, ${text}` : props.moves[i]!) : "";
};

function indexAt(e: PointerEvent) {
  const box = root.value!.getBoundingClientRect();
  const f = box.width ? (e.clientX - box.left) / box.width : 0;
  return Math.max(0, Math.min(last.value, Math.round(f * last.value)));
}

let sent = -1;
function onDown(e: PointerEvent) {
  if (e.button !== 0) return;
  dragging.value = true;
  root.value!.setPointerCapture(e.pointerId);
  const i = indexAt(e);
  hover.value = i;
  sent = i;
  emit("select", i);
}
function onMove(e: PointerEvent) {
  const i = indexAt(e);
  hover.value = i;
  if (dragging.value && i !== sent) {
    sent = i;
    emit("select", i);
  }
}
function onUp(e: PointerEvent) {
  dragging.value = false;
  if (e.pointerType !== "mouse") hover.value = null;
}
function onLeave() {
  if (!dragging.value) hover.value = null;
}

function onKey(e: KeyboardEvent) {
  if (e.ctrlKey || e.metaKey || e.altKey) return;
  const steps: Record<string, number> = {
    ArrowLeft: -1,
    ArrowDown: -1,
    ArrowRight: 1,
    ArrowUp: 1,
    PageDown: -10,
    PageUp: 10,
  };
  let to: number;
  if (e.key in steps) to = here.value + steps[e.key]!;
  else if (e.key === "Home") to = 0;
  else if (e.key === "End") to = last.value;
  else return;
  // The board's own arrow keys would step a second time.
  e.preventDefault();
  e.stopPropagation();
  emit("select", Math.max(0, Math.min(last.value, to)));
}

/** The floating label keeps inside the strip: its edge, not its centre, follows the pointer near either end. */
const tip = computed(() =>
  hover.value === null
    ? null
    : { left: pct(hover.value), shift: `${-(Math.min(hover.value, last.value) / last.value) * 100}%` },
);
</script>

<template>
  <div
    ref="root"
    class="tl"
    :class="{ 'tl--drag': dragging }"
    role="slider"
    tabindex="0"
    :aria-label="label"
    aria-orientation="horizontal"
    aria-valuemin="0"
    :aria-valuemax="last"
    :aria-valuenow="here"
    :aria-valuetext="describe(here) || 'Start'"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @pointercancel="onUp"
    @pointerleave="onLeave"
    @keydown="onKey"
  >
    <div class="tl-plot" aria-hidden="true">
      <svg class="tl-svg" :viewBox="`0 0 ${last} 100`" preserveAspectRatio="none" focusable="false">
        <defs>
          <clipPath :id="`${id}-w`"><rect x="0" y="0" :width="last" height="50" /></clipPath>
          <clipPath :id="`${id}-b`"><rect x="0" y="50" :width="last" height="50" /></clipPath>
        </defs>
        <path :d="area" class="tl-white" :clip-path="`url(#${id}-w)`" />
        <path :d="area" class="tl-black" :clip-path="`url(#${id}-b)`" />
        <path :d="ticks" class="tl-ticks" />
        <path :d="`M0 50H${last}`" class="tl-even" />
        <path :d="outline" class="tl-line" />
      </svg>
      <!-- What is not played yet is dimmed. -->
      <div class="tl-ahead" :style="{ insetInlineStart: pct(here) }"></div>
      <div v-if="tip" class="tl-guide" :style="{ insetInlineStart: tip.left }"></div>
    </div>
    <div class="tl-now" :style="{ insetInlineStart: pct(here) }" aria-hidden="true">
      <span v-if="dot !== null" class="tl-dot" :style="{ insetBlockStart: `${y(dot)}%` }"></span>
    </div>
    <div
      v-if="tip && hover !== null"
      class="tl-tip"
      :style="{ insetInlineStart: tip.left, translate: `${tip.shift} 0` }"
    >
      <span class="tl-move">{{ moves[hover] }}</span>
      <span v-if="texts[hover]" class="tl-eval num">{{ texts[hover] }}</span>
    </div>
  </div>
</template>

<style scoped>
.tl {
  --tl-height: 2rem;

  position: relative;
  height: var(--tl-height);
  min-width: 0;
  border-radius: var(--e-radius-sm);
  cursor: pointer;
  touch-action: none;
  user-select: none;
}

.tl:focus-visible {
  outline: var(--e-stroke-focus) solid var(--e-focus-ring);
  outline-offset: 2px;
}

.tl-plot {
  position: absolute;
  inset: 0;
  overflow: hidden;
  border-radius: inherit;
  background: color-mix(in oklab, var(--e-ink) 6%, var(--e-surface));
  box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--e-border) 70%, transparent);
}

.tl-svg {
  display: block;
  width: 100%;
  height: 100%;
}

/* White's side is the pale wood, Black's the dark, as on the eval bar. */
.tl-white {
  fill: #f4ede2;
}

.tl-black {
  fill: #2a2420;
}

.tl-ticks,
.tl-even,
.tl-line {
  fill: none;
  vector-effect: non-scaling-stroke;
}

.tl-ticks {
  stroke: var(--e-border-strong);
  stroke-dasharray: 2 4;
  stroke-opacity: 0.6;
  stroke-width: 1;
}

.tl-even {
  stroke: var(--e-primary);
  stroke-opacity: 0.45;
  stroke-width: 1;
}

.tl-line {
  stroke: var(--e-primary);
  stroke-linejoin: round;
  stroke-width: 1.5;
}

.tl-ahead {
  position: absolute;
  inset-block: 0;
  inset-inline-end: 0;
  background: color-mix(in oklab, var(--e-surface) 55%, transparent);
  pointer-events: none;
}

.tl-guide {
  position: absolute;
  inset-block: 0;
  width: 1px;
  background: var(--e-ink);
  opacity: 0.55;
  pointer-events: none;
}

/* Where the board is: a bar through the strip, a notch above it and a dot on the line. */
.tl-now {
  position: absolute;
  inset-block: 0;
  width: 0;
  pointer-events: none;
}

.tl-now::before {
  position: absolute;
  inset-block: -0.125rem;
  inset-inline-start: -1px;
  width: 2px;
  background: var(--e-primary);
  content: "";
}

.tl-dot {
  position: absolute;
  inset-inline-start: -0.3125rem;
  width: 0.625rem;
  height: 0.625rem;
  margin-block-start: -0.3125rem;
  border-radius: 50%;
  background: var(--e-primary);
  box-shadow: 0 0 0 2px var(--e-surface);
}

.tl-tip {
  position: absolute;
  z-index: 2;
  inset-block-end: calc(100% + var(--e-gap-snug));
  display: flex;
  align-items: baseline;
  gap: var(--e-gap-snug);
  padding: var(--e-gap-tight) var(--e-gap-snug);
  border-radius: var(--e-radius-sm);
  background: var(--e-surface);
  box-shadow:
    0 0 0 1px var(--e-border-strong),
    var(--e-shadow-drop);
  font: 600 var(--e-text-sm) / 1.2 var(--e-font-mono);
  white-space: nowrap;
  pointer-events: none;
}

.tl-eval {
  color: var(--e-muted);
  font-weight: 500;
}
</style>
