<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, useId } from "vue";

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
// These are strings: when the data has not changed they are equal, and Vue leaves the SVG alone.
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
/** Where the board is, as a share of the strip, and where the line is at that point (0 top, 1 bottom). */
const place = computed(() => {
  const v = props.values[here.value];
  return {
    "--p": String(here.value / last.value),
    "--y": String(v === null || v === undefined ? 0.5 : y(v) / 100),
  };
});
const hasDot = computed(() => props.values[here.value] !== null && props.values[here.value] !== undefined);

const dragging = ref(false);
const root = ref<HTMLElement>();
const guide = ref<HTMLElement>();
const tip = ref<HTMLElement>();
const tipMove = ref<HTMLElement>();
const tipEval = ref<HTMLElement>();

const describe = (i: number) => {
  const text = props.texts[i];
  return props.moves[i] ? (text ? `${props.moves[i]}, ${text}` : props.moves[i]!) : "";
};

// The pointer is followed with direct writes in an animation frame: no reactive state, so nothing re-renders while the
// mouse moves, and the guide, the label and a drag all keep pace with the display.
let width = 0;
let left = 0;
let pointerX = 0;
let frame = 0;
let shown = -1;
let sent = -1;
let wantHover = false;
let observer: ResizeObserver | undefined;

function measure() {
  const box = root.value?.getBoundingClientRect();
  if (!box) return;
  width = box.width;
  left = box.left;
}
onMounted(() => {
  observer = new ResizeObserver(measure);
  observer.observe(root.value!);
  measure();
});
onBeforeUnmount(() => {
  observer?.disconnect();
  cancelAnimationFrame(frame);
});

function paint() {
  frame = 0;
  const g = guide.value;
  const t = tip.value;
  if (!g || !t) return;
  if (!wantHover || !width) {
    g.style.opacity = "0";
    t.style.opacity = "0";
    shown = -1;
    return;
  }
  const f = Math.max(0, Math.min(1, (pointerX - left) / width));
  const i = Math.round(f * last.value);
  // The guide is the pointer, to the pixel; the label names the move it would land on.
  g.style.transform = `translate3d(${f * width}px,0,0)`;
  g.style.opacity = "0.55";
  if (i !== shown) {
    shown = i;
    tipMove.value!.textContent = props.moves[i] ?? "";
    const text = props.texts[i];
    tipEval.value!.textContent = text ?? "";
    tipEval.value!.hidden = !text;
  }
  // The label keeps inside the strip: its edge, not its centre, follows the pointer near either end.
  t.style.transform = `translate3d(${f * width}px,0,0) translate(${-f * 100}%,0)`;
  t.style.opacity = "1";
  if (dragging.value && i !== sent) {
    sent = i;
    emit("select", i);
  }
}
function queue() {
  if (!frame) frame = requestAnimationFrame(paint);
}

function indexAt(x: number) {
  const f = width ? Math.max(0, Math.min(1, (x - left) / width)) : 0;
  return Math.round(f * last.value);
}

function onEnter() {
  measure();
}
function onDown(e: PointerEvent) {
  if (e.button !== 0) return;
  measure();
  dragging.value = true;
  root.value!.setPointerCapture(e.pointerId);
  pointerX = e.clientX;
  wantHover = true;
  // The first press answers at once, not a frame later.
  sent = indexAt(e.clientX);
  emit("select", sent);
  queue();
}
function onMove(e: PointerEvent) {
  pointerX = e.clientX;
  wantHover = true;
  queue();
}
function onUp(e: PointerEvent) {
  dragging.value = false;
  if (e.pointerType !== "mouse") {
    wantHover = false;
    queue();
  }
}
function onLeave() {
  if (dragging.value) return;
  wantHover = false;
  queue();
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
</script>

<template>
  <div
    ref="root"
    class="tl"
    :class="{ 'tl--drag': dragging }"
    :style="place"
    role="slider"
    tabindex="0"
    :aria-label="label"
    aria-orientation="horizontal"
    aria-valuemin="0"
    :aria-valuemax="last"
    :aria-valuenow="here"
    :aria-valuetext="describe(here) || 'Start'"
    @pointerenter="onEnter"
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
      <div class="tl-ahead"></div>
      <div ref="guide" class="tl-guide"></div>
    </div>
    <div class="tl-now" aria-hidden="true">
      <span class="tl-dot" :class="{ 'tl-dot--none': !hasDot }"></span>
    </div>
    <div ref="tip" class="tl-tip" aria-hidden="true">
      <span ref="tipMove" class="tl-move"></span>
      <span ref="tipEval" class="tl-eval num" hidden></span>
    </div>
  </div>
</template>

<style scoped>
.tl {
  --tl-height: 2rem;
  --ease: 0.14s cubic-bezier(0.3, 0.7, 0.3, 1);

  position: relative;
  height: var(--tl-height);
  min-width: 0;
  border-radius: var(--e-radius-sm);
  container-type: inline-size;
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
  transition: box-shadow 0.12s;
}

.tl:hover .tl-plot {
  box-shadow: inset 0 0 0 1px var(--e-border-strong);
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

/* What is ahead: a veil scaled from the marker to the end, so it eases with the marker and never lays out. */
.tl-ahead {
  position: absolute;
  inset: 0;
  background: color-mix(in oklab, var(--e-surface) 55%, transparent);
  pointer-events: none;
  transform: scaleX(calc(1 - var(--p)));
  transform-origin: 100% 50%;
  transition: transform var(--ease);
}

.tl-guide {
  position: absolute;
  inset-block: 0;
  inset-inline-start: 0;
  width: 1px;
  background: var(--e-ink);
  opacity: 0;
  pointer-events: none;
  will-change: transform;
}

/* Where the board is: a bar through the strip and a dot on the line, both easing to the next move. */
.tl-now {
  position: absolute;
  inset-block: 0;
  inset-inline-start: 0;
  width: 0;
  pointer-events: none;
  transform: translateX(calc(var(--p) * 100cqw));
  transition: transform var(--ease);
  will-change: transform;
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
  inset-block-start: 0;
  inset-inline-start: -0.3125rem;
  width: 0.625rem;
  height: 0.625rem;
  margin-block-start: -0.3125rem;
  border-radius: 50%;
  background: var(--e-primary);
  box-shadow: 0 0 0 2px var(--e-surface);
  transform: translateY(calc(var(--y) * var(--tl-height)));
  transition:
    transform var(--ease),
    opacity 0.12s;
}

.tl-dot--none {
  opacity: 0;
}

/* A drag follows the pointer exactly: no easing to lag behind it. */
.tl--drag .tl-now,
.tl--drag .tl-ahead,
.tl--drag .tl-dot {
  transition: none;
}

.tl-tip {
  position: absolute;
  z-index: 2;
  inset-block-end: calc(100% + var(--e-gap-snug));
  inset-inline-start: 0;
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
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.1s;
  white-space: nowrap;
  will-change: transform;
}

.tl-eval {
  color: var(--e-muted);
  font-weight: 500;
}

@media (prefers-reduced-motion: reduce) {
  .tl-now,
  .tl-ahead,
  .tl-dot {
    transition: none;
  }
}
</style>
